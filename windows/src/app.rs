use std::ffi::CString;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey, ModifiersState};
use winit::window::{Window, WindowAttributes, WindowId};

use acord_viewport::{
    viewport_create, viewport_destroy, viewport_render, viewport_resize,
    viewport_set_text, viewport_get_text, viewport_set_theme, viewport_set_lang,
    viewport_set_line_indicator, viewport_set_gutter_rainbow,
    viewport_set_auto_pair_flags,
    viewport_send_command, viewport_free_string,
    viewport_take_sidecar_bytes, viewport_apply_sidecar_bytes, viewport_free_bytes,
    ViewportHandle,
};
use acord_viewport::sidecar;
use acord_viewport::browser::{self, BrowserHandle};

use crate::config::Config;
use crate::shortcuts::{match_shortcut, MenuAction};
use acord_viewport::editor::ShellAction;

pub struct App {
    window: Option<Window>,
    handle: *mut ViewportHandle,
    config: Config,
    cursor_pos: PhysicalPosition<f64>,
    scale: f32,
    modifiers: ModifiersState,
    current_file: Option<PathBuf>,
    last_autosave_attempt: Instant,
    last_autosaved_hash: Option<u64>,

    browser_window: Option<Window>,
    browser_handle: Option<BrowserHandle>,
    browser_cursor: PhysicalPosition<f64>,
    browser_scale: f32,
}

impl App {
    pub fn new() -> Self {
        Self {
            window: None,
            handle: std::ptr::null_mut(),
            config: Config::load(),
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            scale: 1.0,
            modifiers: ModifiersState::empty(),
            current_file: None,
            last_autosave_attempt: Instant::now(),
            last_autosaved_hash: None,
            browser_window: None,
            browser_handle: None,
            browser_cursor: PhysicalPosition::new(0.0, 0.0),
            browser_scale: 1.0,
        }
    }

    fn sync_settings(&mut self) {
        if self.handle.is_null() { return; }
        let theme = match self.config.theme_mode() {
            "dark" => "kicad",
            "light" => "latte",
            _ => "kicad",
        };
        let c_theme = CString::new(theme).unwrap();
        viewport_set_theme(self.handle, c_theme.as_ptr());

        let ind = CString::new(self.config.line_indicator()).unwrap();
        viewport_set_line_indicator(self.handle, ind.as_ptr());
        viewport_set_gutter_rainbow(self.handle, self.config.gutter_rainbow());
        viewport_set_auto_pair_flags(self.handle, self.config.auto_pair_flags());

        let view = acord_viewport::editor::SettingsView {
            theme_mode: self.config.theme_mode().to_string(),
            line_indicator: self.config.line_indicator().to_string(),
            gutter_rainbow: self.config.gutter_rainbow(),
            auto_save_dir: self.config.notes_dir().to_string_lossy().into_owned(),
        };
        unsafe { (*self.handle).state.settings_view = view; }
    }

    fn dispatch_menu(&mut self, action: MenuAction, event_loop: &ActiveEventLoop) {
        if self.handle.is_null() { return; }
        match action {
            MenuAction::Quit => event_loop.exit(),
            MenuAction::Bold => { viewport_send_command(self.handle, 1); }
            MenuAction::Italic => { viewport_send_command(self.handle, 2); }
            MenuAction::InsertTable => { viewport_send_command(self.handle, 3); }
            MenuAction::Evaluate => { viewport_send_command(self.handle, 5); }
            MenuAction::ZoomIn => { viewport_send_command(self.handle, 7); }
            MenuAction::ZoomOut => { viewport_send_command(self.handle, 8); }
            MenuAction::ZoomReset => { viewport_send_command(self.handle, 9); }
            MenuAction::LiveMode => { viewport_send_command(self.handle, 11); }
            MenuAction::EditorMode => { viewport_send_command(self.handle, 12); }
            MenuAction::ViewMode => { viewport_send_command(self.handle, 13); }
            MenuAction::Find => { viewport_send_command(self.handle, 14); }
            MenuAction::Open => self.open_file(),
            MenuAction::Save => self.save_file(),
            MenuAction::SaveAs => self.save_file_as(),
            MenuAction::NewNote => self.new_note(),
            MenuAction::Settings => unsafe {
                (*self.handle).state.settings_open = !(*self.handle).state.settings_open;
            },
            MenuAction::ExportCrate => {}
            MenuAction::ToggleBrowser => self.toggle_browser(event_loop),
        }
    }

    fn drain_shell_actions(&mut self, event_loop: &ActiveEventLoop) {
        if self.handle.is_null() { return; }
        let action = unsafe { (*self.handle).state.take_pending_shell_action() };
        let Some(action) = action else { return };
        match action {
            ShellAction::NewNote => self.new_note(),
            ShellAction::Open => self.open_file(),
            ShellAction::Save => self.save_file(),
            ShellAction::SaveAs => self.save_file_as(),
            ShellAction::Quit => event_loop.exit(),
            ShellAction::Settings => {}
            ShellAction::ExportCrate => {}
            ShellAction::ToggleBrowser => self.toggle_browser(event_loop),
            ShellAction::SetThemeMode(v) => {
                self.config.set("themeMode", &v);
                self.sync_settings();
            }
            ShellAction::SetLineIndicator(v) => {
                self.config.set("lineIndicatorMode", &v);
                self.sync_settings();
            }
            ShellAction::SetGutterRainbow(b) => {
                self.config.set("gutterRainbow", if b { "true" } else { "false" });
                self.sync_settings();
            }
            ShellAction::PickAutoSaveDir => {
                let dialog = rfd::FileDialog::new()
                    .set_directory(self.config.notes_dir());
                if let Some(path) = dialog.pick_folder() {
                    self.config.set("autoSaveDirectory", &path.to_string_lossy());
                    self.sync_settings();
                }
            }
        }
    }

    fn toggle_browser(&mut self, event_loop: &ActiveEventLoop) {
        if self.browser_window.is_some() {
            self.close_browser();
        } else {
            self.open_browser(event_loop);
        }
    }

    fn open_browser(&mut self, event_loop: &ActiveEventLoop) {
        let mut attrs = WindowAttributes::default()
            .with_title("Documents - Acord")
            .with_inner_size(LogicalSize::new(900.0, 650.0));
        if let Some(icon) = load_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }
        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(_) => return,
        };
        self.browser_scale = window.scale_factor() as f32;
        let size = window.inner_size();
        let w = size.width as f32 / self.browser_scale;
        let h = size.height as f32 / self.browser_scale;

        use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        let display = match window.display_handle() {
            Ok(d) => d.as_raw(),
            Err(_) => return,
        };
        let win_handle = match window.window_handle() {
            Ok(w) => w.as_raw(),
            Err(_) => return,
        };

        let notes_dir = self.config.notes_dir();
        let _ = std::fs::create_dir_all(&notes_dir);

        match browser::handle::create(display, win_handle, w, h, self.browser_scale, notes_dir) {
            Some(handle) => {
                self.browser_handle = Some(handle);
                self.browser_window = Some(window);
            }
            None => {
                drop(window);
            }
        }
    }

    fn close_browser(&mut self) {
        self.browser_handle = None;
        self.browser_window = None;
    }

    fn drain_browser_open(&mut self) {
        let Some(handle) = self.browser_handle.as_mut() else { return };
        let Some(path) = browser::handle::take_pending_open(handle) else { return };
        if let Ok(bytes) = std::fs::read(&path) {
            self.load_file_bytes(&path, bytes);
        }
        self.close_browser();
    }

    fn open_file(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown"])
            .add_filter("All Files", &["*"]);
        if let Some(path) = dialog.pick_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                self.load_file_bytes(&path, bytes);
            }
        }
    }

    fn load_file_bytes(&mut self, path: &std::path::Path, bytes: Vec<u8>) {
        let (text_bytes, archive) = if self.is_acord_note(path) {
            sidecar::extract_from_md(&bytes)
        } else {
            (bytes, self.read_external_sidecar(path))
        };
        let text = String::from_utf8_lossy(&text_bytes).into_owned();
        let c = CString::new(text).unwrap_or_default();
        viewport_set_text(self.handle, c.as_ptr());
        if let Some(zip) = archive {
            viewport_apply_sidecar_bytes(self.handle, zip.as_ptr(), zip.len());
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("md");
        let c_ext = CString::new(ext).unwrap();
        viewport_set_lang(self.handle, c_ext.as_ptr());
        if let Some(w) = &self.window {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Acord");
            w.set_title(&format!("{name} - Acord"));
            w.focus_window();
        }
        self.current_file = Some(path.to_path_buf());
        self.last_autosaved_hash = None;
    }

    fn is_acord_note(&self, path: &std::path::Path) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("md") {
            return false;
        }
        let notes_dir = self.config.notes_dir();
        let canon_dir = std::fs::canonicalize(&notes_dir).unwrap_or(notes_dir);
        let canon_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        canon_path.starts_with(&canon_dir)
    }

    fn external_sidecar_path(&self, original: &std::path::Path) -> PathBuf {
        let canon = std::fs::canonicalize(original).unwrap_or_else(|_| original.to_path_buf());
        let s = canon.to_string_lossy();
        let trimmed = s.trim_start_matches('/').trim_start_matches('\\');
        let encoded: String = trimmed
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' => '.',
                _ => c,
            })
            .collect();
        self.config
            .notes_dir()
            .join(".external")
            .join(format!("{encoded}.acord"))
    }

    fn read_external_sidecar(&self, original: &std::path::Path) -> Option<Vec<u8>> {
        let path = self.external_sidecar_path(original);
        std::fs::read(path).ok()
    }

    fn write_external_sidecar(&self, original: &std::path::Path, archive: Option<&[u8]>) {
        let path = self.external_sidecar_path(original);
        match archive {
            Some(bytes) if !bytes.is_empty() => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&path, bytes);
            }
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    fn take_archive_bytes(&self) -> Option<Vec<u8>> {
        let mut len: usize = 0;
        let ptr = viewport_take_sidecar_bytes(self.handle, &mut len as *mut usize);
        if ptr.is_null() || len == 0 {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
        viewport_free_bytes(ptr, len);
        Some(bytes)
    }

    fn save_file(&mut self) {
        if let Some(path) = self.current_file.clone() {
            self.write_to(&path);
            return;
        }
        let notes_dir = self.config.notes_dir();
        let _ = std::fs::create_dir_all(&notes_dir);
        let path = notes_dir.join(format!("{}.md", self.derive_default_filename()));
        self.write_to(&path);
        self.current_file = Some(path);
    }

    fn save_file_as(&mut self) {
        let notes_dir = self.config.notes_dir();
        let _ = std::fs::create_dir_all(&notes_dir);
        let dialog = rfd::FileDialog::new()
            .add_filter("Markdown", &["md"])
            .add_filter("All Files", &["*"])
            .set_directory(&notes_dir)
            .set_file_name(format!("{}.md", self.derive_default_filename()));
        if let Some(path) = dialog.save_file() {
            self.write_to(&path);
            self.current_file = Some(path);
        }
    }

    fn write_to(&mut self, path: &std::path::Path) {
        let text_ptr = viewport_get_text(self.handle);
        if text_ptr.is_null() { return; }
        let text = unsafe { std::ffi::CStr::from_ptr(text_ptr) }
            .to_string_lossy()
            .into_owned();
        viewport_free_string(text_ptr);

        let archive = self.take_archive_bytes();
        let in_library = self.is_acord_note(path);
        let file_bytes: Vec<u8> = match (&archive, in_library) {
            (Some(arc), true) => sidecar::embed_in_md(text.as_bytes(), arc),
            _ => text.as_bytes().to_vec(),
        };
        if std::fs::write(path, &file_bytes).is_ok() {
            self.last_autosaved_hash = Some(text_hash(&text));
        }
        if !in_library {
            self.write_external_sidecar(path, archive.as_deref());
        }
    }

    fn derive_default_filename(&self) -> String {
        let text_ptr = viewport_get_text(self.handle);
        let text = if text_ptr.is_null() {
            String::new()
        } else {
            let s = unsafe { std::ffi::CStr::from_ptr(text_ptr) }
                .to_string_lossy()
                .into_owned();
            viewport_free_string(text_ptr);
            s
        };
        let title = text.lines().next().unwrap_or("").trim_start();
        let title = title.trim_start_matches('#').trim();
        let cleaned: String = title
            .chars()
            .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            .collect();
        let cleaned = cleaned.trim();
        if cleaned.is_empty() {
            "Untitled".to_string()
        } else {
            cleaned.chars().take(60).collect()
        }
    }

    fn new_note(&mut self) {
        viewport_send_command(self.handle, 12);
        let stub = CString::new("# ").unwrap();
        viewport_set_text(self.handle, stub.as_ptr());
        if let Some(w) = &self.window {
            w.set_title("Acord");
        }
        self.current_file = None;
        self.last_autosaved_hash = None;
    }

    /// Hash-gated autosave. Mirrors the macOS Swift `persistViewportToNotesDir`:
    /// fires on a poll cadence, skips the disk write when the buffer hash
    /// matches the last saved value. Without the hash gate this would rewrite
    /// the note every poll tick (~MB/s on a busy doc).
    fn try_autosave(&mut self) {
        if self.handle.is_null() { return; }
        let text_ptr = viewport_get_text(self.handle);
        if text_ptr.is_null() { return; }
        let text = unsafe { std::ffi::CStr::from_ptr(text_ptr) }
            .to_string_lossy()
            .into_owned();
        viewport_free_string(text_ptr);

        let hash = text_hash(&text);
        if Some(hash) == self.last_autosaved_hash { return; }

        // skip the launch stub so it can't overwrite last session's Untitled.md.
        if self.current_file.is_none() && is_effectively_blank(&text) {
            return;
        }

        let path = self.current_file.clone().unwrap_or_else(|| {
            self.config.notes_dir().join("Untitled.md")
        });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let archive = self.take_archive_bytes();
        let in_library = self.is_acord_note(&path);
        let file_bytes: Vec<u8> = match (&archive, in_library) {
            (Some(arc), true) => sidecar::embed_in_md(text.as_bytes(), arc),
            _ => text.as_bytes().to_vec(),
        };
        if std::fs::write(&path, &file_bytes).is_ok() {
            self.last_autosaved_hash = Some(hash);
        }
        if !in_library {
            self.write_external_sidecar(&path, archive.as_deref());
        }
    }

    fn winit_button(button: MouseButton) -> u8 {
        match button {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Other(n) => n as u8,
            _ => 0,
        }
    }

    fn handle_browser_event(&mut self, event: WindowEvent) {
        let Some(handle) = self.browser_handle.as_mut() else { return };

        match event {
            WindowEvent::CloseRequested => {
                self.close_browser();
            }
            WindowEvent::Resized(size) => {
                let w = size.width as f32 / self.browser_scale;
                let h = size.height as f32 / self.browser_scale;
                browser::handle::resize(handle, w, h, self.browser_scale);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.browser_scale = scale_factor as f32;
                if let Some(win) = &self.browser_window {
                    let size = win.inner_size();
                    let w = size.width as f32 / self.browser_scale;
                    let h = size.height as f32 / self.browser_scale;
                    browser::handle::resize(handle, w, h, self.browser_scale);
                }
            }
            WindowEvent::RedrawRequested => {
                browser::handle::render(handle);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.browser_cursor = position;
                let x = position.x as f32 / self.browser_scale;
                let y = position.y as f32 / self.browser_scale;
                browser::handle::push_mouse_move(handle, x, y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                browser::handle::push_mouse_button(handle, Self::winit_button(button), pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => (dx * 20.0, dy * 20.0),
                    MouseScrollDelta::PixelDelta(d) => (d.x as f32, d.y as f32),
                };
                browser::handle::push_scroll(handle, dx, -dy);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                use iced_wgpu::core::keyboard;
                use iced_wgpu::core::Event as IcedEvent;
                let pressed = event.state == ElementState::Pressed;

                if pressed {
                    if let Some(action) = match_shortcut(self.modifiers, &event.logical_key) {
                        let msg = match action {
                            MenuAction::ZoomIn => Some(browser::BrowserMessage::ScaleUp),
                            MenuAction::ZoomOut => Some(browser::BrowserMessage::ScaleDown),
                            MenuAction::ZoomReset => Some(browser::BrowserMessage::ScaleReset),
                            _ => None,
                        };
                        if let Some(msg) = msg {
                            handle.state.update(msg);
                            handle.needs_redraw = true;
                            return;
                        }
                    }
                }

                let modifiers = decode_winit_modifiers(self.modifiers);
                let key = winit_key_to_iced(&event.logical_key);
                let text = event.text.as_ref().map(|s| iced_wgpu::core::SmolStr::new(s.as_str()));
                let physical_key = keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified);
                let location = keyboard::Location::Standard;
                let modified_key = key.clone();
                let ev = if pressed {
                    keyboard::Event::KeyPressed {
                        key,
                        modified_key,
                        physical_key,
                        location,
                        modifiers,
                        text,
                        repeat: event.repeat,
                    }
                } else {
                    keyboard::Event::KeyReleased {
                        key,
                        modified_key,
                        physical_key,
                        location,
                        modifiers,
                    }
                };
                browser::handle::push_event(handle, IcedEvent::Keyboard(ev));
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                use iced_wgpu::core::keyboard;
                use iced_wgpu::core::Event as IcedEvent;
                browser::handle::push_event(
                    handle,
                    IcedEvent::Keyboard(keyboard::Event::ModifiersChanged(decode_winit_modifiers(mods.state()))),
                );
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }

        let mut attrs = WindowAttributes::default()
            .with_title("Acord")
            .with_inner_size(LogicalSize::new(1100.0, 750.0));

        // Load window icon from the bundled PNG (rasterized from SVG at build
        // time or shipped alongside the exe). Falls back to no icon silently.
        if let Some(icon) = load_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }

        let window = event_loop.create_window(attrs).expect("create window");
        self.scale = window.scale_factor() as f32;

        let size = window.inner_size();
        let w = size.width as f32 / self.scale;
        let h = size.height as f32 / self.scale;

        // Get raw HWND and pass to viewport.
        use raw_window_handle::HasWindowHandle;
        let wh = window.window_handle().expect("window handle");
        let raw = wh.as_raw();

        let hwnd = match raw {
            #[cfg(target_os = "windows")]
            raw_window_handle::RawWindowHandle::Win32(h) => h.hwnd.get() as *mut c_void,
            #[cfg(target_os = "macos")]
            raw_window_handle::RawWindowHandle::AppKit(h) => h.ns_view.as_ptr(),
            _ => std::ptr::null_mut(),
        };

        self.handle = viewport_create(hwnd, w, h, self.scale);
        self.sync_settings();
        viewport_send_command(self.handle, 12);
        let stub = CString::new("# ").unwrap();
        viewport_set_text(self.handle, stub.as_ptr());
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let is_browser = self.browser_window.as_ref().map(|w| w.id() == id).unwrap_or(false);
        if is_browser {
            self.handle_browser_event(event);
            return;
        }

        if self.handle.is_null() { return; }

        match event {
            WindowEvent::CloseRequested => {
                if !self.handle.is_null() {
                    viewport_destroy(self.handle);
                    self.handle = std::ptr::null_mut();
                }
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                let w = size.width as f32 / self.scale;
                let h = size.height as f32 / self.scale;
                viewport_resize(self.handle, w, h, self.scale);
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor as f32;
                if let Some(win) = &self.window {
                    let size = win.inner_size();
                    let w = size.width as f32 / self.scale;
                    let h = size.height as f32 / self.scale;
                    viewport_resize(self.handle, w, h, self.scale);
                }
            }

            WindowEvent::RedrawRequested => {
                viewport_render(self.handle);
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = position;
                let x = position.x as f32 / self.scale;
                let y = position.y as f32 / self.scale;
                acord_viewport::viewport_mouse_event(
                    self.handle, x, y, 255, false,
                );
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let x = self.cursor_pos.x as f32 / self.scale;
                let y = self.cursor_pos.y as f32 / self.scale;
                let pressed = state == ElementState::Pressed;
                acord_viewport::viewport_mouse_event(
                    self.handle, x, y, Self::winit_button(button), pressed,
                );
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let x = self.cursor_pos.x as f32 / self.scale;
                let y = self.cursor_pos.y as f32 / self.scale;
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => (dx * 20.0, dy * 20.0),
                    MouseScrollDelta::PixelDelta(d) => (d.x as f32, d.y as f32),
                };
                acord_viewport::viewport_scroll_event(self.handle, x, y, dx, -dy);
            }

            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;

                if pressed {
                    if let Some(action) = match_shortcut(self.modifiers, &event.logical_key) {
                        self.dispatch_menu(action, event_loop);
                        return;
                    }
                }

                let text_str = event.text.as_ref().map(|s| s.to_string());
                let text_c = text_str.as_deref()
                    .and_then(|s| CString::new(s).ok());
                let text_ptr = text_c.as_ref()
                    .map(|c| c.as_ptr())
                    .unwrap_or(std::ptr::null());

                let keycode = winit_key_to_code(&event.logical_key);
                let mod_flags = encode_modifiers(self.modifiers);

                acord_viewport::viewport_key_event(
                    self.handle, keycode, mod_flags, pressed, text_ptr,
                );
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                if !self.handle.is_null() {
                    let state = mods.state();
                    let h = unsafe { &mut *self.handle };
                    use iced_wgpu::core::keyboard;
                    use iced_wgpu::core::Event;
                    h.events.push(Event::Keyboard(
                        keyboard::Event::ModifiersChanged(decode_winit_modifiers(state)),
                    ));
                    h.needs_redraw = true;
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.last_autosave_attempt.elapsed() >= Duration::from_millis(500) {
            self.last_autosave_attempt = Instant::now();
            self.try_autosave();
        }
        self.drain_shell_actions(_event_loop);
        self.drain_browser_open();
        if let Some(w) = &self.window {
            if !self.handle.is_null() {
                w.request_redraw();
            }
        }
        if let Some(w) = &self.browser_window {
            w.request_redraw();
        }
    }
}

fn text_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// true when the buffer is empty or just leading heading markers.
fn is_effectively_blank(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    trimmed.trim_start_matches('#').trim().is_empty()
}

/// Map winit logical keys to iced keyboard keys for direct iced event push.
fn winit_key_to_iced(key: &Key) -> iced_wgpu::core::keyboard::Key {
    use iced_wgpu::core::keyboard::{key as ikey, Key as IKey};
    match key {
        Key::Named(n) => match n {
            NamedKey::Enter => IKey::Named(ikey::Named::Enter),
            NamedKey::Tab => IKey::Named(ikey::Named::Tab),
            NamedKey::Backspace => IKey::Named(ikey::Named::Backspace),
            NamedKey::Escape => IKey::Named(ikey::Named::Escape),
            NamedKey::Delete => IKey::Named(ikey::Named::Delete),
            NamedKey::ArrowLeft => IKey::Named(ikey::Named::ArrowLeft),
            NamedKey::ArrowRight => IKey::Named(ikey::Named::ArrowRight),
            NamedKey::ArrowUp => IKey::Named(ikey::Named::ArrowUp),
            NamedKey::ArrowDown => IKey::Named(ikey::Named::ArrowDown),
            NamedKey::Home => IKey::Named(ikey::Named::Home),
            NamedKey::End => IKey::Named(ikey::Named::End),
            NamedKey::PageUp => IKey::Named(ikey::Named::PageUp),
            NamedKey::PageDown => IKey::Named(ikey::Named::PageDown),
            NamedKey::Space => IKey::Named(ikey::Named::Space),
            _ => IKey::Unidentified,
        },
        Key::Character(s) => IKey::Character(iced_wgpu::core::SmolStr::new(s.as_str())),
        _ => IKey::Unidentified,
    }
}

/// Map winit logical keys to the macOS-style keycodes the bridge expects.
/// For Named keys, return the matching keycode. For character keys, the
/// bridge ignores the keycode and uses the text parameter directly, so
/// we return 0 (unmapped).
fn winit_key_to_code(key: &Key) -> u32 {
    match key {
        Key::Named(n) => match n {
            NamedKey::Enter => 36,
            NamedKey::Tab => 48,
            NamedKey::Backspace => 51,
            NamedKey::Escape => 53,
            NamedKey::Delete => 117,
            NamedKey::ArrowLeft => 123,
            NamedKey::ArrowRight => 124,
            NamedKey::ArrowDown => 125,
            NamedKey::ArrowUp => 126,
            NamedKey::Home => 115,
            NamedKey::End => 119,
            NamedKey::PageUp => 116,
            NamedKey::PageDown => 121,
            NamedKey::F1 => 122,
            NamedKey::F2 => 120,
            NamedKey::F3 => 99,
            NamedKey::F4 => 118,
            NamedKey::F5 => 96,
            NamedKey::F6 => 97,
            NamedKey::F7 => 98,
            NamedKey::F8 => 100,
            NamedKey::F9 => 101,
            NamedKey::F10 => 109,
            NamedKey::F11 => 103,
            NamedKey::F12 => 111,
            _ => 0,
        },
        _ => 0,
    }
}

fn encode_modifiers(state: ModifiersState) -> u32 {
    let mut flags = 0u32;
    if state.shift_key() { flags |= 1 << 17; }
    if state.control_key() { flags |= 1 << 18; }
    if state.alt_key() { flags |= 1 << 19; }
    if state.super_key() { flags |= 1 << 20; }
    // Mirror Ctrl→LOGO so the viewport's `modifiers.logo()` shortcut arms fire.
    // Matches `decode_winit_modifiers` below; without this, only menu-accelerated
    // shortcuts (B/I/T) reach the viewport on Windows.
    if state.control_key() { flags |= 1 << 20; }
    flags
}

fn decode_winit_modifiers(state: ModifiersState) -> iced_wgpu::core::keyboard::Modifiers {
    let mut m = iced_wgpu::core::keyboard::Modifiers::empty();
    if state.shift_key() { m |= iced_wgpu::core::keyboard::Modifiers::SHIFT; }
    if state.control_key() { m |= iced_wgpu::core::keyboard::Modifiers::CTRL; }
    if state.alt_key() { m |= iced_wgpu::core::keyboard::Modifiers::ALT; }
    // On Windows, Ctrl is the action modifier (not Logo/Super).
    // Map Ctrl to LOGO so iced's Cmd+C/V/X bindings work via Ctrl on Windows.
    if state.control_key() { m |= iced_wgpu::core::keyboard::Modifiers::LOGO; }
    m
}

/// Load the app icon from `assets/Acord.svg` (relative to exe) or a
/// pre-rasterized PNG next to the exe. Returns None on any failure.
fn load_window_icon() -> Option<winit::window::Icon> {
    // Try loading a PNG icon next to the exe first.
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    // Try pre-rasterized icon.png next to exe.
    let png_path = exe_dir.join("icon.png");
    let bytes = if png_path.exists() {
        std::fs::read(&png_path).ok()?
    } else {
        // Fall back to the SVG in the assets dir (repo layout).
        let svg_path = exe_dir.join("../assets/Acord.svg")
            .canonicalize().ok()
            .or_else(|| {
                // Running from repo root via cargo run.
                std::path::PathBuf::from("assets/Acord.svg").canonicalize().ok()
            })?;
        // Use rsvg-convert at runtime as a fallback.
        let output = std::process::Command::new("rsvg-convert")
            .args(["--width", "256", "--height", "256"])
            .arg(&svg_path)
            .output()
            .ok()?;
        if !output.status.success() { return None; }
        output.stdout
    };

    let img = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

use std::ffi::CString;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use acord_viewport::{
    viewport_create, viewport_destroy, viewport_render, viewport_resize,
    viewport_set_text, viewport_get_text, viewport_set_theme, viewport_set_lang,
    viewport_set_line_indicator, viewport_set_gutter_rainbow,
    viewport_set_auto_pair_flags,
    viewport_send_command, viewport_free_string,
    ViewportHandle,
};

use crate::config::Config;
use crate::shortcuts::{match_shortcut, MenuAction};

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
        }
    }

    fn sync_settings(&self) {
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
            MenuAction::Settings => {
                let cfg = config_path();
                // Prefer xdg-open; fall back to $EDITOR or nano.
                let opened = std::process::Command::new("xdg-open").arg(&cfg).spawn().is_ok();
                if !opened {
                    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
                    let _ = std::process::Command::new(editor).arg(&cfg).spawn();
                }
            }
            MenuAction::ExportCrate => { /* TODO: wire crate export */ }
        }
    }

    fn open_file(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown"])
            .add_filter("All Files", &["*"]);
        if let Some(path) = dialog.pick_file() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let c = CString::new(text).unwrap_or_default();
                viewport_set_text(self.handle, c.as_ptr());
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("md");
                let c_ext = CString::new(ext).unwrap();
                viewport_set_lang(self.handle, c_ext.as_ptr());
                if let Some(w) = &self.window {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Acord");
                    w.set_title(&format!("{name} - Acord"));
                }
                self.current_file = Some(path);
                self.last_autosaved_hash = None;
            }
        }
    }

    fn save_file(&mut self) {
        match self.current_file.clone() {
            Some(path) => self.write_to(&path),
            None => self.save_file_as(),
        }
    }

    fn save_file_as(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("Markdown", &["md"])
            .add_filter("All Files", &["*"])
            .set_file_name("note.md");
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
        if std::fs::write(path, &text).is_ok() {
            self.last_autosaved_hash = Some(text_hash(&text));
        }
    }

    fn new_note(&mut self) {
        let stub = CString::new("# ").unwrap();
        viewport_set_text(self.handle, stub.as_ptr());
        if let Some(w) = &self.window {
            w.set_title("Acord");
        }
        self.current_file = None;
        self.last_autosaved_hash = None;
    }

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

        let path = self.current_file.clone().unwrap_or_else(|| {
            self.config.notes_dir().join("Untitled.md")
        });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&path, &text).is_ok() {
            self.last_autosaved_hash = Some(hash);
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
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }

        let mut attrs = WindowAttributes::default()
            .with_title("Acord")
            .with_inner_size(LogicalSize::new(1100.0, 750.0));

        if let Some(icon) = load_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }

        let window = event_loop.create_window(attrs).expect("create window");
        self.scale = window.scale_factor() as f32;

        let size = window.inner_size();
        let w = size.width as f32 / self.scale;
        let h = size.height as f32 / self.scale;

        // Pass the raw display+window handle to the viewport. wgpu picks the
        // X11 or Wayland backend automatically based on which RawDisplayHandle
        // variant winit hands us, so a single binary works on both.
        use raw_window_handle::HasWindowHandle;
        let wh = window.window_handle().expect("window handle");
        let raw = wh.as_raw();

        let surface_handle = match raw {
            #[cfg(target_os = "linux")]
            raw_window_handle::RawWindowHandle::Xlib(h) => h.window as *mut std::ffi::c_void,
            #[cfg(target_os = "linux")]
            raw_window_handle::RawWindowHandle::Xcb(h) => h.window.get() as *mut std::ffi::c_void,
            #[cfg(target_os = "linux")]
            raw_window_handle::RawWindowHandle::Wayland(h) => h.surface.as_ptr(),
            _ => std::ptr::null_mut(),
        };

        self.handle = viewport_create(surface_handle, w, h, self.scale);
        self.sync_settings();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
                acord_viewport::viewport_mouse_event(self.handle, x, y, 255, false);
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

                // App-level shortcut? Fire on press only and short-circuit so the
                // viewport's text_editor doesn't also see the keystroke (otherwise
                // Ctrl+S would type 's' as well as save).
                if pressed {
                    if let Some(action) = match_shortcut(self.modifiers, &event.logical_key) {
                        self.dispatch_menu(action, event_loop);
                        return;
                    }
                }

                let text_str = event.text.as_ref().map(|s| s.to_string());
                let text_c = text_str.as_deref().and_then(|s| CString::new(s).ok());
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
        if let Some(w) = &self.window {
            if !self.handle.is_null() {
                w.request_redraw();
            }
        }
    }
}

fn text_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Maps winit logical keys to the macOS-style virtual keycodes the bridge
/// expects. Character keys go through `text` instead, so 0 is fine for those.
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
    // Mirror Ctrl→LOGO so iced text_editor's Cmd+C/V/X/Z/A bindings fire on
    // Ctrl. Same trick the Windows shell uses; both action-modifier-on-Ctrl
    // platforms need it.
    if state.control_key() { flags |= 1 << 20; }
    flags
}

fn decode_winit_modifiers(state: ModifiersState) -> iced_wgpu::core::keyboard::Modifiers {
    let mut m = iced_wgpu::core::keyboard::Modifiers::empty();
    if state.shift_key() { m |= iced_wgpu::core::keyboard::Modifiers::SHIFT; }
    if state.control_key() { m |= iced_wgpu::core::keyboard::Modifiers::CTRL; }
    if state.alt_key() { m |= iced_wgpu::core::keyboard::Modifiers::ALT; }
    if state.control_key() { m |= iced_wgpu::core::keyboard::Modifiers::LOGO; }
    m
}

fn config_path() -> std::path::PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return std::path::PathBuf::from(xdg).join("acord").join("config.json");
        }
    }
    if let Some(cfg) = dirs::config_dir() {
        return cfg.join("acord").join("config.json");
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(".acord")
        .join("config.json")
}

/// Loads `icon.png` next to the exe. Returns None on any failure — winit
/// silently uses the WM default in that case.
fn load_window_icon() -> Option<winit::window::Icon> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let png_path = exe_dir.join("icon.png");
    let bytes = if png_path.exists() {
        std::fs::read(&png_path).ok()?
    } else {
        // Fall back to repo-root assets when running via cargo run.
        let svg_path = std::path::PathBuf::from("assets/Acord.svg").canonicalize().ok()?;
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

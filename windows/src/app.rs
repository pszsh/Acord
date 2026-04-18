use std::ffi::{c_void, CString};

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
    viewport_send_command, viewport_free_string,
    ViewportHandle,
};

use crate::config::Config;
use crate::menu::{AppMenu, MenuAction};

pub struct App {
    window: Option<Window>,
    handle: *mut ViewportHandle,
    config: Config,
    menu: Option<AppMenu>,
    cursor_pos: PhysicalPosition<f64>,
    scale: f32,
}

impl App {
    pub fn new() -> Self {
        Self {
            window: None,
            handle: std::ptr::null_mut(),
            config: Config::load(),
            menu: None,
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            scale: 1.0,
        }
    }

    fn sync_settings(&self) {
        if self.handle.is_null() { return; }
        let theme = match self.config.theme_mode() {
            "dark" => "kicad",
            "light" => "latte",
            _ => "kicad", // Windows: default dark. No NSAppearance auto-detect.
        };
        let c_theme = CString::new(theme).unwrap();
        viewport_set_theme(self.handle, c_theme.as_ptr());

        let ind = CString::new(self.config.line_indicator()).unwrap();
        viewport_set_line_indicator(self.handle, ind.as_ptr());
        viewport_set_gutter_rainbow(self.handle, self.config.gutter_rainbow());
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
            MenuAction::Undo => { /* TODO */ },
            MenuAction::Redo => { /* TODO */ },
            MenuAction::ExportCrate => { /* TODO */ },
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
                let ext = path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("md");
                let c_ext = CString::new(ext).unwrap();
                viewport_set_lang(self.handle, c_ext.as_ptr());
                if let Some(w) = &self.window {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Acord");
                    w.set_title(&format!("{name} - Acord"));
                }
            }
        }
    }

    fn save_file(&self) {
        self.save_file_as();
    }

    fn save_file_as(&self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("Markdown", &["md"])
            .add_filter("All Files", &["*"])
            .set_file_name("note.md");
        if let Some(path) = dialog.save_file() {
            let text_ptr = viewport_get_text(self.handle);
            if !text_ptr.is_null() {
                let text = unsafe { std::ffi::CStr::from_ptr(text_ptr) }
                    .to_string_lossy()
                    .into_owned();
                viewport_free_string(text_ptr);
                let _ = std::fs::write(&path, text);
            }
        }
    }

    fn new_note(&mut self) {
        let empty = CString::new("").unwrap();
        viewport_set_text(self.handle, empty.as_ptr());
        if let Some(w) = &self.window {
            w.set_title("Acord");
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

        let attrs = WindowAttributes::default()
            .with_title("Acord")
            .with_inner_size(LogicalSize::new(1100.0, 750.0));
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

        // Set up native menu bar.
        let app_menu = AppMenu::new();
        #[cfg(target_os = "windows")]
        {
            if let raw_window_handle::RawWindowHandle::Win32(h) = raw {
                unsafe { app_menu.menu.init_for_hwnd(h.hwnd.get()).ok(); }
            }
        }
        self.menu = Some(app_menu);
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
                let text_str = event.text.as_ref().map(|s| s.to_string());
                let text_c = text_str.as_deref()
                    .and_then(|s| CString::new(s).ok());
                let text_ptr = text_c.as_ref()
                    .map(|c| c.as_ptr())
                    .unwrap_or(std::ptr::null());

                let keycode = winit_key_to_code(&event.logical_key);
                let modifiers = if let Some(w) = &self.window {
                    // No direct modifier query on winit 0.30 Window.
                    // Modifiers come via ModifiersChanged. We track them.
                    0u32
                } else {
                    0u32
                };

                acord_viewport::viewport_key_event(
                    self.handle, keycode, modifiers, pressed, text_ptr,
                );
            }

            WindowEvent::ModifiersChanged(mods) => {
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
        // Poll menu events.
        while let Some(action) = AppMenu::poll() {
            self.dispatch_menu(action, _event_loop);
        }
        // Request a redraw if the viewport has pending work.
        if let Some(w) = &self.window {
            if !self.handle.is_null() {
                // Always request redraw — viewport_render short-circuits
                // internally when idle (needs_redraw == false && no pending
                // eval). Requesting unconditionally is simpler than reading
                // the handle's state from here, and wgpu PresentMode::Fifo
                // throttles to vsync anyway.
                w.request_redraw();
            }
        }
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

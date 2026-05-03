use std::ffi::{c_char, c_void, CStr, CString};

pub mod block;
pub mod blocks;
mod bridge;
pub mod browser;
pub mod editor;
pub mod export;
pub mod handle;
pub mod heading_block;
pub mod hr_block;
pub mod module;
pub mod oklab;
pub mod palette;
pub mod selection;
pub mod sidecar;
pub mod syntax;
pub mod table_block;
pub mod text_block;
pub mod text_widget;
pub mod tree_block;

pub use acord_core::*;

use editor::EditorState;
use iced_graphics::Viewport;
use iced_runtime::user_interface;
use iced_wgpu::core::Event;

#[allow(dead_code)]
pub struct ViewportHandle {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    scale: f32,

    renderer: iced_wgpu::Renderer,
    viewport: Viewport,
    cache: user_interface::Cache,
    pub state: EditorState,
    pub events: Vec<Event>,
    cursor: iced_wgpu::core::mouse::Cursor,
    /// Set true on any FFI input or state-change call. handle::render() early-returns
    /// when this is false AND no pending eval debounce, so the vsync display link
    /// becomes a microsecond no-op while the editor is idle.
    pub needs_redraw: bool,
}

/// Install a panic hook that flushes a full backtrace to stderr AND to
/// `~/.acord/crash.log` before the process aborts. Called once on first
/// viewport_create. Without the file fallback, the Windows release build
/// (`#![windows_subsystem = "windows"]`) detaches the console and stderr
/// goes nowhere — users get a silent crash with no diagnostic surface.
fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let prior = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            use std::io::Write;
            let bt = std::backtrace::Backtrace::force_capture();
            let header = "===== ACORD RUST PANIC =====";
            let footer = "============================";
            {
                let mut err = std::io::stderr().lock();
                let _ = writeln!(err, "{}", header);
                let _ = writeln!(err, "{}", info);
                let _ = writeln!(err, "{}", bt);
                let _ = writeln!(err, "{}", footer);
                let _ = err.flush();
            }
            if let Some(home) = dirs::home_dir() {
                let dir = home.join(".acord");
                let _ = std::fs::create_dir_all(&dir);
                let path = dir.join("crash.log");
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true).append(true).open(&path)
                {
                    let _ = writeln!(f, "{} {}", header, chrono_now());
                    let _ = writeln!(f, "{}", info);
                    let _ = writeln!(f, "{}", bt);
                    let _ = writeln!(f, "{}", footer);
                }
            }
            prior(info);
        }));
    });
}

/// Best-effort timestamp for the crash log header. Avoids pulling chrono
/// for one line — uses SystemTime::now() epoch seconds as a stable suffix.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("(epoch {}s)", d.as_secs()))
        .unwrap_or_else(|_| String::from("(time unavailable)"))
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_create(
    nsview: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
) -> *mut ViewportHandle {
    install_panic_hook();
    if nsview.is_null() {
        return std::ptr::null_mut();
    }
    match handle::create(nsview, width, height, scale) {
        Some(h) => Box::into_raw(Box::new(h)),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_destroy(handle: *mut ViewportHandle) {
    if handle.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_render(handle: *mut ViewportHandle) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    handle::render(h);
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_resize(
    handle: *mut ViewportHandle,
    width: f32,
    height: f32,
    scale: f32,
) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    handle::resize(h, width, height, scale);
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_mouse_event(
    handle: *mut ViewportHandle,
    x: f32,
    y: f32,
    button: u8,
    pressed: bool,
) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    bridge::push_mouse_event(h, x, y, button, pressed);
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_key_event(
    handle: *mut ViewportHandle,
    key: u32,
    modifiers: u32,
    pressed: bool,
    text: *const c_char,
) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    let text_str = if text.is_null() {
        None
    } else {
        Some(unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy())
    };
    bridge::push_key_event(h, key, modifiers, pressed, text_str.as_deref());
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_scroll_event(
    handle: *mut ViewportHandle,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    bridge::push_scroll_event(h, x, y, delta_x, delta_y);
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_set_text(handle: *mut ViewportHandle, text: *const c_char) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    let s = if text.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(text) }.to_str().unwrap_or("")
    };
    // Goes through `load_doc` so any embedded sidecar archive comment is
    // pulled out before the markdown body reaches the parser.
    h.state.load_doc(s);
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_set_lang(handle: *mut ViewportHandle, ext: *const c_char) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    if ext.is_null() {
        h.state.lang = None;
    } else {
        let s = unsafe { CStr::from_ptr(ext) }.to_str().unwrap_or("");
        h.state.set_lang_from_ext(s);
    }
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_get_text(handle: *mut ViewportHandle) -> *mut c_char {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let text = h.state.save_doc();
    CString::new(text).unwrap_or_default().into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_free_string(s: *mut c_char) {
    if s.is_null() { return; }
    unsafe { drop(CString::from_raw(s)); }
}

/// returns the archive zip bytes (or null when empty); writes the length to len_out.
#[unsafe(no_mangle)]
pub extern "C" fn viewport_take_sidecar_bytes(
    handle: *mut ViewportHandle,
    len_out: *mut usize,
) -> *mut u8 {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => {
            if !len_out.is_null() { unsafe { *len_out = 0; } }
            return std::ptr::null_mut();
        }
    };
    let bytes = match h.state.save_sidecar_bytes() {
        Some(b) => b,
        None => {
            if !len_out.is_null() { unsafe { *len_out = 0; } }
            return std::ptr::null_mut();
        }
    };
    let mut boxed = bytes.into_boxed_slice();
    if !len_out.is_null() { unsafe { *len_out = boxed.len(); } }
    let ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    ptr
}

/// applies archive zip bytes back into the document.
#[unsafe(no_mangle)]
pub extern "C" fn viewport_apply_sidecar_bytes(
    handle: *mut ViewportHandle,
    bytes: *const u8,
    len: usize,
) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    if bytes.is_null() || len == 0 { return; }
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    h.state.apply_sidecar_bytes(slice);
    h.needs_redraw = true;
}

/// frees byte buffers returned by viewport_take_sidecar_bytes.
#[unsafe(no_mangle)]
pub extern "C" fn viewport_free_bytes(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 { return; }
    unsafe { drop(Box::from_raw(std::slice::from_raw_parts_mut(ptr, len))); }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_set_theme(handle: *mut ViewportHandle, name: *const c_char) {
    let s = if name.is_null() {
        "mocha"
    } else {
        unsafe { CStr::from_ptr(name) }.to_str().unwrap_or("mocha")
    };
    palette::set_theme(s);
    if let Some(h) = unsafe { handle.as_mut() } {
        h.needs_redraw = true;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_set_line_indicator(handle: *mut ViewportHandle, mode: *const c_char) {
    let s = if mode.is_null() {
        "on"
    } else {
        unsafe { CStr::from_ptr(mode) }.to_str().unwrap_or("on")
    };
    if let Some(h) = unsafe { handle.as_mut() } {
        h.state.line_indicator = editor::LineIndicator::from_str(s);
        h.needs_redraw = true;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_set_gutter_rainbow(handle: *mut ViewportHandle, enabled: bool) {
    if let Some(h) = unsafe { handle.as_mut() } {
        h.state.gutter_rainbow = enabled;
        h.needs_redraw = true;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_set_auto_pair_flags(handle: *mut ViewportHandle, flags: u32) {
    editor::auto_pair::set_flags(flags as u8);
    if let Some(h) = unsafe { handle.as_mut() } {
        h.needs_redraw = true;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_get_auto_pair_flags() -> u32 {
    editor::auto_pair::flags() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_send_command(handle: *mut ViewportHandle, command: u32) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    match command {
        1 => h.state.update(editor::Message::ToggleBold),
        2 => h.state.update(editor::Message::ToggleItalic),
        3 => h.state.update(editor::Message::InsertTable),
        4 => h.state.update(editor::Message::SmartEval),
        5 => h.state.update(editor::Message::Evaluate),
        6 => h.state.update(editor::Message::TogglePreview),
        7 => h.state.update(editor::Message::ZoomIn),
        8 => h.state.update(editor::Message::ZoomOut),
        9 => h.state.update(editor::Message::ZoomReset),
        // 11 = live, 12 = editor, 13 = view
        11 => h.state.update(editor::Message::SetRenderMode(editor::RenderMode::Live)),
        12 => h.state.update(editor::Message::SetRenderMode(editor::RenderMode::Editor)),
        13 => h.state.update(editor::Message::SetRenderMode(editor::RenderMode::View)),
        16 => h.state.settings_open = !h.state.settings_open,
        _ => return,
    };
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_set_settings_view(
    handle: *mut ViewportHandle,
    theme_mode: *const c_char,
    line_indicator: *const c_char,
    gutter_rainbow: bool,
    auto_save_dir: *const c_char,
) {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    let read = |p: *const c_char| -> String {
        if p.is_null() { return String::new(); }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    };
    h.state.settings_view = editor::SettingsView {
        theme_mode: read(theme_mode),
        line_indicator: read(line_indicator),
        gutter_rainbow,
        auto_save_dir: read(auto_save_dir),
    };
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_take_shell_action(handle: *mut ViewportHandle) -> *mut c_char {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let Some(action) = h.state.take_pending_shell_action() else {
        return std::ptr::null_mut();
    };
    let s = match action {
        editor::ShellAction::NewNote => "new_note".to_string(),
        editor::ShellAction::Open => "open".to_string(),
        editor::ShellAction::Save => "save".to_string(),
        editor::ShellAction::SaveAs => "save_as".to_string(),
        editor::ShellAction::Quit => "quit".to_string(),
        editor::ShellAction::Settings => "settings".to_string(),
        editor::ShellAction::ExportCrate => "export_crate".to_string(),
        editor::ShellAction::ToggleBrowser => "toggle_browser".to_string(),
        editor::ShellAction::SetThemeMode(v) => format!("set_theme_mode:{}", v),
        editor::ShellAction::SetLineIndicator(v) => format!("set_line_indicator:{}", v),
        editor::ShellAction::SetGutterRainbow(b) => format!("set_gutter_rainbow:{}", b),
        editor::ShellAction::PickAutoSaveDir => "pick_auto_save_dir".to_string(),
    };
    CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Export the note as a standalone Rust crate at `out_dir/name/`. Returns
/// a heap-allocated C string on success (the absolute path of the created
/// folder), or null on failure. Free the returned string with
/// `viewport_free_string`.
#[unsafe(no_mangle)]
pub extern "C" fn viewport_export_crate(
    handle: *mut ViewportHandle,
    out_dir: *const c_char,
    name: *const c_char,
) -> *mut c_char {
    let h = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    if out_dir.is_null() || name.is_null() {
        return std::ptr::null_mut();
    }
    let out_dir_str = match unsafe { CStr::from_ptr(out_dir) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match export::export_crate(&h.state, std::path::Path::new(out_dir_str), name_str) {
        Ok(path) => {
            let s = path.to_string_lossy().into_owned();
            CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

use browser::BrowserHandle;

#[unsafe(no_mangle)]
pub extern "C" fn browser_create(
    nsview: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
    notes_dir: *const c_char,
) -> *mut BrowserHandle {
    if nsview.is_null() || notes_dir.is_null() { return std::ptr::null_mut(); }
    let dir = match unsafe { CStr::from_ptr(notes_dir) }.to_str() {
        Ok(s) => std::path::PathBuf::from(s),
        Err(_) => return std::ptr::null_mut(),
    };
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        match browser::handle::create_from_native(nsview, width, height, scale, dir) {
            Some(h) => Box::into_raw(Box::new(h)),
            None => std::ptr::null_mut(),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (width, height, scale, dir);
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_destroy(handle: *mut BrowserHandle) {
    if handle.is_null() { return; }
    unsafe { drop(Box::from_raw(handle)); }
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_render(handle: *mut BrowserHandle) {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return };
    browser::handle::render(h);
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_resize(handle: *mut BrowserHandle, width: f32, height: f32, scale: f32) {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return };
    browser::handle::resize(h, width, height, scale);
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_mouse_event(handle: *mut BrowserHandle, x: f32, y: f32, button: u8, pressed: bool) {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return };
    browser::handle::push_mouse_move(h, x, y);
    if button != 255 {
        browser::handle::push_mouse_button(h, button, pressed);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_scroll_event(handle: *mut BrowserHandle, delta_x: f32, delta_y: f32) {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return };
    browser::handle::push_scroll(h, delta_x, delta_y);
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_key_event(
    handle: *mut BrowserHandle,
    key: u32,
    modifiers: u32,
    pressed: bool,
    text: *const c_char,
) {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return };
    let text_str = if text.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(text) }.to_string_lossy())
    };
    browser::handle::push_key_native(h, key, modifiers, pressed, text_str.as_deref());
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_take_pending_open(handle: *mut BrowserHandle) -> *mut c_char {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return std::ptr::null_mut() };
    let Some(path) = browser::handle::take_pending_open(h) else { return std::ptr::null_mut() };
    let s = path.to_string_lossy().into_owned();
    CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn browser_refresh(handle: *mut BrowserHandle) {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return };
    browser::handle::refresh(h);
}

/// dispatches a numeric zoom command into the browser's scale state.
#[unsafe(no_mangle)]
pub extern "C" fn browser_send_command(handle: *mut BrowserHandle, command: u32) {
    let h = match unsafe { handle.as_mut() } { Some(h) => h, None => return };
    let msg = match command {
        7 => browser::BrowserMessage::ScaleUp,
        8 => browser::BrowserMessage::ScaleDown,
        9 => browser::BrowserMessage::ScaleReset,
        _ => return,
    };
    h.state.update(msg);
    h.needs_redraw = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_render_mode(handle: *mut ViewportHandle) -> u32 {
    let h = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return 0,
    };
    match h.state.render_mode {
        editor::RenderMode::Live => 0,
        editor::RenderMode::Editor => 1,
        editor::RenderMode::View => 2,
    }
}

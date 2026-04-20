use std::ffi::{c_char, c_void, CStr, CString};

pub mod block;
pub mod blocks;
mod bridge;
mod editor;
pub mod export;
mod handle;
pub mod heading_block;
pub mod hr_block;
pub mod module;
pub mod oklab;
pub mod palette;
pub mod selection;
pub mod sidecar;
mod syntax;
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
    // Goes through `save_doc` so any tables with persistent metadata get
    // their data round-tripped via the embedded sidecar archive comment.
    let text = h.state.save_doc();
    CString::new(text).unwrap_or_default().into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn viewport_free_string(s: *mut c_char) {
    if s.is_null() { return; }
    unsafe { drop(CString::from_raw(s)); }
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
        11 => h.state.exit_editor_mode(),
        12 => h.state.enter_editor_mode(),
        13 => h.state.enter_view_mode(),
        _ => return,
    };
    h.needs_redraw = true;
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

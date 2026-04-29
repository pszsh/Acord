use std::path::PathBuf;

use iced_graphics::{Shell, Viewport};
use iced_runtime::user_interface::{self, UserInterface};
use iced_wgpu::core::renderer::Style;
use iced_wgpu::core::{
    clipboard, mouse, window, Color, Event, Font, Pixels, Point, Size, Theme,
};
use iced_wgpu::Engine;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::palette;
use super::state::{BrowserMessage, BrowserState};
use super::ui;

/// Owns the browser window's wgpu surface, iced renderer, and BrowserState.
pub struct BrowserHandle {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub scale: f32,

    pub renderer: iced_wgpu::Renderer,
    pub viewport: Viewport,
    pub cache: user_interface::Cache,
    pub state: BrowserState,
    pub events: Vec<Event>,
    pub cursor: mouse::Cursor,
    pub needs_redraw: bool,
}

/// The browser doesn't read or write the system clipboard.
struct NoopClipboard;

impl clipboard::Clipboard for NoopClipboard {
    fn read(&self, _kind: clipboard::Kind) -> Option<String> { None }
    fn write(&mut self, _kind: clipboard::Kind, _contents: String) {}
}

/// Caller must keep the underlying winit Window alive for the surface's lifetime.
pub fn create(
    raw_display: RawDisplayHandle,
    raw_window: RawWindowHandle,
    width: f32,
    height: f32,
    scale: f32,
    notes_dir: PathBuf,
) -> Option<BrowserHandle> {
    #[cfg(target_os = "macos")]
    let backends = wgpu::Backends::METAL;
    #[cfg(target_os = "windows")]
    let backends = wgpu::Backends::DX12;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let backends = wgpu::Backends::VULKAN;

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });

    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: raw_display,
        raw_window_handle: raw_window,
    };

    let surface = unsafe { instance.create_surface_unsafe(target).ok()? };

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .ok()?;

    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;

    let phys_w = (width * scale) as u32;
    let phys_h = (height * scale) as u32;

    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats.first().copied()?;

    surface.configure(
        &device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: phys_w.max(1),
            height: phys_h.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        },
    );

    let engine = Engine::new(&adapter, device.clone(), queue.clone(), format, None, Shell::headless());
    let renderer = iced_wgpu::Renderer::new(engine, Font::DEFAULT, Pixels(13.0));
    let viewport = Viewport::with_physical_size(Size::new(phys_w.max(1), phys_h.max(1)), scale);

    Some(BrowserHandle {
        surface,
        device,
        queue,
        format,
        width: phys_w,
        height: phys_h,
        scale,
        renderer,
        viewport,
        cache: user_interface::Cache::new(),
        state: BrowserState::new(notes_dir),
        events: Vec::new(),
        cursor: mouse::Cursor::Available(Point::new(0.0, 0.0)),
        needs_redraw: true,
    })
}

/// One frame: drains pending events into messages, applies them, then redraws.
pub fn render(handle: &mut BrowserHandle) {
    let pending = !handle.events.is_empty();
    if !handle.needs_redraw && !pending {
        return;
    }

    let frame = match handle.surface.get_current_texture() {
        Ok(f) => f,
        Err(_) => return,
    };
    let view = frame.texture.create_view(&Default::default());
    let logical_size = handle.viewport.logical_size();

    handle
        .events
        .push(Event::Window(window::Event::RedrawRequested(iced_wgpu::core::time::Instant::now())));

    // First UI build receives input events and emits messages.
    let cache = std::mem::take(&mut handle.cache);
    let mut ui = UserInterface::build(
        ui::view(&handle.state),
        Size::new(logical_size.width, logical_size.height),
        cache,
        &mut handle.renderer,
    );

    let mut clipboard = NoopClipboard;
    let mut messages: Vec<BrowserMessage> = Vec::new();

    let _ = ui.update(
        &handle.events,
        handle.cursor,
        &mut handle.renderer,
        &mut clipboard,
        &mut messages,
    );
    handle.events.clear();

    let cache = ui.into_cache();

    for msg in messages.drain(..) {
        handle.state.update(msg);
    }

    // Second UI build draws against post-message state.
    let mut ui = UserInterface::build(
        ui::view(&handle.state),
        Size::new(logical_size.width, logical_size.height),
        cache,
        &mut handle.renderer,
    );

    let theme = Theme::Dark;
    let style = Style { text_color: Color::WHITE };

    ui.draw(&mut handle.renderer, &theme, &style, handle.cursor);
    handle.cache = ui.into_cache();

    handle
        .renderer
        .present(Some(palette::current().base), handle.format, &view, &handle.viewport);

    frame.present();
    handle.needs_redraw = false;
}

pub fn resize(handle: &mut BrowserHandle, width: f32, height: f32, scale: f32) {
    let phys_w = (width * scale) as u32;
    let phys_h = (height * scale) as u32;
    if phys_w == 0 || phys_h == 0 { return; }

    handle.width = phys_w;
    handle.height = phys_h;
    handle.scale = scale;
    handle.viewport = Viewport::with_physical_size(Size::new(phys_w, phys_h), scale);

    handle.surface.configure(
        &handle.device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: handle.format,
            width: phys_w,
            height: phys_h,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        },
    );
    handle.needs_redraw = true;
}

pub fn push_mouse_move(handle: &mut BrowserHandle, x: f32, y: f32) {
    let position = Point::new(x, y);
    handle.cursor = mouse::Cursor::Available(position);
    handle.events.push(Event::Mouse(mouse::Event::CursorMoved { position }));
    handle.needs_redraw = true;
}

pub fn push_mouse_button(handle: &mut BrowserHandle, button: u8, pressed: bool) {
    let btn = match button {
        0 => mouse::Button::Left,
        1 => mouse::Button::Right,
        2 => mouse::Button::Middle,
        n => mouse::Button::Other(n as u16),
    };
    let ev = if pressed {
        mouse::Event::ButtonPressed(btn)
    } else {
        mouse::Event::ButtonReleased(btn)
    };
    handle.events.push(Event::Mouse(ev));
    handle.needs_redraw = true;
}

pub fn push_scroll(handle: &mut BrowserHandle, delta_x: f32, delta_y: f32) {
    handle.events.push(Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: delta_x, y: delta_y },
    }));
    handle.needs_redraw = true;
}

pub fn push_event(handle: &mut BrowserHandle, event: Event) {
    handle.events.push(event);
    handle.needs_redraw = true;
}

pub fn take_pending_open(handle: &mut BrowserHandle) -> Option<PathBuf> {
    handle.state.take_pending_open()
}

pub fn refresh(handle: &mut BrowserHandle) {
    handle.state.refresh();
    handle.needs_redraw = true;
}

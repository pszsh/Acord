use std::ffi::c_void;
use std::ptr::NonNull;

use iced_graphics::{Shell, Viewport};
use iced_runtime::user_interface::{self, UserInterface};
use iced_wgpu::core::renderer::Style;
use iced_wgpu::core::time::Instant;
use iced_wgpu::core::{clipboard, keyboard, mouse, window, Color, Event, Font, Pixels, Point, Size, Theme};
use iced_wgpu::Engine;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
#[cfg(target_os = "macos")]
use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle};
#[cfg(target_os = "windows")]
use raw_window_handle::{Win32WindowHandle, WindowsDisplayHandle};

use crate::editor::{EditorState, Message, RenderMode};
use crate::palette;
use crate::table_block::TableMessage;
use crate::ViewportHandle;

struct AcordClipboard {
    board: std::cell::RefCell<arboard::Clipboard>,
}

impl clipboard::Clipboard for AcordClipboard {
    fn read(&self, _kind: clipboard::Kind) -> Option<String> {
        // arboard uses NSPasteboard on macOS, Win32 on Windows — no subprocess.
        // Image-first: if the pasteboard holds a bitmap, encode it to PNG in
        // the on-disk image cache and yield a markdown reference. Wrapping
        // newlines guarantee the `![](…)` lands as the only thing on its line
        // so `parse_image_ref` will pick it up.
        let mut board = self.board.borrow_mut();
        if let Ok(img) = board.get_image() {
            if let Some(path) = crate::editor::write_clipboard_image_to_cache(&img) {
                return Some(format!("\n![]({})\n", path));
            }
        }
        // Line-ending normalisation: web pages and cross-platform apps keep
        // `\r\n` in the pasteboard; collapse to `\n` so iced's buffer and
        // our gutter line counter agree.
        board.get_text()
            .ok()
            .map(|s| s.replace("\r\n", "\n").replace('\r', "\n"))
    }

    fn write(&mut self, _kind: clipboard::Kind, contents: String) {
        let _ = self.board.borrow_mut().set_text(contents);
    }
}

pub fn create(
    native_handle: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
) -> Option<ViewportHandle> {
    let ptr = NonNull::new(native_handle)?;

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

    #[cfg(target_os = "macos")]
    let (raw_window, raw_display) = (
        RawWindowHandle::AppKit(AppKitWindowHandle::new(ptr)),
        RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
    );
    #[cfg(target_os = "windows")]
    let (raw_window, raw_display) = {
        let wh = Win32WindowHandle::new(std::num::NonZero::new(ptr.as_ptr() as isize).unwrap());
        (
            RawWindowHandle::Win32(wh),
            RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
        )
    };

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

    let engine = Engine::new(
        &adapter,
        device.clone(),
        queue.clone(),
        format,
        None,
        Shell::headless(),
    );

    let renderer = iced_wgpu::Renderer::new(engine, Font::DEFAULT, Pixels(16.0));

    let viewport =
        Viewport::with_physical_size(Size::new(phys_w.max(1), phys_h.max(1)), scale);

    let focus_point = Point::new(width / 2.0, height / 2.0);
    let initial_events = vec![
        Event::Mouse(mouse::Event::CursorMoved { position: focus_point }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ];

    Some(ViewportHandle {
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
        state: EditorState::new(),
        events: initial_events,
        cursor: mouse::Cursor::Available(focus_point),
        // First frame must paint.
        needs_redraw: true,
    })
}

pub fn render(handle: &mut ViewportHandle) {
    // Idle-frame short circuit. The Swift CVDisplayLink ticks viewport_render() at
    // vsync regardless of activity. Without this gate we'd rebuild the entire widget
    // tree, run update + draw, and present a frame ~60 times per second forever.
    // We still wake up while `eval_dirty` is set so the eval debounce in
    // EditorState::tick() can fire after typing stops.
    let pending_events = !handle.events.is_empty();
    if !handle.needs_redraw && !handle.state.has_pending_eval() && !pending_events {
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
        .push(Event::Window(window::Event::RedrawRequested(Instant::now())));

    let cache = std::mem::take(&mut handle.cache);
    let mut ui = UserInterface::build(
        handle.state.view(),
        Size::new(logical_size.width, logical_size.height),
        cache,
        &mut handle.renderer,
    );

    let mut clipboard = AcordClipboard {
        board: std::cell::RefCell::new(arboard::Clipboard::new().unwrap()),
    };
    let mut messages: Vec<Message> = Vec::new();
    let mut consumed: Vec<usize> = Vec::new();
    // Captured during the event scan, applied to `handle.state.mods` AFTER
    // `ui` is released — the UI build above borrows `&handle.state` so we
    // can't mutate any field of state while it's alive.
    let mut latest_mods: Option<keyboard::Modifiers> = None;
    // Cmd+A escalation: armed by the first press, escalates on the second.
    // Some(true) = arm for next press, Some(false) = disarm. None = unchanged.
    // Events are scanned in order so the LAST write wins — a Cmd+A followed
    // by a mouse click in the same frame correctly disarms.
    let mut new_cmd_a_armed: Option<bool> = None;

    for (ev_idx, event) in handle.events.iter().enumerate() {
        // Default-disarm Cmd+A for any user input. The Cmd+A arm below
        // overwrites with Some(true) when it actually wants to arm. Events
        // are scanned in order so the LAST write wins for the frame.
        let is_user_input = matches!(
            event,
            Event::Keyboard(keyboard::Event::KeyPressed { .. })
                | Event::Mouse(mouse::Event::ButtonPressed(_))
        );
        if is_user_input {
            new_cmd_a_armed = Some(false);
        }

        // View mode: consume all events except mode-switch keys.
        // Ctrl+I, Ctrl+/, Ctrl+Esc, `i`, `/` are handled by their own
        // match arms below. Everything else is swallowed.
        if handle.state.render_mode == RenderMode::View {
            let is_mode_switch = match event {
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Character(c), modifiers, ..
                }) => {
                    (modifiers.control() && (c.as_str() == "i" || c.as_str() == "/"))
                    || (!modifiers.logo() && !modifiers.control() && !modifiers.alt()
                        && (c.as_str() == "i" || c.as_str() == "/"))
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Escape), modifiers, ..
                }) => modifiers.control(),
                _ => false,
            };
            if !is_mode_switch {
                consumed.push(ev_idx);
                continue;
            }
        }

        match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(c),
                modifiers,
                ..
            }) if modifiers.logo() => {
                match c.as_str() {
                    "p" => {
                        messages.push(Message::TogglePreview);
                        consumed.push(ev_idx);
                    }
                    "t" => {
                        messages.push(Message::InsertTable);
                        consumed.push(ev_idx);
                    }
                    // Cmd+A — first press lets the focused block do its
                    // local select-all (text_editor selects its text;
                    // table cells in select mode upgrade to whole-table
                    // selection). Second press while still armed escalates
                    // to whole-document selection.
                    "a" => {
                        if handle.state.cmd_a_armed {
                            messages.push(Message::SelectAllBlocks);
                            new_cmd_a_armed = Some(false);
                            consumed.push(ev_idx);
                        } else {
                            // First press path. Decide what "local select all"
                            // means for the focused block.
                            if handle.state.table_is_focused_block()
                                && !handle.state.focused_table_is_select_all()
                                && handle.state.editing.is_none()
                            {
                                // Cell-selected table → escalate to whole-table.
                                messages.push(Message::FocusedTableOp(
                                    TableMessage::SelectAll,
                                ));
                                consumed.push(ev_idx);
                            }
                            // For text blocks (and table cells in edit mode),
                            // do NOT consume — let iced's text_editor /
                            // text_input handle Cmd+A natively.
                            new_cmd_a_armed = Some(true);
                        }
                    }
                    "b" => {
                        messages.push(Message::ToggleBold);
                        consumed.push(ev_idx);
                    }
                    "i" => {
                        messages.push(Message::ToggleItalic);
                        consumed.push(ev_idx);
                    }
                    "e" => {
                        messages.push(Message::SmartEval);
                        consumed.push(ev_idx);
                    }
                    "z" if modifiers.shift() => {
                        messages.push(Message::Redo);
                        consumed.push(ev_idx);
                    }
                    "z" => {
                        messages.push(Message::Undo);
                        consumed.push(ev_idx);
                    }
                    "f" => {
                        messages.push(Message::ToggleFind);
                        consumed.push(ev_idx);
                    }
                    "g" if modifiers.shift() => {
                        messages.push(Message::FindPrev);
                        consumed.push(ev_idx);
                    }
                    "g" => {
                        messages.push(Message::FindNext);
                        consumed.push(ev_idx);
                    }
                    _ => {}
                }
            }
            // Ctrl+I → Editor mode, Ctrl+/ → Live mode
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(c),
                modifiers,
                ..
            }) if modifiers.control() && !modifiers.logo() => {
                match c.as_str() {
                    "i" => {
                        messages.push(Message::SetRenderMode(RenderMode::Editor));
                        consumed.push(ev_idx);
                    }
                    "/" => {
                        messages.push(Message::SetRenderMode(RenderMode::Live));
                        consumed.push(ev_idx);
                    }
                    _ => {}
                }
            }
            // Ctrl+Escape → View mode
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                modifiers,
                ..
            }) if modifiers.control() => {
                messages.push(Message::SetRenderMode(RenderMode::View));
                consumed.push(ev_idx);
            }
            // View mode: `i` → Editor, `/` → Live
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(c),
                modifiers,
                ..
            }) if handle.state.render_mode == RenderMode::View
                && !modifiers.logo() && !modifiers.control() && !modifiers.alt() => {
                match c.as_str() {
                    "i" => {
                        messages.push(Message::SetRenderMode(RenderMode::Editor));
                        consumed.push(ev_idx);
                    }
                    "/" => {
                        messages.push(Message::SetRenderMode(RenderMode::Live));
                        consumed.push(ev_idx);
                    }
                    _ => {}
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(named),
                modifiers,
                ..
            }) if modifiers.logo() && modifiers.alt() => {
                use keyboard::key::Named;
                let op = match named {
                    Named::ArrowUp => Some(TableMessage::InsertRowAbove),
                    Named::ArrowDown => Some(TableMessage::InsertRowBelow),
                    Named::ArrowLeft => Some(TableMessage::InsertColLeft),
                    Named::ArrowRight => Some(TableMessage::InsertColRight),
                    Named::Backspace if modifiers.shift() => Some(TableMessage::DeleteCol),
                    Named::Backspace => Some(TableMessage::DeleteRow),
                    _ => None,
                };
                if let Some(tmsg) = op {
                    messages.push(Message::FocusedTableOp(tmsg));
                    consumed.push(ev_idx);
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(named),
                modifiers,
                ..
            }) if !modifiers.logo() && !modifiers.alt() && !modifiers.control()
                && handle.state.table_is_focused_block() =>
            {
                use keyboard::key::Named;
                match named {
                    Named::Tab if modifiers.shift() => {
                        messages.push(Message::TableShiftTab);
                        consumed.push(ev_idx);
                    }
                    Named::Tab => {
                        messages.push(Message::TableTab);
                        consumed.push(ev_idx);
                    }
                    Named::Enter => {
                        messages.push(Message::TableEnter);
                        consumed.push(ev_idx);
                    }
                    // Up arrow inside a table cell. If we're on a non-top row,
                    // move the cell focus up. If we're on row 0, escape upward
                    // into the previous text block (synthesize one if none).
                    Named::ArrowUp => {
                        if let Some((block_idx, row, _)) =
                            handle.state.active_table_focused_row()
                        {
                            if row == 0 {
                                messages.push(Message::EscapeTableUp(block_idx));
                            } else {
                                messages.push(Message::TableMoveUp);
                            }
                            consumed.push(ev_idx);
                        }
                    }
                    // Mirror of Up — row navigation with edge-escape.
                    Named::ArrowDown => {
                        if let Some((block_idx, row, total)) =
                            handle.state.active_table_focused_row()
                        {
                            if row + 1 >= total {
                                messages.push(Message::EscapeTableDown(block_idx));
                            } else {
                                messages.push(Message::TableMoveDown);
                            }
                            consumed.push(ev_idx);
                        }
                    }
                    // Left/Right walk the row. No edge-escape — at column 0 or
                    // the last column the move just no-ops; cell stays put.
                    Named::ArrowLeft => {
                        messages.push(Message::TableMoveLeft);
                        consumed.push(ev_idx);
                    }
                    Named::ArrowRight => {
                        messages.push(Message::TableMoveRight);
                        consumed.push(ev_idx);
                    }
                    // Backspace/Delete behavior depends on selection scope:
                    //   - whole table selected → clear every cell's content
                    //   - single cell selected (not editing) → clear that cell
                    // Edit mode is handled by text_input's own backspace.
                    Named::Backspace | Named::Delete
                        if handle.state.focused_table_is_select_all() =>
                    {
                        messages.push(Message::FocusedTableOp(TableMessage::ClearAll));
                        consumed.push(ev_idx);
                    }
                    Named::Backspace | Named::Delete
                        if handle.state.has_selected_cell_not_editing() =>
                    {
                        messages.push(Message::ClearSelectedCell);
                        consumed.push(ev_idx);
                    }
                    _ => {}
                }
            }
            // Cmd+Backspace with the whole document selected → wipe all
            // blocks down to one empty text block.
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Backspace),
                modifiers,
                ..
            }) if modifiers.logo() && !modifiers.alt() && !modifiers.control()
                && handle.state.all_blocks_selected =>
            {
                messages.push(Message::DeleteAllBlocks);
                consumed.push(ev_idx);
            }
            // Cmd+Backspace with the whole table selected → delete the table.
            // Mirrors the user's "Cmd+Delete deletes whatever's selected" rule
            // applied at table scope. Single-cell selection has its own
            // Cmd+Alt+Backspace = delete row binding below.
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Backspace),
                modifiers,
                ..
            }) if modifiers.logo() && !modifiers.alt() && !modifiers.control()
                && handle.state.focused_table_is_select_all() =>
            {
                messages.push(Message::DeleteCurrentTable);
                consumed.push(ev_idx);
            }
            // Plain Backspace/Delete with whole document selected → clear all
            // block content but keep structure.
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(named),
                modifiers,
                ..
            }) if (matches!(named, keyboard::key::Named::Backspace | keyboard::key::Named::Delete))
                && !modifiers.logo() && !modifiers.alt() && !modifiers.control()
                && handle.state.all_blocks_selected =>
            {
                messages.push(Message::ClearAllBlocks);
                consumed.push(ev_idx);
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                modifiers,
                ..
            }) if !modifiers.control() => {
                if handle.state.context_menu.is_some() {
                    messages.push(Message::HideContextMenu);
                    consumed.push(ev_idx);
                } else if handle.state.find.visible {
                    messages.push(Message::HideFind);
                    consumed.push(ev_idx);
                } else if handle.state.editing.is_some() {
                    messages.push(Message::ExitCellEdit);
                    consumed.push(ev_idx);
                } else {
                    // Nothing to dismiss — chain mode switch.
                    // Live → Editor, Editor → View
                    match handle.state.render_mode {
                        RenderMode::Live => {
                            messages.push(Message::SetRenderMode(RenderMode::Editor));
                            consumed.push(ev_idx);
                        }
                        RenderMode::Editor => {
                            messages.push(Message::SetRenderMode(RenderMode::View));
                            consumed.push(ev_idx);
                        }
                        RenderMode::View => {}
                    }
                }
            }
            // Printable-key entry into a selected cell. When a table cell is
            // selected (highlighted) but not yet in edit mode, hitting any
            // printable character should overwrite the cell with that
            // character and enter edit mode in one step.
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(c),
                modifiers,
                ..
            }) if !modifiers.logo() && !modifiers.alt() && !modifiers.control()
                && handle.state.has_selected_cell_not_editing() =>
            {
                if let Some(first) = c.chars().next() {
                    if !first.is_control() {
                        messages.push(Message::EnterCellEditWithChar(first));
                        consumed.push(ev_idx);
                    }
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(mods)) => {
                latest_mods = Some(*mods);
            }
            _ => {}
        }
    }

    // Strip keyboard events we've already routed into Messages, so iced's
    // text_input/text_editor doesn't also process them and corrupt cell content.
    if !consumed.is_empty() {
        let consumed_set: std::collections::HashSet<usize> = consumed.into_iter().collect();
        let mut filtered: Vec<Event> = Vec::with_capacity(handle.events.len());
        for (i, e) in handle.events.drain(..).enumerate() {
            if !consumed_set.contains(&i) {
                filtered.push(e);
            }
        }
        handle.events = filtered;
    }

    let _ = ui.update(
        &handle.events,
        handle.cursor,
        &mut handle.renderer,
        &mut clipboard,
        &mut messages,
    );
    handle.events.clear();

    // Snapshot which cell (if any) is currently focused in any table, so that
    // subsequent structural edit shortcuts (insert row, delete col, ...) can
    // target the right block without a separate focus-tracking field.
    let focused_id = {
        use iced_wgpu::core::widget::operation::{Focusable, Operation};
        use iced_wgpu::core::widget::Id as CoreId;
        use iced_wgpu::core::Rectangle;

        struct FindFocusedId {
            focused: Option<CoreId>,
        }

        impl Operation<()> for FindFocusedId {
            fn focusable(
                &mut self,
                id: Option<&CoreId>,
                _bounds: Rectangle,
                state: &mut dyn Focusable,
            ) {
                if state.is_focused() && id.is_some() && self.focused.is_none() {
                    self.focused = id.cloned();
                }
            }

            fn traverse(
                &mut self,
                operate: &mut dyn FnMut(&mut dyn Operation<()>),
            ) {
                operate(self);
            }

            fn container(&mut self, _id: Option<&CoreId>, _bounds: Rectangle) {}
        }

        let mut op = FindFocusedId { focused: None };
        ui.operate(&handle.renderer, &mut op);
        op.focused
    };

    let cache = ui.into_cache();

    if let Some(mods) = latest_mods {
        handle.state.mods = mods;
    }
    if let Some(armed) = new_cmd_a_armed {
        handle.state.cmd_a_armed = armed;
    }
    // Update cursor pos BEFORE draining messages so right-click handlers can
    // anchor the context menu at the current position in the same frame.
    if let Some(pt) = handle.cursor.position() {
        handle.state.cursor_pos = pt;
    }
    handle.state.sync_focused_cell(focused_id.as_ref());

    for msg in messages.drain(..) {
        handle.state.update(msg);
    }

    // Drain any clipboard write the editor queued during update/tick.
    if let Some(text) = handle.state.pending_clipboard.take() {
        if let Ok(mut board) = arboard::Clipboard::new() {
            let _ = board.set_text(text);
        }
    }

    handle.state.tick();
    let pending_focus = handle.state.take_pending_focus();
    // Drain BEFORE the second `ui` is built — `view()` re-borrows state and
    // would block any subsequent mutable take.
    let pending_scroll = handle.state.take_pending_scroll();

    let theme = Theme::Dark;
    let style = Style {
        text_color: Color::WHITE,
    };

    let mut ui = UserInterface::build(
        handle.state.view(),
        Size::new(logical_size.width, logical_size.height),
        cache,
        &mut handle.renderer,
    );

    if let Some(focus_id) = pending_focus {
        use iced_wgpu::core::widget::operation::focusable;
        let mut op = focusable::focus(focus_id);
        ui.operate(&handle.renderer, &mut op);
    }

    // Forward any wheel-scroll delta that an inner text_editor swallowed
    // (Action::Scroll) to the outer document scrollable. text_editor captures
    // WheelScrolled when the cursor is over its bounds, which would otherwise
    // leave the page stuck. The editor.rs Action::Scroll handler accumulates
    // pixel deltas into pending_scroll; here we drain and apply them.
    if let Some(delta_y) = pending_scroll {
        use iced_wgpu::core::widget::operation::scrollable::{scroll_by, AbsoluteOffset};
        use iced_wgpu::core::widget::Id as CoreId;
        let mut op = scroll_by::<()>(
            CoreId::new(crate::editor::DOC_SCROLLABLE_ID),
            AbsoluteOffset { x: 0.0, y: delta_y },
        );
        ui.operate(&handle.renderer, &mut op);
    }

    ui.draw(&mut handle.renderer, &theme, &style, handle.cursor);
    handle.cache = ui.into_cache();

    handle
        .renderer
        .present(Some(palette::current().base), handle.format, &view, &handle.viewport);

    frame.present();

    // Frame is on screen. Clear dirty so the next vsync tick is a free no-op
    // unless something genuinely changed (input event, eval debounce, etc.).
    handle.needs_redraw = false;
}

pub fn resize(handle: &mut ViewportHandle, width: f32, height: f32, scale: f32) {
    let phys_w = (width * scale) as u32;
    let phys_h = (height * scale) as u32;
    if phys_w == 0 || phys_h == 0 {
        return;
    }

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
}

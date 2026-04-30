use iced_wgpu::core::keyboard::{self, key};
use iced_wgpu::core::mouse;
use iced_wgpu::core::{Event, Point};
use smol_str::SmolStr;

use crate::ViewportHandle;

pub fn push_mouse_event(handle: &mut ViewportHandle, x: f32, y: f32, button: u8, pressed: bool) {
    let position = Point::new(x, y);
    handle.cursor = mouse::Cursor::Available(position);

    handle.events.push(Event::Mouse(mouse::Event::CursorMoved { position }));

    // Sentinel: button == 255 means "pointer move only — do not fire any
    // ButtonPressed/Released event." Used by mouseMoved and mouseDragged in
    // the Swift shell. Without this, every drag tick would re-fire
    // ButtonPressed(Left) and iced's text_editor would interpret each tick as
    // a new click, restarting the active selection on every frame and making
    // click+drag selection twitch / over-highlight.
    if button == 255 {
        return;
    }

    let btn = match button {
        0 => mouse::Button::Left,
        1 => mouse::Button::Right,
        2 => mouse::Button::Middle,
        n => mouse::Button::Other(n as u16),
    };

    if pressed {
        handle.events.push(Event::Mouse(mouse::Event::ButtonPressed(btn)));
    } else {
        handle.events.push(Event::Mouse(mouse::Event::ButtonReleased(btn)));
    }
}

pub fn push_key_event(
    handle: &mut ViewportHandle,
    keycode: u32,
    modifier_flags: u32,
    pressed: bool,
    text: Option<&str>,
) {
    for ev in build_key_events(keycode, modifier_flags, pressed, text) {
        handle.events.push(ev);
    }
}

pub fn build_key_events(
    keycode: u32,
    modifier_flags: u32,
    pressed: bool,
    text: Option<&str>,
) -> Vec<Event> {
    let modifiers = decode_modifiers(modifier_flags);
    let mut out = Vec::with_capacity(2);

    out.push(Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)));

    let physical = key::Physical::Unidentified(key::NativeCode::MacOS(keycode as u16));

    let named = keycode_to_named(keycode);
    let logical = if let Some(n) = named {
        keyboard::Key::Named(n)
    } else {
        text.filter(|s| !s.is_empty())
            .map(|s| keyboard::Key::Character(SmolStr::new(s)))
            .unwrap_or(keyboard::Key::Unidentified)
    };

    let has_action_modifier = modifiers.logo() || modifiers.control();
    let insert_text = if named.is_some() || has_action_modifier {
        None
    } else {
        text.filter(|s| !s.is_empty()).map(SmolStr::new)
    };

    if pressed {
        out.push(Event::Keyboard(keyboard::Event::KeyPressed {
            key: logical.clone(),
            modified_key: logical,
            physical_key: physical,
            location: keyboard::Location::Standard,
            modifiers,
            text: insert_text,
            repeat: false,
        }));
    } else {
        out.push(Event::Keyboard(keyboard::Event::KeyReleased {
            key: logical.clone(),
            modified_key: logical,
            physical_key: physical,
            location: keyboard::Location::Standard,
            modifiers,
        }));
    }
    out
}

fn keycode_to_named(keycode: u32) -> Option<keyboard::key::Named> {
    use keyboard::key::Named;
    match keycode {
        36 => Some(Named::Enter),
        48 => Some(Named::Tab),
        51 => Some(Named::Backspace),
        53 => Some(Named::Escape),
        117 => Some(Named::Delete),
        123 => Some(Named::ArrowLeft),
        124 => Some(Named::ArrowRight),
        125 => Some(Named::ArrowDown),
        126 => Some(Named::ArrowUp),
        115 => Some(Named::Home),
        119 => Some(Named::End),
        116 => Some(Named::PageUp),
        121 => Some(Named::PageDown),
        122 => Some(Named::F1),
        120 => Some(Named::F2),
        99 => Some(Named::F3),
        118 => Some(Named::F4),
        96 => Some(Named::F5),
        97 => Some(Named::F6),
        98 => Some(Named::F7),
        100 => Some(Named::F8),
        101 => Some(Named::F9),
        109 => Some(Named::F10),
        103 => Some(Named::F11),
        111 => Some(Named::F12),
        _ => None,
    }
}

pub fn push_scroll_event(
    handle: &mut ViewportHandle,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) {
    let position = Point::new(x, y);
    handle.cursor = mouse::Cursor::Available(position);
    handle.events.push(Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels {
            x: delta_x,
            y: delta_y,
        },
    }));
}

fn decode_modifiers(flags: u32) -> keyboard::Modifiers {
    let mut m = keyboard::Modifiers::empty();
    // NSEvent modifier flags
    if flags & (1 << 17) != 0 { m |= keyboard::Modifiers::SHIFT; }
    if flags & (1 << 18) != 0 { m |= keyboard::Modifiers::CTRL; }
    if flags & (1 << 19) != 0 { m |= keyboard::Modifiers::ALT; }
    if flags & (1 << 20) != 0 { m |= keyboard::Modifiers::LOGO; }
    m
}

//! iced style functions matching Acord's look.
//!
//! Each reads from the global Acord palette ([`crate::palette::current`]) at
//! call time, so they automatically follow theme switches.

use iced_wgpu::core::{Background, Border, Color, Shadow, Theme};
use iced_widget::{button, text_input};

use crate::palette;

/// hover/pressed background for a flat menu-row button.
pub fn menu_item(_theme: &Theme, status: button::Status) -> button::Style {
    let p = palette::current();
    let bg = match status {
        button::Status::Hovered => Some(Background::Color(p.surface1)),
        button::Status::Pressed => Some(Background::Color(p.surface2)),
        _ => None,
    };
    button::Style {
        background: bg,
        text_color: p.text,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: false,
    }
}

/// solid surface-1 button with a 1px outline; used by Acord's find bar.
pub fn outlined_button(_theme: &Theme, _status: button::Status) -> button::Style {
    let p = palette::current();
    button::Style {
        background: Some(Background::Color(p.surface1)),
        text_color: p.text,
        border: Border {
            color: p.surface2,
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// outlined text input matching Acord's find bar.
pub fn outlined_input(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    let p = palette::current();
    text_input::Style {
        background: Background::Color(p.surface0),
        border: Border {
            color: p.surface2,
            width: 1.0,
            radius: 3.0.into(),
        },
        icon: p.overlay2,
        placeholder: p.overlay0,
        value: p.text,
        selection: Color { a: 0.4, ..p.blue },
    }
}

//! Modal dialog scaffolding — matches Acord's settings panel.
//!
//! [`overlay`] dims the underlying view and centers a panel; [`segmented_row`]
//! is the labeled multi-button row used for theme/line-indicator toggles.

use iced_wgpu::Renderer;
use iced_wgpu::core::{Background, Border, Color, Element, Length, Padding, Shadow, Theme};
use iced_widget::{button, container, row, text};

use crate::palette;
use crate::syntax;

/// Wraps `panel` content in a centered card over a translucent dim backdrop.
/// `panel` is rendered as-is — the caller controls its content, padding, and
/// width. Set `width` to a fixed value (e.g. `Length::Fixed(font_size * 28.0)`)
/// for a stable layout.
pub fn overlay<'a, Message>(
    panel: Element<'a, Message, Theme, Renderer>,
    width: Length,
    font_size: f32,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'static,
{
    let p = palette::current();
    let f = font_size;
    let radius = f * 0.30;

    let card = container(panel)
        .padding(Padding { top: f, right: f, bottom: f, left: f })
        .width(width)
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.surface0)),
            border: Border {
                color: p.surface1,
                width: 1.0,
                radius: radius.into(),
            },
            text_color: Some(p.text),
            shadow: Shadow::default(),
            snap: false,
        });

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_t: &Theme| container::Style {
            background: Some(Background::Color(Color { r: 0.0, g: 0.0, b: 0.0, a: 0.4 })),
            border: Border::default(),
            text_color: None,
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

/// A "label … [Option1] [Option2] [Option3]" row with the current option
/// highlighted. `options` is `(display_label, value)`; `current` is the
/// active value; `on_select(value)` fires when one is clicked.
pub fn segmented_row<'a, Message>(
    label: &str,
    options: &[(&'a str, &'a str)],
    current: &str,
    font_size: f32,
    on_select: impl Fn(&'a str) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'static,
{
    let p = palette::current();
    let f = font_size;
    let label_size = f * 0.92;
    let radius = f * 0.18;

    let mut buttons: Vec<Element<'a, Message, Theme, Renderer>> = Vec::new();
    for (display, value) in options {
        let active = *value == current;
        let display = display.to_string();
        let v = *value;
        buttons.push(
            button(
                text(display).size(label_size).font(syntax::EDITOR_FONT),
            )
            .padding(Padding { top: f * 0.18, right: f * 0.55, bottom: f * 0.18, left: f * 0.55 })
            .style(move |_t: &Theme, _s| button::Style {
                background: Some(Background::Color(if active { p.surface2 } else { p.surface1 })),
                text_color: if active { p.text } else { p.subtext0 },
                border: Border { color: p.surface2, width: 1.0, radius: radius.into() },
                shadow: Shadow::default(),
                snap: false,
            })
            .on_press(on_select(v))
            .into(),
        );
    }

    let label_w = text(label.to_string())
        .size(label_size)
        .font(syntax::EDITOR_FONT)
        .color(p.text)
        .width(Length::Fill);

    row![label_w, row(buttons).spacing(f * 0.25)]
        .spacing(f)
        .into()
}

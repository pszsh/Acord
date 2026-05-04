//! Menu strip + dropdown panel, generic over the host's `Message` type.
//!
//! `strip()` builds the horizontal category bar; `dropdown()` builds an
//! anchored panel of label+shortcut rows. Hosts handle "is this category
//! open?" themselves and stack the dropdown over their content with
//! `iced_widget::stack!` after picking the click position.

use iced_wgpu::Renderer;
use iced_wgpu::core::{Background, Border, Element, Length, Padding, Shadow, Theme};
use iced_widget::{button, container, row, text};

use crate::palette;
use crate::syntax;
use crate::widgets::style;

/// One row in a dropdown — either a clickable item or a horizontal separator.
#[derive(Clone)]
pub enum Row<Message: Clone + 'static> {
    Item {
        label: String,
        shortcut: String,
        on_press: Message,
    },
    Separator,
}

impl<Message: Clone + 'static> Row<Message> {
    pub fn item(label: impl Into<String>, shortcut: impl Into<String>, on_press: Message) -> Self {
        Row::Item { label: label.into(), shortcut: shortcut.into(), on_press }
    }

    pub fn separator() -> Self {
        Row::Separator
    }
}

/// One category in the strip bar.
#[derive(Clone)]
pub struct Category<K: Clone> {
    pub key: K,
    pub label: String,
}

impl<K: Clone> Category<K> {
    pub fn new(key: K, label: impl Into<String>) -> Self {
        Self { key, label: label.into() }
    }
}

/// Approximate button width for a category label, useful when computing
/// horizontal anchor offsets for dropdowns.
pub fn category_button_width(label: &str, font_size: f32) -> f32 {
    let char_w = font_size * 0.6;
    let pad_x = font_size * 0.85;
    label.len() as f32 * char_w + pad_x * 2.0
}

/// Renders the horizontal category bar. `is_active` tells whether the dropdown
/// for that category is currently open (renders a highlighted background).
pub fn strip<'a, K, Message, F>(
    categories: &'a [Category<K>],
    is_active: impl Fn(&K) -> bool,
    font_size: f32,
    mut on_toggle: F,
) -> Element<'a, Message, Theme, Renderer>
where
    K: Clone + 'a,
    Message: Clone + 'static,
    F: FnMut(K) -> Message + 'a,
{
    let p = palette::current();
    let f = font_size;
    let pad_x = f * 0.85;
    let pad_y = f * 0.18;
    let label_size = f * 0.92;

    let mut row_items: Vec<Element<'a, Message, Theme, Renderer>> = Vec::new();
    for cat in categories {
        let active = is_active(&cat.key);
        let key = cat.key.clone();
        let on_press = on_toggle(key);
        row_items.push(
            button(
                text(cat.label.clone())
                    .size(label_size)
                    .font(syntax::EDITOR_FONT),
            )
            .width(Length::Fixed(category_button_width(&cat.label, f)))
            .padding(Padding { top: pad_y, right: pad_x, bottom: pad_y, left: pad_x })
            .style(move |_t: &Theme, _s| button::Style {
                background: if active { Some(Background::Color(p.surface1)) } else { None },
                text_color: p.text,
                border: Border::default(),
                shadow: Shadow::default(),
                snap: false,
            })
            .on_press(on_press)
            .into(),
        );
    }

    container(row(row_items).spacing(0.0))
        .width(Length::Fill)
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.mantle)),
            border: Border::default(),
            text_color: Some(p.text),
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

/// Renders a dropdown panel of rows. Caller positions the panel over the
/// strip via `iced_widget::stack!` (typically with a `column!` of [empty
/// padding, dropdown]).
pub fn dropdown<'a, Message>(
    rows: Vec<Row<Message>>,
    font_size: f32,
    width: Length,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'static,
{
    let p = palette::current();
    let f = font_size;
    let item_pad_x = f * 0.95;
    let item_pad_y = f * 0.32;
    let radius = f * 0.30;
    let separator_h = (f * 0.08).max(1.0);
    let label_size = f * 0.85;
    let hint_size = f * 0.78;

    let mut items: Vec<Element<'a, Message, Theme, Renderer>> = Vec::new();
    for r in rows {
        match r {
            Row::Item { label, shortcut, on_press } => {
                let label_w = text(label).size(label_size).font(syntax::EDITOR_FONT).width(Length::Fill);
                let hint_w = text(shortcut).size(hint_size).font(syntax::EDITOR_FONT).color(p.overlay0);
                items.push(
                    button(row![label_w, hint_w].spacing(f))
                        .width(Length::Fill)
                        .padding(Padding { top: item_pad_y, right: item_pad_x, bottom: item_pad_y, left: item_pad_x })
                        .style(style::menu_item)
                        .on_press(on_press)
                        .into(),
                );
            }
            Row::Separator => {
                items.push(
                    container(text(""))
                        .width(Length::Fill)
                        .height(Length::Fixed(separator_h))
                        .style(move |_t: &Theme| container::Style {
                            background: Some(Background::Color(p.surface1)),
                            border: Border::default(),
                            text_color: None,
                            shadow: Shadow::default(),
                            snap: false,
                        })
                        .into(),
                );
            }
        }
    }

    container(iced_widget::column(items).spacing(0.0).width(width))
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border {
                color: p.surface1,
                width: 1.0,
                radius: radius.into(),
            },
            text_color: Some(p.text),
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

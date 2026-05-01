use iced_wgpu::core::text::{Span as TextSpan, Wrapping};
use iced_wgpu::core::{Background, Border, Color, Element, Length, Padding, Pixels, Point, Size, Theme};
use iced_widget::text::Rich;
use iced_widget::{
    button, column, container, mouse_area, opaque, responsive, row, scrollable, span, stack, text,
    text_input, Space,
};

use crate::palette;
use crate::syntax::{highlight_color, highlight_font};
use super::model::{BrowserItem, BrowserItemKind};
use super::preview::PreviewLine;
use super::state::{BrowserMessage, BrowserState, ContextMenu};

const TARGET_CARD_W: f32 = 280.0;
const MIN_CARD_W: f32 = 180.0;
const GAP: f32 = 16.0;
const OUTER_PAD: f32 = 16.0;
const CARD_PAD: f32 = 10.0;
const CARD_ASPECT: f32 = 0.72;

pub fn view(state: &BrowserState) -> Element<'_, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();

    let body_inner: Element<_, _, _> = if state.items.is_empty() {
        empty_state()
    } else {
        responsive(|size| scrollable(grid(state, size)).height(Length::Fill).into()).into()
    };

    // Captures right-clicks that fall between cards. Cards have their own
    // on_right_press, so this only fires on the gaps and empty regions.
    let body: Element<_, _, _> = mouse_area(body_inner)
        .on_right_press(BrowserMessage::ShowEmptyContextMenu)
        .into();

    let main: Element<_, _, _> = column![
        breadcrumb(state),
        rule(p.surface1),
        body,
    ]
    .height(Length::Fill)
    .into();

    let layered: Element<_, _, _> = match state.context_menu.as_ref() {
        Some(menu) => stack![main, context_menu_overlay(state, menu)].into(),
        None => main,
    };

    container(layered)
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border::default(),
            text_color: Some(p.text),
            shadow: Default::default(),
            snap: false,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn breadcrumb(state: &BrowserState) -> Element<'_, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    let segments = state.path_segments();
    let last_idx = segments.len().saturating_sub(1);

    let mut row_items: Vec<Element<_, _, _>> = Vec::new();

    for (i, (name, path)) in segments.into_iter().enumerate() {
        if i > 0 {
            row_items.push(
                text(">").size(11.0).color(p.overlay0).into()
            );
        }
        let is_last = i == last_idx;
        let label = text(name).size(12.0).color(if is_last { p.text } else { p.subtext0 });
        let btn = button(label)
            .padding(Padding { top: 2.0, right: 4.0, bottom: 2.0, left: 4.0 })
            .style(move |_t: &Theme, _s| button::Style {
                background: None,
                text_color: if is_last { p.text } else { p.subtext0 },
                border: Border::default(),
                shadow: Default::default(),
                snap: false,
            })
            .on_press(BrowserMessage::NavigateTo(path));
        row_items.push(btn.into());
    }

    container(row(row_items).spacing(2.0))
        .padding(Padding { top: 8.0, right: 16.0, bottom: 8.0, left: 16.0 })
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.mantle)),
            border: Border::default(),
            text_color: Some(p.text),
            shadow: Default::default(),
            snap: false,
        })
        .width(Length::Fill)
        .into()
}

fn rule(color: Color) -> Element<'static, BrowserMessage, Theme, iced_wgpu::Renderer> {
    container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border::default(),
            text_color: None,
            shadow: Default::default(),
            snap: false,
        })
        .into()
}

fn empty_state() -> Element<'static, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    container(
        column![
            text("No documents").size(16.0).color(p.subtext0),
            text("Create a new note or add files to this folder").size(12.0).color(p.overlay0),
        ]
        .spacing(8.0)
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(Padding { top: 100.0, right: 0.0, bottom: 0.0, left: 0.0 })
    .center_x(Length::Fill)
    .into()
}

/// picks the column count whose card-width sits closest to the scale-adjusted target.
fn columns_for_width(avail_w: f32, scale: f32) -> usize {
    let target = TARGET_CARD_W * scale;
    let min_w = MIN_CARD_W * scale;
    let inner = (avail_w - 2.0 * OUTER_PAD).max(0.0);
    if inner < min_w {
        return 1;
    }
    let mut best = 1usize;
    let mut best_diff = f32::MAX;
    for n in 1..=8 {
        let nf = n as f32;
        let card_w = (inner - (nf - 1.0) * GAP * scale) / nf;
        if card_w < min_w {
            break;
        }
        let diff = (card_w - target).abs();
        if diff < best_diff {
            best_diff = diff;
            best = n;
        }
    }
    best
}

/// lays out items as a fill-the-width grid of fixed-aspect cards.
fn grid(state: &BrowserState, size: Size) -> Element<'_, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let scale = state.scale;
    let cols = columns_for_width(size.width, scale);

    let inner = (size.width - 2.0 * OUTER_PAD).max(0.0);
    let card_w = ((inner - (cols as f32 - 1.0) * GAP * scale) / cols as f32).max(MIN_CARD_W * scale);
    let card_h = card_w * CARD_ASPECT;

    let mut rows: Vec<Element<_, _, _>> = Vec::new();
    for chunk in state.items.chunks(cols) {
        let mut row_items: Vec<Element<_, _, _>> = Vec::new();
        for item in chunk {
            row_items.push(card(item, state, scale, card_w, card_h));
        }
        while row_items.len() < cols {
            row_items.push(
                Space::new()
                    .width(Length::Fixed(card_w))
                    .height(Length::Fixed(card_h))
                    .into()
            );
        }
        rows.push(
            row(row_items)
                .spacing(GAP * scale)
                .into()
        );
    }

    container(
        column(rows)
            .spacing(GAP * scale)
            .width(Length::Fill)
    )
    .padding(OUTER_PAD)
    .width(Length::Fill)
    .into()
}

/// stacks a kind-specific preview above a title strip inside one click target.
fn card<'a>(
    item: &'a BrowserItem,
    state: &'a BrowserState,
    scale: f32,
    card_w: f32,
    card_h: f32,
) -> Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    let selected = state.is_selected(item);
    let renaming = state.is_renaming(item);

    let title_size = 12.0 * scale;
    let title_h = title_size * 1.4 + 4.0;
    let preview_h = (card_h - title_h - CARD_PAD * 2.0 - 6.0 * scale).max(0.0);

    let preview: Element<_, _, _> = match item.kind {
        BrowserItemKind::Folder => folder_preview(&item.preview, scale, preview_h),
        BrowserItemKind::File => file_preview(&item.preview_lines, scale, preview_h),
    };

    let title: Element<_, _, _> = if renaming {
        text_input("Name", &state.rename_text)
            .on_input(BrowserMessage::UpdateRename)
            .on_submit(BrowserMessage::CommitRename)
            .size(title_size)
            .padding(Padding { top: 2.0, right: 4.0, bottom: 2.0, left: 4.0 })
            .into()
    } else {
        container(
            text(item.name.clone())
                .size(title_size)
                .color(p.text)
                .wrapping(Wrapping::None),
        )
        .width(Length::Fill)
        .height(Length::Fixed(title_h))
        .clip(true)
        .into()
    };

    let content = column![preview, title].spacing(6.0 * scale);

    let item_path = item.path.clone();
    let is_file = item.kind == BrowserItemKind::File;

    let body = container(content)
        .width(Length::Fixed(card_w))
        .height(Length::Fixed(card_h))
        .padding(CARD_PAD)
        .clip(true)
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(if selected { p.surface1 } else { p.surface0 })),
            border: Border {
                color: if selected { p.blue } else { Color::TRANSPARENT },
                width: if selected { 2.0 } else { 0.0 },
                radius: (8.0 * scale).into(),
            },
            text_color: Some(p.text),
            shadow: Default::default(),
            snap: false,
        });

    let open_msg = match item.kind {
        BrowserItemKind::Folder => BrowserMessage::NavigateTo(item_path.clone()),
        BrowserItemKind::File => BrowserMessage::Open(item_path.clone()),
    };

    mouse_area(body)
        .on_press(BrowserMessage::Select(item_path.clone()))
        .on_double_click(open_msg)
        .on_right_press(BrowserMessage::ShowContextMenu {
            path: item_path,
            is_file,
        })
        .into()
}

/// renders a folder icon and item-count summary inside the card's preview slot.
fn folder_preview(
    summary: &str,
    scale: f32,
    preview_h: f32,
) -> Element<'static, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    container(
        row![
            text("\u{1F4C1}").size(24.0 * scale).color(p.blue),
            text(summary.to_string()).size(10.0 * scale).color(p.subtext0),
        ]
        .spacing(8.0 * scale)
    )
    .width(Length::Fill)
    .height(Length::Fixed(preview_h))
    .padding(8.0 * scale)
    .style(move |_t: &Theme| container::Style {
        background: Some(Background::Color(p.mantle)),
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
        text_color: Some(p.text),
        shadow: Default::default(),
        snap: false,
    })
    .into()
}

/// renders pre-highlighted preview lines as a clipped column of rich-text.
fn file_preview<'a>(
    lines: &'a [PreviewLine],
    scale: f32,
    preview_h: f32,
) -> Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    let body_size = 9.0 * scale;
    let line_spacing = 2.0 * scale;

    if lines.is_empty() {
        return container(text("(empty)").size(body_size).color(p.overlay0))
            .width(Length::Fill)
            .height(Length::Fixed(preview_h))
            .padding(8.0 * scale)
            .style(move |_t: &Theme| container::Style {
                background: Some(Background::Color(p.mantle)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: (4.0 * scale).into(),
                },
                text_color: Some(p.subtext0),
                shadow: Default::default(),
                snap: false,
            })
            .into();
    }

    let mut col_items: Vec<Element<_, _, _>> = Vec::new();
    for line in lines {
        let size = match line.heading {
            Some(1) => body_size * 1.5,
            Some(2) => body_size * 1.3,
            Some(3) => body_size * 1.15,
            _ => body_size,
        };
        col_items.push(preview_line(line, size, p.subtext0));
    }

    let inner = column(col_items).spacing(line_spacing);

    container(inner)
        .width(Length::Fill)
        .height(Length::Fixed(preview_h))
        .padding(8.0 * scale)
        .clip(true)
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.mantle)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: (4.0 * scale).into(),
            },
            text_color: Some(p.subtext0),
            shadow: Default::default(),
            snap: false,
        })
        .into()
}

/// turns one preview line's syntax spans into a rich-text element at the given size.
fn preview_line<'a>(
    line: &'a PreviewLine,
    size: f32,
    fallback: Color,
) -> Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer> {
    if line.text.is_empty() {
        return Space::new().width(Length::Shrink).height(Length::Fixed(size * 0.6)).into();
    }

    let mut spans: Vec<TextSpan<'a, ()>> = Vec::new();
    let mut cursor = 0usize;
    for (range, kind) in &line.spans {
        if range.start > cursor {
            spans.push(plain_span(&line.text[cursor..range.start], fallback));
        }
        let slice = &line.text[range.start..range.end];
        let color = highlight_color(*kind);
        let mut s = span(slice).color(color);
        if let Some(font) = highlight_font(*kind) {
            s = s.font(font);
        }
        spans.push(s);
        cursor = range.end;
    }
    if cursor < line.text.len() {
        spans.push(plain_span(&line.text[cursor..], fallback));
    }

    Rich::with_spans(spans).size(Pixels(size)).into()
}

fn plain_span<'a>(text: &'a str, color: Color) -> TextSpan<'a, ()> {
    span(text).color(color)
}

/// stacks a click-out catcher behind a positioned menu pinned at the right-click anchor.
fn context_menu_overlay<'a>(
    state: &'a BrowserState,
    menu: &'a ContextMenu,
) -> Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let dismiss = mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(BrowserMessage::HideContextMenu)
        .on_right_press(BrowserMessage::HideContextMenu);

    let full = state.context_acts_on_selection();
    let positioned = positioned_menu(menu.anchor, menu_column(state, full));

    stack![dismiss, positioned].into()
}

/// places the menu column at an absolute anchor by padding from the top-left.
fn positioned_menu<'a>(
    anchor: Point,
    inner: Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer>,
) -> Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let menu = opaque(inner);
    column![
        Space::new().width(Length::Shrink).height(Length::Fixed(anchor.y.max(0.0))),
        row![
            Space::new().width(Length::Fixed(anchor.x.max(0.0))).height(Length::Shrink),
            menu,
        ],
    ]
    .into()
}

/// renders the unified menu used by both the context menu and the menu bar.
/// `full` decides whether to show selection-dependent items beyond New Folder.
fn menu_column<'a>(
    state: &'a BrowserState,
    full: bool,
) -> Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    let mut items: Vec<Element<_, _, _>> = Vec::new();

    if full {
        let single = state.single_selected();
        if let Some(path) = &single {
            let label = if path.is_dir() { "Open Folder" } else { "Open" };
            items.push(menu_item(label, BrowserMessage::ContextOpen));
            items.push(menu_item("Rename", BrowserMessage::ContextRename));
        }
        items.push(menu_item("Duplicate", BrowserMessage::ContextDuplicate));
        items.push(menu_separator());
        items.push(menu_item("Delete", BrowserMessage::ContextTrash));
        items.push(menu_separator());
    }

    items.push(menu_item("New Folder", BrowserMessage::NewFolder));
    if full {
        items.push(menu_item("New Folder with Selection", BrowserMessage::NewFolderWithSelection));
    }

    container(column(items).spacing(0.0))
        .width(Length::Fixed(220.0))
        .padding(Padding { top: 4.0, right: 0.0, bottom: 4.0, left: 0.0 })
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.surface1)),
            border: Border {
                color: p.surface2,
                width: 1.0,
                radius: 6.0.into(),
            },
            text_color: Some(p.text),
            shadow: Default::default(),
            snap: false,
        })
        .into()
}

/// one clickable row inside a menu.
fn menu_item(
    label: &'static str,
    msg: BrowserMessage,
) -> Element<'static, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    button(text(label).size(12.0).color(p.text))
        .padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 12.0 })
        .width(Length::Fill)
        .style(move |_t: &Theme, status| {
            let bg = match status {
                button::Status::Hovered => Some(Background::Color(p.surface2)),
                _ => None,
            };
            button::Style {
                background: bg,
                text_color: p.text,
                border: Border::default(),
                shadow: Default::default(),
                snap: false,
            }
        })
        .on_press(msg)
        .into()
}

/// a thin separator line between menu sections.
fn menu_separator() -> Element<'static, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .padding(Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 6.0 })
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.surface2)),
            border: Border::default(),
            text_color: None,
            shadow: Default::default(),
            snap: false,
        })
        .width(Length::Fill)
        .into()
}

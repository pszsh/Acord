use iced_wgpu::core::{Background, Border, Color, Element, Length, Padding, Theme};
use iced_widget::{button, column, container, mouse_area, row, scrollable, text, text_input, Space};

use crate::palette;
use super::model::{BrowserItem, BrowserItemKind};
use super::state::{BrowserMessage, BrowserState};

const CARDS_PER_ROW: usize = 3;
const CARD_BASE_W: f32 = 240.0;

pub fn view(state: &BrowserState) -> Element<'_, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();

    let body: Element<_, _, _> = if state.items.is_empty() {
        empty_state()
    } else {
        scrollable(grid(state)).height(Length::Fill).into()
    };

    let main = column![
        breadcrumb(state),
        rule(p.surface1),
        body,
    ]
    .height(Length::Fill);

    container(main)
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

fn grid(state: &BrowserState) -> Element<'_, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let scale = state.scale;
    let mut rows: Vec<Element<_, _, _>> = Vec::new();
    let chunk_size = CARDS_PER_ROW;

    for chunk in state.items.chunks(chunk_size) {
        let mut row_items: Vec<Element<_, _, _>> = Vec::new();
        for item in chunk {
            row_items.push(card(item, state, scale));
        }
        // Pad short final row so cards keep their fixed width instead of stretching.
        while row_items.len() < chunk_size {
            row_items.push(
                Space::new()
                    .width(Length::Fill)
                    .height(Length::Shrink)
                    .into()
            );
        }
        rows.push(
            row(row_items)
                .spacing(16.0 * scale)
                .into()
        );
    }

    container(
        column(rows)
            .spacing(16.0 * scale)
            .width(Length::Fill)
    )
    .padding(16.0 * scale)
    .width(Length::Fill)
    .into()
}

fn card<'a>(
    item: &'a BrowserItem,
    state: &'a BrowserState,
    scale: f32,
) -> Element<'a, BrowserMessage, Theme, iced_wgpu::Renderer> {
    let p = palette::current();
    let selected = state.is_selected(item);
    let renaming = state.is_renaming(item);

    let preview_h = (CARD_BASE_W * scale) * 0.55;
    let card_w = CARD_BASE_W * scale;

    let preview: Element<_, _, _> = match item.kind {
        BrowserItemKind::Folder => container(
            row![
                text("\u{1F4C1}").size(24.0 * scale).color(p.blue),
                text(item.preview.clone()).size(10.0 * scale).color(p.subtext0),
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
        .into(),
        BrowserItemKind::File => container(
            text(item.preview.clone()).size(10.0 * scale).color(p.subtext0)
        )
        .width(Length::Fill)
        .height(Length::Fixed(preview_h))
        .padding(8.0 * scale)
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(p.mantle)),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
            text_color: Some(p.subtext0),
            shadow: Default::default(),
            snap: false,
        })
        .into(),
    };

    let title: Element<_, _, _> = if renaming {
        text_input("Name", &state.rename_text)
            .on_input(BrowserMessage::UpdateRename)
            .on_submit(BrowserMessage::CommitRename)
            .size(12.0 * scale)
            .padding(Padding { top: 2.0, right: 4.0, bottom: 2.0, left: 4.0 })
            .into()
    } else {
        text(item.name.clone()).size(12.0 * scale).color(p.text).into()
    };

    let content = column![preview, title].spacing(6.0 * scale);

    let item_path = item.path.clone();
    let is_file = item.kind == BrowserItemKind::File;

    let body = container(content)
        .width(Length::Fixed(card_w))
        .padding(10.0 * scale)
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

    let click_msg = match item.kind {
        BrowserItemKind::Folder => BrowserMessage::NavigateTo(item_path.clone()),
        BrowserItemKind::File => BrowserMessage::Open(item_path.clone()),
    };

    mouse_area(body)
        .on_press(click_msg)
        .on_right_press(BrowserMessage::ShowContextMenu {
            anchor: iced_wgpu::core::Point::new(0.0, 0.0),
            path: item_path,
            is_file,
        })
        .into()
}

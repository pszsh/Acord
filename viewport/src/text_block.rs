//! `TextBlock` — the trait-implementing wrapper around `text_widget::Content`.
//!
//! Owns the editor content and language hint for syntax highlighting. Lives in
//! `EditorState::blocks` as a `Box<dyn Block>`.

use iced_wgpu::core::text::Wrapping;
use iced_wgpu::core::text::highlighter::Format;
use iced_wgpu::core::{
    Background, Border, Color, Element, Length, Padding, Point, Theme,
};
use crate::text_widget::{self, Style};

use crate::block::{Block, BlockCommand, LayeredView, ViewCtx};
use crate::palette;
use crate::selection::{BlockId, InnerPath};
use crate::syntax::{self, SyntaxHighlighter, SyntaxSettings};

pub struct TextBlock {
    pub id: BlockId,
    pub content: text_widget::Content,
    /// Document-relative starting line. Maintained by `recount_lines`.
    pub start_line: usize,
    /// Language hint for syntax highlighting.
    pub lang: String,
}

impl TextBlock {
    pub fn new(id: BlockId, text: &str, start_line: usize, lang: String) -> Self {
        Self {
            id,
            content: text_widget::Content::with_text(text),
            start_line,
            lang,
        }
    }
}

impl<Message: Clone + 'static> Block<Message> for TextBlock {
    fn id(&self) -> BlockId {
        self.id
    }

    fn kind_tag(&self) -> &'static str {
        "text"
    }

    fn start_line(&self) -> usize {
        self.start_line
    }

    fn set_start_line(&mut self, line: usize) {
        self.start_line = line;
    }

    fn line_count(&self) -> usize {
        self.content.line_count()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn view<'a>(&'a self, ctx: &ViewCtx<'_, Message>) -> LayeredView<'a, Message> {
        let block_idx = ctx.block_index;
        let on_action = ctx.on_text_action;

        let editor = text_widget::TextEditor::new(&self.content)
            .on_action(move |action| on_action(block_idx, action))
            .font(syntax::EDITOR_FONT)
            .size(ctx.font_size)
            .height(Length::Fill)
            .padding(Padding {
                top: 8.0,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            })
            .wrapping(Wrapping::Word)
            .style(|_theme: &Theme, _status: text_widget::Status| {
                let p = palette::current();
                Style {
                    background: Background::Color(p.base),
                    border: Border::default(),
                    placeholder: p.overlay0,
                    value: p.text,
                    selection: Color { a: 0.4, ..p.blue },
                }
            });

        let settings = SyntaxSettings {
            lang: self.lang.clone(),
            source: self.content.text(),
        };
        let editor_el: Element<'a, Message, Theme, iced_wgpu::Renderer> = editor
            .highlight_with::<SyntaxHighlighter>(
                settings,
                |highlight, _theme| Format {
                    color: Some(syntax::highlight_color(highlight.kind)),
                    font: syntax::highlight_font(highlight.kind),
                },
            )
            .into();

        LayeredView::just(editor_el)
    }

    fn to_md(&self) -> String {
        self.content.text()
    }

    fn hit_test(&self, _point: Point) -> Option<InnerPath> {
        Some(InnerPath::Whole)
    }

    fn apply(&mut self, _cmd: BlockCommand) {
        // Text mutations go through `text_editor::Action` routed via
        // `Message::BlockAction` in the editor's update loop. BlockCommand
        // on a text block is a no-op.
    }

    fn selectable_paths(&self) -> Box<dyn Iterator<Item = InnerPath> + '_> {
        Box::new(std::iter::once(InnerPath::Whole))
    }
}

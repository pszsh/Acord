use iced_wgpu::core::alignment;
use iced_wgpu::core::text::LineHeight;
use iced_wgpu::core::{
    mouse, Element, Font, Length, Pixels, Point, Rectangle, Theme,
};
use iced_wgpu::core::font::Weight;
use iced_widget::canvas;

use crate::block::{Block, BlockCommand, LayeredView, ViewCtx};
use crate::oklab;
use crate::palette;
use crate::selection::{BlockId, InnerPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingLevel {
    H1,
    H2,
    H3,
    H4,
}

impl HeadingLevel {
    pub fn scale(&self) -> f32 {
        match self {
            HeadingLevel::H1 => 2.0,
            HeadingLevel::H2 => 1.5,
            HeadingLevel::H3 => 1.17,
            HeadingLevel::H4 => 1.0,
        }
    }

    pub fn weight(&self) -> Weight {
        match self {
            HeadingLevel::H1 => Weight::Black,
            HeadingLevel::H2 => Weight::Bold,
            HeadingLevel::H3 => Weight::Semibold,
            HeadingLevel::H4 => Weight::Medium,
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
        }
    }

    pub fn from_u8(level: u8) -> HeadingLevel {
        match level {
            1 => HeadingLevel::H1,
            2 => HeadingLevel::H2,
            3 => HeadingLevel::H3,
            _ => HeadingLevel::H4,
        }
    }
}

struct HeadingProgram {
    level: HeadingLevel,
    text: String,
    font_size: f32,
}

impl<Message: Clone> canvas::Program<Message, Theme, iced_wgpu::Renderer> for HeadingProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced_wgpu::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<iced_wgpu::Renderer>> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let p = palette::current();

        let color = match self.level {
            HeadingLevel::H1 => p.rosewater,
            HeadingLevel::H2 => p.peach,
            HeadingLevel::H3 => p.yellow,
            HeadingLevel::H4 => p.green,
        };

        frame.fill_text(canvas::Text {
            content: self.text.clone(),
            position: Point::new(8.0, 4.0),
            max_width: bounds.width - 16.0,
            color: oklab::lighten_for_size(color, self.font_size),
            size: Pixels(self.font_size),
            line_height: LineHeight::Relative(1.4),
            font: Font { weight: self.level.weight(), ..Font::DEFAULT },
            align_x: iced_wgpu::core::text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: iced_wgpu::core::text::Shaping::Basic,
        });

        vec![frame.into_geometry()]
    }
}

fn build<Message: Clone + 'static>(
    level: HeadingLevel,
    text: &str,
    base_font_size: f32,
) -> Element<'static, Message, Theme, iced_wgpu::Renderer> {
    let font_size = base_font_size * level.scale();
    let height = font_size * 1.4 + 8.0;
    canvas::Canvas::new(HeadingProgram {
        level,
        text: text.to_string(),
        font_size,
    })
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .into()
}

pub struct HeadingBlock {
    pub id: BlockId,
    pub level: HeadingLevel,
    pub text: String,
    pub start_line: usize,
}

impl HeadingBlock {
    pub fn new(id: BlockId, level: HeadingLevel, text: String, start_line: usize) -> Self {
        Self { id, level, text, start_line }
    }
}

impl<Message: Clone + 'static> Block<Message> for HeadingBlock {
    fn id(&self) -> BlockId {
        self.id
    }

    fn kind_tag(&self) -> &'static str {
        "heading"
    }

    fn start_line(&self) -> usize {
        self.start_line
    }

    fn set_start_line(&mut self, line: usize) {
        self.start_line = line;
    }

    fn line_count(&self) -> usize {
        1
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn view<'a>(&'a self, ctx: &ViewCtx<'_, Message>) -> LayeredView<'a, Message> {
        LayeredView::just(build(self.level, &self.text, ctx.font_size))
    }

    fn to_md(&self) -> String {
        let prefix = "#".repeat(self.level.as_u8() as usize);
        format!("{prefix} {}", self.text)
    }

    fn hit_test(&self, _point: Point) -> Option<InnerPath> {
        Some(InnerPath::Whole)
    }

    fn apply(&mut self, cmd: BlockCommand) {
        match cmd {
            BlockCommand::SetHeadingLevel(level) => {
                self.level = HeadingLevel::from_u8(level);
            }
            BlockCommand::SetHeadingText(text) => {
                self.text = text;
            }
            _ => {}
        }
    }

    fn selectable_paths(&self) -> Box<dyn Iterator<Item = InnerPath> + '_> {
        Box::new(std::iter::once(InnerPath::Whole))
    }
}

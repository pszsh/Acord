use iced_wgpu::core::{mouse, Element, Length, Point, Rectangle, Theme};
use iced_widget::canvas;

use crate::block::{Block, BlockCommand, LayeredView, ViewCtx};
use crate::palette;
use crate::selection::{BlockId, InnerPath};

struct HRProgram;

impl<Message: Clone> canvas::Program<Message, Theme, iced_wgpu::Renderer> for HRProgram {
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
        let y = bounds.height / 2.0;
        let margin = 8.0;
        let path = canvas::Path::line(
            Point::new(margin, y),
            Point::new(bounds.width - margin, y),
        );
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_width(1.0)
                .with_color(p.overlay0),
        );
        vec![frame.into_geometry()]
    }
}

fn build<Message: Clone + 'static>() -> Element<'static, Message, Theme, iced_wgpu::Renderer> {
    canvas::Canvas::new(HRProgram)
        .width(Length::Fill)
        .height(Length::Fixed(20.0))
        .into()
}

pub struct HrBlock {
    pub id: BlockId,
    pub start_line: usize,
}

impl HrBlock {
    pub fn new(id: BlockId, start_line: usize) -> Self {
        Self { id, start_line }
    }
}

impl<Message: Clone + 'static> Block<Message> for HrBlock {
    fn id(&self) -> BlockId {
        self.id
    }

    fn kind_tag(&self) -> &'static str {
        "hr"
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

    fn view<'a>(&'a self, _ctx: &ViewCtx<'_, Message>) -> LayeredView<'a, Message> {
        LayeredView::just(build())
    }

    fn to_md(&self) -> String {
        "---".to_string()
    }

    fn hit_test(&self, _point: Point) -> Option<InnerPath> {
        Some(InnerPath::Whole)
    }

    fn apply(&mut self, _cmd: BlockCommand) {
        // HRs have no structural state to mutate.
    }

    fn selectable_paths(&self) -> Box<dyn Iterator<Item = InnerPath> + '_> {
        Box::new(std::iter::once(InnerPath::Whole))
    }
}

use iced_wgpu::core::text::LineHeight;
use iced_wgpu::core::{
    alignment, Background, Border, Element, Font, Length, Padding, Pixels,
    Point, Rectangle, Shadow, Theme,
};
use iced_widget::canvas;
use iced_widget::container;

use crate::block::{Block, BlockCommand, LayeredView, ViewCtx};
use crate::palette;
use crate::selection::{BlockId, InnerPath};

const BASE_FONT: f32 = 13.0;

fn node_height(font_size: f32) -> f32 { font_size * (20.0 / BASE_FONT) }
fn indent_px(font_size: f32) -> f32 { font_size * (20.0 / BASE_FONT) }
fn branch_inset(font_size: f32) -> f32 { font_size * (12.0 / BASE_FONT) }
fn glyph_width(font_size: f32) -> f32 { font_size * (7.2 / BASE_FONT) }
const WIDGET_INNER_PADDING: Padding = Padding {
    top: 4.0,
    right: 8.0,
    bottom: 4.0,
    left: 8.0,
};
const WIDGET_OUTER_PADDING: Padding = Padding {
    top: 2.0,
    right: 0.0,
    bottom: 2.0,
    left: 8.0,
};

#[derive(Debug, Clone)]
pub enum TreeMessage {}

struct TreeNode {
    label: String,
    depth: usize,
    is_last: bool,
}

fn flatten_tree(val: &serde_json::Value, depth: usize, is_last: bool, out: &mut Vec<TreeNode>) {
    match val {
        serde_json::Value::Array(items) => {
            if depth > 0 {
                out.push(TreeNode {
                    label: "[array]".into(),
                    depth,
                    is_last,
                });
            }
            let len = items.len();
            for (i, item) in items.iter().enumerate() {
                flatten_tree(item, depth + 1, i == len - 1, out);
            }
        }
        serde_json::Value::Object(_) => {
            out.push(TreeNode {
                label: "{object}".into(),
                depth,
                is_last,
            });
        }
        serde_json::Value::String(s) => {
            out.push(TreeNode {
                label: format!("\"{}\"", s),
                depth,
                is_last,
            });
        }
        serde_json::Value::Number(n) => {
            out.push(TreeNode {
                label: n.to_string(),
                depth,
                is_last,
            });
        }
        serde_json::Value::Bool(b) => {
            out.push(TreeNode {
                label: b.to_string(),
                depth,
                is_last,
            });
        }
        serde_json::Value::Null => {
            out.push(TreeNode {
                label: "null".into(),
                depth,
                is_last,
            });
        }
    }
}

pub struct TreeProgram {
    nodes: Vec<TreeNode>,
    total_height: f32,
    content_width: f32,
    font_size: f32,
}

impl TreeProgram {
    pub fn from_json_scaled(val: &serde_json::Value, font_size: f32) -> Self {
        let mut nodes = Vec::new();
        match val {
            serde_json::Value::Array(items) => {
                let len = items.len();
                for (i, item) in items.iter().enumerate() {
                    flatten_tree(item, 0, i == len - 1, &mut nodes);
                }
            }
            _ => {
                flatten_tree(val, 0, true, &mut nodes);
            }
        }
        let nh = node_height(font_size);
        let ind = indent_px(font_size);
        let gw = glyph_width(font_size);
        let total_height = (nodes.len() as f32 * nh).max(nh);
        let content_width = nodes.iter()
            .map(|n| {
                let depth_px = n.depth as f32 * ind + 16.0;
                let label_px = (n.label.chars().count() as f32 + 3.0) * gw;
                depth_px + label_px
            })
            .fold(60.0_f32, f32::max);
        Self { nodes, total_height, content_width, font_size }
    }

    pub fn height(&self) -> f32 {
        self.total_height
    }

    pub fn width(&self) -> f32 {
        self.content_width
    }
}

impl<Message: Clone> canvas::Program<Message, Theme, iced_wgpu::Renderer> for TreeProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced_wgpu::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_wgpu::core::mouse::Cursor,
    ) -> Vec<canvas::Geometry<iced_wgpu::Renderer>> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let p = palette::current();
        let ws = palette::widget_surface();
        let connector_color = p.overlay0;
        let label_color = ws.body_text;
        let array_color = p.overlay1;

        let nh = node_height(self.font_size);
        let ind = indent_px(self.font_size);
        let bi = branch_inset(self.font_size);

        for (i, node) in self.nodes.iter().enumerate() {
            let y = i as f32 * nh;
            let indent_x = node.depth as f32 * ind + 8.0;

            if node.depth > 0 {
                let parent_x = (node.depth - 1) as f32 * ind + 8.0;
                let connector = canvas::Path::new(|b| {
                    b.move_to(Point::new(parent_x, y));
                    b.line_to(Point::new(parent_x, y + nh / 2.0));
                    b.line_to(Point::new(parent_x + bi, y + nh / 2.0));
                });
                frame.stroke(
                    &connector,
                    canvas::Stroke::default()
                        .with_width(1.0)
                        .with_color(connector_color),
                );

                if !node.is_last {
                    let vert = canvas::Path::line(
                        Point::new(indent_x - ind, y + nh / 2.0),
                        Point::new(indent_x - ind, y + nh),
                    );
                    frame.stroke(
                        &vert,
                        canvas::Stroke::default()
                            .with_width(1.0)
                            .with_color(connector_color),
                    );
                }
            }

            let text_color = if node.label.starts_with('[') || node.label.starts_with('{') {
                array_color
            } else {
                label_color
            };

            let branch_char = if node.depth == 0 {
                String::new()
            } else if node.is_last {
                "\u{2514}\u{2500} ".into() // └─
            } else {
                "\u{251C}\u{2500} ".into() // ├─
            };

            let display = format!("{}{}", branch_char, node.label);

            frame.fill_text(canvas::Text {
                content: display,
                position: Point::new(indent_x, y + 2.0),
                max_width: bounds.width - indent_x,
                color: text_color,
                size: Pixels(self.font_size),
                line_height: LineHeight::Relative(1.3),
                font: Font::MONOSPACE,
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Top,
                shaping: iced_wgpu::core::text::Shaping::Basic,
            });
        }

        vec![frame.into_geometry()]
    }
}

/// Builds the framed canvas Element for a tree block. Returns `'static`
/// because `TreeProgram::from_json` clones the labels into an owned `Vec<TreeNode>` —
/// nothing in the returned widget tree borrows from `data`.
/// Total rendered height of a tree element including padding and border.
pub fn element_height(data: &serde_json::Value, font_size: f32) -> f32 {
    let program = TreeProgram::from_json_scaled(data, font_size);
    program.height()
        + WIDGET_INNER_PADDING.top + WIDGET_INNER_PADDING.bottom
        + WIDGET_OUTER_PADDING.top + WIDGET_OUTER_PADDING.bottom
}

pub fn build<Message: Clone + 'static>(
    data: &serde_json::Value,
    font_size: f32,
) -> Element<'static, Message, Theme, iced_wgpu::Renderer> {
    let program = TreeProgram::from_json_scaled(data, font_size);
    let h = program.height();
    let w = program.width();
    let canvas_el: Element<'static, Message, Theme, iced_wgpu::Renderer> =
        canvas::Canvas::new(program)
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .into();

    let framed = container(canvas_el)
        .padding(WIDGET_INNER_PADDING)
        .style(|_theme: &Theme| {
            let ws = palette::widget_surface();
            container::Style {
                background: Some(Background::Color(ws.fill)),
                border: Border {
                    color: ws.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                text_color: Some(ws.body_text),
                shadow: Shadow::default(),
                snap: false,
            }
        });

    container(framed)
        .padding(WIDGET_OUTER_PADDING)
        .width(Length::Shrink)
        .style(|_theme: &Theme| container::Style {
            background: None,
            border: Border::default(),
            text_color: None,
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

/// Trait-implementing struct for a tree block. Owns the JSON value; the
/// canvas program is rebuilt fresh on each `view` call (cheap — flatten_tree
/// is O(nodes) and the JSON is already parsed).
pub struct TreeBlock {
    pub id: BlockId,
    pub data: serde_json::Value,
    pub start_line: usize,
}

impl TreeBlock {
    pub fn new(id: BlockId, data: serde_json::Value, start_line: usize) -> Self {
        Self { id, data, start_line }
    }
}

impl<Message: Clone + 'static> Block<Message> for TreeBlock {
    fn id(&self) -> BlockId {
        self.id
    }

    fn kind_tag(&self) -> &'static str {
        "tree"
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
        LayeredView::just(build(&self.data, ctx.font_size))
    }

    fn to_md(&self) -> String {
        // Trees aren't currently round-tripped through markdown — they only
        // appear as eval results.
        String::new()
    }

    fn hit_test(&self, _point: Point) -> Option<InnerPath> {
        Some(InnerPath::Whole)
    }

    fn apply(&mut self, _cmd: BlockCommand) {
        // Trees are read-only.
    }

    fn selectable_paths(&self) -> Box<dyn Iterator<Item = InnerPath> + '_> {
        Box::new(std::iter::once(InnerPath::Whole))
    }
}

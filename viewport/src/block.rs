//! The `Block` trait that all block kinds implement.
//!
//! Each concrete struct (TextBlock, TableBlock, HeadingBlock, HrBlock,
//! TreeBlock) keeps its own internal data but exposes a common interface for
//! iteration, dispatch, selection participation, hit-testing, and serialization.
//!
//! Two rules apply throughout the trait:
//!
//! 1. **Iteration over recursion.** `selectable_paths` returns an iterator and
//!    must be implemented without self-recursion. When nesting (cells containing
//!    blocks) lands, this protects deep documents from stack overflow.
//! 2. **Layered draw order.** `view` returns a `LayeredView`, not a single
//!    element. The document compositor merges overlays from every block into
//!    shared layers, so cursorline (Below) and selection borders (Above) end up
//!    in the right z-order regardless of where they were declared.

use iced_wgpu::core::{Element, Point, Theme};
use crate::text_widget;

use crate::selection::{BlockId, InnerPath, NodePath, Selection};
use crate::table_block::TableMessage;

/// Z-ordering for the document compositor. Explicit draw order decoupled from
/// the structural list, so overlays render in the right place regardless of
/// where they were declared.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// Drawn behind block content. Cursorline highlights, selection background tints.
    Below = 0,
    /// The block's structural body. Most blocks emit their main element here.
    Content = 1,
    /// Drawn in front of content. Selection borders, focus rings, eval result tags.
    Above = 2,
    /// The very top. Drag previews, ghosted reorder anchors.
    Floating = 3,
}

/// What a block returns from `view`. The compositor merges layered output across
/// all blocks, drawing in layer order rather than source order.
pub struct LayeredView<'a, Message> {
    /// The block's main structural element. Always rendered in document order
    /// in a column; conceptually at `Layer::Content`.
    pub base: Element<'a, Message, Theme, iced_wgpu::Renderer>,
    /// Decorative overlays tagged with their target layer.
    pub overlays: Vec<(Layer, Element<'a, Message, Theme, iced_wgpu::Renderer>)>,
}

impl<'a, Message> LayeredView<'a, Message> {
    /// Convenience for blocks with no overlays (HR, headings without cursorline).
    pub fn just(base: Element<'a, Message, Theme, iced_wgpu::Renderer>) -> Self {
        Self { base, overlays: Vec::new() }
    }
}

/// Shared rendering context passed by reference into every `Block::view` call.
/// Instead of embedding selection or focus state on every block, blocks query a
/// shared context.
///
/// `on_text_action` and `on_table_msg` are plain function pointers, not boxed
/// closures: the editor only needs message constructors that wrap an index, no
/// captured state. This makes `ViewCtx` cheap to copy and avoids the
/// invariant-lifetime trap that capturing closures would impose on the trait
/// `view` signature.
pub struct ViewCtx<'a, Message: 'a> {
    pub block_index: usize,
    pub selection: &'a Selection,
    pub focus: Option<&'a NodePath>,
    pub editing: Option<&'a NodePath>,
    pub font_size: f32,
    pub is_dark: bool,
    pub on_text_action: fn(usize, text_widget::Action) -> Message,
    pub on_table_msg: fn(usize, TableMessage) -> Message,
    /// Computed values for cells whose raw text is a `/=...` formula.
    /// Keyed by (table block id, col, row). A `Some(Value)` here means:
    /// show the computed display form when not editing; a missing entry
    /// means render the cell's raw text.
    pub computed_cells: &'a std::collections::HashMap<
        (BlockId, u32, u32),
        acord_core::interp::Value,
    >,
}

/// Structural commands that mutate a block. The editor routes a `BlockCommand`
/// to a specific block based on the active focus / selection rather than
/// dispatching kind-specific messages directly.
#[derive(Debug, Clone)]
pub enum BlockCommand {
    // Table commands
    InsertRowAbove(usize),
    InsertRowBelow(usize),
    DeleteRow(usize),
    InsertColLeft(usize),
    InsertColRight(usize),
    DeleteCol(usize),
    SetCellValue { row: usize, col: usize, value: String },
    ResizeCol { col: usize, width: f32 },
    ResizeRow { row: usize, height: f32 },
    MoveCol { from: usize, to: usize },
    MoveRow { from: usize, to: usize },
    // Heading commands
    SetHeadingLevel(u8),
    SetHeadingText(String),
    // Generic
    SelectAll,
    Clear,
}

/// The protocol every block kind implements.
///
/// Generic over `Message` so concrete impls can plug in the editor's message
/// type without `block.rs` taking a hard dependency on `editor.rs`. The trait
/// stays dyn-compatible because the generic is at the trait level (not on
/// individual methods) — `Box<dyn Block<crate::editor::Message>>` works.
pub trait Block<Message> {
    fn id(&self) -> BlockId;
    fn kind_tag(&self) -> &'static str;

    /// Document-relative line where this block begins. Maintained by
    /// `recount_lines` after any structural mutation.
    fn start_line(&self) -> usize;
    fn set_start_line(&mut self, line: usize);

    /// Line count this block contributes to the document. For text blocks
    /// this is the editor `Content::line_count()`; for tables it's
    /// `rows.len() + 1` (header + separator + data); fixed at 1 for
    /// headings, HRs, and trees.
    fn line_count(&self) -> usize;

    /// True if this block was produced by an eval `/=` table result rather
    /// than parsed from markdown. Drives read-only behaviour and skips
    /// markdown serialization. Defaults to false; only `TableBlock` overrides.
    fn is_eval_result(&self) -> bool {
        false
    }

    /// Downcast hooks for the editor to access kind-specific fields
    /// (`TextBlock::content`, `TableBlock::rows`, ...) from a `Box<dyn Block>`.
    /// Every concrete impl just returns `self` — these are required because
    /// `Block` is generic over `Message` so we can't use Rust's trait
    /// upcasting to `Any` directly.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Render the block. `ctx` carries selection / focus / theme / font as
    /// shared state. Returns layered output rather than a single element so
    /// the document compositor can merge overlays from multiple blocks into
    /// shared layers.
    ///
    /// `ctx`'s lifetime is independent from the returned `LayeredView`'s `'a`
    /// (which is tied to `&self`). Implementations must read what they need
    /// from `ctx` eagerly into Copy locals — they may NOT capture `ctx` into
    /// the returned element. This is what lets the editor's `view_blocks` build
    /// a fresh per-iteration `ViewCtx` on the stack and return elements that
    /// outlive the loop.
    fn view<'a>(&'a self, ctx: &ViewCtx<'_, Message>) -> LayeredView<'a, Message>;

    /// Markdown serialization. Rich side-channel state (col widths, row
    /// heights, cell formulas) is serialized separately into the embedded
    /// sidecar archive — this method returns plain markdown only.
    fn to_md(&self) -> String;

    /// Cursor coordinate (in this block's local space) -> innermost selectable
    /// path. Returns `None` if the point doesn't hit anything selectable.
    fn hit_test(&self, point: Point) -> Option<InnerPath>;

    /// Apply a structural mutation. Unsupported commands are silently ignored.
    fn apply(&mut self, cmd: BlockCommand);

    /// Iterate (NOT recurse) over all selectable inner paths in this block.
    /// MUST be iterative — when nesting lands this gets exercised on deep trees.
    fn selectable_paths(&self) -> Box<dyn Iterator<Item = InnerPath> + '_>;
}

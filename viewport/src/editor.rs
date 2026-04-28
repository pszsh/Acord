use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use iced_wgpu::core::keyboard::{self, Modifiers};
use iced_wgpu::core::keyboard::key;
use iced_wgpu::core::text::{Highlight, Wrapping};
use iced_wgpu::core::{
    border, padding, alignment, Background, Border, Color, Element, Font, Length,
    Padding, Point, Shadow, Theme,
};
use iced_widget::container;
use iced_widget::markdown;
use iced_widget::MouseArea;
use crate::text_widget::{self, Action, AnchoredItem, Binding, Cursor, KeyPress, Motion, Position, Status};
use iced_widget::text_input;
use iced_wgpu::core::text::highlighter::Format;
use iced_wgpu::core::widget::Id as WidgetId;

use crate::block::{Block as BlockTrait, ViewCtx};
use crate::blocks::{self, BoxedBlock};
use crate::heading_block::HeadingBlock;
use crate::hr_block::HrBlock;
use crate::oklab;
use crate::palette;
use crate::sidecar::{self, Sidecar, TableSidecar};
use crate::syntax::{self, SyntaxHighlighter, SyntaxSettings, compute_line_decors};
use crate::table_block::{self, TableBlock, TableMessage};
use crate::text_block::TextBlock;
use crate::tree_block::TreeBlock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Blocks rendered, eval runs, tables interactive.
    Live,
    /// Raw markdown in a single text_editor, no eval, no block splitting.
    Editor,
    /// Read-only rendered view. Press `i` for Editor, `/` for Live.
    View,
}

/// User-facing line-number gutter / cursorline behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineIndicator {
    /// Absolute line numbers, full-row cursorline band.
    On,
    /// Hidden — no line numbers, no cursorline band. The gutter strip
    /// stays at its layout width so the editor doesn't reflow.
    Off,
    /// Vim-style: relative line numbers (cursor line shows its absolute
    /// number, others show signed distance), cursorline band on.
    Vim,
}

impl LineIndicator {
    pub fn from_str(s: &str) -> Self {
        match s {
            "off" => LineIndicator::Off,
            "vim" => LineIndicator::Vim,
            _ => LineIndicator::On,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Message {
    BlockAction(usize, text_widget::Action),
    FocusBlock(usize),
    EditorAction(text_widget::Action),
    TogglePreview,
    MarkdownLink(markdown::Uri),
    InsertTable,
    ToggleBold,
    ToggleItalic,
    ToggleStrike,
    ToggleUnderline,
    ToggleBlockquote,
    /// Wrap the selection in matching delimiters; if the selection is
    /// already wrapped (markers immediately surround it, with or without
    /// being included in the selection), unwrap it.
    WrapWith(&'static str, &'static str),
    /// Insert a paired `[]` / `{}` and place the cursor between them.
    /// Only applied to `[` and `{`; quotes/parens deliberately do NOT pair
    /// on type — use Cmd+"/'/9 to wrap a selection.
    AutoPair(&'static str, &'static str),
    /// Cmd+0: incremental scope exit. Each press closes the innermost
    /// unclosed pair within the current block; once everything is closed,
    /// jumps the cursor past the next outer scope's closing delimiter; once
    /// fully at block scope, ensures a "newline sandwich" (cursor on a
    /// blank line with one blank line of padding above and below).
    FixUp,
    Evaluate,
    /// Full-document ordered eval: every module evaluated in sequence.
    EvalAll,
    SmartEval,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Undo,
    Redo,
    ToggleFind,
    HideFind,
    FindQueryChanged(String),
    FindNext,
    FindPrev,
    ReplaceQueryChanged(String),
    ReplaceOne,
    ReplaceAll,
    TableMsg(usize, TableMessage),
    DeleteCurrentTable,
    FocusedTableOp(TableMessage),
    TableTab,
    TableShiftTab,
    TableEnter,
    /// Up arrow on the top row of a table. Find the text block immediately
    /// above and focus its end; synthesize a fresh text block if none exists.
    EscapeTableUp(usize),
    /// Down arrow on the last row of a table. Mirror of `EscapeTableUp`.
    EscapeTableDown(usize),
    /// Move the focused cell up by one row, staying inside the same table.
    TableMoveUp,
    /// Move the focused cell down by one row, staying inside the same table.
    TableMoveDown,
    /// Move the focused cell left by one column.
    TableMoveLeft,
    /// Move the focused cell right by one column.
    TableMoveRight,
    /// Backspace / Delete on a selected (not editing) cell. Empties the cell
    /// without removing the row — Excel/Numbers semantics.
    ClearSelectedCell,
    /// Second Cmd+A press — escalate to whole-document selection. Every block
    /// renders highlighted; Backspace clears all content; Cmd+Backspace wipes
    /// the document down to a single empty text block.
    SelectAllBlocks,
    /// Plain Backspace/Delete with `all_blocks_selected == true`. Empties
    /// every block's content but keeps the structure (block count, block
    /// types, table row/col counts).
    ClearAllBlocks,
    /// Cmd+Backspace with `all_blocks_selected == true`. Wipes the document
    /// down to a single empty text block.
    DeleteAllBlocks,
    /// Right-click on a table cell. Opens the context menu anchored at the
    /// current cursor position. Only block_idx is needed — the menu acts on
    /// the existing selection, not on the right-clicked cell.
    ShowContextMenu { block_idx: usize },
    /// Explicitly close the context menu (Escape key, etc.). Most other
    /// messages auto-close it via `update()`'s top-of-loop drop logic.
    HideContextMenu,
    /// Push a literal string into the clipboard out-channel. Used by the
    /// table spillover popup's copy button and by Cmd+C-on-selected-cell
    /// where the value is already in hand at dispatch time.
    CopyLiteral(String),
    /// Cmd+C while the focused block is a table — copy the current selection
    /// (or the spillover cell, if open) as TSV. Dispatched from handle.rs
    /// when the keyboard event would otherwise reach a non-cell-edit context.
    CopyFocusedTableSelection,
    /// Escape from cell edit mode. The cell stays selected (highlighted) but
    /// goes back to the static-text rendering — same as the Excel/Numbers
    /// gesture for "stop editing this cell".
    ExitCellEdit,
    /// User pressed a printable character with a cell selected but not yet
    /// editing. Replace the cell's content with that single character and
    /// enter edit mode — Excel/Numbers "start typing into the selection".
    EnterCellEditWithChar(char),
    /// Tab key inside a text block. iced's default `Binding::from_key_press`
    /// returns None for Tab, so without our own binding the key does nothing.
    IndentTab,
    OutdentTab,
    SetRenderMode(RenderMode),
    /// Mouse pressed on an inline `/=` result. Starts the long-press timer.
    InlineResultPress { block_id: crate::selection::BlockId, after_line: usize },
    /// Mouse released anywhere after pressing on an inline result. Cancels
    /// any pending long-press that hasn't fired yet.
    InlineResultRelease,
    /// Double-clicked an inline `/=` result. Copies the source line + result
    /// to clipboard AND drops a `let  = result` template two lines down.
    InlineResultDoubleClick { block_id: crate::selection::BlockId, after_line: usize },
}

pub const RESULT_PREFIX: &str = "→ ";

/// Long-press / double-click state for the click-and-hold-on-result gesture.
#[derive(Debug, Clone)]
pub struct InlinePressState {
    pub block_id: crate::selection::BlockId,
    pub after_line: usize,
    pub started_at: Instant,
    pub fired_long_press: bool,
}

const LONG_PRESS_MS: u128 = 300;

pub const ERROR_PREFIX: &str = "⚠ ";

const EVAL_DEBOUNCE_MS: u128 = 300;

// ── Document layers ─────────────────────────────────────────────────
// Layer 0 = registry + layout (user-authored structure).
// Layers 1-3 hold computed eval artifacts, independently invalidated.

/// Attachment point linking a computed item to a layer-0 text block.
#[derive(Debug, Clone)]
pub struct Anchor {
    pub block_id: crate::selection::BlockId,
    pub after_line: usize,
}

/// Layer 1: inline eval result (→ value / ⚠ error).
#[derive(Debug, Clone)]
pub struct InlineResult {
    pub anchor: Anchor,
    pub text: String,
    pub is_error: bool,
}

impl InlineResult {
    pub fn element_height(&self, line_h: f32) -> f32 { line_h }
}

/// Layer 2: computed table from `/=|` evaluation.
#[derive(Debug, Clone)]
pub struct ComputedTable {
    pub anchor: Anchor,
    pub rows: Vec<Vec<String>>,
    pub col_widths: Vec<f32>,
}

impl ComputedTable {
    pub fn element_height(&self, line_h: f32) -> f32 {
        let row_h = line_h + 4.0;
        let outer_pad = 4.0 + 4.0;
        self.rows.len().max(1) as f32 * row_h + outer_pad
    }
}

/// Layer 3: computed tree from `/=\` evaluation.
#[derive(Debug, Clone)]
pub struct ComputedTree {
    pub anchor: Anchor,
    pub data: serde_json::Value,
}

impl ComputedTree {
    pub fn element_height(&self, font_size: f32) -> f32 {
        crate::tree_block::element_height(&self.data, font_size)
    }
}

/// Layer 4: embedded image from `![alt](src)`.
#[derive(Debug, Clone)]
pub struct ComputedImage {
    pub anchor: Anchor,
    pub src: String,
    pub alt: String,
    /// Pre-computed display height based on image aspect ratio and editor
    /// width. Falls back to a placeholder height while loading.
    pub display_height: f32,
}

/// Cached image data keyed by source path/URL. Handle must be built once
/// and reused — `Handle::from_bytes` mints a fresh Id every call.
pub struct ImageCacheEntry {
    pub handle: iced_widget::image::Handle,
    pub width: u32,
    pub height: u32,
}

const IMAGE_PLACEHOLDER_H: f32 = 24.0;
const IMAGE_MAX_H: f32 = 600.0;
const IMAGE_PADDING: f32 = 48.0;
const IMAGE_VPAD: f32 = 4.0;

/// Ref to a layer item for interleaved rendering.
enum LayerItem<'a> {
    Inline(&'a InlineResult),
    Table(&'a ComputedTable),
    Tree(&'a ComputedTree),
    Image(&'a ComputedImage),
}

impl LayerItem<'_> {
    fn element_height(&self, line_h: f32, font_size: f32) -> f32 {
        match self {
            Self::Inline(r) => r.element_height(line_h),
            Self::Table(t) => t.element_height(line_h),
            Self::Tree(t) => t.element_height(font_size),
            Self::Image(img) => img.display_height + IMAGE_VPAD * 2.0,
        }
    }
}

pub const FIND_INPUT_ID: &str = "find_input";
pub const REPLACE_INPUT_ID: &str = "replace_input";
/// Stable id for the multi-block document scrollable. handle.rs targets this
/// via `iced_core::widget::operation::scrollable::scroll_by` to forward
/// wheel-scroll deltas captured by an inner `text_editor` (which would
/// otherwise swallow them when the cursor is over the editor's bounds).
pub const DOC_SCROLLABLE_ID: &str = "doc_scrollable";
const UNDO_MAX: usize = 200;
const COALESCE_MS: u128 = 500;

struct UndoSnapshot {
    text: String,
    cursor_line: usize,
    cursor_col: usize,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum EditKind {
    Insert,
    Backspace,
    Delete,
    Enter,
    Paste,
    Other,
}

pub struct FindState {
    pub visible: bool,
    pub query: String,
    pub replacement: String,
    pub matches: Vec<(usize, usize)>,
    pub current: usize,
}

impl FindState {
    fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            replacement: String::new(),
            matches: Vec::new(),
            current: 0,
        }
    }
}

pub struct EditorState {
    pub registry: HashMap<crate::selection::BlockId, BoxedBlock>,
    pub layout: Vec<crate::selection::BlockId>,
    pub modules: Vec<crate::module::Module>,
    pub focused_block: usize,
    pub font_size: f32,
    pub preview: bool,
    pub render_mode: RenderMode,
    pub parsed: Vec<markdown::Item>,
    pub lang: Option<String>,
    scroll_offset: f32,
    eval_dirty: bool,
    last_edit: Instant,

    undo_stack: Vec<UndoSnapshot>,
    redo_stack: Vec<UndoSnapshot>,
    last_edit_kind: EditKind,
    last_edit_time: Instant,

    pub find: FindState,
    pub pending_focus: Option<WidgetId>,

    /// Stand-in `Content` returned by `content()` when the focused block isn't
    /// text-bearing AND there are no text blocks anywhere in the document
    /// (e.g. a heading-only file). Always empty; never written to.
    fallback_text: text_widget::Content,

    /// Live keyboard modifier state. Updated by handle.rs from
    /// `Event::Keyboard(ModifiersChanged)`. Drives modifier-aware click
    /// translation (Cmd → Toggle, Shift → Extend, etc.).
    pub mods: Modifiers,

    /// Single source of truth for selection. Mirrored from `focused_block`
    /// changes via `set_focused_block`; the compositor reads this for
    /// cursorline / cell tint / cross-block range visuals.
    pub(crate) selection: crate::selection::Selection,
    /// The path keys are routed to. A single point even when `selection` is a
    /// range or set.
    pub(crate) focus: Option<crate::selection::NodePath>,
    /// Path currently in text-input edit mode (cell static-vs-edit).
    #[allow(dead_code)]
    pub(crate) editing: Option<crate::selection::NodePath>,
    /// Cmd+A escalation flag. Set after a first Cmd+A (block-local select);
    /// a second Cmd+A while still armed escalates to whole-document
    /// selection. Cleared on any other input. handle.rs owns the
    /// arm/disarm logic; the editor only reads it.
    pub cmd_a_armed: bool,
    /// Whole-document selection mode — every block renders highlighted,
    /// plain Backspace clears all block content, Cmd+Backspace wipes the
    /// document. Set by `Message::SelectAllBlocks`, cleared by any click
    /// or any single-block selection change.
    pub all_blocks_selected: bool,
    /// Latest mouse cursor position in viewport coordinates. handle.rs
    /// updates this from `handle.cursor` BEFORE draining messages, so the
    /// `Message::TableMsg(_, ContextMenu)` handler can read the position
    /// to anchor the context menu overlay.
    pub cursor_pos: Point,
    /// Pending pixel scroll delta to apply to the document scrollable on
    /// the next render frame. Captured here when iced's `text_editor`
    /// swallows a wheel-scroll event (it captures `Action::Scroll` when
    /// the cursor is over the editor's bounds), and forwarded to the outer
    /// scrollable via `iced_core::widget::operation::scrollable::scroll_by`
    /// in handle.rs::render. Accumulates if multiple scroll events land
    /// in the same frame.
    pub pending_scroll: f32,
    /// Active context menu, if any. Set by right-clicking a cell;
    /// auto-cleared by `update()` whenever a message arrives that isn't
    /// itself a context-menu operation. So clicking a menu button
    /// dispatches the action AND clears the menu in one shot, and clicking
    /// anywhere outside the menu also dismisses it.
    pub context_menu: Option<ContextMenuState>,

    // ── Document layers (computed eval artifacts) ──
    pub eval_results: Vec<InlineResult>,
    pub computed_tables: Vec<ComputedTable>,
    pub computed_trees: Vec<ComputedTree>,
    /// Per-cell evaluated formula results. Keyed by (table block id, col, row).
    /// Cells whose raw text starts with `/=` and are not being edited render
    /// the computed value instead; anything not in this map renders raw.
    pub computed_cells: HashMap<(crate::selection::BlockId, u32, u32), acord_core::interp::Value>,

    /// Active long-press / pending-result-gesture state. Set by
    /// `InlineResultPress`, cleared by `InlineResultRelease` /
    /// `InlineResultDoubleClick`. `tick()` checks the elapsed time to fire
    /// the copy when it crosses `LONG_PRESS_MS`.
    pub inline_press: Option<InlinePressState>,

    /// Line-indicator preference: controls cursorline band + relative-vs-
    /// absolute line numbers. Pushed in from Swift via FFI.
    pub line_indicator: LineIndicator,
    /// Whether the gutter line numbers cycle through the rainbow palette
    /// based on distance from the cursor. Independent of `line_indicator`.
    pub gutter_rainbow: bool,

    /// Cross-platform clipboard out-channel. Editor logic writes here;
    /// the shell drains it after each frame via `viewport_take_clipboard`
    /// and pushes the text to the system clipboard.
    pub pending_clipboard: Option<String>,

    // ── Images ──
    pub computed_images: Vec<ComputedImage>,
    pub image_cache: HashMap<String, ImageCacheEntry>,

    /// Previous global cursor line (block start_line + intra-block line).
    /// Used by `tick()` to detect cursor-line changes and trigger eval.
    prev_cursor_line: usize,
}

/// Per-eval table name→id bookkeeping. `keys` is every alias a table is
/// reachable by (heading name, positional `table_N`, qualified `mod::name`);
/// `canonical` is the preferred key for each BlockId, used as the
/// `current_table` anchor when evaluating formulas inside that table.
pub struct TableIndex {
    pub keys: HashMap<String, crate::selection::BlockId>,
    pub canonical: HashMap<crate::selection::BlockId, String>,
}

/// Mirror of `Interpreter::resolve_table_key` for use during dep-graph
/// building, when we don't have the live interpreter handy but do have
/// the full alias→id map from `register_visible_tables`.
fn resolve_ref_key(
    r: &acord_core::interp::FormulaRef,
    table_index: &TableIndex,
) -> Option<String> {
    match &r.block {
        Some(b) => {
            let k = format!("{}::{}", b.to_lowercase(), r.table.to_lowercase());
            if table_index.keys.contains_key(&k) { Some(k) } else { None }
        }
        None => {
            let bare = r.table.to_lowercase();
            if table_index.keys.contains_key(&bare) { Some(bare) } else { None }
        }
    }
}

/// State for the on-screen context menu overlay. Anchored at viewport
/// (x, y) — the position the user right-clicked. Carries the table's
/// block index so menu items targeting "this table" know which one.
/// Notably does NOT carry the right-clicked row/col — right-click is
/// purely a menu trigger and doesn't make the clicked cell "current."
#[derive(Debug, Clone)]
pub struct ContextMenuState {
    pub block_idx: usize,
    pub x: f32,
    pub y: f32,
}

fn md_style() -> markdown::Style {
    let p = palette::current();
    markdown::Style {
        font: Font::default(),
        inline_code_highlight: Highlight {
            background: p.surface0.into(),
            border: border::rounded(4),
        },
        inline_code_padding: padding::left(2).right(2),
        inline_code_color: p.green,
        inline_code_font: Font::MONOSPACE,
        code_block_font: Font::MONOSPACE,
        link_color: p.blue,
    }
}

impl EditorState {
    pub fn new() -> Self {
        let sample = concat!(
            "# Block Compositor\n",
            "Acord renders structured documents with mixed content.\n\n",
            "## Data Table\n",
            "| Name  | Age | Role     |\n",
            "|-------|-----|----------|\n",
            "| Alice | 30  | Engineer |\n",
            "| Bob   | 25  | Designer |\n",
            "| Carol | 35  | Manager  |\n\n",
            "---\n\n",
            "### Code Section\n",
            "let x = 42\n",
            "/= x * 2\n",
        );
        let block_vec = blocks::parse_blocks(sample, "rust");
        let (registry, layout) = Self::vec_to_registry(block_vec);
        Self {
            registry,
            layout,
            modules: Vec::new(),
            focused_block: 0,
            font_size: 14.0,
            preview: false,
            render_mode: RenderMode::Live,
            parsed: Vec::new(),
            lang: Some("rust".into()),
            scroll_offset: 0.0,
            eval_dirty: false,
            last_edit: Instant::now(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit_kind: EditKind::Other,
            last_edit_time: Instant::now(),
            find: FindState::new(),
            pending_focus: None,
            fallback_text: text_widget::Content::with_text(""),
            mods: Modifiers::default(),
            selection: crate::selection::Selection::None,
            focus: None,
            editing: None,
            cmd_a_armed: false,
            all_blocks_selected: false,
            cursor_pos: Point::ORIGIN,
            context_menu: None,
            pending_scroll: 0.0,
            eval_results: Vec::new(),
            computed_tables: Vec::new(),
            computed_trees: Vec::new(),
            computed_cells: HashMap::new(),
            inline_press: None,
            line_indicator: LineIndicator::On,
            gutter_rainbow: true,
            pending_clipboard: None,
            computed_images: Vec::new(),
            image_cache: HashMap::new(),
            prev_cursor_line: 0,
        }
    }

    // ── registry + layout helpers ──────────────────────────────────

    fn vec_to_registry(blocks: Vec<BoxedBlock>) -> (HashMap<crate::selection::BlockId, BoxedBlock>, Vec<crate::selection::BlockId>) {
        let mut registry = HashMap::with_capacity(blocks.len());
        let mut layout = Vec::with_capacity(blocks.len());
        for block in blocks {
            let id = block.id();
            layout.push(id);
            registry.insert(id, block);
        }
        (registry, layout)
    }

    fn registry_to_vec(&mut self) -> Vec<BoxedBlock> {
        self.layout.iter().filter_map(|id| self.registry.remove(id)).collect()
    }

    fn replace_blocks(&mut self, blocks: Vec<BoxedBlock>) {
        self.registry.clear();
        self.layout.clear();
        for block in blocks {
            let id = block.id();
            self.layout.push(id);
            self.registry.insert(id, block);
        }
    }

    fn block_at(&self, idx: usize) -> Option<&BoxedBlock> {
        self.layout.get(idx).and_then(|id| self.registry.get(id))
    }

    fn block_at_mut(&mut self, idx: usize) -> Option<&mut BoxedBlock> {
        self.layout.get(idx).copied().and_then(move |id| self.registry.get_mut(&id))
    }

    fn insert_block(&mut self, idx: usize, block: BoxedBlock) {
        let id = block.id();
        self.layout.insert(idx, id);
        self.registry.insert(id, block);
    }

    fn remove_block(&mut self, idx: usize) -> Option<BoxedBlock> {
        if idx < self.layout.len() {
            let id = self.layout.remove(idx);
            self.registry.remove(&id)
        } else {
            None
        }
    }

    fn push_block(&mut self, block: BoxedBlock) {
        let id = block.id();
        self.layout.push(id);
        self.registry.insert(id, block);
    }

    fn clear_blocks(&mut self) {
        self.layout.clear();
        self.registry.clear();
    }

    fn block_count(&self) -> usize {
        self.layout.len()
    }

    fn recount_block_lines(&mut self) {
        let mut line = 0;
        for &id in &self.layout {
            if let Some(block) = self.registry.get_mut(&id) {
                block.set_start_line(line);
                line += block.line_count();
            }
        }
    }

    // ── Layer helpers ─────────────────────────────────────────────

    fn clear_layers_for_blocks(&mut self, ids: &[crate::selection::BlockId]) {
        self.eval_results.retain(|r| !ids.contains(&r.anchor.block_id));
        self.computed_tables.retain(|t| !ids.contains(&t.anchor.block_id));
        self.computed_trees.retain(|t| !ids.contains(&t.anchor.block_id));
    }

    /// Map a line number in concatenated module source back to a per-block anchor.
    /// `boundaries` is a sorted vec of (cumulative_line_start, block_id).
    fn map_line_to_anchor(
        boundaries: &[(usize, crate::selection::BlockId)],
        global_line: usize,
    ) -> Anchor {
        let mut best_id = boundaries.first().map(|b| b.1).unwrap_or(0);
        let mut best_start = 0;
        for &(start, id) in boundaries {
            if start <= global_line {
                best_id = id;
                best_start = start;
            } else {
                break;
            }
        }
        Anchor {
            block_id: best_id,
            after_line: global_line - best_start,
        }
    }

    /// Scan text blocks for `![alt](src)` image references and populate
    /// `computed_images`. Loads image bytes into `image_cache` on first
    /// encounter (sync for local files). Replaces previous images for the
    /// given block set — unchanged sources keep their cache entry.
    fn scan_images(
        &mut self,
        boundaries: &[(usize, crate::selection::BlockId)],
        block_ids: &[crate::selection::BlockId],
    ) {
        self.computed_images.retain(|img| !block_ids.contains(&img.anchor.block_id));

        let mut new_srcs: Vec<(Anchor, String, String)> = Vec::new();
        for &(_start, block_id) in boundaries {
            let block = match self.registry.get(&block_id) {
                Some(b) => b,
                None => continue,
            };
            let text = if let Some(tb) = block.as_any().downcast_ref::<TextBlock>() {
                tb.content.text()
            } else {
                continue;
            };
            for (line_idx, line) in text.lines().enumerate() {
                if let Some((alt, src)) = parse_image_ref(line) {
                    let anchor = Anchor { block_id, after_line: line_idx };
                    new_srcs.push((anchor, src, alt));
                }
            }
        }

        // Editor width estimate for aspect-ratio scaling.
        let editor_w = 800.0f32; // approximate; TODO: pass actual width

        for (anchor, src, alt) in new_srcs {
            // Load into cache if absent.
            if !self.image_cache.contains_key(&src) {
                if let Some(entry) = load_image_from_path(&src) {
                    self.image_cache.insert(src.clone(), entry);
                }
            }
            let display_height = if let Some(entry) = self.image_cache.get(&src) {
                let max_w = (editor_w - IMAGE_PADDING).max(1.0);
                let scale_w = max_w.min(entry.width as f32);
                let aspect = entry.height as f32 / entry.width.max(1) as f32;
                (scale_w * aspect).min(IMAGE_MAX_H)
            } else {
                IMAGE_PLACEHOLDER_H
            };
            self.computed_images.push(ComputedImage {
                anchor,
                src,
                alt,
                display_height,
            });
        }
    }

    fn block_index_at_line(&self, global_line: usize) -> Option<usize> {
        for (i, &id) in self.layout.iter().enumerate() {
            if let Some(block) = self.registry.get(&id) {
                let start = block.start_line();
                let end = start + block.line_count();
                if global_line >= start && global_line < end {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Update the focused block index AND mirror it into the central
    /// `selection` / `focus` fields. Clears any active cell edit mode —
    /// changing the focused block always exits whatever cell was being edited.
    /// Also drops any per-table whole-table selection, since focus is moving.
    fn set_focused_block(&mut self, idx: usize) {
        self.focused_block = idx;
        self.editing = None;
        for block in self.registry.values_mut() {
            if let Some(tb) = block.as_any_mut().downcast_mut::<TableBlock>() {
                tb.table_selected = false;
            }
        }
        if let Some(block) = self.block_at(idx) {
            let path = crate::selection::NodePath::block(block.id());
            self.selection = crate::selection::Selection::Caret(path.clone());
            self.focus = Some(path);
        }
    }

    /// Mark a specific cell of a table block as selected (highlighted but not
    /// in edit mode). Clears any active edit. Used by single-click and by
    /// Escape-from-edit.
    fn set_selected_cell(&mut self, idx: usize, row: usize, col: usize) {
        self.focused_block = idx;
        self.editing = None;
        if let Some(block) = self.block_at(idx) {
            let path = crate::selection::NodePath::cell(block.id(), row, col);
            self.selection = crate::selection::Selection::Caret(path.clone());
            self.focus = Some(path);
        }
    }

    /// Mark a specific cell as in edit mode (renders as text_input + takes
    /// iced focus). Used by double-click, by printable-key entry from
    /// selection, and by Tab/Enter navigation that wants to keep editing
    /// rolling forward into the next cell.
    fn set_editing_cell(&mut self, idx: usize, row: usize, col: usize) {
        self.focused_block = idx;
        let bid = self.block_at(idx).map(|b| b.id());
        if let Some(bid) = bid {
            let path = crate::selection::NodePath::cell(bid, row, col);
            self.editing = Some(path.clone());
            self.selection = crate::selection::Selection::Caret(path.clone());
            self.focus = Some(path);
            self.pending_focus = Some(table_block::cell_id(bid, row, col));
        }
    }

    /// Up arrow on the top row of a table at `table_idx`. If the immediately-
    /// previous block is a text block, focus its end. Otherwise insert a fresh
    /// empty text block just before the table and focus it.
    fn escape_table_up(&mut self, table_idx: usize) {
        if table_idx > 0 {
            if let Some(tb) = self.text_block_at(table_idx - 1) {
                let block_id = tb.id;
                let last_line = tb.content.line_count().saturating_sub(1);
                let last_col = tb
                    .content
                    .line(last_line)
                    .map(|l| l.text.len())
                    .unwrap_or(0);
                self.set_focused_block(table_idx - 1);
                self.pending_focus = Some(block_editor_id(block_id));
                self.safe_move_to(Cursor {
                    position: Position { line: last_line, column: last_col },
                    selection: None,
                });
                return;
            }
        }
        // No text block immediately above — synthesize one.
        self.push_undo_snapshot();
        let lang = self.lang_str();
        let new_id = blocks::next_id();
        let new_block: BoxedBlock = Box::new(TextBlock::new(new_id, "", 0, lang));
        let insert_at = table_idx.min(self.block_count());
        self.insert_block(insert_at, new_block);
        self.recount_block_lines();
        self.set_focused_block(insert_at);
        self.pending_focus = Some(block_editor_id(new_id));
        self.reparse();
    }

    /// Down arrow on the last row of a table at `table_idx`. Mirror of
    /// `escape_table_up`: focus the immediately-following text block if any,
    /// otherwise synthesize a fresh empty text block right after the table.
    fn escape_table_down(&mut self, table_idx: usize) {
        let next_idx = table_idx + 1;
        if next_idx < self.block_count() {
            if let Some(tb) = self.text_block_at(next_idx) {
                let block_id = tb.id;
                self.set_focused_block(next_idx);
                self.pending_focus = Some(block_editor_id(block_id));
                self.safe_move_to(Cursor {
                    position: Position { line: 0, column: 0 },
                    selection: None,
                });
                return;
            }
        }
        self.push_undo_snapshot();
        let lang = self.lang_str();
        let new_id = blocks::next_id();
        let new_block: BoxedBlock = Box::new(TextBlock::new(new_id, "", 0, lang));
        let insert_at = next_idx.min(self.block_count());
        self.insert_block(insert_at, new_block);
        self.recount_block_lines();
        self.set_focused_block(insert_at);
        self.pending_focus = Some(block_editor_id(new_id));
        self.reparse();
    }

    fn lang_str(&self) -> String {
        self.lang.clone().unwrap_or_default()
    }

    /// Tab width in spaces. Per-language for now defaults to 4 (matches Python
    /// and most house-styles); a lookup table can be added when other languages
    /// want narrower indents.
    fn tab_width(&self) -> usize {
        4
    }

    fn text_block_at(&self, idx: usize) -> Option<&TextBlock> {
        self.block_at(idx).and_then(|b| b.as_any().downcast_ref::<TextBlock>())
    }

    fn text_block_at_mut(&mut self, idx: usize) -> Option<&mut TextBlock> {
        self.block_at_mut(idx)
            .and_then(|b| b.as_any_mut().downcast_mut::<TextBlock>())
    }

    fn table_block_at(&self, idx: usize) -> Option<&TableBlock> {
        self.block_at(idx).and_then(|b| b.as_any().downcast_ref::<TableBlock>())
    }

    fn table_block_at_mut(&mut self, idx: usize) -> Option<&mut TableBlock> {
        self.block_at_mut(idx)
            .and_then(|b| b.as_any_mut().downcast_mut::<TableBlock>())
    }

    fn first_text_block_index(&self) -> Option<usize> {
        self.layout.iter().enumerate().find_map(|(i, id)| {
            self.registry.get(id).and_then(|b| {
                if b.as_any().is::<TextBlock>() { Some(i) } else { None }
            })
        })
    }

    fn content(&self) -> &text_widget::Content {
        if let Some(tb) = self.text_block_at(self.focused_block) {
            return &tb.content;
        }
        if let Some(idx) = self.first_text_block_index() {
            if let Some(tb) = self.text_block_at(idx) {
                return &tb.content;
            }
        }
        &self.fallback_text
    }

    fn content_mut(&mut self) -> &mut text_widget::Content {
        let target = if self
            .block_at(self.focused_block)
            .map(|b| b.as_any().is::<TextBlock>())
            .unwrap_or(false)
        {
            Some(self.focused_block)
        } else {
            self.first_text_block_index()
        };
        if let Some(idx) = target {
            let id = self.layout[idx];
            return &mut self
                .registry.get_mut(&id).unwrap()
                .as_any_mut()
                .downcast_mut::<TextBlock>()
                .unwrap()
                .content;
        }
        &mut self.fallback_text
    }

    fn full_text(&self) -> String {
        let mut parts = Vec::new();
        for &id in &self.layout {
            if let Some(block) = self.registry.get(&id) {
                let md = block.to_md();
                if block.kind_tag() == "text" || !md.is_empty() {
                    parts.push(md);
                }
            }
        }
        parts.join("\n")
    }

    fn line_height(&self) -> f32 {
        self.font_size * 1.3
    }

    /// Move the focused content's cursor to `target`, clamping line and column
    /// into the current text so we never hand cosmic-text an out-of-bounds index.
    /// Defends against an iced text_editor bug where `Content::move_to` passes
    /// the caller's `column` straight to cosmic-text without validation.
    fn safe_move_to(&mut self, mut cursor: Cursor) {
        {
            let content = self.content();
            let line_count = content.line_count();
            if line_count == 0 {
                cursor.position.line = 0;
                cursor.position.column = 0;
            } else {
                if cursor.position.line >= line_count {
                    cursor.position.line = line_count - 1;
                }
                let line_len = content
                    .line(cursor.position.line)
                    .map(|l| l.text.len())
                    .unwrap_or(0);
                if cursor.position.column > line_len {
                    cursor.position.column = line_len;
                }
            }
            if let Some(sel) = cursor.selection.as_mut() {
                if line_count == 0 {
                    sel.line = 0;
                    sel.column = 0;
                } else {
                    if sel.line >= line_count {
                        sel.line = line_count - 1;
                    }
                    let sel_line_len = content
                        .line(sel.line)
                        .map(|l| l.text.len())
                        .unwrap_or(0);
                    if sel.column > sel_line_len {
                        sel.column = sel_line_len;
                    }
                }
            }
        }
        self.content_mut().move_to(cursor);
    }

    /// Handle arrow/backspace/delete at block boundaries.
    /// Returns true if the action was consumed (focus change or merge).
    fn handle_block_boundary(&mut self, action: &text_widget::Action) -> bool {
        let idx = self.focused_block;
        if !self.text_block_at(idx).is_some() {
            return false;
        }

        match action {
            Action::Move(Motion::Up) | Action::Select(Motion::Up) => {
                let cursor = self.content().cursor();
                if cursor.position.line == 0 && idx > 0 {
                    self.set_focused_block(idx - 1);
                    return true;
                }
            }
            Action::Move(Motion::Down) | Action::Select(Motion::Down) => {
                let cursor = self.content().cursor();
                let line_count = self.content().line_count();
                if cursor.position.line + 1 >= line_count && idx + 1 < self.block_count() {
                    self.set_focused_block(idx + 1);
                    return true;
                }
            }
            Action::Edit(text_widget::Edit::Backspace) => {
                let cursor = self.content().cursor();
                if cursor.position.line == 0 && cursor.position.column == 0 && cursor.selection.is_none() {
                    if idx > 0 {
                        return self.merge_with_previous(idx);
                    }
                }
            }
            Action::Edit(text_widget::Edit::Delete) => {
                let cursor = self.content().cursor();
                let line_count = self.content().line_count();
                let last_line = line_count.saturating_sub(1);
                let last_line_text = self.content().line(last_line)
                    .map(|l| l.text.len())
                    .unwrap_or(0);
                if cursor.position.line == last_line
                    && cursor.position.column >= last_line_text
                    && cursor.selection.is_none()
                {
                    if idx + 1 < self.block_count() {
                        return self.merge_with_next(idx);
                    }
                }
            }
            _ => {}
        }
        false
    }

    fn merge_text_pair(first: &str, second: &str) -> String {
        if first.is_empty() {
            second.to_string()
        } else if second.is_empty() {
            first.to_string()
        } else {
            format!("{}\n{}", first, second)
        }
    }

    fn merge_with_previous(&mut self, idx: usize) -> bool {
        if idx == 0 {
            return false;
        }
        let prev_idx = idx - 1;
        if !self.text_block_at(prev_idx).is_some() {
            // Previous is non-text (HR, heading) -- remove it instead
            self.remove_block(prev_idx);
            let new_focus = prev_idx.min(self.block_count().saturating_sub(1));
            self.set_focused_block(new_focus);
            self.recount_block_lines();
            return true;
        }
        let prev_text = self
            .text_block_at(prev_idx)
            .map(|tb| tb.content.text())
            .unwrap_or_default();
        let cur_text = self
            .text_block_at(idx)
            .map(|tb| tb.content.text())
            .unwrap_or_default();
        let merged = Self::merge_text_pair(&prev_text, &cur_text);
        let prev_line_count = self
            .text_block_at(prev_idx)
            .map(|tb| tb.content.line_count())
            .unwrap_or(1);
        if let Some(tb) = self.text_block_at_mut(prev_idx) {
            tb.content = text_widget::Content::with_text(&merged);
        }
        self.remove_block(idx);
        self.set_focused_block(prev_idx);
        self.safe_move_to(Cursor {
            position: Position { line: prev_line_count.saturating_sub(1), column: 0 },
            selection: None,
        });
        self.recount_block_lines();
        true
    }

    fn merge_with_next(&mut self, idx: usize) -> bool {
        let next_idx = idx + 1;
        if next_idx >= self.block_count() {
            return false;
        }
        if !self.text_block_at(next_idx).is_some() {
            self.remove_block(next_idx);
            self.recount_block_lines();
            return true;
        }
        let cur_text = self
            .text_block_at(idx)
            .map(|tb| tb.content.text())
            .unwrap_or_default();
        let next_text = self
            .text_block_at(next_idx)
            .map(|tb| tb.content.text())
            .unwrap_or_default();
        let merged = Self::merge_text_pair(&cur_text, &next_text);
        if let Some(tb) = self.text_block_at_mut(idx) {
            tb.content = text_widget::Content::with_text(&merged);
        }
        self.remove_block(next_idx);
        self.recount_block_lines();
        true
    }

    /// Load a document from raw file bytes. Pulls any embedded sidecar
    /// archive out of the markdown, sets the text body, then applies the
    /// sidecar metadata to the parsed table blocks. Used by the FFI
    /// `viewport_set_text` entrypoint so callers don't have to know the
    /// archive format exists.
    pub fn load_doc(&mut self, text: &str) {
        // In editor mode, loading text should preserve the single-block state.
        // The save→observe→setText round-trip calls load_doc; if we reparse
        // here we'd silently exit editor mode and corrupt the block structure.
        if self.render_mode == RenderMode::Editor {
            let loaded = sidecar::extract_archive(text);
            let clean = strip_result_lines(&loaded.markdown);
            self.set_text(&clean);
            return;
        }
        let loaded = sidecar::extract_archive(text);
        let clean = strip_result_lines(&loaded.markdown);
        self.set_text(&clean);
        self.eval_results.clear();
        self.computed_tables.clear();
        self.computed_trees.clear();
        if let Some(sc) = loaded.sidecar {
            self.apply_sidecar(&sc);
        }
        // Trigger full eval when loading into Live or View mode.
        if self.render_mode == RenderMode::Live || self.render_mode == RenderMode::View {
            self.run_eval_all();
        }
    }

    /// Save the document to raw file bytes: assign sidecar ids to any tables
    /// that need them, build the sidecar from current block metadata, and
    /// embed it as a base64 zip in an HTML comment at the end of the file.
    /// Tables with no rich metadata produce no archive — the file stays a
    /// pure plain `.md`. Used by the FFI `viewport_get_text` entrypoint.
    pub fn save_doc(&mut self) -> String {
        let body = self.get_clean_text();
        // build_block_files reads self.modules; make sure it reflects the
        // current document. rebuild_modules is idempotent and cheap.
        self.rebuild_modules();
        let sidecar = self.build_sidecar();
        let block_files = self.build_block_files();
        sidecar::embed_archive(&body, &sidecar, &block_files)
    }

    /// Build the per-block source files for the archive. One `.cord` file
    /// per logical block — a logical block is a group of parser spans
    /// bounded by H1/H2 headings or `---` (the same grouping `self.modules`
    /// already maintains for `use`-resolution). Tables and prose are not
    /// boundaries; they live inside whichever block contains them.
    ///
    /// Filename: heading-led blocks get `<snake_name>.cord`; HR-led
    /// (anonymous) blocks get `block_N.cord` where N is the positional
    /// index across all logical blocks (0-based). Collisions get `_2`,
    /// `_3`, … suffixes.
    pub fn build_block_files(&self) -> Vec<sidecar::BlockFile> {
        use std::collections::HashSet;
        let mut files = Vec::with_capacity(self.modules.len());
        let mut used: HashSet<String> = HashSet::new();

        for (index, module) in self.modules.iter().enumerate() {
            let mut source_parts: Vec<String> = Vec::with_capacity(module.block_ids.len());
            let mut title = String::new();
            for &bid in &module.block_ids {
                let Some(block) = self.registry.get(&bid) else { continue };
                if title.is_empty() {
                    if let Some(hb) = block.as_any().downcast_ref::<HeadingBlock>() {
                        title = hb.text.clone();
                    }
                }
                source_parts.push(block.to_md());
            }
            let source = source_parts.join("\n");

            let kind = if module.heading_block.is_some() { "section" } else { "anonymous" };
            let filename = self.unique_cord_filename(&module.name, index, &mut used);
            let content = format!(
                "---\nkind = \"{}\"\nindex = {}\ntitle = \"{}\"\n---\n{}",
                kind,
                index,
                title.replace('\\', "\\\\").replace('"', "\\\""),
                source
            );
            files.push(sidecar::BlockFile { filename, content });
        }
        files
    }

    fn unique_cord_filename(
        &self,
        module_name: &str,
        index: usize,
        used: &mut std::collections::HashSet<String>,
    ) -> String {
        let base = if module_name.is_empty() {
            format!("block_{}", index)
        } else {
            module_name.to_string()
        };
        let mut candidate = format!("{}.cord", base);
        let mut n = 2;
        while used.contains(&candidate) {
            candidate = format!("{}_{}.cord", base, n);
            n += 1;
        }
        used.insert(candidate.clone());
        candidate
    }

    /// Build a `Sidecar` snapshot from the current block tree, keyed by the
    /// positional index of each non-eval table in layout order ("0", "1", ...).
    /// Only tables with persistent metadata produce entries.
    fn build_sidecar(&self) -> Sidecar {
        let mut sc = Sidecar::default();
        sc.version = 1;
        let mut position: usize = 0;
        for block_id in &self.layout {
            let Some(block) = self.registry.get(block_id) else { continue };
            let Some(tb) = block.as_any().downcast_ref::<TableBlock>() else { continue };
            if tb.is_eval_result {
                continue;
            }
            let entry = if tb.has_persistent_metadata() {
                let mut entry = TableSidecar::default();
                entry.col_widths = tb.col_widths.clone();
                for (row_idx, h) in tb.row_heights.iter().enumerate() {
                    if let Some(height) = h {
                        entry.row_heights.insert(row_idx.to_string(), *height);
                    }
                }
                for (r, row) in tb.rows.iter().enumerate() {
                    for (c, cell) in row.iter().enumerate() {
                        if cell.trim_start().starts_with("/=") {
                            let addr = acord_core::interp::display_addr(c as u32, r as u32);
                            entry.formulas.insert(addr, cell.clone());
                        }
                    }
                }
                Some(entry)
            } else {
                None
            };
            if let Some(entry) = entry {
                sc.tables.insert(position.to_string(), entry);
            }
            position += 1;
        }
        sc
    }

    /// Apply a previously-loaded `Sidecar` to the current block tree, matching
    /// entries to tables by positional index in layout order. Non-eval tables
    /// count; eval-result tables are skipped. Missing entries leave tables
    /// unchanged.
    fn apply_sidecar(&mut self, sc: &Sidecar) {
        let mut position: usize = 0;
        let layout = self.layout.clone();
        for block_id in &layout {
            let Some(block) = self.registry.get_mut(block_id) else { continue };
            let Some(tb) = block.as_any_mut().downcast_mut::<TableBlock>() else { continue };
            if tb.is_eval_result {
                continue;
            }
            if let Some(entry) = sc.tables.get(&position.to_string()) {
                for (i, w) in entry.col_widths.iter().enumerate() {
                    if i < tb.col_widths.len() {
                        tb.col_widths[i] = *w;
                    }
                }
                for (key, height) in &entry.row_heights {
                    if let Ok(row_idx) = key.parse::<usize>() {
                        if tb.row_heights.len() <= row_idx {
                            tb.row_heights.resize(row_idx + 1, None);
                        }
                        tb.row_heights[row_idx] = Some(*height);
                    }
                }
                for (addr, raw) in &entry.formulas {
                    if let Some((col, row)) = acord_core::interp::parse_cell_address(addr) {
                        let (r, c) = (row as usize, col as usize);
                        while tb.rows.len() <= r { tb.rows.push(Vec::new()); }
                        let target_cols = (c + 1).max(tb.col_widths.len());
                        while tb.col_widths.len() < target_cols { tb.col_widths.push(120.0); }
                        while tb.row_heights.len() < tb.rows.len() { tb.row_heights.push(None); }
                        while tb.rows[r].len() <= c { tb.rows[r].push(String::new()); }
                        tb.rows[r][c] = raw.clone();
                    }
                }
            }
            position += 1;
        }
    }

    pub fn set_text(&mut self, text: &str) {
        // Snapshot undo before any wholesale text replacement so undo can
        // recover the prior state. Identity-skip when nothing actually
        // changes — Swift's observe loop can call set_text with the text we
        // just emitted, and we don't want round-trips piling up phantom undos.
        let current = self.get_clean_text();
        if current != text {
            self.push_undo_snapshot();
            self.last_edit_kind = EditKind::Other;
        }
        self.replace_text_no_undo(text);
    }

    /// Wholesale text replacement WITHOUT pushing an undo snapshot. Used by
    /// `restore_snapshot` (the undo/redo path) where touching the undo stack
    /// would loop.
    fn replace_text_no_undo(&mut self, text: &str) {
        // In editor mode, the document is a single raw text block. Don't
        // reparse into structured blocks — that would silently exit editor
        // mode while the render_mode flag still says Editor.
        if self.render_mode == RenderMode::Editor {
            let lang = self.lang_str();
            self.clear_blocks();
            self.push_block(Box::new(TextBlock::new(blocks::next_id(), text, 0, lang)));
            self.recount_block_lines();
            self.set_focused_block(0);
            return;
        }
        let lang = self.lang_str();
        if self.layout.is_empty() {
            self.replace_blocks(blocks::parse_blocks(text, &lang));
        } else {
            let mut block_vec = self.registry_to_vec();
            blocks::reparse_incremental(&mut block_vec, text, &lang);
            self.replace_blocks(block_vec);
        }
        if self.focused_block >= self.block_count() {
            self.set_focused_block(0);
        }
        self.scroll_offset = 0.0;
        self.reparse();
    }

    /// Per-frame focus sync. Walks tables and sets `is_active`/`focused_cell`
    /// based on iced's currently-focused widget id. `focused_cell` is preserved
    /// across blur (keyboard shortcuts need it); `is_active` flips off every
    /// frame and only flips on for the table whose cell matches.
    pub fn sync_focused_cell(&mut self, focused_id: Option<&WidgetId>) {
        for block in self.registry.values_mut() {
            if let Some(tb) = block.as_any_mut().downcast_mut::<TableBlock>() {
                tb.is_active = false;
            }
        }
        let Some(target_id) = focused_id else { return };
        for block in self.registry.values_mut() {
            let Some(tb) = block.as_any_mut().downcast_mut::<TableBlock>() else { continue };
            if tb.is_eval_result {
                continue;
            }
            let bid = tb.id;
            let rows = tb.rows.len();
            let cols = tb.col_widths.len();
            let mut hit: Option<(usize, usize)> = None;
            for r in 0..rows {
                for c in 0..cols {
                    let candidate = table_block::cell_id(bid, r, c);
                    if candidate == *target_id {
                        hit = Some((r, c));
                        break;
                    }
                }
                if hit.is_some() {
                    break;
                }
            }
            if let Some(rc) = hit {
                tb.focused_cell = Some(rc);
                tb.is_active = true;
                return;
            }
        }
    }

    /// A non-eval table currently has a selected cell. Used by handle.rs to
    /// gate keyboard interception of arrow keys, Tab, Enter, Backspace etc.
    /// Keys off `focused_cell` (logical selection, preserved across blur),
    /// NOT `is_active` (which only tracks whether iced widget focus is in the
    /// cell text_input — true only during edit mode).
    pub(crate) fn active_table_index(&self) -> Option<usize> {
        self.focused_table_index()
    }

    /// True iff the editor's *currently focused block* is a non-eval table
    /// that has a selected cell. This is the right gate for any table-specific
    /// keybinding — `focused_table_index()` would also return Some for a
    /// table whose selection was set on a previous click but where focus has
    /// since moved to a text block, which would cause the table to silently
    /// steal arrow keys / Backspace / Cmd+Backspace from the text block.
    pub(crate) fn table_is_focused_block(&self) -> bool {
        if let Some(block) = self.block_at(self.focused_block) {
            if let Some(tb) = block.as_any().downcast_ref::<TableBlock>() {
                return !tb.is_eval_result && tb.focused_cell.is_some();
            }
        }
        false
    }

    /// True iff the focused block is a table currently in whole-table
    /// select-all mode. handle.rs uses this to route plain Backspace to
    /// "clear all cells" and Cmd+Backspace to "delete the entire table."
    pub(crate) fn focused_table_is_select_all(&self) -> bool {
        if let Some(block) = self.block_at(self.focused_block) {
            if let Some(tb) = block.as_any().downcast_ref::<TableBlock>() {
                return !tb.is_eval_result && tb.table_selected;
            }
        }
        false
    }

    /// Returns (block_idx, row, total_rows) for the currently active table's
    /// focused cell, or None if no table is active. handle.rs uses the
    /// total_rows to detect "Down arrow on the last row" for edge-escape.
    pub(crate) fn active_table_focused_row(&self) -> Option<(usize, usize, usize)> {
        let idx = self.active_table_index()?;
        let tb = self.table_block_at(idx)?;
        let (r, _c) = tb.focused_cell?;
        Some((idx, r, tb.rows.len()))
    }

    /// Returns the index of the editor's currently focused block IF it's a
    /// table with a selected cell. Returns None if focus is on a text block,
    /// heading, etc. — even if some other table somewhere in the document
    /// has `focused_cell` set (which is common since `focused_cell` is
    /// preserved across blur so users can click back into a prior table and
    /// have their selection restored).
    ///
    /// All table-targeted operations (DeleteCurrentTable, FocusedTableOp,
    /// TableTab/Enter/Move*, EnterCellEditWithChar, ClearSelectedCell) use
    /// this — so they all consistently mean "the table the user is in right
    /// now," not "the topmost table that has ever been touched."
    pub(crate) fn focused_table_index(&self) -> Option<usize> {
        let block = self.block_at(self.focused_block)?;
        let tb = block.as_any().downcast_ref::<TableBlock>()?;
        if !tb.is_eval_result && tb.focused_cell.is_some() {
            Some(self.focused_block)
        } else {
            None
        }
    }

    /// True iff the editor's *currently focused block* is a table with a
    /// selected cell that's not in edit mode. handle.rs uses this to decide
    /// whether to intercept printable keys for "type to enter edit mode."
    ///
    /// MUST check `focused_block` rather than "any table has focused_cell" —
    /// `focused_cell` is intentionally preserved across blur so clicking
    /// back into a table restores selection, which means it can't double as
    /// a "currently active" signal.
    pub(crate) fn has_selected_cell_not_editing(&self) -> bool {
        if self.editing.is_some() {
            return false;
        }
        let Some(block) = self.block_at(self.focused_block) else {
            return false;
        };
        let Some(tb) = block.as_any().downcast_ref::<TableBlock>() else {
            return false;
        };
        !tb.is_eval_result && tb.focused_cell.is_some()
    }

    /// True when handle.rs should intercept Cmd+C and route it to the
    /// table-cell copy path instead of letting iced's text widget handle it.
    /// Conditions: focused block is a table; not currently editing a cell
    /// (cell-edit mode delegates to text_input's own copy); and either a
    /// selection is non-empty or a spillover popup is open.
    pub(crate) fn should_intercept_table_copy(&self) -> bool {
        if self.editing.is_some() { return false; }
        let Some(block) = self.block_at(self.focused_block) else { return false; };
        let Some(tb) = block.as_any().downcast_ref::<TableBlock>() else { return false; };
        !tb.selection.is_empty() || tb.spillover.is_some()
    }

    /// Build the clipboard payload from the focused table — selection takes
    /// precedence over spillover; spillover provides the single-cell payload
    /// when no explicit selection exists. None if neither applies.
    fn copy_focused_table_selection(&self) -> Option<String> {
        let block = self.block_at(self.focused_block)?;
        let tb = block.as_any().downcast_ref::<TableBlock>()?;
        if !tb.selection.is_empty() {
            return tb.copy_selection_payload();
        }
        let (r, c) = tb.spillover?;
        tb.rows.get(r).and_then(|row| row.get(c)).cloned()
    }

    pub fn set_lang_from_ext(&mut self, ext: &str) {
        self.lang = lang_from_extension(ext);
    }

    pub fn tick(&mut self) {
        if self.render_mode != RenderMode::Live { return; }
        if self.eval_dirty && self.last_edit.elapsed().as_millis() >= EVAL_DEBOUNCE_MS {
            self.eval_dirty = false;
            self.run_eval();
        }
        // Cursor-line-change trigger: when the cursor moves to a different
        // line (arrow keys, click, etc.) without an edit, re-evaluate.
        {
            let block_start = self.layout.get(self.focused_block)
                .and_then(|id| self.registry.get(id))
                .map(|b| b.start_line())
                .unwrap_or(0);
            let intra = self.content().cursor().position.line;
            let global_line = block_start + intra;
            if global_line != self.prev_cursor_line {
                self.prev_cursor_line = global_line;
                if !self.eval_dirty {
                    self.run_eval();
                }
            }
        }
        // Fire the long-press copy at the threshold — if the user is still
        // holding past LONG_PRESS_MS without having released, double-clicked,
        // or moved off, drop the result onto the clipboard.
        let due = self.inline_press.as_ref().is_some_and(|s| {
            !s.fired_long_press && s.started_at.elapsed().as_millis() >= LONG_PRESS_MS
        });
        if due {
            if let Some(s) = self.inline_press.as_mut() {
                s.fired_long_press = true;
                let bid = s.block_id;
                let line = s.after_line;
                self.copy_inline_result(bid, line);
            }
        }
        // Table hover-to-spillover dwell: each table polls its own armed
        // timer and opens the popup once the 3s threshold passes.
        let block_ids: Vec<crate::selection::BlockId> = self.layout.clone();
        for id in block_ids {
            if let Some(block) = self.registry.get_mut(&id) {
                if let Some(tb) = block.as_any_mut().downcast_mut::<TableBlock>() {
                    tb.check_hover_spillover();
                }
            }
        }
    }

    /// True if an eval debounce is still pending. Used by handle::render to keep
    /// the vsync loop ticking through the debounce window even when no new input
    /// is arriving, so tick() eventually fires run_eval.
    pub fn has_pending_eval(&self) -> bool {
        self.eval_dirty
            || self.inline_press.as_ref().is_some_and(|s| !s.fired_long_press)
            || self.layout.iter().any(|id| {
                self.registry.get(id)
                    .and_then(|b| b.as_any().downcast_ref::<TableBlock>())
                    .is_some_and(|tb| tb.has_pending_hover())
            })
    }

    fn reparse(&mut self) {
        let text = self.get_clean_text();
        self.parsed = markdown::parse(&text).collect();
        self.rebuild_modules();
    }

    /// Build the BlockInfo slice used by module/table detection.
    /// Shared between `rebuild_modules` and `register_visible_tables`.
    fn build_block_infos(&self) -> Vec<crate::module::BlockInfo> {
        use crate::heading_block::HeadingBlock;
        use crate::module::BlockInfo;
        self.layout.iter().filter_map(|&id| {
            let block = self.registry.get(&id)?;
            let tag = block.kind_tag();
            let (heading_level, heading_text) = if let Some(hb) = block.as_any().downcast_ref::<HeadingBlock>() {
                (hb.level.as_u8(), hb.text.clone())
            } else {
                (0, String::new())
            };
            let text_content = if tag == "text" { block.to_md() } else { String::new() };
            Some(BlockInfo { id, kind_tag: tag, heading_level, heading_text, text_content })
        }).collect()
    }

    /// Rebuild the module list and apply table naming from headings.
    fn rebuild_modules(&mut self) {
        use crate::module::{compute_modules, detect_table_names};

        let infos = self.build_block_infos();
        self.modules = compute_modules(&infos);

        let names = detect_table_names(&infos);
        for assignment in names {
            if let Some(block) = self.registry.get_mut(&assignment.table_id) {
                if let Some(tb) = block.as_any_mut().downcast_mut::<TableBlock>() {
                    tb.table_name = Some(assignment.name);
                }
            }
        }
    }

    /// Register every non-eval-result table in the document on the
    /// interpreter under all names it's reachable by from the focused
    /// block's module. Also sets `current_block` on the interp so bare
    /// H4 refs resolve correctly.
    fn register_visible_tables(
        &self,
        interp: &mut acord_core::interp::Interpreter,
        focused_block_idx: usize,
    ) -> TableIndex {
        use crate::module::{
            compute_positional_ids, detect_table_names, normalize_name, TableNameScope,
        };

        let infos = self.build_block_infos();
        let table_names = detect_table_names(&infos);
        let pos_ids = compute_positional_ids(&infos);

        let mut block_to_module: HashMap<crate::selection::BlockId, String> = HashMap::new();
        for m in &self.modules {
            for &bid in &m.block_ids {
                block_to_module.insert(bid, m.name.clone());
            }
        }

        let focused_id = self.layout.get(focused_block_idx).copied();
        let focused_module_name = focused_id.and_then(|id| block_to_module.get(&id).cloned());
        interp.set_current_block(focused_module_name.as_deref());

        let mut keys_map: HashMap<String, crate::selection::BlockId> = HashMap::new();
        let mut canonical: HashMap<crate::selection::BlockId, String> = HashMap::new();

        for (table_id, pos_name, _pos_block_pos) in &pos_ids.tables {
            let Some(block) = self.registry.get(table_id) else { continue };
            let Some(tb) = block.as_any().downcast_ref::<TableBlock>() else { continue };
            if tb.is_eval_result { continue; }
            let rows = tb.rows.clone();

            let heading = table_names.iter().find(|a| a.table_id == *table_id);
            let module_name = block_to_module.get(table_id).cloned();

            // Canonical key (used as `current_table` anchor when evaluating
            // formulas inside this table): heading name when global/present,
            // `module::heading` for H4, positional as final fallback.
            let canonical_key = match heading {
                Some(h) => {
                    let hname = normalize_name(&h.name);
                    match h.scope {
                        TableNameScope::Global => hname,
                        TableNameScope::BlockScoped => {
                            if let Some(ref m) = module_name {
                                format!("{}::{}", m, hname)
                            } else {
                                hname
                            }
                        }
                    }
                }
                None => pos_name.to_lowercase(),
            };
            canonical.insert(*table_id, canonical_key.clone());

            // Build the full set of keys this table is reachable by.
            let mut keys: Vec<String> = vec![pos_name.to_lowercase(), canonical_key.clone()];
            if let Some(h) = heading {
                let hname = normalize_name(&h.name);
                if h.scope == TableNameScope::BlockScoped {
                    // Also expose bare heading for refs FROM inside the
                    // owning module (resolve_table_key_fallback also handles
                    // this, but registering explicitly avoids the fallback
                    // hop and disambiguates collisions between modules).
                    if module_name.as_deref() == focused_module_name.as_deref() {
                        keys.push(hname);
                    }
                }
            }
            if let Some(ref m) = module_name {
                keys.push(format!("{}::{}", m, pos_name.to_lowercase()));
            }

            keys.sort();
            keys.dedup();
            for k in &keys {
                interp.register_table(k, rows.clone());
                keys_map.insert(k.clone(), *table_id);
            }
        }

        TableIndex { keys: keys_map, canonical }
    }

    /// True if any non-eval-result table in the document has at least one
    /// cell whose text starts with `/=`. Used to early-out `run_eval` when
    /// neither text blocks nor tables have anything to evaluate.
    fn any_visible_cell_formulas(&self) -> bool {
        for block in self.registry.values() {
            if let Some(tb) = block.as_any().downcast_ref::<TableBlock>() {
                if tb.is_eval_result { continue; }
                if tb.rows.iter().any(|row| row.iter().any(|c| c.trim_start().starts_with("/="))) {
                    return true;
                }
            }
        }
        false
    }

    /// Parse, topo-sort, and evaluate every cell formula across visible
    /// tables. Results land in `self.computed_cells`; cycles yield
    /// `Value::Error("cycle")`. Also threads computed values back into
    /// `interp`'s table registry so subsequent text-block reads see the
    /// formula result rather than the raw `/=...` string.
    fn evaluate_cell_formulas(
        &mut self,
        interp: &mut acord_core::interp::Interpreter,
        table_index: &TableIndex,
    ) {
        use acord_core::interp::{parse_formula_with_spice, ParsedFormula, Value};

        struct Cell {
            table_key: String,
            col: u32,
            row: u32,
            block_id: crate::selection::BlockId,
            ast: ParsedFormula,
        }

        let mut formulas: Vec<Cell> = Vec::new();
        let mut parse_errors: Vec<(crate::selection::BlockId, u32, u32, String)> = Vec::new();

        let mut seen_blocks: std::collections::HashSet<crate::selection::BlockId> =
            std::collections::HashSet::new();
        for (_, &block_id) in &table_index.keys {
            if !seen_blocks.insert(block_id) { continue; }
            let Some(block) = self.registry.get(&block_id) else { continue };
            let Some(tb) = block.as_any().downcast_ref::<TableBlock>() else { continue };
            let canonical = match table_index.canonical.get(&block_id) {
                Some(k) => k.clone(),
                None => continue,
            };
            for (r, row) in tb.rows.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    let trimmed = cell.trim_start();
                    let Some(body) = trimmed.strip_prefix("/=") else { continue };
                    // The interpreter's spice flag reflects any `use spice`
                    // already executed in the code blocks for this module.
                    // Formulas inside tables inherit that flag.
                    match parse_formula_with_spice(body, interp.spice_enabled()) {
                        Ok(ast) => formulas.push(Cell {
                            table_key: canonical.clone(),
                            col: c as u32,
                            row: r as u32,
                            block_id,
                            ast,
                        }),
                        Err(e) => parse_errors.push((block_id, c as u32, r as u32, e)),
                    }
                }
            }
        }

        // Clear prior computed values for visible tables only — tables
        // outside the focused module's scope keep their stale results so
        // their cells don't flash blank between cross-module evals.
        self.computed_cells.retain(|k, _| !seen_blocks.contains(&k.0));

        for (bid, c, r, e) in parse_errors {
            self.computed_cells.insert((bid, c, r), Value::Error(format!("parse: {}", e)));
        }

        if formulas.is_empty() {
            return;
        }

        // Build dep graph. Node i is formulas[i]. Edge dep_idx → i means
        // formula i reads the cell that formula dep_idx computes — so
        // dep_idx must evaluate first.
        let node_key: HashMap<(String, u32, u32), usize> = formulas.iter().enumerate()
            .map(|(i, f)| ((f.table_key.clone(), f.col, f.row), i))
            .collect();
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); formulas.len()];
        let mut in_degree: Vec<usize> = vec![0; formulas.len()];

        for (i, f) in formulas.iter().enumerate() {
            let refs = f.ast.refs(&f.table_key);
            for r in refs {
                let resolved = resolve_ref_key(&r, table_index);
                if let Some(key) = resolved {
                    if let Some(&dep) = node_key.get(&(key, r.cell.0, r.cell.1)) {
                        if dep != i {
                            edges[dep].push(i);
                            in_degree[i] += 1;
                        }
                    }
                }
            }
        }

        let mut queue: std::collections::VecDeque<usize> = in_degree.iter().enumerate()
            .filter_map(|(i, &d)| if d == 0 { Some(i) } else { None })
            .collect();
        let mut order: Vec<usize> = Vec::new();
        while let Some(i) = queue.pop_front() {
            order.push(i);
            let next = std::mem::take(&mut edges[i]);
            for j in next {
                in_degree[j] -= 1;
                if in_degree[j] == 0 {
                    queue.push_back(j);
                }
            }
        }

        let ordered: std::collections::HashSet<usize> = order.iter().copied().collect();

        for i in &order {
            let f = &formulas[*i];
            interp.set_current_table(Some(&f.table_key));
            let result = match interp.eval_formula(&f.ast) {
                Ok(v) => v,
                Err(e) => Value::Error(e),
            };
            interp.set_current_table(None);

            // Thread the computed value back into the interpreter's table
            // registry so subsequent formulas AND text-block reads see it
            // instead of the raw `/=...` string.
            if !result.is_error() {
                let display = result.display();
                // Write into every alias of this table.
                for (alias_key, &bid) in &table_index.keys {
                    if bid == f.block_id {
                        interp.write_cell_raw(alias_key, f.col, f.row, &display);
                    }
                }
            }
            self.computed_cells.insert((f.block_id, f.col, f.row), result);
        }

        for i in 0..formulas.len() {
            if ordered.contains(&i) { continue; }
            let f = &formulas[i];
            self.computed_cells.insert((f.block_id, f.col, f.row), Value::Error("cycle".into()));
        }
    }

    /// Apply cell writes logged by the interpreter to the live TableBlocks.
    /// Writes land in `rows[r][c]` and grow the table as needed (strict
    /// bounds would discourage using formulas to populate empty cells).
    fn apply_table_writes(
        &mut self,
        writes: Vec<acord_core::interp::TableWrite>,
        table_index: &TableIndex,
    ) {
        for w in writes {
            let Some(&block_id) = table_index.keys.get(&w.table_key) else { continue };
            let Some(block) = self.registry.get_mut(&block_id) else { continue };
            let Some(tb) = block.as_any_mut().downcast_mut::<TableBlock>() else { continue };
            let (c, r) = (w.cell.0 as usize, w.cell.1 as usize);
            while tb.rows.len() <= r { tb.rows.push(Vec::new()); }
            let target_cols = (c + 1).max(tb.col_widths.len());
            while tb.col_widths.len() < target_cols { tb.col_widths.push(120.0); }
            while tb.row_heights.len() < tb.rows.len() { tb.row_heights.push(None); }
            while tb.rows[r].len() <= c { tb.rows[r].push(String::new()); }
            tb.rows[r][c] = w.value;
        }
    }

    /// Check if block structure changed after an edit. Serializes current
    /// blocks, re-parses, applies an incremental diff, then re-seats the
    /// focused block index against the post-reparse layout.
    fn check_block_structure(&mut self) {
        let cursor = self.content().cursor();
        let full = self.full_text();
        let lang = self.lang_str();
        let old_count = self.block_count();
        {
            let mut block_vec = self.registry_to_vec();
            blocks::reparse_incremental(&mut block_vec, &full, &lang);
            self.replace_blocks(block_vec);
        }
        if self.focused_block >= self.block_count() {
            self.set_focused_block(self.block_count().saturating_sub(1));
        }
        if self.block_count() != old_count {
            if let Some(bi) = self.block_index_at_line(cursor.position.line) {
                self.set_focused_block(bi);
            }
        }
        self.rebuild_modules();
    }

    /// Wrap a selection in matching delimiters or unwrap an existing pair.
    /// Used by Cmd+B (`**`), Cmd+I (`*`), Cmd+~ (`~~`), Cmd+", etc.
    ///
    /// Unwrap detection looks at characters IMMEDIATELY outside the
    /// selection, not just inside it — so the selection can be the inner
    /// text (without markers) and Cmd+B still toggles off.
    ///
    /// Star-marker parity rule: bold (`**`) unwraps when the surrounding
    /// star count on each side is >= 2 AND even (2 → 0, 4 → 2, …).
    /// Italic (`*`) unwraps when the count is odd (1, 3, 5 …). This
    /// keeps `**bold**` + Cmd+I → wraps to `***bold***` (bold-italic),
    /// not destructive; and `***both***` + Cmd+B → `*both*` (strips bold).
    fn toggle_wrap(&mut self, open: &str, close: &str) {
        let text = self.content().text();
        let cursor = self.content().cursor();
        let pos = byte_offset_for_cursor(&text, &cursor.position);
        let (start, end) = match self.selection_byte_range(&text, pos) {
            Some(range) => range,
            None => {
                // No selection: insert paired markers and park cursor between.
                let s = format!("{open}{close}");
                self.content_mut().perform(text_widget::Action::Edit(
                    text_widget::Edit::Paste(Arc::new(s)),
                ));
                for _ in 0..close.chars().count() {
                    self.content_mut().perform(text_widget::Action::Move(Motion::Left));
                }
                self.reparse();
                return;
            }
        };

        let selected = &text[start..end];
        let before = &text[..start];
        let after = &text[end..];

        let star_marker = open.chars().all(|c| c == '*') && close == open;
        if star_marker {
            let mlen = open.len();
            // Sym-strip when markers are inside the selection itself.
            if selected.starts_with(open) && selected.ends_with(close) && selected.len() >= mlen * 2 {
                let inner = &selected[mlen..selected.len() - mlen];
                self.content_mut().perform(text_widget::Action::Edit(
                    text_widget::Edit::Paste(Arc::new(inner.to_string())),
                ));
                self.reparse();
                return;
            }
            let outer = count_trailing_char(before, '*').min(count_leading_char(after, '*'));
            let should_unwrap = match mlen {
                2 => outer >= 2 && outer % 2 == 0, // bold
                1 => outer >= 1 && outer % 2 == 1, // italic
                _ => outer >= mlen,
            };
            if should_unwrap {
                self.replace_range(start - mlen, end + mlen, selected);
                return;
            }
        } else {
            // Non-star markers: simple symmetric strip.
            let olen = open.len();
            let clen = close.len();
            if selected.starts_with(open) && selected.ends_with(close) && selected.len() >= olen + clen {
                let inner = &selected[olen..selected.len() - clen];
                self.content_mut().perform(text_widget::Action::Edit(
                    text_widget::Edit::Paste(Arc::new(inner.to_string())),
                ));
                self.reparse();
                return;
            }
            if before.ends_with(open) && after.starts_with(close) {
                self.replace_range(start - olen, end + clen, selected);
                return;
            }
        }

        // Default: wrap.
        let wrapped = format!("{open}{selected}{close}");
        self.content_mut().perform(text_widget::Action::Edit(
            text_widget::Edit::Paste(Arc::new(wrapped)),
        ));
        self.reparse();
    }

    /// Replace a byte range in the current content with `replacement`. Used
    /// by toggle_wrap's unwrap path so we can rewrite text that sits OUTSIDE
    /// the selection (the surrounding markers).
    fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        let text = self.content().text();
        if start > end || end > text.len() { return; }
        let mut new_text = String::with_capacity(text.len() - (end - start) + replacement.len());
        new_text.push_str(&text[..start]);
        new_text.push_str(replacement);
        new_text.push_str(&text[end..]);
        // Rebuild the content with the new text and place cursor at end of
        // replacement so successive toggles continue to operate at the
        // same logical spot.
        let cursor_byte = start + replacement.len();
        self.content_mut().perform(text_widget::Action::Move(Motion::DocumentStart));
        self.content_mut().perform(text_widget::Action::Select(Motion::DocumentEnd));
        self.content_mut().perform(text_widget::Action::Edit(
            text_widget::Edit::Paste(Arc::new(new_text.clone())),
        ));
        // Position cursor at byte offset cursor_byte by walking from start.
        let target = line_col_for_byte(&new_text, cursor_byte);
        self.content_mut().perform(text_widget::Action::Move(Motion::DocumentStart));
        for _ in 0..target.0 {
            self.content_mut().perform(text_widget::Action::Move(Motion::Down));
        }
        self.content_mut().perform(text_widget::Action::Move(Motion::Home));
        for _ in 0..target.1 {
            self.content_mut().perform(text_widget::Action::Move(Motion::Right));
        }
        self.reparse();
    }

    /// Compute the byte range of the current selection (start, end) or None
    /// when no selection is active.
    fn selection_byte_range(&self, text: &str, _cursor_pos: usize) -> Option<(usize, usize)> {
        let sel = self.content().selection()?;
        // We need the start position; use cursor + selection length to
        // bracket. Selection is the text between selection-start and cursor;
        // search both directions in the buffer to find a unique location.
        // For toggle_wrap's purposes, we use the cursor position as the END
        // and walk back by sel.len() to find start. This is correct when
        // selection extends backward from the cursor; otherwise we fall
        // back to a forward search.
        let cursor = self.content().cursor();
        let cursor_byte = byte_offset_for_cursor(text, &cursor.position);
        let len = sel.len();
        // Try cursor at end of selection.
        if cursor_byte >= len && &text[cursor_byte - len..cursor_byte] == sel.as_str() {
            return Some((cursor_byte - len, cursor_byte));
        }
        // Try cursor at start of selection.
        if cursor_byte + len <= text.len() && &text[cursor_byte..cursor_byte + len] == sel.as_str() {
            return Some((cursor_byte, cursor_byte + len));
        }
        // Fall back to searching the doc.
        text.find(sel.as_str()).map(|s| (s, s + len))
    }

    /// Insert paired delimiters at the cursor and place the caret between
    /// them. Used for `[` → `[|]` and `{` → `{|}`. Quotes/parens are
    /// deliberately NOT auto-paired.
    fn auto_pair(&mut self, open: &str, close: &str) {
        let combined = format!("{open}{close}");
        self.content_mut().perform(text_widget::Action::Edit(
            text_widget::Edit::Paste(Arc::new(combined)),
        ));
        for _ in 0..close.chars().count() {
            self.content_mut().perform(text_widget::Action::Move(Motion::Left));
        }
    }

    /// Toggle blockquote prefix on the current line(s). With a selection
    /// spanning multiple lines, prefix `> ` to each; if every line already
    /// has `> `, strip it.
    fn toggle_blockquote(&mut self) {
        let text = self.content().text();
        let cursor = self.content().cursor();
        let lines: Vec<&str> = text.lines().collect();
        let cur_line = cursor.position.line.min(lines.len().saturating_sub(1));
        // Single-line toggle: simplest meaningful form.
        if cur_line >= lines.len() { return; }
        let line = lines[cur_line];
        let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        if let Some(rest) = line.strip_prefix("> ") {
            new_lines[cur_line] = rest.to_string();
        } else {
            new_lines[cur_line] = format!("> {line}");
        }
        let new_text = new_lines.join("\n");
        self.content_mut().perform(text_widget::Action::Move(Motion::DocumentStart));
        self.content_mut().perform(text_widget::Action::Select(Motion::DocumentEnd));
        self.content_mut().perform(text_widget::Action::Edit(
            text_widget::Edit::Paste(Arc::new(new_text)),
        ));
        self.reparse();
    }

    /// Cmd+0 catch-all. See `Message::FixUp` for the spec.
    fn fix_up(&mut self) {
        let text = self.content().text();
        let cursor = self.content().cursor();
        let pos = byte_offset_for_cursor(&text, &cursor.position);
        // 1. Innermost unclosed delimiter? Close it.
        if let Some(close) = innermost_unclosed_delim(&text[..pos]) {
            self.content_mut().perform(text_widget::Action::Edit(
                text_widget::Edit::Paste(Arc::new(close.to_string())),
            ));
            return;
        }
        // 2. Forward to the next outer scope's closing delimiter and step past it.
        if let Some(jump_to) = next_closing_delim_after(&text, pos) {
            let target = line_col_for_byte(&text, jump_to + 1);
            self.content_mut().perform(text_widget::Action::Move(Motion::DocumentStart));
            for _ in 0..target.0 {
                self.content_mut().perform(text_widget::Action::Move(Motion::Down));
            }
            self.content_mut().perform(text_widget::Action::Move(Motion::Home));
            for _ in 0..target.1 {
                self.content_mut().perform(text_widget::Action::Move(Motion::Right));
            }
            return;
        }
        // 3. At block scope: ensure newline sandwich.
        self.ensure_newline_sandwich();
    }

    /// Move the cursor onto its own line with exactly one blank line of
    /// padding above and below (3 newlines total around the caret), or up
    /// to EOF on either side.
    fn ensure_newline_sandwich(&mut self) {
        let text = self.content().text();
        let cursor = self.content().cursor();
        let pos = byte_offset_for_cursor(&text, &cursor.position);
        // Walk back: collapse trailing whitespace/newlines before pos to "\n\n".
        let mut left = pos;
        while left > 0 {
            let c = text[..left].chars().rev().next().unwrap();
            if c == '\n' || c.is_whitespace() { left -= c.len_utf8(); } else { break; }
        }
        // Walk forward: collapse leading whitespace/newlines after pos to "\n\n".
        let mut right = pos;
        while right < text.len() {
            let c = text[right..].chars().next().unwrap();
            if c == '\n' || c.is_whitespace() { right += c.len_utf8(); } else { break; }
        }
        let prefix = if left == 0 { String::new() } else { "\n\n".to_string() };
        let suffix = if right == text.len() { String::new() } else { "\n\n".to_string() };
        let middle = "\n";
        let new_text = format!("{}{}{}{}{}",
            &text[..left], prefix, middle, suffix, &text[right..]);
        let cursor_byte = left + prefix.len() + middle.len();
        self.content_mut().perform(text_widget::Action::Move(Motion::DocumentStart));
        self.content_mut().perform(text_widget::Action::Select(Motion::DocumentEnd));
        self.content_mut().perform(text_widget::Action::Edit(
            text_widget::Edit::Paste(Arc::new(new_text.clone())),
        ));
        let target = line_col_for_byte(&new_text, cursor_byte);
        self.content_mut().perform(text_widget::Action::Move(Motion::DocumentStart));
        for _ in 0..target.0 {
            self.content_mut().perform(text_widget::Action::Move(Motion::Down));
        }
        self.content_mut().perform(text_widget::Action::Move(Motion::Home));
        for _ in 0..target.1 {
            self.content_mut().perform(text_widget::Action::Move(Motion::Right));
        }
        self.reparse();
    }

    pub fn get_clean_text(&self) -> String {
        self.full_text()
    }

    /// Switch to editor mode: collapse all blocks into a single text block
    /// containing the raw markdown. The single-block view path renders it
    /// as a full-page text editor. Cmd+A then selects all text naturally.
    pub fn enter_editor_mode(&mut self) {
        if self.render_mode == RenderMode::Editor { return; }
        self.push_undo_snapshot();
        let full = self.full_text();
        self.clear_blocks();
        let lang = self.lang_str();
        self.push_block(Box::new(TextBlock::new(blocks::next_id(), &full, 0, lang)));
        self.recount_block_lines();
        self.set_focused_block(0);
        self.render_mode = RenderMode::Editor;
        self.all_blocks_selected = false;
        self.editing = None;
        self.eval_results.clear();
        self.computed_tables.clear();
        self.computed_trees.clear();
        self.computed_cells.clear();
        // Select all text in the single editor so the user can immediately
        // delete or type over it.
        self.content_mut().perform(Action::Move(Motion::DocumentStart));
        self.content_mut().perform(Action::Select(Motion::DocumentEnd));
        if let Some(tb) = self.text_block_at(0) {
            self.pending_focus = Some(block_editor_id(tb.id));
        }
    }

    /// Switch back to live mode: reparse the single text block into
    /// structured blocks (headings, tables, HRs, etc.).
    pub fn exit_editor_mode(&mut self) {
        if self.render_mode != RenderMode::Editor { return; }
        let text = self.content().text();
        let lang = self.lang_str();
        self.replace_blocks(blocks::parse_blocks(&text, &lang));
        self.recount_block_lines();
        if self.focused_block >= self.block_count() {
            self.set_focused_block(0);
        }
        self.render_mode = RenderMode::Live;
        self.reparse();
    }

    /// Switch to view mode: read-only rendered view. Press `i` for Editor,
    /// `/` for Live.
    pub fn enter_view_mode(&mut self) {
        if self.render_mode == RenderMode::View { return; }
        // If coming from editor mode, reparse back to blocks first
        if self.render_mode == RenderMode::Editor {
            let text = self.content().text();
            let lang = self.lang_str();
            self.replace_blocks(blocks::parse_blocks(&text, &lang));
            self.recount_block_lines();
            if self.focused_block >= self.block_count() {
                self.set_focused_block(0);
            }
            self.reparse();
        }
        self.render_mode = RenderMode::View;
    }

    /// Collect the concatenated text of all text blocks within a module.
    fn module_source_text(&self, module: &crate::module::Module) -> String {
        let mut parts = Vec::new();
        for &bid in &module.block_ids {
            if let Some(block) = self.registry.get(&bid) {
                if block.kind_tag() == "text" {
                    parts.push(block.to_md());
                }
            }
        }
        parts.join("\n")
    }

    /// Build an interpreter pre-populated with root module exports and
    /// any `use`'d module exports for the block at `block_idx`.
    fn build_eval_interpreter(&self, block_idx: usize) -> acord_core::interp::Interpreter {
        use acord_core::interp;

        let mut eval_interp = interp::Interpreter::new();
        let block_id = match self.layout.get(block_idx) {
            Some(&id) => id,
            None => return eval_interp,
        };

        // Find which module this block belongs to
        let my_module = self.modules.iter().find(|m| m.block_ids.contains(&block_id));

        // Evaluate and import root module exports (unless this IS the root)
        let is_root = my_module.map(|m| m.is_root).unwrap_or(false);
        if !is_root {
            if let Some(root) = self.modules.iter().find(|m| m.is_root) {
                let root_text = self.module_source_text(root);
                let mut root_interp = interp::Interpreter::new();
                crate::eval::evaluate_document_with_interp(&mut root_interp, &root_text);
                eval_interp.import_all(&root_interp.exports());
            }
        }

        // Find use declarations in all text blocks of this module and import those modules
        let use_block_ids: Vec<crate::selection::BlockId> = my_module
            .map(|m| m.block_ids.clone())
            .unwrap_or_default();
        for &bid in &use_block_ids {
            if let Some(block) = self.registry.get(&bid) {
                if let Some(tb) = block.as_any().downcast_ref::<TextBlock>() {
                    let text = tb.content.text();
                    let use_decls = interp::extract_use_declarations(&text);
                    for decl in &use_decls {
                        if let Some(dep_module) = self.modules.iter().find(|m| m.name == decl.module) {
                            let dep_text = self.module_source_text(dep_module);
                            let mut dep_interp = interp::Interpreter::new();
                            if let Some(root) = self.modules.iter().find(|m| m.is_root) {
                                if !dep_module.is_root {
                                    let root_text = self.module_source_text(root);
                                    let mut root_interp = interp::Interpreter::new();
                                    crate::eval::evaluate_document_with_interp(&mut root_interp, &root_text);
                                    dep_interp.import_all(&root_interp.exports());
                                }
                            }
                            crate::eval::evaluate_document_with_interp(&mut dep_interp, &dep_text);
                            let dep_exports = dep_interp.exports();
                            match &decl.item {
                                None => eval_interp.import_all(&dep_exports),
                                Some(s) if s == "*" => eval_interp.import_all(&dep_exports),
                                Some(item) => { eval_interp.import_item(&dep_exports, item); }
                            }
                        }
                    }
                }
            }
        }

        eval_interp
    }

    fn run_eval(&mut self) {
        self.rebuild_modules();

        // Find which module the focused block belongs to.
        let focused_id = match self.layout.get(self.focused_block) {
            Some(&id) => id,
            None => return,
        };
        let module = match self.modules.iter().find(|m| m.block_ids.contains(&focused_id)) {
            Some(m) => m.clone(),
            None => return,
        };

        // Collect source text from the module's text blocks, tracking block
        // boundaries so eval result line numbers can be mapped back to anchors.
        let mut source_parts: Vec<String> = Vec::new();
        let mut boundaries: Vec<(usize, crate::selection::BlockId)> = Vec::new();
        let mut cumulative = 0usize;
        let mut block_ids: Vec<crate::selection::BlockId> = Vec::new();
        for &bid in &module.block_ids {
            if let Some(block) = self.registry.get(&bid) {
                if block.kind_tag() == "text" {
                    boundaries.push((cumulative, bid));
                    block_ids.push(bid);
                    let text = block.to_md();
                    let lc = text.lines().count().max(1);
                    source_parts.push(text);
                    cumulative += lc;
                }
            }
        }
        let source = source_parts.join("\n");

        // Image scan runs regardless of eval content.
        self.scan_images(&boundaries, &block_ids);

        let has_text_eval = source.lines().any(|l| l.trim_start().starts_with("/="));
        let has_cell_formulas = self.any_visible_cell_formulas();
        if !has_text_eval && !has_cell_formulas {
            self.clear_layers_for_blocks(&block_ids);
            self.computed_cells.clear();
            return;
        }

        let mut interp = self.build_eval_interpreter(self.focused_block);
        let table_keys = self.register_visible_tables(&mut interp, self.focused_block);

        // Phase 1: evaluate cell formulas (reads current raw cell values).
        // Formulas override their cell's registered value in the interpreter
        // so phase 2 text-block reads see computed values, not /=... strings.
        self.evaluate_cell_formulas(&mut interp, &table_keys);

        // Phase 2: evaluate text-block document.
        let doc = crate::eval::evaluate_document_with_interp(&mut interp, &source);

        // Phase 3: apply text-block cell writes back to live TableBlocks.
        let writes = interp.drain_table_writes();
        self.apply_table_writes(writes, &table_keys);

        // Clear previous results for this module's blocks.
        self.clear_layers_for_blocks(&block_ids);

        // Distribute results to the appropriate layers.
        for r in &doc.results {
            let anchor = Self::map_line_to_anchor(&boundaries, r.line);
            if r.format == "table" {
                match serde_json::from_str::<Vec<Vec<String>>>(&r.result) {
                    Ok(rows) if !rows.is_empty() => {
                        let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                        let mut col_widths = vec![120.0f32; col_count];
                        for row in &rows {
                            for (ci, cell) in row.iter().enumerate() {
                                let w = cell.len() as f32 * 8.0 + 16.0;
                                if ci < col_widths.len() && w > col_widths[ci] {
                                    col_widths[ci] = w;
                                }
                            }
                        }
                        self.computed_tables.push(ComputedTable {
                            anchor,
                            rows,
                            col_widths,
                        });
                        continue;
                    }
                    _ => {}
                }
                // Table parse failed — fall through to inline result
                self.eval_results.push(InlineResult {
                    anchor,
                    text: format!("{}{}", RESULT_PREFIX, r.result),
                    is_error: false,
                });
            } else if r.format == "tree" {
                match serde_json::from_str::<serde_json::Value>(&r.result) {
                    Ok(data) => {
                        self.computed_trees.push(ComputedTree { anchor, data });
                    }
                    Err(_) => {
                        self.eval_results.push(InlineResult {
                            anchor,
                            text: format!("{}{}", RESULT_PREFIX, r.result),
                            is_error: false,
                        });
                    }
                }
            } else {
                self.eval_results.push(InlineResult {
                    anchor,
                    text: format!("{}{}", RESULT_PREFIX, r.result),
                    is_error: false,
                });
            }
        }
        for e in &doc.errors {
            let anchor = Self::map_line_to_anchor(&boundaries, e.line);
            self.eval_results.push(InlineResult {
                anchor,
                text: format!("{}{}", ERROR_PREFIX, e.error),
                is_error: true,
            });
        }

    }

    /// Evaluate every module in document order. Each module gets a fresh
    /// interpreter seeded with root exports (and its own `use` imports).
    /// Used by Cmd+R, mode switches to Live/View, and file loads.
    fn run_eval_all(&mut self) {
        self.rebuild_modules();
        // Clear all computed layers up front.
        self.eval_results.clear();
        self.computed_tables.clear();
        self.computed_trees.clear();
        self.computed_cells.clear();

        // Evaluate each module in order by temporarily pointing focused_block
        // at a text block within it, then calling run_eval() which already
        // handles cross-module imports (root exports + `use` declarations).
        let saved = self.focused_block;
        let modules: Vec<crate::module::Module> = self.modules.clone();
        for module in &modules {
            let anchor_idx = module.block_ids.iter()
                .find_map(|bid| self.layout.iter().position(|id| id == bid));
            if let Some(idx) = anchor_idx {
                self.focused_block = idx;
                self.run_eval();
            }
        }
        self.focused_block = saved;
    }

    pub fn take_pending_focus(&mut self) -> Option<WidgetId> {
        self.pending_focus.take()
    }

    /// Drain the accumulated wheel-scroll delta. handle.rs::render calls
    /// this each frame and, if non-zero, runs a `scroll_by` operation
    /// against the document scrollable. Returns None when no scroll has
    /// been queued (so handle.rs can skip the operation entirely on idle
    /// frames).
    pub fn take_pending_scroll(&mut self) -> Option<f32> {
        if self.pending_scroll.abs() < f32::EPSILON {
            self.pending_scroll = 0.0;
            return None;
        }
        let v = self.pending_scroll;
        self.pending_scroll = 0.0;
        Some(v)
    }

    fn snapshot(&self) -> UndoSnapshot {
        let cursor = self.content().cursor();
        UndoSnapshot {
            text: self.get_clean_text(),
            cursor_line: cursor.position.line,
            cursor_col: cursor.position.column,
        }
    }

    fn push_undo_snapshot(&mut self) {
        let snap = self.snapshot();
        self.undo_stack.push(snap);
        if self.undo_stack.len() > UNDO_MAX {
            self.undo_stack.remove(0);
        }
    }

    fn maybe_snapshot(&mut self, kind: EditKind) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_edit_time).as_millis();
        let should_snap = kind != self.last_edit_kind
            || elapsed > COALESCE_MS
            || kind == EditKind::Enter
            || kind == EditKind::Paste;

        if should_snap {
            self.push_undo_snapshot();
        }

        self.last_edit_kind = kind;
        self.last_edit_time = now;
        self.redo_stack.clear();
    }

    fn classify_edit(action: &text_widget::Action) -> Option<EditKind> {
        match action {
            Action::Edit(edit) => match edit {
                text_widget::Edit::Insert(_) => Some(EditKind::Insert),
                text_widget::Edit::Enter => Some(EditKind::Enter),
                text_widget::Edit::Backspace => Some(EditKind::Backspace),
                text_widget::Edit::Delete => Some(EditKind::Delete),
                text_widget::Edit::Paste(_) => Some(EditKind::Paste),
                _ => Some(EditKind::Other),
            },
            _ => None,
        }
    }

    fn restore_snapshot(&mut self, snap: &UndoSnapshot) {
        // Bypass the undo-recording branch in set_text — we're in the middle of
        // an undo/redo operation and don't want to pile new entries onto the stack.
        self.replace_text_no_undo(&snap.text);
        self.run_eval();
        self.safe_move_to(Cursor {
            position: Position { line: snap.cursor_line, column: snap.cursor_col },
            selection: None,
        });
    }

    fn perform_undo(&mut self) {
        if self.undo_stack.is_empty() {
            return;
        }
        let current = self.snapshot();
        self.redo_stack.push(current);
        let snap = self.undo_stack.pop().unwrap();
        self.restore_snapshot(&snap);
        self.last_edit_kind = EditKind::Other;
    }

    fn perform_redo(&mut self) {
        if self.redo_stack.is_empty() {
            return;
        }
        let current = self.snapshot();
        self.undo_stack.push(current);
        let snap = self.redo_stack.pop().unwrap();
        self.restore_snapshot(&snap);
        self.last_edit_kind = EditKind::Other;
    }

    fn update_find_matches(&mut self) {
        self.find.matches.clear();
        self.find.current = 0;
        if self.find.query.is_empty() {
            return;
        }
        let text = self.get_clean_text();
        let query_lower = self.find.query.to_lowercase();
        let text_lower = text.to_lowercase();

        let mut line = 0usize;
        let mut col = 0usize;
        let mut byte = 0usize;

        for (i, ch) in text_lower.char_indices() {
            while byte < i {
                byte += 1;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
                continue;
            }
            if text_lower[i..].starts_with(&query_lower) {
                self.find.matches.push((line, col));
            }
            col += 1;
        }
    }

    fn navigate_to_match(&mut self) {
        if self.find.matches.is_empty() {
            return;
        }
        let idx = self.find.current.min(self.find.matches.len() - 1);
        let (line, col) = self.find.matches[idx];
        self.safe_move_to(Cursor {
            position: Position { line, column: col },
            selection: None,
        });
    }

    /// Whether `message` is safe to dispatch while the editor is in
    /// `RenderMode::View`. Allowlist: scroll, click/drag selection,
    /// find, copy, zoom, focus, navigation — anything that doesn't
    /// touch document content. Edit-shaped `text_widget::Action`s and
    /// every mutating top-level `Message` get dropped at the gate.
    fn message_is_view_safe(message: &Message) -> bool {
        match message {
            Message::SetRenderMode(_) => true,
            Message::FocusBlock(_) => true,
            Message::TogglePreview => true,
            Message::MarkdownLink(_) => true,
            Message::ZoomIn | Message::ZoomOut | Message::ZoomReset => true,
            Message::ToggleFind | Message::HideFind => true,
            Message::FindQueryChanged(_)
            | Message::FindNext
            | Message::FindPrev => true,
            Message::ReplaceQueryChanged(_) => true,
            Message::TableMoveUp
            | Message::TableMoveDown
            | Message::TableMoveLeft
            | Message::TableMoveRight => true,
            Message::SelectAllBlocks => true,
            Message::ShowContextMenu { .. } | Message::HideContextMenu => true,
            Message::CopyLiteral(_) | Message::CopyFocusedTableSelection => true,
            Message::InlineResultPress { .. } | Message::InlineResultRelease => true,
            Message::EvalAll => true,
            Message::EditorAction(action) | Message::BlockAction(_, action) => {
                !action.is_edit()
            }
            _ => false,
        }
    }

    pub fn update(&mut self, message: Message) {
        // View mode: drop anything that would change the document. Mode
        // switches, focus, scroll, click/drag selection, find, copy,
        // navigation — all pass through. Allowlist; new mutating
        // messages should fall through to the default `false` arm and
        // get dropped.
        if self.render_mode == RenderMode::View && !Self::message_is_view_safe(&message) {
            return;
        }

        // Drop whole-document selection on any message that isn't itself an
        // operation on that selection. Click, key press, table action — all
        // collapse the doc-wide selection back to single-block / single-cell.
        let preserve_doc_selection = matches!(
            &message,
            Message::SelectAllBlocks
                | Message::ClearAllBlocks
                | Message::DeleteAllBlocks
        );
        if !preserve_doc_selection && self.all_blocks_selected {
            self.all_blocks_selected = false;
        }

        // Drop the context menu on any message that isn't itself a context
        // menu operation. Includes button clicks INSIDE the menu — they
        // dispatch the action AND auto-clear in one shot. Clicking outside
        // the menu (which generates a SelectCell or BlockAction) also
        // dismisses for free.
        let preserve_context_menu = matches!(
            &message,
            Message::ShowContextMenu { .. }
        );
        if !preserve_context_menu && self.context_menu.is_some() {
            self.context_menu = None;
        }

        match message {
            Message::EditorAction(action) => {
                let is_edit = action.is_edit();
                let is_enter = matches!(&action, Action::Edit(text_widget::Edit::Enter));
                let is_paste = matches!(&action, Action::Edit(text_widget::Edit::Paste(_)));

                if let Some(kind) = Self::classify_edit(&action) {
                    self.maybe_snapshot(kind);
                }

                if let Action::Scroll { lines } = &action {
                    let lh = self.line_height();
                    // Single-block-mode gutter still uses scroll_offset for
                    // its own scroll tracking. Keep this update so the gutter
                    // doesn't desync. The multi-block path ignores this and
                    // uses `pending_scroll` (forwarded to the outer
                    // scrollable in handle.rs::render).
                    self.scroll_offset += *lines as f32 * lh;
                    self.scroll_offset = self.scroll_offset.max(0.0);
                    let focused_id = self.layout.get(self.focused_block).copied();
                    let items_h: f32 = focused_id
                        .map(|id| self.item_offsets(id).iter().map(|(_, h)| h).sum())
                        .unwrap_or(0.0);
                    let max = (self.content().line_count() as f32 - 1.0) * lh + items_h;
                    self.scroll_offset = self.scroll_offset.min(max.max(0.0));
                    // Accumulate the pixel delta for the outer scrollable.
                    // text_editor's `Action::Scroll` carries lines, not pixels —
                    // multiply by line height. Multiple scroll events in one
                    // frame stack up here.
                    self.pending_scroll += *lines as f32 * lh;
                }

                // Smart-backspace inside leading whitespace: with no
                // selection, delete back to the previous tab stop in a
                // single user-visible step. Mutually exclusive with
                // handle_block_boundary's col-0 merge case (col > 0 here).
                let smart_backspace_count: Option<usize> =
                    if matches!(&action, Action::Edit(text_widget::Edit::Backspace)) {
                        let cursor = self.content().cursor();
                        if cursor.selection.is_none() && cursor.position.column > 0 {
                            let line_text = self
                                .content()
                                .line(cursor.position.line)
                                .map(|l| l.text.to_string())
                                .unwrap_or_default();
                            let col = cursor.position.column.min(line_text.len());
                            let prefix = &line_text[..col];
                            if !prefix.is_empty() && prefix.chars().all(|c| c == ' ') {
                                let tab = self.tab_width();
                                let n = (col - 1) % tab + 1;
                                if n > 1 { Some(n) } else { None }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                let dedent = if let text_widget::Action::Edit(text_widget::Edit::Insert(ch)) = &action {
                    matches!(ch, '}' | ']' | ')').then(|| {
                        let cursor = self.content().cursor();
                        let line_text = self.content().line(cursor.position.line)
                            .map(|l| l.text.to_string())
                            .unwrap_or_default();
                        let prefix = &line_text[..cursor.position.column];
                        if prefix.chars().all(|c| c == ' ' || c == '\t') && prefix.len() >= 4 {
                            Some(prefix.len())
                        } else {
                            None
                        }
                    }).flatten()
                } else {
                    None
                };

                let handled_boundary = self.handle_block_boundary(&action);
                if !handled_boundary {
                    if let Some(n) = smart_backspace_count {
                        for _ in 0..n {
                            self.content_mut().perform(text_widget::Action::Edit(
                                text_widget::Edit::Backspace,
                            ));
                        }
                    } else {
                        self.content_mut().perform(action);
                    }
                }

                // Auto-indent on Enter. Compute AFTER perform(Enter), reading
                // the line that was just split — that's the line whose
                // indentation we want to inherit on the new line below it.
                if is_enter && !handled_boundary {
                    let cursor = self.content().cursor();
                    if cursor.position.line > 0 {
                        let prev_line = self
                            .content()
                            .line(cursor.position.line - 1)
                            .map(|l| l.text.to_string())
                            .unwrap_or_default();
                        let base = leading_whitespace(&prev_line).to_string();
                        let trimmed = prev_line.trim_end();
                        let opens_block = matches!(
                            trimmed.as_bytes().last(),
                            Some(b'{' | b'[' | b'(')
                        );
                        let indent = if opens_block {
                            format!("{base}{}", " ".repeat(self.tab_width()))
                        } else {
                            base
                        };
                        if !indent.is_empty() {
                            self.content_mut().perform(text_widget::Action::Edit(
                                text_widget::Edit::Paste(Arc::new(indent)),
                            ));
                        }
                    }
                }

                if let Some(col) = dedent {
                    let remove = col.min(4);
                    self.content_mut().perform(text_widget::Action::Move(Motion::Left));
                    for _ in 0..remove {
                        self.content_mut().perform(text_widget::Action::Edit(
                            text_widget::Edit::Backspace,
                        ));
                    }
                    self.content_mut().perform(text_widget::Action::Move(Motion::Right));
                }

                if is_edit {
                    self.last_edit = Instant::now();
                    if self.lang.is_none() {
                        self.lang = detect_lang_from_content(&self.content().text());
                    }
                    self.reparse();

                    if self.render_mode == RenderMode::Live {
                        if is_enter || is_paste {
                            self.check_block_structure();
                        }
                        self.eval_dirty = true;
                    }
                }
            }
            Message::InsertTable => {
                self.push_undo_snapshot();

                let rows: Vec<Vec<String>> = vec![
                    vec!["Header 1".into(), "Header 2".into(), "Header 3".into()],
                    vec!["".into(), "".into(), "".into()],
                    vec!["".into(), "".into(), "".into()],
                ];
                let new_id = blocks::next_id();
                let mut new_table = TableBlock::new(new_id, rows, 0);
                // Park focus on the first data cell (skip the header row) so
                // the user lands ready to type values, not to edit headers.
                new_table.focused_cell = Some((1, 0));
                let new_block: BoxedBlock = Box::new(new_table);

                let insert_at = (self.focused_block + 1).min(self.block_count());
                self.insert_block(insert_at, new_block);
                self.recount_block_lines();
                // Land in edit mode on the first data cell so the user can
                // type immediately. set_editing_cell handles index, central
                // selection, and pending_focus in one shot.
                self.set_editing_cell(insert_at, 1, 0);
                self.reparse();
                // Intentionally NOT calling run_eval() — see eval_segment_range
                // for the destruction-class bug this avoids.
            }
            Message::ToggleBold => self.toggle_wrap("**", "**"),
            Message::ToggleItalic => self.toggle_wrap("*", "*"),
            Message::ToggleStrike => self.toggle_wrap("~~", "~~"),
            Message::ToggleUnderline => self.toggle_wrap("<u>", "</u>"),
            Message::WrapWith(open, close) => self.toggle_wrap(open, close),
            Message::ToggleBlockquote => self.toggle_blockquote(),
            Message::AutoPair(open, close) => self.auto_pair(open, close),
            Message::FixUp => self.fix_up(),
            Message::Evaluate => {
                self.run_eval();
            }
            Message::EvalAll => {
                self.run_eval_all();
            }
            Message::SmartEval => {
                let cursor = self.content().cursor();
                let text = self.content().text();
                let lines: Vec<&str> = text.lines().collect();
                let line_idx = cursor.position.line;
                if line_idx < lines.len() {
                    let line = lines[line_idx].trim();
                    if let Some(varname) = parse_let_binding(line) {
                        let insert = format!("\n/= {varname}");
                        self.content_mut().perform(text_widget::Action::Move(Motion::End));
                        self.content_mut().perform(text_widget::Action::Edit(
                            text_widget::Edit::Paste(Arc::new(insert)),
                        ));
                        self.reparse();
                        self.run_eval();
                    }
                }
            }
            Message::TogglePreview => {
                self.preview = !self.preview;
                if self.preview {
                    self.reparse();
                }
            }
            Message::MarkdownLink(_url) => {}
            Message::ZoomIn => {
                self.font_size = (self.font_size + 1.0).min(48.0);
            }
            Message::ZoomOut => {
                self.font_size = (self.font_size - 1.0).max(8.0);
            }
            Message::ZoomReset => {
                self.font_size = 14.0;
            }
            Message::Undo => {
                self.perform_undo();
            }
            Message::Redo => {
                self.perform_redo();
            }
            Message::ToggleFind => {
                self.find.visible = !self.find.visible;
                if self.find.visible {
                    self.pending_focus = Some(WidgetId::new(FIND_INPUT_ID));
                }
            }
            Message::HideFind => {
                self.find.visible = false;
            }
            Message::FindQueryChanged(q) => {
                self.find.query = q;
                self.update_find_matches();
                if !self.find.matches.is_empty() {
                    self.find.current = 0;
                    self.navigate_to_match();
                }
            }
            Message::FindNext => {
                if !self.find.matches.is_empty() {
                    self.find.current = (self.find.current + 1) % self.find.matches.len();
                    self.navigate_to_match();
                }
            }
            Message::FindPrev => {
                if !self.find.matches.is_empty() {
                    self.find.current = if self.find.current == 0 {
                        self.find.matches.len() - 1
                    } else {
                        self.find.current - 1
                    };
                    self.navigate_to_match();
                }
            }
            Message::ReplaceQueryChanged(r) => {
                self.find.replacement = r;
            }
            Message::ReplaceOne => {
                if self.find.matches.is_empty() || self.find.query.is_empty() {
                    return;
                }
                self.push_undo_snapshot();
                self.redo_stack.clear();

                let (match_line, match_col) = self.find.matches[self.find.current];
                let clean = self.get_clean_text();
                let query_lower = self.find.query.to_lowercase();
                let query_char_count = query_lower.chars().count();
                let mut lines: Vec<String> = clean.lines().map(|l| l.to_string()).collect();
                if match_line < lines.len() {
                    let line = &lines[match_line];
                    // Case-fold per-substring at the match column to avoid
                    // byte-index divergence between line and line.to_lowercase().
                    let chars: Vec<(usize, char)> = line.char_indices().collect();
                    if match_col < chars.len() {
                        let window: String = chars[match_col..]
                            .iter()
                            .take(query_char_count)
                            .map(|(_, c)| *c)
                            .collect::<String>()
                            .to_lowercase();
                        if window == query_lower {
                            let byte_start = chars[match_col].0;
                            let byte_end = if match_col + query_char_count < chars.len() {
                                chars[match_col + query_char_count].0
                            } else {
                                line.len()
                            };
                            let before = &line[..byte_start];
                            let after = &line[byte_end..];
                            lines[match_line] =
                                format!("{before}{}{after}", self.find.replacement);
                        }
                    }
                }
                let new_text = lines.join("\n");
                self.set_text(&new_text);
                self.run_eval();
                self.update_find_matches();
                if !self.find.matches.is_empty() {
                    self.find.current = self.find.current.min(self.find.matches.len() - 1);
                    self.navigate_to_match();
                }
            }
            Message::ReplaceAll => {
                if self.find.matches.is_empty() || self.find.query.is_empty() {
                    return;
                }
                self.push_undo_snapshot();
                self.redo_stack.clear();

                // Case-fold per-substring via char iteration. Never index
                // into a pre-lowercased copy — the byte layout can diverge
                // for characters whose lowercase changes byte length (Turkish
                // İ → "i\u{307}", German ß → "ss", etc.).
                let clean = self.get_clean_text();
                let query_lower = self.find.query.to_lowercase();
                let query_char_count = query_lower.chars().count();
                let chars: Vec<(usize, char)> = clean.char_indices().collect();
                let mut result = String::with_capacity(clean.len());
                let mut ci = 0;
                while ci < chars.len() {
                    let remaining = chars.len() - ci;
                    if remaining >= query_char_count {
                        let window: String = chars[ci..ci + query_char_count]
                            .iter()
                            .map(|(_, c)| *c)
                            .collect::<String>()
                            .to_lowercase();
                        if window == query_lower {
                            result.push_str(&self.find.replacement);
                            ci += query_char_count;
                            continue;
                        }
                    }
                    result.push(chars[ci].1);
                    ci += 1;
                }
                self.set_text(&result);
                self.run_eval();
                self.update_find_matches();
            }
            Message::TableMsg(idx, tmsg) => {
                let structural = matches!(
                    &tmsg,
                    TableMessage::InsertRowAbove
                        | TableMessage::InsertRowBelow
                        | TableMessage::DeleteRow
                        | TableMessage::InsertColLeft
                        | TableMessage::InsertColRight
                        | TableMessage::DeleteCol
                        | TableMessage::AddRow
                        | TableMessage::AddColumn
                );
                // DeleteCol on a single-column table collapses to DeleteTable.
                if matches!(&tmsg, TableMessage::DeleteCol) {
                    if let Some(tb) = self.table_block_at(idx) {
                        if !tb.is_eval_result
                            && tb.rows.first().map(|r| r.len()).unwrap_or(0) <= 1
                        {
                            self.update(Message::DeleteCurrentTable);
                            return;
                        }
                    }
                }
                // The corner-cell delete affordance promotes straight to the
                // top-level DeleteCurrentTable handler. We need to ensure the
                // target table is the one focused before that runs — the
                // click on the affordance counts as touching the table.
                if matches!(&tmsg, TableMessage::DeleteTable) {
                    if let Some(tb) = self.table_block_at_mut(idx) {
                        if tb.focused_cell.is_none() {
                            tb.focused_cell = Some((0, 0));
                        }
                        tb.is_active = true;
                    }
                    self.update(Message::DeleteCurrentTable);
                    return;
                }
                // Right-click → ShowContextMenu. Before opening the menu,
                // hoist focus to the right-clicked table+cell so subsequent
                // `FocusedTableOp` actions (Insert/Delete row/col) target
                // this table and so iced's focus machinery doesn't snap the
                // scroll position back to whatever the prior focused block
                // was. Pre-existing multi-cell selection IS preserved —
                // only `focused_block` and the right-clicked cell's
                // `focused_cell` are updated, not the selection HashSet.
                if let TableMessage::ContextMenu(r, c) = &tmsg {
                    let (r, c) = (*r, *c);
                    if let Some(tb) = self.table_block_at_mut(idx) {
                        tb.focused_cell = Some((r, c));
                        tb.is_active = true;
                    }
                    self.focused_block = idx;
                    self.update(Message::ShowContextMenu { block_idx: idx });
                    return;
                }
                // SelectAll/ClearAll need editor-level housekeeping: SelectAll
                // also marks the table as the focused block so the keyboard
                // gates pick it up; ClearAll snapshots undo and re-runs eval.
                let select_all = matches!(&tmsg, TableMessage::SelectAll);
                let clear_all = matches!(&tmsg, TableMessage::ClearAll);
                if clear_all {
                    self.push_undo_snapshot();
                    self.redo_stack.clear();
                }
                if structural {
                    self.push_undo_snapshot();
                }

                // SelectCell / EditCell are click-driven and need to also
                // mutate the editor-level `editing` / `selection` / focus.
                // Capture before tb.handle so we can call the helpers after.
                let select_target = if let TableMessage::SelectCell(r, c) = &tmsg {
                    Some((*r, *c))
                } else {
                    None
                };
                let edit_target = if let TableMessage::EditCell(r, c) = &tmsg {
                    Some((*r, *c))
                } else {
                    None
                };

                // Capture mods BEFORE the borrow so the click can be
                // resolved into a SelectionMode without aliasing.
                let mods = self.mods;

                if let Some(tb) = self.table_block_at_mut(idx) {
                    tb.handle(tmsg);
                }

                if let Some((r, c)) = select_target {
                    // Resolve the modifier-aware selection mode and apply it
                    // to the table's HashSet selection. Click affects ONE
                    // cell — rectangular range comes from drag, not click.
                    //   no mod    → Replace (selection becomes just this cell)
                    //   Cmd       → Toggle  (invert this cell's membership)
                    //   Shift     → Add     (add this cell, never remove)
                    //   Cmd+Shift → Remove  (remove this cell, never add)
                    let mode = if mods.logo() && mods.shift() {
                        crate::table_block::SelectionMode::Subtract
                    } else if mods.logo() {
                        crate::table_block::SelectionMode::Toggle
                    } else if mods.shift() {
                        crate::table_block::SelectionMode::Extend
                    } else {
                        crate::table_block::SelectionMode::Replace
                    };
                    if let Some(tb) = self.table_block_at_mut(idx) {
                        tb.apply_click_selection(r, c, mode);
                    }
                    self.set_selected_cell(idx, r, c);
                }
                if let Some((r, c)) = edit_target {
                    self.set_editing_cell(idx, r, c);
                }
                if select_all {
                    // Whole-table selection — focused block is this table,
                    // editing is cleared. The table_selected flag was already
                    // set by tb.handle above.
                    self.focused_block = idx;
                    self.editing = None;
                    if let Some(block) = self.block_at(idx) {
                        let path = crate::selection::NodePath::block(block.id());
                        self.selection = crate::selection::Selection::Caret(path.clone());
                        self.focus = Some(path);
                    }
                }
                if clear_all {
                    self.eval_dirty = true;
                    self.last_edit = Instant::now();
                    self.reparse();
                }

                if structural {
                    self.recount_block_lines();
                    if let Some(tb) = self.table_block_at(idx) {
                        if let Some((r, c)) = tb.focused_cell {
                            self.pending_focus = Some(table_block::cell_id(tb.id, r, c));
                        }
                    }
                    self.reparse();
                }
            }
            Message::DeleteCurrentTable => {
                if let Some(target) = self.focused_table_index() {
                    self.push_undo_snapshot();
                    self.redo_stack.clear();
                    self.remove_block(target);
                    if self.layout.is_empty() {
                        let lang = self.lang_str();
                        self.push_block(Box::new(TextBlock::new(blocks::next_id(), "", 0, lang)));
                    }
                    self.recount_block_lines();
                    let new_focus = target.min(self.block_count().saturating_sub(1));
                    self.set_focused_block(new_focus);
                    if let Some(tb) = self.text_block_at(new_focus) {
                        self.pending_focus = Some(block_editor_id(tb.id));
                    }
                    self.reparse();
                }
            }
            Message::FocusedTableOp(tmsg) => {
                if let Some(idx) = self.focused_table_index() {
                    self.update(Message::TableMsg(idx, tmsg));
                }
            }
            Message::TableTab => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let Some((cur_r, cur_c)) = tb.focused_cell else { return };
                let col_count = tb.col_count();
                if cur_c + 1 >= col_count {
                    self.update(Message::TableMsg(idx, TableMessage::AddColumn));
                }
                // Tab keeps edit mode rolling forward so the user can type
                // straight into the next cell without a second click.
                self.set_editing_cell(idx, cur_r, cur_c + 1);
            }
            Message::TableShiftTab => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let Some((cur_r, cur_c)) = tb.focused_cell else { return };
                if cur_c == 0 { return; }
                self.set_editing_cell(idx, cur_r, cur_c - 1);
            }
            Message::TableEnter => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let Some((cur_r, cur_c)) = tb.focused_cell else { return };
                let row_count = tb.row_count();
                if cur_r + 1 >= row_count {
                    self.update(Message::TableMsg(idx, TableMessage::AddRow));
                }
                self.set_editing_cell(idx, cur_r + 1, cur_c);
            }
            Message::EscapeTableUp(table_idx) => {
                self.escape_table_up(table_idx);
            }
            Message::EscapeTableDown(table_idx) => {
                self.escape_table_down(table_idx);
            }
            Message::ExitCellEdit => {
                // Exit edit mode but keep the cell selected — same as
                // Excel/Numbers' Escape behavior. The cell flips back to
                // its static-text rendering on the next frame.
                if let Some(path) = self.editing.clone() {
                    self.editing = None;
                    if let crate::selection::InnerPath::Cell { row, col } = path.inner {
                        // Locate the table by id and re-park focus on the
                        // (now selected, not editing) cell.
                        for i in 0..self.block_count() {
                            if let Some(tb) = self.block_at(i).and_then(|b| b.as_any().downcast_ref::<TableBlock>()) {
                                if tb.id == path.block_id {
                                    self.set_selected_cell(i, row, col);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Message::EnterCellEditWithChar(c) => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let Some((r, col)) = tb.focused_cell else { return };
                // Replace the cell content with just the typed character —
                // Excel/Numbers "start typing into the selection" semantics.
                if let Some(tb) = self.table_block_at_mut(idx) {
                    if r < tb.rows.len() && col < tb.rows[r].len() {
                        tb.rows[r][col] = c.to_string();
                    }
                }
                self.set_editing_cell(idx, r, col);
            }
            Message::TableMoveUp => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let block_id = tb.id;
                let Some((cur_r, cur_c)) = tb.focused_cell else { return };
                if cur_r == 0 {
                    return;
                }
                if let Some(tb) = self.table_block_at_mut(idx) {
                    tb.apply_click_selection(
                        cur_r - 1,
                        cur_c,
                        crate::table_block::SelectionMode::Replace,
                    );
                }
                self.pending_focus = Some(table_block::cell_id(block_id, cur_r - 1, cur_c));
            }
            Message::TableMoveDown => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let block_id = tb.id;
                let Some((cur_r, cur_c)) = tb.focused_cell else { return };
                let row_count = tb.row_count();
                if cur_r + 1 >= row_count {
                    return;
                }
                if let Some(tb) = self.table_block_at_mut(idx) {
                    tb.apply_click_selection(
                        cur_r + 1,
                        cur_c,
                        crate::table_block::SelectionMode::Replace,
                    );
                }
                self.pending_focus = Some(table_block::cell_id(block_id, cur_r + 1, cur_c));
            }
            Message::TableMoveLeft => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let block_id = tb.id;
                let Some((cur_r, cur_c)) = tb.focused_cell else { return };
                if cur_c == 0 {
                    return;
                }
                if let Some(tb) = self.table_block_at_mut(idx) {
                    tb.apply_click_selection(
                        cur_r,
                        cur_c - 1,
                        crate::table_block::SelectionMode::Replace,
                    );
                }
                self.pending_focus = Some(table_block::cell_id(block_id, cur_r, cur_c - 1));
            }
            Message::TableMoveRight => {
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                let block_id = tb.id;
                let Some((cur_r, cur_c)) = tb.focused_cell else { return };
                let col_count = tb.col_count();
                if cur_c + 1 >= col_count {
                    return;
                }
                if let Some(tb) = self.table_block_at_mut(idx) {
                    tb.apply_click_selection(
                        cur_r,
                        cur_c + 1,
                        crate::table_block::SelectionMode::Replace,
                    );
                }
                self.pending_focus = Some(table_block::cell_id(block_id, cur_r, cur_c + 1));
            }
            Message::ClearSelectedCell => {
                // Empty every selected cell. Does nothing if there's no
                // selection or if a cell is currently being edited (the
                // text_input handles its own backspace). Honors the multi-cell
                // selection HashSet first; falls back to single focused_cell
                // if the HashSet is empty.
                if self.editing.is_some() {
                    return;
                }
                let Some(idx) = self.focused_table_index() else { return };
                let Some(tb) = self.table_block_at(idx) else { return };
                if tb.is_eval_result {
                    return;
                }
                let targets: Vec<(usize, usize)> = if !tb.selection.is_empty() {
                    tb.selection.iter().copied().collect()
                } else if let Some(rc) = tb.focused_cell {
                    vec![rc]
                } else {
                    return;
                };
                self.push_undo_snapshot();
                self.redo_stack.clear();
                if let Some(tb) = self.table_block_at_mut(idx) {
                    for (r, c) in targets {
                        if r < tb.rows.len() && c < tb.rows[r].len() {
                            tb.rows[r][c].clear();
                        }
                    }
                }
                self.eval_dirty = true;
                self.last_edit = Instant::now();
                self.reparse();
            }
            Message::SelectAllBlocks => {
                self.enter_editor_mode();
            }
            Message::SetRenderMode(mode) => {
                match mode {
                    RenderMode::Live => {
                        if self.render_mode == RenderMode::Editor {
                            self.exit_editor_mode();
                        } else if self.render_mode == RenderMode::View {
                            self.render_mode = RenderMode::Live;
                            self.reparse();
                            // Restore keyboard focus to the focused text block.
                            if let Some(tb) = self.text_block_at(self.focused_block) {
                                self.pending_focus = Some(block_editor_id(tb.id));
                            }
                        }
                        self.run_eval_all();
                    }
                    RenderMode::Editor => self.enter_editor_mode(),
                    RenderMode::View => {
                        self.enter_view_mode();
                        self.run_eval_all();
                    }
                }
            }
            Message::ClearAllBlocks => {
                // Plain Backspace/Delete with the whole document selected —
                // wipe to a single empty text block, matching standard editor
                // select-all + delete behavior.
                self.push_undo_snapshot();
                self.redo_stack.clear();
                self.clear_blocks();
                let lang = self.lang_str();
                self.push_block(Box::new(TextBlock::new(
                    blocks::next_id(),
                    "",
                    0,
                    lang,
                )));
                self.recount_block_lines();
                self.set_focused_block(0);
                self.all_blocks_selected = false;
                self.eval_dirty = true;
                self.last_edit = Instant::now();
                self.reparse();
            }
            Message::ShowContextMenu { block_idx } => {
                // Anchor at the current cursor position. handle.rs writes
                // self.cursor_pos before draining messages so this read is
                // current. The position is in viewport coordinates — view_blocks
                // uses it directly via container padding inside an iced stack.
                self.context_menu = Some(ContextMenuState {
                    block_idx,
                    x: self.cursor_pos.x,
                    y: self.cursor_pos.y,
                });
            }
            Message::HideContextMenu => {
                self.context_menu = None;
            }
            Message::CopyLiteral(text) => {
                self.pending_clipboard = Some(text);
            }
            Message::CopyFocusedTableSelection => {
                if let Some(text) = self.copy_focused_table_selection() {
                    self.pending_clipboard = Some(text);
                }
            }
            Message::DeleteAllBlocks => {
                // Cmd+Backspace with the whole document selected — wipe to a
                // single empty text block. Same destructive scope as
                // selecting all in a regular editor and hitting Delete.
                self.push_undo_snapshot();
                self.redo_stack.clear();
                self.clear_blocks();
                let lang = self.lang_str();
                self.push_block(Box::new(TextBlock::new(
                    blocks::next_id(),
                    "",
                    0,
                    lang,
                )));
                self.recount_block_lines();
                self.all_blocks_selected = false;
                self.set_focused_block(0);
                if let Some(tb) = self.text_block_at(0) {
                    self.pending_focus = Some(block_editor_id(tb.id));
                }
                self.eval_dirty = true;
                self.last_edit = Instant::now();
                self.reparse();
            }
            Message::IndentTab => {
                let tab = self.tab_width();
                let col = self.content().cursor().position.column;
                let to_next_stop = tab - (col % tab);
                let spaces = " ".repeat(to_next_stop);
                self.content_mut().perform(text_widget::Action::Edit(
                    text_widget::Edit::Paste(Arc::new(spaces)),
                ));
                self.last_edit = Instant::now();
                self.eval_dirty = true;
                self.reparse();
            }
            Message::OutdentTab => {
                let tab = self.tab_width();
                let cursor = self.content().cursor();
                let line_text = self
                    .content()
                    .line(cursor.position.line)
                    .map(|l| l.text.to_string())
                    .unwrap_or_default();
                let leading: usize = line_text
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .count();
                if leading == 0 {
                    return;
                }
                let remove = leading.min(tab);
                self.content_mut().perform(text_widget::Action::Move(Motion::Home));
                for _ in 0..remove {
                    self.content_mut().perform(text_widget::Action::Edit(
                        text_widget::Edit::Delete,
                    ));
                }
                let new_col = cursor.position.column.saturating_sub(remove);
                self.safe_move_to(Cursor {
                    position: Position {
                        line: cursor.position.line,
                        column: new_col,
                    },
                    selection: None,
                });
                self.last_edit = Instant::now();
                self.eval_dirty = true;
                self.reparse();
            }
            Message::BlockAction(idx, action) => {
                if idx < self.block_count() {
                    self.set_focused_block(idx);
                }
                self.update(Message::EditorAction(action));
            }
            Message::FocusBlock(idx) => {
                if idx < self.block_count() {
                    self.set_focused_block(idx);
                }
            }
            Message::InlineResultPress { block_id, after_line } => {
                self.inline_press = Some(InlinePressState {
                    block_id,
                    after_line,
                    started_at: Instant::now(),
                    fired_long_press: false,
                });
            }
            Message::InlineResultRelease => {
                self.inline_press = None;
            }
            Message::InlineResultDoubleClick { block_id, after_line } => {
                self.inline_press = None;
                self.handle_result_extract(block_id, after_line);
            }
        }
    }

    /// Look up the inline result for `(block_id, after_line)` and return its
    /// raw value text (the part after the `→ ` prefix). `None` if no result
    /// is attached or the result is an error.
    fn inline_result_value(&self, block_id: crate::selection::BlockId, after_line: usize) -> Option<String> {
        let r = self.eval_results.iter().find(|r| {
            r.anchor.block_id == block_id && r.anchor.after_line == after_line && !r.is_error
        })?;
        Some(r.text.trim_start_matches(RESULT_PREFIX).trim().to_string())
    }

    /// Read line `line_idx` from the TextBlock with the given id, if any.
    fn read_line_at(&self, block_id: crate::selection::BlockId, line_idx: usize) -> Option<String> {
        let block = self.registry.get(&block_id)?;
        let tb = block.as_any().downcast_ref::<TextBlock>()?;
        tb.content.line(line_idx).map(|l| l.text.to_string())
    }

    /// Copy `{line}  → {value}` to clipboard. Used by both long-press (just
    /// copy) and double-click (copy then insert template).
    fn copy_inline_result(&mut self, block_id: crate::selection::BlockId, after_line: usize) {
        let value = match self.inline_result_value(block_id, after_line) {
            Some(v) => v,
            None => return,
        };
        let line = self.read_line_at(block_id, after_line).unwrap_or_default();
        let trimmed = line.trim_end();
        self.pending_clipboard = Some(format!("{trimmed}  {RESULT_PREFIX}{value}"));
    }

    /// Double-click on a result: copy + drop a `let  = value` line two lines
    /// below the source `/=`. Cursor lands right after `let ` so the user can
    /// type the variable name.
    fn handle_result_extract(&mut self, block_id: crate::selection::BlockId, after_line: usize) {
        let value = match self.inline_result_value(block_id, after_line) {
            Some(v) => v,
            None => return,
        };
        self.copy_inline_result(block_id, after_line);

        let block_idx = match self.layout.iter().position(|id| *id == block_id) {
            Some(i) => i,
            None => return,
        };
        // Only TextBlocks accept text-buffer mutations through this path.
        if self.text_block_at(block_idx).is_none() { return; }

        self.push_undo_snapshot();
        self.redo_stack.clear();
        self.set_focused_block(block_idx);

        // Move cursor to end of the source `/=` line.
        let content = self.content_mut();
        content.perform(Action::Move(Motion::DocumentStart));
        for _ in 0..after_line {
            content.perform(Action::Move(Motion::Down));
        }
        content.perform(Action::Move(Motion::End));

        // Drop a blank line then `let  = value`. Two spaces between `let` and
        // `=` — the user types the variable name into the gap.
        let paste = format!("\n\nlet  = {value}");
        content.perform(Action::Edit(text_widget::Edit::Paste(Arc::new(paste))));

        // Cursor is at the end of `value`. Walk back past `value`, the `=`,
        // and the two flanking spaces — landing right after `let `.
        let back = 3 + value.chars().count();
        for _ in 0..back {
            content.perform(Action::Move(Motion::Left));
        }

        self.last_edit = Instant::now();
        self.eval_dirty = true;
        self.reparse();
    }

    pub fn view(&self) -> Element<'_, Message, Theme, iced_wgpu::Renderer> {
        let main_content: Element<'_, Message, Theme, iced_wgpu::Renderer> = if self.preview {
            let settings = markdown::Settings::with_text_size(self.font_size, md_style());
            let preview = markdown::view(&self.parsed, settings)
                .map(Message::MarkdownLink);

            iced_widget::container(
                iced_widget::scrollable(
                    iced_widget::container(preview)
                        .padding(Padding { top: 38.0, right: 16.0, bottom: 16.0, left: 16.0 })
                )
                .height(Length::Fill)
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &Theme| {
                let p = palette::current();
                container::Style {
                    background: Some(Background::Color(p.base)),
                    border: Border::default(),
                    text_color: Some(p.text),
                    shadow: Shadow::default(),
                    snap: false,
                }
            })
            .into()
        } else {
            self.view_blocks()
        };

        let mode_label = match self.render_mode {
            RenderMode::Live => "Live",
            RenderMode::Editor => "Editor",
            RenderMode::View => "View",
        };
        let cursor = self.content().cursor();
        let line = cursor.position.line + 1;
        let col = cursor.position.column + 1;

        let render_mode = self.render_mode;
        let status_bar = iced_widget::container(
            iced_widget::row([
                iced_widget::text(format!("{mode_label}  Ln {line}, Col {col}"))
                    .font(Font::MONOSPACE)
                    .size(11.0)
                    .color(oklab::lighten_for_size(Color::WHITE, 11.0))
                    .into(),
            ])
        )
        .width(Length::Fill)
        .padding(Padding { top: 3.0, right: 10.0, bottom: 3.0, left: 10.0 })
        .style(move |_theme: &Theme| {
            let p = palette::current();
            let darken = |c: Color| Color { r: c.r * 0.45, g: c.g * 0.45, b: c.b * 0.45, a: c.a };
            let bg = match render_mode {
                RenderMode::Live => darken(p.mauve),
                RenderMode::Editor => darken(p.blue),
                RenderMode::View => darken(p.pink),
            };
            container::Style {
                background: Some(Background::Color(bg)),
                border: Border::default(),
                text_color: None,
                shadow: Shadow::default(),
                snap: false,
            }
        });

        let mut col_items: Vec<Element<'_, Message, Theme, iced_wgpu::Renderer>> = Vec::new();

        col_items.push(main_content);

        if self.find.visible {
            col_items.push(self.find_bar());
        }

        col_items.push(status_bar.into());

        iced_widget::column(col_items)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_blocks(&self) -> Element<'_, Message, Theme, iced_wgpu::Renderer> {
        let has_computed_layers = !self.eval_results.is_empty()
            || !self.computed_tables.is_empty()
            || !self.computed_trees.is_empty();
        let single_text_block = self.block_count() == 1
            && self.block_at(0).map(|b| b.as_any().is::<TextBlock>()).unwrap_or(false)
            && !has_computed_layers;

        let title_bar_h = 38.0_f32;

        let mut block_elements: Vec<Element<'_, Message, Theme, iced_wgpu::Renderer>> = Vec::new();

        if !single_text_block && !self.layout.is_empty() {
            if !self.block_at(0).map(|b| b.as_any().is::<TextBlock>()).unwrap_or(true) {
                block_elements.push(
                    iced_widget::container(iced_widget::text(""))
                        .height(Length::Fixed(title_bar_h))
                        .width(Length::Fill)
                        .into()
                );
            }
        }

        let lang_for_block = self.lang_str();

        let mut global_line = 0usize;
        for (bi, &block_id) in self.layout.iter().enumerate() {
            let block = self.registry.get(&block_id).unwrap();
            let any = block.as_any();

            if let Some(tb) = any.downcast_ref::<TextBlock>() {
                let block_idx = bi;
                let line_h = self.font_size * 1.3;

                if single_text_block {
                    let is_focused = bi == self.focused_block;
                    let cursor_line = tb.content.cursor().position.line;
                    let text = tb.content.text();
                    let decors = compute_line_decors(&text);

                    let anchored_items = self.build_anchored_items(tb.id);
                    let editor = text_widget::TextEditor::new(&tb.content)
                        .id(block_editor_id(tb.id))
                        .on_action(move |action| Message::BlockAction(block_idx, action))
                        .font(syntax::EDITOR_FONT)
                        .size(self.font_size)
                        .height(Length::Fill)
                        .padding(Padding { top: title_bar_h, right: 8.0, bottom: 8.0, left: 8.0 })
                        .wrapping(Wrapping::Word)
                        .key_binding(macos_key_binding)
                        .anchored(anchored_items)
                        .show_gutter(true)
                        .gutter_offset(0)
                        .focused(is_focused)
                        .cursor_line(if is_focused { Some(cursor_line) } else { None })
                        .line_indicator(self.line_indicator)
                        .gutter_rainbow(self.gutter_rainbow)
                        .line_decors(decors)
                        .style(|_theme, _status| {
                            let p = palette::current();
                            text_widget::Style {
                                background: Background::Color(p.base),
                                border: Border::default(),
                                placeholder: p.overlay0,
                                value: p.text,
                                selection: Color { a: 0.4, ..p.blue },
                            }
                        });

                    let settings = SyntaxSettings {
                        lang: lang_for_block.clone(),
                        source: tb.content.text(),
                    };
                    let editor_el: Element<'_, Message, Theme, iced_wgpu::Renderer> = editor
                        .highlight_with::<SyntaxHighlighter>(
                            settings,
                            |highlight, _theme| Format {
                                color: Some(syntax::highlight_color(highlight.kind)),
                                font: syntax::highlight_font(highlight.kind),
                            },
                        )
                        .into();

                    block_elements.push(editor_el);
                } else {
                    let top_pad = if bi == 0 { title_bar_h } else { 0.0 };
                    let is_focused = bi == self.focused_block;
                    let anchored_items = self.build_anchored_items(tb.id);
                    let cursor_line = tb.content.cursor().position.line;
                    let line_count = tb.content.line_count();
                    let text = tb.content.text();
                    let decors = compute_line_decors(&text);
                    let this_global_line = global_line;
                    global_line += line_count;
                    let _ = line_h; // text_widget::layout owns the height now

                    // Length::Shrink lets text_widget::layout publish the
                    // actual rendered height (visual_rows × line_h + items
                    // + padding). Computing it here from logical line count
                    // undercounts when wrap fires, which leaves the next
                    // block sitting on top of this block's tail.
                    let editor = text_widget::TextEditor::new(&tb.content)
                        .id(block_editor_id(tb.id))
                        .on_action(move |action| Message::BlockAction(block_idx, action))
                        .font(syntax::EDITOR_FONT)
                        .size(self.font_size)
                        .height(Length::Shrink)
                        .padding(Padding { top: top_pad, right: 8.0, bottom: 4.0, left: 8.0 })
                        .wrapping(Wrapping::Word)
                        .key_binding(macos_key_binding)
                        .anchored(anchored_items)
                        .show_gutter(true)
                        .gutter_offset(this_global_line)
                        .focused(is_focused)
                        .cursor_line(if is_focused { Some(cursor_line) } else { None })
                        .line_indicator(self.line_indicator)
                        .gutter_rainbow(self.gutter_rainbow)
                        .line_decors(decors)
                        .style(|_theme, _status| {
                            let p = palette::current();
                            text_widget::Style {
                                background: Background::Color(p.base),
                                border: Border::default(),
                                placeholder: p.overlay0,
                                value: p.text,
                                selection: Color { a: 0.4, ..p.blue },
                            }
                        });

                    let settings = SyntaxSettings {
                        lang: lang_for_block.clone(),
                        source: tb.content.text(),
                    };
                    let editor_el: Element<'_, Message, Theme, iced_wgpu::Renderer> = editor
                        .highlight_with::<SyntaxHighlighter>(
                            settings,
                            |highlight, _theme| Format {
                                color: Some(syntax::highlight_color(highlight.kind)),
                                font: syntax::highlight_font(highlight.kind),
                            },
                        )
                        .into();

                    block_elements.push(editor_el);
                }
                continue;
            }

            if let Some(tab) = any.downcast_ref::<TableBlock>() {
                let block_idx = bi;
                // Translate the central `editing` path into a (row, col) for
                // this specific table, so the renderer can branch each cell.
                let editing_cell = match self.editing.as_ref() {
                    Some(path) if path.block_id == tab.id => match &path.inner {
                        crate::selection::InnerPath::Cell { row, col } => Some((*row, *col)),
                        _ => None,
                    },
                    _ => None,
                };
                block_elements.push(
                    table_block::table_view(
                        tab,
                        editing_cell,
                        self.font_size,
                        &self.computed_cells,
                        move |tmsg| Message::TableMsg(block_idx, tmsg),
                    )
                );
                global_line += if tab.rows.is_empty() { 0 } else { tab.rows.len() + 1 };
                continue;
            }

            // Heading / HR / Tree go through the trait `view` method via a
            // per-iteration ViewCtx. The new trait signature decouples the
            // returned LayeredView's lifetime from `ctx`, so a stack-local
            // ViewCtx is fine — implementations must read what they need from
            // ctx into Copy locals and not capture ctx into the element.
            let ctx: ViewCtx<'_, Message> = ViewCtx {
                block_index: bi,
                selection: &self.selection,
                focus: self.focus.as_ref(),
                editing: self.editing.as_ref(),
                font_size: self.font_size,
                is_dark: true,
                on_text_action: |idx, action| Message::BlockAction(idx, action),
                on_table_msg: |idx, tmsg| Message::TableMsg(idx, tmsg),
                computed_cells: &self.computed_cells,
            };

            if let Some(hb) = any.downcast_ref::<HeadingBlock>() {
                let layered = <HeadingBlock as BlockTrait<Message>>::view(hb, &ctx);
                block_elements.push(layered.base);
                global_line += 1;
                continue;
            }

            if let Some(hr) = any.downcast_ref::<HrBlock>() {
                let layered = <HrBlock as BlockTrait<Message>>::view(hr, &ctx);
                block_elements.push(layered.base);
                global_line += 1;
                continue;
            }

            if let Some(tree) = any.downcast_ref::<TreeBlock>() {
                let layered = <TreeBlock as BlockTrait<Message>>::view(tree, &ctx);
                block_elements.push(layered.base);
                global_line += 1;
                continue;
            }
        }

        let inner: Element<'_, Message, Theme, iced_wgpu::Renderer> = if block_elements.is_empty() {
            iced_widget::container(iced_widget::text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else if single_text_block {
            block_elements.remove(0)
        } else {
            iced_widget::scrollable(
                iced_widget::column(block_elements)
                    .width(Length::Fill)
            )
            .id(WidgetId::new(DOC_SCROLLABLE_ID))
            .height(Length::Fill)
            .into()
        };

        // Whole-document selection visual: tint the entire content area blue.
        // The container only paints background — it doesn't intercept clicks,
        // so the underlying blocks remain interactive (which is intentional:
        // a click anywhere drops the selection back to single-block scope).
        let inner: Element<'_, Message, Theme, iced_wgpu::Renderer> = if self.all_blocks_selected {
            let p = palette::current();
            iced_widget::container(inner)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_theme: &Theme| iced_widget::container::Style {
                    background: Some(Background::Color(Color { a: 0.18, ..p.blue })),
                    border: Border::default(),
                    text_color: None,
                    shadow: iced_wgpu::core::Shadow::default(),
                    snap: false,
                })
                .into()
        } else {
            inner
        };

        // Context menu overlay. Stacked above the main content; positioned
        // at the cursor anchor via container padding from top-left. Clicks
        // anywhere outside the menu hit the main content (still alive on
        // the layer below) AND auto-clear the menu via update()'s top-of-loop
        // drop logic.
        let with_ctx: Element<'_, Message, Theme, iced_wgpu::Renderer> =
            if let Some(menu_state) = &self.context_menu {
                iced_widget::stack![inner, self.context_menu_view(menu_state)].into()
            } else {
                inner
            };

        // Spillover popup overlay — opens when wrap is off and the user
        // clicks a clipped cell. Only one is open at a time per editor.
        if let Some(popup) = self.spillover_view() {
            iced_widget::stack![with_ctx, popup].into()
        } else {
            with_ctx
        }
    }

    /// Find the first table block with an open spillover and render its
    /// popup. Returns None when no spillover is active. The popup is
    /// fixed-positioned at the top-center of the viewport — close enough
    /// for now; cell-anchored positioning is a polish pass away.
    fn spillover_view(&self) -> Option<Element<'_, Message, Theme, iced_wgpu::Renderer>> {
        let p = palette::current();
        let cell_text = self.layout.iter()
            .filter_map(|id| self.registry.get(id))
            .find_map(|block| {
                let tb = block.as_any().downcast_ref::<TableBlock>()?;
                let (r, c) = tb.spillover?;
                tb.rows.get(r).and_then(|row| row.get(c)).cloned()
            })?;

        let copy_btn = iced_widget::button(
            iced_widget::text("Copy")
                .size(11.0)
                .font(syntax::EDITOR_FONT)
        )
        .padding(Padding { top: 2.0, right: 8.0, bottom: 2.0, left: 8.0 })
        .style(context_menu_item_style)
        .on_press(Message::CopyLiteral(cell_text.clone()));

        let close_btn = iced_widget::button(
            iced_widget::text("\u{2715}")
                .size(11.0)
                .font(syntax::EDITOR_FONT)
        )
        .padding(Padding { top: 2.0, right: 8.0, bottom: 2.0, left: 8.0 })
        .style(context_menu_item_style)
        .on_press(Message::FocusedTableOp(TableMessage::CloseSpillover));

        let header = iced_widget::row![
            iced_widget::Space::new().width(Length::Fill).height(Length::Shrink),
            copy_btn,
            close_btn,
        ]
        .spacing(4.0)
        .align_y(iced_wgpu::core::Alignment::Center);

        let body = iced_widget::scrollable(
            iced_widget::container(
                iced_widget::text(cell_text)
                    .size(self.font_size)
                    .font(syntax::EDITOR_FONT)
                    .color(p.text)
            )
            .padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 12.0 })
            .width(Length::Fill)
        )
        .height(Length::Fixed(220.0));

        let popup = iced_widget::container(
            iced_widget::column![header, body].spacing(2.0)
        )
        .padding(Padding { top: 6.0, right: 6.0, bottom: 6.0, left: 6.0 })
        .width(Length::Fixed(420.0))
        .style(move |_theme: &Theme| iced_widget::container::Style {
            background: Some(Background::Color(p.surface0)),
            border: Border {
                color: p.surface1,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: Some(p.text),
            shadow: iced_wgpu::core::Shadow::default(),
            snap: false,
        });

        // Position via Shrink-sized leading spacers (same trick as the
        // context menu overlay) — Fill+padding triggers a viewport-wide
        // re-layout on every popup open and steals events.
        let popup_el: Element<'_, Message, Theme, iced_wgpu::Renderer> = popup.into();
        let v_spacer = iced_widget::Space::new()
            .width(Length::Shrink)
            .height(Length::Fixed(60.0));
        let h_spacer = iced_widget::Space::new()
            .width(Length::Fixed(120.0))
            .height(Length::Shrink);
        Some(
            iced_widget::column![
                v_spacer,
                iced_widget::row![h_spacer, popup_el]
            ]
            .into()
        )
    }

    /// Get (after_line, height) offset pairs for a block's anchored items.
    fn item_offsets(&self, block_id: crate::selection::BlockId) -> Vec<(usize, f32)> {
        let lh = self.line_height();
        self.collect_layer_items(block_id)
            .iter()
            .map(|(line, item)| (*line, item.element_height(lh, self.font_size)))
            .collect()
    }



    /// Collect all layer items for a block into a sorted vec of (after_line, item).
    fn collect_layer_items(&self, block_id: crate::selection::BlockId) -> Vec<(usize, LayerItem<'_>)> {
        let mut items: Vec<(usize, LayerItem<'_>)> = Vec::new();
        for r in &self.eval_results {
            if r.anchor.block_id == block_id {
                items.push((r.anchor.after_line, LayerItem::Inline(r)));
            }
        }
        for ct in &self.computed_tables {
            if ct.anchor.block_id == block_id {
                items.push((ct.anchor.after_line, LayerItem::Table(ct)));
            }
        }
        for ct in &self.computed_trees {
            if ct.anchor.block_id == block_id {
                items.push((ct.anchor.after_line, LayerItem::Tree(ct)));
            }
        }
        for img in &self.computed_images {
            if img.anchor.block_id == block_id {
                items.push((img.anchor.after_line, LayerItem::Image(img)));
            }
        }
        items.sort_by_key(|(line, _)| *line);
        items
    }

    /// Build anchored child Elements for the text widget compositor.
    /// Converts layer items into AnchoredItem structs with pre-built Elements.
    fn build_anchored_items<'a>(
        &'a self,
        block_id: crate::selection::BlockId,
    ) -> Vec<AnchoredItem<'a, Message>> {
        let p = palette::current();
        let lh = self.line_height();
        let items = self.collect_layer_items(block_id);
        let mut anchored = Vec::with_capacity(items.len());

        for (after_line, item) in &items {
            match item {
                LayerItem::Inline(r) => {
                    let color = if r.is_error { p.red } else { p.green };
                    let inner = iced_widget::container(
                        iced_widget::text(&r.text)
                            .font(syntax::EDITOR_FONT)
                            .size(self.font_size)
                            .color(oklab::lighten_for_size(color, self.font_size))
                    )
                    .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 40.0 })
                    .width(Length::Fill);
                    // Errors don't carry a copyable result value, so they
                    // don't get the gesture wrapper.
                    let el: Element<'a, Message, Theme, iced_wgpu::Renderer> = if r.is_error {
                        inner.into()
                    } else {
                        let bid = r.anchor.block_id;
                        let line = r.anchor.after_line;
                        MouseArea::new(inner)
                            .on_press(Message::InlineResultPress { block_id: bid, after_line: line })
                            .on_release(Message::InlineResultRelease)
                            .on_double_click(Message::InlineResultDoubleClick { block_id: bid, after_line: line })
                            .into()
                    };
                    anchored.push(AnchoredItem {
                        after_line: *after_line,
                        height: item.element_height(lh, self.font_size),
                        element: el,
                    });
                }
                LayerItem::Table(ct) => {
                    let mut table_rows: Vec<Element<'a, Message, Theme, iced_wgpu::Renderer>> = Vec::new();
                    for (ri, row) in ct.rows.iter().enumerate() {
                        let is_header = ri == 0;
                        let cells: Vec<Element<'a, Message, Theme, iced_wgpu::Renderer>> = row.iter()
                            .enumerate()
                            .map(|(ci, cell)| {
                                let cw = ct.col_widths.get(ci).copied().unwrap_or(80.0);
                                let mut txt = iced_widget::text(cell)
                                    .font(syntax::EDITOR_FONT)
                                    .size(self.font_size)
                                    .color(oklab::lighten_for_size(p.text, self.font_size));
                                if is_header {
                                    txt = txt.font(Font { weight: iced_wgpu::core::font::Weight::Bold, ..syntax::EDITOR_FONT });
                                }
                                iced_widget::container(txt)
                                .width(Length::Fixed(cw))
                                .padding(Padding { top: 2.0, right: 8.0, bottom: 2.0, left: 8.0 })
                                .style(move |_theme: &Theme| {
                                    let bg_alpha = if is_header { 0.12 } else { 0.06 };
                                    container::Style {
                                        background: Some(Background::Color(Color { a: bg_alpha, ..p.surface1 })),
                                        border: Border { color: p.surface1, width: 0.5, radius: border::Radius::default() },
                                        text_color: None,
                                        shadow: Shadow::default(),
                                        snap: false,
                                    }
                                })
                                .into()
                            })
                            .collect();
                        table_rows.push(iced_widget::row(cells).into());
                    }
                    let el: Element<'a, Message, Theme, iced_wgpu::Renderer> =
                        iced_widget::container(iced_widget::column(table_rows))
                            .padding(Padding { top: 4.0, right: 8.0, bottom: 4.0, left: 40.0 })
                            .width(Length::Fill)
                            .into();
                    anchored.push(AnchoredItem {
                        after_line: *after_line,
                        height: item.element_height(lh, self.font_size),
                        element: el,
                    });
                }
                LayerItem::Tree(ct) => {
                    let el = crate::tree_block::build(&ct.data, self.font_size);
                    anchored.push(AnchoredItem {
                        after_line: *after_line,
                        height: item.element_height(lh, self.font_size),
                        element: el,
                    });
                }
                LayerItem::Image(img) => {
                    let el: Element<'a, Message, Theme, iced_wgpu::Renderer> =
                        if let Some(entry) = self.image_cache.get(&img.src) {
                            iced_widget::container(
                                iced_widget::image(entry.handle.clone())
                                    .width(Length::Fill)
                                    .height(Length::Fixed(img.display_height))
                            )
                            .padding(Padding { top: IMAGE_VPAD, right: 8.0, bottom: IMAGE_VPAD, left: 40.0 })
                            .width(Length::Fill)
                            .into()
                        } else {
                            // Placeholder while loading or on failure.
                            iced_widget::container(
                                iced_widget::text(format!("[image: {}]", img.alt))
                                    .font(syntax::EDITOR_FONT)
                                    .size(self.font_size)
                                    .color(p.overlay0)
                            )
                            .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 40.0 })
                            .width(Length::Fill)
                            .into()
                        };
                    anchored.push(AnchoredItem {
                        after_line: *after_line,
                        height: item.element_height(lh, self.font_size),
                        element: el,
                    });
                }
            }
        }

        anchored
    }

    /// Build the context menu overlay for a right-clicked cell. Returns a
    /// fill container that holds the actual menu in the top-left corner of
    /// a padded region — padding (top: y, left: x) anchors it to the click.
    fn context_menu_view(
        &self,
        state: &ContextMenuState,
    ) -> Element<'_, Message, Theme, iced_wgpu::Renderer> {
        let p = palette::current();
        let block_idx = state.block_idx;

        let item = |label: &str, msg: Message| -> Element<'_, Message, Theme, iced_wgpu::Renderer> {
            iced_widget::button(
                iced_widget::text(label.to_string())
                    .size(12.0)
                    .font(syntax::EDITOR_FONT)
            )
            .width(Length::Fill)
            .padding(Padding { top: 4.0, right: 12.0, bottom: 4.0, left: 12.0 })
            .style(context_menu_item_style)
            .on_press(msg)
            .into()
        };

        let separator: Element<'_, Message, Theme, iced_wgpu::Renderer> =
            iced_widget::container(iced_widget::text(""))
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(move |_theme: &Theme| iced_widget::container::Style {
                    background: Some(Background::Color(p.surface1)),
                    border: Border::default(),
                    text_color: None,
                    shadow: iced_wgpu::core::Shadow::default(),
                    snap: false,
                })
                .into();

        let menu_items: Vec<Element<'_, Message, Theme, iced_wgpu::Renderer>> = vec![
            item(
                "Insert row above",
                Message::FocusedTableOp(TableMessage::InsertRowAbove),
            ),
            item(
                "Insert row below",
                Message::FocusedTableOp(TableMessage::InsertRowBelow),
            ),
            item("Delete row", Message::FocusedTableOp(TableMessage::DeleteRow)),
            separator,
            item(
                "Insert column left",
                Message::FocusedTableOp(TableMessage::InsertColLeft),
            ),
            item(
                "Insert column right",
                Message::FocusedTableOp(TableMessage::InsertColRight),
            ),
            item(
                "Delete column",
                Message::FocusedTableOp(TableMessage::DeleteCol),
            ),
            iced_widget::container(iced_widget::text(""))
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(move |_theme: &Theme| iced_widget::container::Style {
                    background: Some(Background::Color(p.surface1)),
                    border: Border::default(),
                    text_color: None,
                    shadow: iced_wgpu::core::Shadow::default(),
                    snap: false,
                })
                .into(),
            item(
                "Select all",
                Message::TableMsg(block_idx, TableMessage::SelectAll),
            ),
            {
                let wrap_on = self.table_block_at(block_idx)
                    .map(|tb| tb.wrap)
                    .unwrap_or(true);
                item(
                    if wrap_on { "Wrap: on" } else { "Wrap: off" },
                    Message::TableMsg(block_idx, TableMessage::ToggleWrap),
                )
            },
            item("Delete table", Message::DeleteCurrentTable),
        ];

        let menu = iced_widget::container(
            iced_widget::column(menu_items).spacing(0.0).width(Length::Fixed(180.0))
        )
        .style(move |_theme: &Theme| iced_widget::container::Style {
            background: Some(Background::Color(p.surface0)),
            border: Border {
                color: p.surface1,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: Some(p.text),
            shadow: iced_wgpu::core::Shadow::default(),
            snap: false,
        });

        // Position via shrink-sized leading spacers, NOT a Fill container with
        // padding. Fill+padding triggers a viewport-wide re-layout on every
        // menu open, which jumps the scrollable and swallows events on the
        // overlay layer. Shrink sizing keeps the overlay limited to its own
        // bounds, so clicks outside pass through to the scrollable beneath.
        let menu_element: Element<'_, Message, Theme, iced_wgpu::Renderer> = menu.into();
        let v_spacer = iced_widget::Space::new()
            .width(Length::Shrink)
            .height(Length::Fixed(state.y));
        let h_spacer = iced_widget::Space::new()
            .width(Length::Fixed(state.x))
            .height(Length::Shrink);
        iced_widget::column![
            v_spacer,
            iced_widget::row![h_spacer, menu_element]
        ]
        .into()
    }

    fn find_bar(&self) -> Element<'_, Message, Theme, iced_wgpu::Renderer> {
        let p = palette::current();

        let search_input = text_input::TextInput::new("Find...", &self.find.query)
            .on_input(Message::FindQueryChanged)
            .on_submit(Message::FindNext)
            .id(WidgetId::new(FIND_INPUT_ID))
            .font(Font::MONOSPACE)
            .size(13.0)
            .padding(Padding { top: 3.0, right: 6.0, bottom: 3.0, left: 6.0 })
            .width(Length::FillPortion(3))
            .style(find_input_style);

        let replace_input = text_input::TextInput::new("Replace...", &self.find.replacement)
            .on_input(Message::ReplaceQueryChanged)
            .on_submit(Message::ReplaceOne)
            .id(WidgetId::new(REPLACE_INPUT_ID))
            .font(Font::MONOSPACE)
            .size(13.0)
            .padding(Padding { top: 3.0, right: 6.0, bottom: 3.0, left: 6.0 })
            .width(Length::FillPortion(3))
            .style(find_input_style);

        let match_label = if self.find.matches.is_empty() {
            if self.find.query.is_empty() {
                String::new()
            } else {
                "0/0".into()
            }
        } else {
            format!("{}/{}", self.find.current + 1, self.find.matches.len())
        };

        let label: Element<'_, Message, Theme, iced_wgpu::Renderer> =
            iced_widget::text(match_label)
                .font(Font::MONOSPACE)
                .size(11.0)
                .color(oklab::lighten_for_size(p.overlay1, 11.0))
                .into();

        let btn = |txt: String, msg: Message| -> Element<'_, Message, Theme, iced_wgpu::Renderer> {
            iced_widget::button(
                iced_widget::text(txt).font(Font::MONOSPACE).size(11.0)
            )
            .on_press(msg)
            .padding(Padding { top: 2.0, right: 6.0, bottom: 2.0, left: 6.0 })
            .style(find_btn_style)
            .into()
        };

        let row = iced_widget::row![
            search_input,
            label,
            btn("Prev".into(), Message::FindPrev),
            btn("Next".into(), Message::FindNext),
            replace_input,
            btn("Repl".into(), Message::ReplaceOne),
            btn("All".into(), Message::ReplaceAll),
            btn("X".into(), Message::HideFind),
        ]
        .spacing(4.0)
        .align_y(alignment::Vertical::Center);

        iced_widget::container(row)
            .width(Length::Fill)
            .padding(Padding { top: 4.0, right: 8.0, bottom: 4.0, left: 8.0 })
            .style(|_theme: &Theme| {
                let p = palette::current();
                container::Style {
                    background: Some(Background::Color(p.mantle)),
                    border: Border::default(),
                    text_color: None,
                    shadow: Shadow::default(),
                    snap: false,
                }
            })
            .into()
    }
}

fn find_input_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
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

fn find_btn_style(
    _theme: &Theme,
    _status: iced_widget::button::Status,
) -> iced_widget::button::Style {
    let p = palette::current();
    iced_widget::button::Style {
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

fn context_menu_item_style(
    _theme: &Theme,
    status: iced_widget::button::Status,
) -> iced_widget::button::Style {
    let p = palette::current();
    let bg = match status {
        iced_widget::button::Status::Hovered => Some(Background::Color(p.surface1)),
        iced_widget::button::Status::Pressed => Some(Background::Color(p.surface2)),
        _ => None,
    };
    iced_widget::button::Style {
        background: bg,
        text_color: p.text,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: false,
    }
}

// Strip obsolete inline-result lines from documents saved before eval
// results moved into anchored child elements.

fn is_result_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with(RESULT_PREFIX) || trimmed.starts_with(ERROR_PREFIX)
}

fn strip_result_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !is_result_line(l)).collect();
    let mut result = lines.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn block_editor_id(block_id: u64) -> WidgetId {
    WidgetId::from(format!("block_editor_{block_id}"))
}

fn parse_let_binding(line: &str) -> Option<String> {
    let rest = line.strip_prefix("let ")?;
    let eq_pos = rest.find('=')?;
    if rest.as_bytes().get(eq_pos + 1) == Some(&b'=') {
        return None;
    }
    let name_part = rest[..eq_pos].trim();
    let name = if let Some(colon) = name_part.find(':') {
        name_part[..colon].trim()
    } else {
        name_part
    };
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

fn macos_key_binding(key_press: KeyPress) -> Option<Binding<Message>> {
    let KeyPress { key, modifiers, status, .. } = &key_press;

    if !matches!(status, Status::Focused { .. }) {
        return None;
    }

    match key.as_ref() {
        keyboard::Key::Character("z") if modifiers.logo() && modifiers.shift() => {
            Some(Binding::Custom(Message::Redo))
        }
        keyboard::Key::Character("z") if modifiers.logo() => {
            Some(Binding::Custom(Message::Undo))
        }
        keyboard::Key::Character("=" | "+") if modifiers.logo() => {
            Some(Binding::Custom(Message::ZoomIn))
        }
        keyboard::Key::Character("-") if modifiers.logo() => {
            Some(Binding::Custom(Message::ZoomOut))
        }
        // Cmd+0 lives in handle.rs now (FixUp); Cmd+Shift+0 resets zoom.
        keyboard::Key::Character("[") if !modifiers.logo() && !modifiers.alt() && !modifiers.control() => {
            Some(Binding::Custom(Message::AutoPair("[", "]")))
        }
        keyboard::Key::Character("{") if !modifiers.logo() && !modifiers.alt() && !modifiers.control() => {
            Some(Binding::Custom(Message::AutoPair("{", "}")))
        }
        keyboard::Key::Named(key::Named::Backspace) if modifiers.alt() => {
            Some(Binding::Sequence(vec![
                Binding::Select(Motion::WordLeft),
                Binding::Backspace,
            ]))
        }
        keyboard::Key::Named(key::Named::Delete) if modifiers.alt() => {
            Some(Binding::Sequence(vec![
                Binding::Select(Motion::WordRight),
                Binding::Delete,
            ]))
        }
        keyboard::Key::Named(key::Named::ArrowUp) if modifiers.logo() && modifiers.shift() => {
            Some(Binding::Select(Motion::DocumentStart))
        }
        keyboard::Key::Named(key::Named::ArrowDown) if modifiers.logo() && modifiers.shift() => {
            Some(Binding::Select(Motion::DocumentEnd))
        }
        keyboard::Key::Named(key::Named::ArrowUp) if modifiers.logo() => {
            Some(Binding::Move(Motion::DocumentStart))
        }
        keyboard::Key::Named(key::Named::ArrowDown) if modifiers.logo() => {
            Some(Binding::Move(Motion::DocumentEnd))
        }
        keyboard::Key::Named(key::Named::Tab)
            if !modifiers.logo() && !modifiers.alt() && !modifiers.control() && modifiers.shift() =>
        {
            Some(Binding::Custom(Message::OutdentTab))
        }
        keyboard::Key::Named(key::Named::Tab)
            if !modifiers.logo() && !modifiers.alt() && !modifiers.control() =>
        {
            Some(Binding::Custom(Message::IndentTab))
        }
        _ => Binding::from_key_press(key_press),
    }
}

fn lang_from_extension(ext: &str) -> Option<String> {
    let lang = match ext {
        "rs" => "rust",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "go" => "go",
        "rb" => "ruby",
        "sh" | "bash" | "zsh" => "bash",
        "java" => "java",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "json" => "json",
        "lua" => "lua",
        "php" => "php",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "swift" => "swift",
        "zig" => "zig",
        "sql" => "sql",
        "mk" => "make",
        "cord" | "cordial" => "rust",
        _ => return None,
    };
    Some(lang.to_string())
}

fn detect_lang_from_content(text: &str) -> Option<String> {
    let keywords = ["fn ", "let ", "if ", "else ", "while ", "for ", "/="];
    let mut hits = 0;
    for line in text.lines().take(50) {
        let trimmed = line.trim();
        for kw in &keywords {
            if trimmed.starts_with(kw) || trimmed.contains(&format!(" {kw}")) {
                hits += 1;
            }
        }
        if hits >= 2 {
            return Some("rust".into());
        }
    }
    None
}

fn leading_whitespace(line: &str) -> &str {
    let end = line.len() - line.trim_start().len();
    &line[..end]
}

/// Count consecutive trailing occurrences of `c` at the end of `s`.
fn count_trailing_char(s: &str, c: char) -> usize {
    s.chars().rev().take_while(|&x| x == c).count()
}

/// Count consecutive leading occurrences of `c` at the start of `s`.
fn count_leading_char(s: &str, c: char) -> usize {
    s.chars().take_while(|&x| x == c).count()
}

/// Convert an iced `Position { line, column }` to a byte offset within
/// `text`. column is interpreted as char count (cosmic-text convention).
fn byte_offset_for_cursor(text: &str, pos: &text_widget::Position) -> usize {
    let mut byte = 0usize;
    let mut line_idx = 0usize;
    for line in text.split_inclusive('\n') {
        if line_idx == pos.line {
            let col = pos.column;
            for (i, _) in line.char_indices().take(col) {
                byte += line.as_bytes()[i..i + 1].len();
            }
            // Walk col chars precisely.
            let mut walked = 0usize;
            for (ci, _) in line.char_indices() {
                if walked == col { return byte.saturating_sub(line.len()) + ci; }
                walked += 1;
            }
            // col >= line length: clamp to end of line content (before \n).
            return byte + line.trim_end_matches('\n').len();
        }
        byte += line.len();
        line_idx += 1;
    }
    text.len()
}

/// Inverse of `byte_offset_for_cursor`. Returns (line, column).
fn line_col_for_byte(text: &str, byte: usize) -> (usize, usize) {
    let mut acc = 0usize;
    let mut line_idx = 0usize;
    for line in text.split_inclusive('\n') {
        if byte < acc + line.len() {
            let local = &line[..byte - acc];
            return (line_idx, local.chars().count());
        }
        acc += line.len();
        line_idx += 1;
    }
    let last_line = text.lines().count().saturating_sub(1);
    (last_line, text.lines().last().map(|l| l.chars().count()).unwrap_or(0))
}

/// Walk `text` left-to-right tracking a delimiter stack. Return the
/// `close` char of the innermost still-open pair, or None if balanced.
/// Pairs tracked: `()`, `[]`, `{}`. (Quotes/HTML are intentionally out
/// of scope — too ambiguous in markdown.)
fn innermost_unclosed_delim(text: &str) -> Option<char> {
    let mut stack: Vec<char> = Vec::new();
    for c in text.chars() {
        match c {
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '{' => stack.push('}'),
            ')' | ']' | '}' => {
                if stack.last() == Some(&c) { stack.pop(); }
            }
            _ => {}
        }
    }
    stack.last().copied()
}

/// Find the byte offset of the next outer scope's CLOSING delimiter
/// after `pos`. Used by FixUp to step the cursor out one scope.
fn next_closing_delim_after(text: &str, pos: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let bytes = text.as_bytes();
    for i in pos..bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 { return Some(i); }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Parse a markdown image reference `![alt](src)` from a line. Returns
/// `(alt, src)` if found. Only matches if the `![` is the first
/// non-whitespace on the line (inline images inside text are not rendered
/// as block-level anchored items).
fn parse_image_ref(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("![") { return None; }
    let after_bang = &trimmed[2..];
    let close_bracket = after_bang.find(']')?;
    let alt = after_bang[..close_bracket].to_string();
    let rest = &after_bang[close_bracket + 1..];
    if !rest.starts_with('(') { return None; }
    let close_paren = rest.find(')')?;
    let src = rest[1..close_paren].trim().to_string();
    if src.is_empty() { return None; }
    Some((alt, src))
}

/// Load an image. `src` may be `http(s)://`, `~/…`, or a filesystem path.
/// Result is always PNG bytes regardless of source format.
fn load_image_from_path(src: &str) -> Option<ImageCacheEntry> {
    let raw = if src.starts_with("http://") || src.starts_with("https://") {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build()
            .into();
        let mut resp = agent.get(src).call().ok()?;
        resp.body_mut().read_to_vec().ok()?
    } else {
        let path = if src.starts_with("~/") {
            dirs::home_dir()?.join(&src[2..])
        } else {
            std::path::PathBuf::from(src)
        };
        std::fs::read(&path).ok()?
    };
    let img = image::load_from_memory(&raw).ok()?;
    let (width, height) = (img.width(), img.height());
    let rgba = img.into_rgba8();
    let pixels = rgba.into_raw();
    let handle = iced_widget::image::Handle::from_rgba(width, height, pixels);
    Some(ImageCacheEntry { handle, width, height })
}

/// Encode a clipboard image (RGBA from `arboard`) to PNG and write it into
/// `~/.acord/cache/images/{hash}.png`. Returns the absolute path as a
/// String suitable for embedding in a `![]( … )` markdown reference.
/// Content-addressed: re-pasting the same pixels reuses the same file.
pub fn write_clipboard_image_to_cache(img: &arboard::ImageData) -> Option<String> {
    let dir = dirs::home_dir()?.join(".acord").join("cache").join("images");
    std::fs::create_dir_all(&dir).ok()?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    img.width.hash(&mut hasher);
    img.height.hash(&mut hasher);
    img.bytes.hash(&mut hasher);
    let name = format!("{:016x}.png", hasher.finish());
    let path = dir.join(&name);

    if !path.exists() {
        let buf = image::RgbaImage::from_raw(
            img.width as u32,
            img.height as u32,
            img.bytes.to_vec(),
        )?;
        buf.save_with_format(&path, image::ImageFormat::Png).ok()?;
    }
    Some(path.to_string_lossy().into_owned())
}


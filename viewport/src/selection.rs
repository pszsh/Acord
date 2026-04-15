//! Central selection model.
//!
//! Every selectable element in the document — a cell, a row, a column, a line,
//! a character range, or a whole block — is addressed by a `NodePath`. The
//! `Selection` enum holds whatever the user has currently selected and is the
//! single source of truth (no per-block selection state).
//!
//! Cursorline highlight, table cell selection, multi-line picks, and cross-block
//! ranges are all the same primitive at different scopes.

pub type BlockId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextPos {
    pub line: usize,
    pub col: usize,
}

impl TextPos {
    pub const ZERO: Self = TextPos { line: 0, col: 0 };
}

/// Address of any selectable element inside a block. Which variants are valid
/// depends on the block kind: a heading only meaningfully has `Whole`; a table
/// has `Cell`/`CellRect`/`Row`/`Col`; a text block has `Line`/`LineRange`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InnerPath {
    /// The whole block (heading, HR, tree, or "this entire table/text block").
    Whole,
    /// A specific line in a text-bearing block (cursorline target).
    Line(usize),
    /// A character range within a text-bearing block.
    LineRange { start: TextPos, end: TextPos },
    /// A cell at (row, col) in a table.
    Cell { row: usize, col: usize },
    /// A rectangular range of cells.
    CellRect { r0: usize, c0: usize, r1: usize, c1: usize },
    /// An entire row of a table.
    Row(usize),
    /// An entire column of a table.
    Col(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodePath {
    pub block_id: BlockId,
    pub inner: InnerPath,
}

impl NodePath {
    pub fn block(block_id: BlockId) -> Self {
        Self { block_id, inner: InnerPath::Whole }
    }

    pub fn line(block_id: BlockId, line: usize) -> Self {
        Self { block_id, inner: InnerPath::Line(line) }
    }

    pub fn cell(block_id: BlockId, row: usize, col: usize) -> Self {
        Self { block_id, inner: InnerPath::Cell { row, col } }
    }
}

/// The single selection state for the entire document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Selection {
    /// Nothing selected.
    #[default]
    None,
    /// Cursor anchor at a single path. No range. Cursorline target.
    Caret(NodePath),
    /// A range from anchor to head. The two paths can live in different blocks.
    Range { anchor: NodePath, head: NodePath },
    /// An independent set (Cmd-click multi-cell, multi-line picks).
    Set(Vec<NodePath>),
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        matches!(self, Selection::None)
    }

    /// True if the given path is a member of this selection. Iterative — no
    /// recursion. Range membership beyond exact endpoints (e.g. "is cell
    /// (2,3) inside the rect from (1,1) to (3,5)?") is the consumer block's
    /// responsibility; this only does point-equality.
    pub fn contains_path(&self, path: &NodePath) -> bool {
        match self {
            Selection::None => false,
            Selection::Caret(p) => p == path,
            Selection::Range { anchor, head } => anchor == path || head == path,
            Selection::Set(paths) => paths.iter().any(|p| p == path),
        }
    }

    /// True if any path in the selection lives in the given block.
    pub fn touches_block(&self, block_id: BlockId) -> bool {
        match self {
            Selection::None => false,
            Selection::Caret(p) => p.block_id == block_id,
            Selection::Range { anchor, head } => {
                anchor.block_id == block_id || head.block_id == block_id
            }
            Selection::Set(paths) => paths.iter().any(|p| p.block_id == block_id),
        }
    }
}

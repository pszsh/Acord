use iced_wgpu::core::widget::Id as WidgetId;
use iced_wgpu::core::text::Wrapping;
use iced_wgpu::core::{
    Background, Border, Color, Element, Font, Length, Padding, Point, Shadow, Theme,
};
use iced_widget::button;
use iced_widget::container;
use iced_widget::text;
use iced_widget::text_input;
use iced_widget::MouseArea;
use iced_wgpu::core::mouse::Interaction;

use crate::block::{Block, BlockCommand, LayeredView, ViewCtx};
use crate::oklab;
use crate::palette;
use crate::selection::{BlockId, InnerPath};
use crate::syntax::EDITOR_FONT;

const MIN_COL_WIDTH: f32 = 60.0;
const DEFAULT_COL_WIDTH: f32 = 120.0;
/// Sanity cap for double-click auto-fit. Drag past it for explicit override.
const AUTO_FIT_MAX: f32 = 600.0;
/// Approximate monospace glyph advance at the editor's default font size.
/// Used when the renderer's actual font_size isn't available (e.g. during
/// table construction). Tracks `font_size * 0.6` for size 13.
const APPROX_CHAR_W: f32 = 7.8;
const CELL_PADDING: Padding = Padding {
    top: 2.0,
    right: 8.0,
    bottom: 2.0,
    left: 8.0,
};
const ROW_NUMBER_WIDTH: f32 = 26.0;
const PLUS_BUTTON_THICKNESS: f32 = 14.0;
/// Default per-row height. Calibrated to match the natural height of an iced
/// text_input at size 13 with CELL_PADDING — 13pt font + ~1.3 line height +
/// 4px vertical padding + 2px border ≈ 23.
const ROW_HEIGHT_ESTIMATE: f32 = 23.0;
const MIN_ROW_HEIGHT: f32 = 18.0;
const ROW_RESIZE_HANDLE_HEIGHT: f32 = 3.0;
/// Vertical gap between rows. Slightly tighter than RESIZE_HANDLE_WIDTH —
/// the horizontal gap stays at 4 so the resize handle has enough hit area.
const CELL_GAP_Y: f32 = 2.0;

#[derive(Debug, Clone, Copy)]
pub enum ReorderDrag {
    Column { from: usize, target: usize, start_x: f32 },
    Row { from: usize, target: usize, start_y: f32 },
}

/// Modifier state at the moment of a selection click or drag. Computed by
/// editor.rs from its tracked modifier state, then passed in via the
/// table-state mutation methods. The modifier determines the OPERATION on
/// the existing selection — single-cell click vs rectangular drag is
/// orthogonal to the modifier and determined by gesture (click = 1 cell,
/// drag = rectangle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// No modifier — selection becomes only the clicked cell (or drag rect).
    Replace,
    /// Cmd — toggle (XOR): each touched cell is added if absent, removed if
    /// present.
    Toggle,
    /// Shift — add (union): each touched cell is added; nothing is removed.
    Extend,
    /// Cmd+Shift — remove (subtract): each touched cell is removed; nothing
    /// is added.
    Subtract,
}

#[derive(Debug, Clone)]
pub enum TableMessage {
    CellChanged(usize, usize, String),
    FocusCell(usize, usize),
    /// Single click on a cell — select it but stay out of edit mode. Cell
    /// renders as static text with a tinted background. The Numbers/Excel
    /// "I might do something to this cell" gesture.
    SelectCell(usize, usize),
    /// Double click on a cell — enter edit mode. Cell renders as a text_input
    /// and gets iced focus. Same target as `SelectCell`, but the editor's
    /// `editing` field is also set to this cell's `NodePath`.
    EditCell(usize, usize),
    /// Click on the corner-cell delete affordance. The editor's TableMsg arm
    /// promotes this to a top-level `DeleteCurrentTable` so undo + block
    /// removal go through the same path as the keyboard shortcut.
    DeleteTable,
    /// Click on the corner-cell drag handle. Selects the whole table —
    /// every cell renders highlighted, plain Backspace clears all cells,
    /// Cmd+Backspace deletes the table outright. Single-cell click clears
    /// this back to single-cell selection.
    SelectAll,
    /// Plain Backspace/Delete with `table_selected == true` — empty every
    /// cell's content but leave the row/column structure intact.
    ClearAll,
    /// Right-click on a cell. Promoted in editor.rs to a top-level
    /// `ShowContextMenu` carrying the cursor anchor position.
    ContextMenu(usize, usize),
    /// Mouse entered a cell. Used by the drag-select system: when a drag
    /// is armed (post-click, pre-release), each `CellEnter` extends the
    /// selection rectangle to that cell. No-op when no drag is active.
    CellEnter(usize, usize),
    AddRow,
    AddColumn,
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    InsertColLeft,
    InsertColRight,
    DeleteCol,
    CursorMove(f32, f32),
    BeginColResize(usize),
    BeginRowResize(usize),
    BeginColReorder(usize),
    BeginRowReorder(usize),
    EndDrag,
    /// Double-click on the column resize handle: fit width to the widest
    /// cell content in the column. f32 carries the current font_size so the
    /// pixel width tracks zoom level.
    AutoFitCol(usize, f32),
    /// Toggle the per-table word-wrap mode. Wrap on (default): rows grow to
    /// fit; nothing clips. Wrap off: cells clip; spillover popup reveals
    /// content on click or 3s hover.
    ToggleWrap,
    /// Open the spillover popup for a cell. Replaces any existing spillover
    /// (only one open per table at a time). Click on a clipped cell when
    /// `wrap == false`.
    OpenSpillover(usize, usize),
    /// Close the active spillover popup. Click outside, ESC, or any cell
    /// selection change.
    CloseSpillover,
    /// Click on a column-header sort arrow: cycles that column through
    /// Neutral → Asc → Desc → Neutral and re-applies the composite sort.
    CycleSort(usize),
}

/// Trait-implementing block for tables. Owns all the per-table mutable state
/// directly (rows, widths, focus, drags, selection) — no separate `TableState`
/// HashMap. Lives in `EditorState::blocks` as a `Box<dyn Block>`.
pub struct TableBlock {
    pub id: BlockId,
    pub start_line: usize,
    /// User-assigned name from a ### or #### heading directly above this table.
    /// H3 = global scope, H4 = block-scoped. None for unnamed tables.
    pub table_name: Option<String>,
    pub rows: Vec<Vec<String>>,
    pub col_widths: Vec<f32>,
    /// Per-row explicit height override. None means use ROW_HEIGHT_ESTIMATE.
    pub row_heights: Vec<Option<f32>>,
    /// Last cell that had focus. PRESERVED across blur so keyboard shortcuts
    /// (Cmd+Opt+Arrow, Cmd+Shift+T, Tab/Enter) keep targeting the right cell
    /// even when the user has clicked elsewhere in the document.
    pub focused_cell: Option<(usize, usize)>,
    /// True only on frames where iced's focus is currently inside one of this
    /// table's cells. Used for "active editing chrome" (ABCD/123 headers) which
    /// must DISAPPEAR when the user clicks out, even though focused_cell is kept.
    pub is_active: bool,
    /// Whole-table selection mode. Set by clicking the corner select-all
    /// affordance. All cells render highlighted; plain Backspace/Delete
    /// clears every cell's content; Cmd+Backspace deletes the whole table.
    /// Cleared the moment a single cell is clicked.
    pub table_selected: bool,
    /// Eval-result tables set this so the widget disables cell editing while
    /// keeping selection and Cmd-C intact. Markdown tables keep it false.
    pub read_only: bool,
    /// True for eval-result tables (regenerated on every eval). Skipped during
    /// markdown serialization.
    pub is_eval_result: bool,
    /// Active column-resize drag: (col index, original width at drag start, drag-start x).
    pub resize_drag: Option<(usize, f32, f32)>,
    /// Active row-resize drag: (row index, original height at drag start, drag-start y).
    pub row_resize_drag: Option<(usize, f32, f32)>,
    pub reorder_drag: Option<ReorderDrag>,
    pub selection: std::collections::HashSet<(usize, usize)>,
    pub selection_anchor: Option<(usize, usize)>,
    /// Drag-rectangle origin. Set on cell click; cleared on mouse release.
    /// When `Some`, every cell-enter event extends the rectangle.
    pub drag_select_start: Option<(usize, usize)>,
    /// SelectionMode captured at the moment the drag started — keeps the
    /// modifier semantics constant for the duration of the drag, even if
    /// the user releases the modifier mid-drag.
    pub drag_select_mode: Option<SelectionMode>,
    /// Selection state at the moment the drag started. Each `cell_enter`
    /// during the drag recomputes the selection by re-applying the mode
    /// against the baseline + the current rectangle. Without a baseline,
    /// repeated rectangle redraws would compound (e.g. Toggle mode would
    /// flip cells multiple times as the rectangle grows and shrinks).
    pub drag_select_baseline: std::collections::HashSet<(usize, usize)>,
    pub last_cursor_x: f32,
    pub last_cursor_y: f32,
    /// Composite sort. Each entry is `(col_idx, dir)`. The first entry is
    /// the dominant sort key; later entries break ties within groups of
    /// equal dominant values. Empty = no sort active (visual neutral).
    pub sort_priority: Vec<(usize, SortDir)>,
    /// When true (default), cell text word-wraps and each row grows to fit
    /// the tallest wrapped cell — no content ever clips. When false, content
    /// is hard-clipped at the cell bounds and the spillover popup reveals
    /// the full text on click or hover.
    pub wrap: bool,
    /// Currently spilled-over cell, if any. Only one popup at a time per
    /// table. Set by click or 3s hover when `wrap == false`.
    pub spillover: Option<(usize, usize)>,
    /// Cell currently being hovered with the dwell timer running. Captured
    /// on CellEnter; consumed by `tick_hover` after the 3s threshold to
    /// open the spillover popup. Cleared on any meaningful interaction
    /// (click, edit, drag, scroll) so a brief mouseover never triggers.
    pub hover_armed: Option<(usize, usize, std::time::Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir { Asc, Desc }

impl TableBlock {
    pub fn new(id: BlockId, rows: Vec<Vec<String>>, start_line: usize) -> Self {
        Self::build(id, rows, start_line, false, None)
    }

    pub fn new_eval(id: BlockId, rows: Vec<Vec<String>>, start_line: usize) -> Self {
        Self::build(id, rows, start_line, true, None)
    }

    fn build(
        id: BlockId,
        rows: Vec<Vec<String>>,
        start_line: usize,
        is_eval_result: bool,
        col_widths_override: Option<Vec<f32>>,
    ) -> Self {
        let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let col_widths = col_widths_override.unwrap_or_else(|| {
            // For eval result tables, size columns to fit content; for markdown
            // tables, fit each column to its header so the new wrap-on default
            // gives short headers a tight column and lets long body text wrap.
            if is_eval_result {
                (0..col_count)
                    .map(|ci| {
                        let max_len = rows
                            .iter()
                            .map(|r| r.get(ci).map(|s| s.len()).unwrap_or(0))
                            .max()
                            .unwrap_or(0);
                        ((max_len as f32) * 9.0).max(MIN_COL_WIDTH).min(300.0)
                    })
                    .collect()
            } else {
                let header = rows.first().map(|r| r.as_slice()).unwrap_or(&[]);
                (0..col_count)
                    .map(|ci| {
                        let chars = header.get(ci).map(|s| s.chars().count()).unwrap_or(0);
                        let raw = chars as f32 * APPROX_CHAR_W
                            + CELL_PADDING.left + CELL_PADDING.right;
                        raw.max(DEFAULT_COL_WIDTH).min(AUTO_FIT_MAX)
                    })
                    .collect()
            }
        });
        let row_count = rows.len();
        Self {
            id,
            start_line,
            table_name: None,
            rows,
            col_widths,
            row_heights: vec![None; row_count],
            focused_cell: None,
            is_active: false,
            table_selected: false,
            read_only: is_eval_result,
            is_eval_result,
            resize_drag: None,
            row_resize_drag: None,
            reorder_drag: None,
            selection: std::collections::HashSet::new(),
            selection_anchor: None,
            drag_select_start: None,
            drag_select_mode: None,
            drag_select_baseline: std::collections::HashSet::new(),
            last_cursor_x: 0.0,
            last_cursor_y: 0.0,
            sort_priority: Vec::new(),
            wrap: true,
            spillover: None,
            hover_armed: None,
        }
    }

    /// 3s dwell threshold for hover-to-spillover. Independent of the eval
    /// debounce so a slow-typing user doesn't accidentally trigger popups.
    pub fn check_hover_spillover(&mut self) -> bool {
        if self.wrap { self.hover_armed = None; return false; }
        let Some((r, c, started)) = self.hover_armed else { return false; };
        if started.elapsed().as_millis() < 3000 { return false; }
        if self.spillover == Some((r, c)) { self.hover_armed = None; return false; }
        if r >= self.rows.len() || c >= self.col_widths.len() {
            self.hover_armed = None;
            return false;
        }
        self.spillover = Some((r, c));
        self.hover_armed = None;
        true
    }

    /// Has a hover dwell timer running. Used by `has_pending_eval`-equivalent
    /// to keep the vsync loop ticking until the 3s threshold fires.
    pub fn has_pending_hover(&self) -> bool {
        !self.wrap && self.hover_armed.is_some()
    }

    /// Build the canonical clipboard payload for the current selection.
    /// Single cell: just the cell text. Multiple cells: TSV — tabs between
    /// columns, newlines between rows. Excel/Numbers/Sheets parse this
    /// natively when pasted back in. Returns None if nothing is selected.
    pub fn copy_selection_payload(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        if self.selection.len() == 1 {
            let &(r, c) = self.selection.iter().next()?;
            return self.rows.get(r).and_then(|row| row.get(c)).cloned();
        }
        let r_min = self.selection.iter().map(|&(r, _)| r).min()?;
        let r_max = self.selection.iter().map(|&(r, _)| r).max()?;
        let c_min = self.selection.iter().map(|&(_, c)| c).min()?;
        let c_max = self.selection.iter().map(|&(_, c)| c).max()?;
        let mut lines: Vec<String> = Vec::with_capacity(r_max - r_min + 1);
        for r in r_min..=r_max {
            let mut cells: Vec<String> = Vec::with_capacity(c_max - c_min + 1);
            for c in c_min..=c_max {
                let cell = if self.selection.contains(&(r, c)) {
                    self.rows.get(r).and_then(|row| row.get(c)).cloned().unwrap_or_default()
                } else {
                    String::new()
                };
                cells.push(cell);
            }
            lines.push(cells.join("\t"));
        }
        Some(lines.join("\n"))
    }

    /// Resize `col` to fit its widest cell content (header + body) at
    /// `font_size`. Width = max char count × monospace char width + horizontal
    /// padding, clamped to [MIN_COL_WIDTH, AUTO_FIT_MAX]. The cap keeps a
    /// pathological cell from blowing the table off-screen — drag past it
    /// for explicit override.
    pub fn auto_fit_col(&mut self, col: usize, font_size: f32) {
        if col >= self.col_widths.len() { return; }
        let max_chars = self.rows.iter()
            .filter_map(|r| r.get(col))
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0);
        let char_w = font_size * 0.6;
        let pad = CELL_PADDING.left + CELL_PADDING.right;
        let raw = max_chars as f32 * char_w + pad;
        self.col_widths[col] = raw.max(MIN_COL_WIDTH).min(AUTO_FIT_MAX);
    }

    /// Cycle the sort state of `col`: Neutral → Asc → Desc → Neutral.
    /// First click on a previously-neutral column appends it to the
    /// END of the priority list (least dominant). Re-clicking advances
    /// its direction in place; the third click removes it.
    pub fn cycle_sort(&mut self, col: usize) {
        if let Some(idx) = self.sort_priority.iter().position(|(c, _)| *c == col) {
            match self.sort_priority[idx].1 {
                SortDir::Asc => self.sort_priority[idx].1 = SortDir::Desc,
                SortDir::Desc => { self.sort_priority.remove(idx); }
            }
        } else {
            self.sort_priority.push((col, SortDir::Asc));
        }
        self.apply_sort();
    }

    /// Sort state for a column, if any. Used by the header chrome to pick
    /// the arrow tint and the optional precedence badge.
    pub fn sort_state_for(&self, col: usize) -> Option<(SortDir, usize)> {
        self.sort_priority.iter().enumerate().find_map(|(i, (c, d))| {
            if *c == col { Some((*d, i)) } else { None }
        })
    }

    /// Apply the composite sort to the data rows (everything below row 0,
    /// which is the header). Stable across equal keys so existing intra-
    /// group order is preserved.
    pub fn apply_sort(&mut self) {
        if self.sort_priority.is_empty() || self.rows.len() <= 2 { return; }
        let priority = self.sort_priority.clone();
        let (_, tail) = self.rows.split_at_mut(1);
        tail.sort_by(|a, b| {
            for (col, dir) in &priority {
                let av = a.get(*col).map(|s| s.as_str()).unwrap_or("");
                let bv = b.get(*col).map(|s| s.as_str()).unwrap_or("");
                let ord = compare_alphanumeric(av, bv);
                let ord = if *dir == SortDir::Desc { ord.reverse() } else { ord };
                if ord != std::cmp::Ordering::Equal { return ord; }
            }
            std::cmp::Ordering::Equal
        });
    }

    pub fn col_count(&self) -> usize {
        self.col_widths.len()
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Apply a modifier-aware single-cell click to the multi-cell selection
    /// set. Called by `EditorState::update`'s TableMsg arm after it reads
    /// `self.mods` and derives the `SelectionMode`. Mutates `selection`,
    /// `selection_anchor`, and `focused_cell` in place.
    ///
    /// Also arms a drag-select: snapshots the selection BEFORE this click as
    /// the baseline so that if the user drags to other cells, the rectangle
    /// can be re-applied against the baseline (without compounding repeated
    /// applications of the modifier op).
    pub fn apply_click_selection(&mut self, row: usize, col: usize, mode: SelectionMode) {
        // Baseline = selection state before the click. The drag handler
        // recomputes from this so the rectangle can shrink/grow without
        // compounding (e.g. toggling the same cell twice).
        let baseline = self.selection.clone();

        match mode {
            SelectionMode::Replace => {
                self.selection.clear();
                self.selection.insert((row, col));
                self.selection_anchor = Some((row, col));
            }
            SelectionMode::Toggle => {
                // Cmd = invert this cell.
                if self.selection.contains(&(row, col)) {
                    self.selection.remove(&(row, col));
                    if self.selection.is_empty() {
                        self.selection_anchor = None;
                    }
                } else {
                    self.selection.insert((row, col));
                    if self.selection_anchor.is_none() {
                        self.selection_anchor = Some((row, col));
                    }
                }
            }
            SelectionMode::Extend => {
                // Shift = add this cell (no removal). Drag extends with a
                // rectangle (also additive).
                self.selection.insert((row, col));
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some((row, col));
                }
            }
            SelectionMode::Subtract => {
                // Cmd+Shift = remove this cell (no addition). Drag removes
                // a rectangle.
                self.selection.remove(&(row, col));
                if self.selection.is_empty() {
                    self.selection_anchor = None;
                }
            }
        }
        self.focused_cell = Some((row, col));
        self.is_active = true;
        self.table_selected = false;

        // Arm the drag. Subsequent CellEnter events extend the rectangle.
        self.drag_select_start = Some((row, col));
        self.drag_select_mode = Some(mode);
        self.drag_select_baseline = baseline;
    }

    /// Extend the active drag-rectangle to (row, col). Recomputes the
    /// selection from the baseline + drag mode + current rectangle. No-op
    /// if no drag is active.
    pub fn apply_drag_to(&mut self, row: usize, col: usize) {
        let Some(start) = self.drag_select_start else { return };
        let Some(mode) = self.drag_select_mode else { return };

        let (r0, r1) = (start.0.min(row), start.0.max(row));
        let (c0, c1) = (start.1.min(col), start.1.max(col));
        let mut rect: std::collections::HashSet<(usize, usize)> =
            std::collections::HashSet::new();
        for ri in r0..=r1 {
            for ci in c0..=c1 {
                if ri < self.rows.len() && ci < self.col_widths.len() {
                    rect.insert((ri, ci));
                }
            }
        }

        self.selection = match mode {
            SelectionMode::Replace => rect,
            SelectionMode::Toggle => {
                // baseline XOR rect
                let mut s = self.drag_select_baseline.clone();
                for cell in rect {
                    if s.contains(&cell) {
                        s.remove(&cell);
                    } else {
                        s.insert(cell);
                    }
                }
                s
            }
            SelectionMode::Extend => {
                // baseline UNION rect
                let mut s = self.drag_select_baseline.clone();
                s.extend(rect);
                s
            }
            SelectionMode::Subtract => {
                // baseline DIFFERENCE rect
                let mut s = self.drag_select_baseline.clone();
                for cell in rect {
                    s.remove(&cell);
                }
                s
            }
        };

        self.focused_cell = Some((row, col));
    }

    /// Clear the drag-select state. Called from EndDrag.
    pub fn end_drag_select(&mut self) {
        self.drag_select_start = None;
        self.drag_select_mode = None;
        self.drag_select_baseline.clear();
    }

    pub fn handle(&mut self, msg: TableMessage) {
        #[cfg(debug_assertions)]
        println!("Table {:?} got message: {:?}", self.id, msg);
        match msg {
            TableMessage::CellChanged(row, col, val) => {
                if self.read_only {
                    return;
                }
                if row < self.rows.len() && col < self.rows[row].len() {
                    self.rows[row][col] = val;
                }
                self.focused_cell = Some((row, col));
            }
            TableMessage::FocusCell(row, col) => {
                self.focused_cell = Some((row, col));
            }
            TableMessage::SelectCell(row, col) => {
                // Single click — selected, not editing. The editor's
                // `editing` field is cleared by the editor-level dispatch
                // in `EditorState::update`'s `TableMsg` arm. The actual
                // multi-cell selection update happens via `apply_click_selection`,
                // called from the editor's TableMsg arm AFTER it has read
                // `self.mods` to derive the SelectionMode. Here we only mark
                // the cell as the focus point and clear table-level selection.
                self.focused_cell = Some((row, col));
                self.is_active = true;
                self.table_selected = false;
                self.hover_armed = None;
                // Wrap-off mode: a click that lands on a different cell
                // re-targets the spillover popup. Clicking the same cell
                // again toggles it closed so the user can dismiss without
                // an explicit ESC.
                if !self.wrap {
                    self.spillover = match self.spillover {
                        Some(prev) if prev == (row, col) => None,
                        _ => Some((row, col)),
                    };
                } else {
                    self.spillover = None;
                }
            }
            TableMessage::EditCell(row, col) => {
                // Double click — selected AND editing. The editor's
                // `editing` field is set, and `pending_focus` is queued so
                // iced moves keyboard focus to the cell's text_input on the
                // next frame.
                self.focused_cell = Some((row, col));
                self.is_active = true;
                self.hover_armed = None;
                self.spillover = None;
            }
            TableMessage::DeleteTable => {
                // Handled at the editor level — the TableMsg arm in
                // editor.rs promotes this to DeleteCurrentTable. The block
                // itself does nothing here, but we still need to ensure the
                // table is registered as focused so the editor's
                // focused_table_index() finds it.
                if self.focused_cell.is_none() {
                    self.focused_cell = Some((0, 0));
                }
            }
            TableMessage::SelectAll => {
                // Whole-table selection. Mark every cell as selected via the
                // table_selected flag — cell rendering keys off this for the
                // highlighted look. focused_cell stays where it was so
                // arrow keys can drop back into single-cell mode naturally.
                self.table_selected = true;
                self.is_active = true;
                if self.focused_cell.is_none() {
                    self.focused_cell = Some((0, 0));
                }
            }
            TableMessage::ClearAll => {
                if self.read_only {
                    return;
                }
                for row in &mut self.rows {
                    for cell in row.iter_mut() {
                        cell.clear();
                    }
                }
            }
            TableMessage::ContextMenu(_row, _col) => {
                // Right-click is purely a menu trigger — it does NOT modify
                // selection state. The context menu operates on whatever was
                // already selected. The editor.rs TableMsg arm handles the
                // overlay anchor; this branch is intentionally a no-op.
            }
            TableMessage::CellEnter(row, col) => {
                // Drag-select extension: only acts when a drag is armed.
                // Without an active drag, hovering over cells is a no-op
                // (every cell still fires CellEnter on every mouse-over,
                // which is a tiny per-frame cost).
                if self.drag_select_start.is_some() {
                    self.apply_drag_to(row, col);
                }
                // Hover-to-spillover dwell: only meaningful with wrap off
                // (clipped cells are the ones that benefit). Re-arming on a
                // different cell resets the timer; same-cell re-entry leaves
                // the existing timer alone so a tiny twitch doesn't restart
                // the dwell.
                if !self.wrap {
                    let already_armed = matches!(
                        self.hover_armed,
                        Some((r, c, _)) if r == row && c == col
                    );
                    if !already_armed {
                        self.hover_armed = Some((row, col, std::time::Instant::now()));
                    }
                }
            }
            TableMessage::AddRow => {
                if self.read_only {
                    return;
                }
                let cols = self.col_count();
                self.rows.push(vec![String::new(); cols]);
                self.row_heights.push(None);
            }
            TableMessage::AddColumn => {
                if self.read_only {
                    return;
                }
                self.col_widths.push(DEFAULT_COL_WIDTH);
                for row in &mut self.rows {
                    row.push(String::new());
                }
            }
            TableMessage::InsertRowAbove => {
                if self.read_only {
                    return;
                }
                let Some((fr, _)) = self.focused_cell else { return };
                // Never insert above the header row — treat as insert-below-header.
                let insert_at = fr.max(1).min(self.rows.len());
                let cols = self.col_count();
                self.rows.insert(insert_at, vec![String::new(); cols]);
                self.row_heights.insert(insert_at, None);
                self.focused_cell = Some((insert_at, 0));
            }
            TableMessage::InsertRowBelow => {
                if self.read_only {
                    return;
                }
                let Some((fr, _)) = self.focused_cell else { return };
                let insert_at = (fr + 1).min(self.rows.len());
                let cols = self.col_count();
                self.rows.insert(insert_at, vec![String::new(); cols]);
                self.row_heights.insert(insert_at, None);
                self.focused_cell = Some((insert_at, 0));
            }
            TableMessage::DeleteRow => {
                if self.read_only {
                    return;
                }
                let Some((fr, fc)) = self.focused_cell else { return };
                if fr == 0 || self.rows.len() <= 1 {
                    return;
                }
                self.rows.remove(fr);
                if fr < self.row_heights.len() {
                    self.row_heights.remove(fr);
                }
                let new_row_count = self.rows.len();
                let new_row = if new_row_count == 1 {
                    0
                } else {
                    fr.min(new_row_count - 1).max(1)
                };
                self.focused_cell = Some((new_row, fc));
            }
            TableMessage::InsertColLeft => {
                if self.read_only {
                    return;
                }
                let Some((fr, fc)) = self.focused_cell else { return };
                self.insert_column_at(fc);
                self.focused_cell = Some((fr, fc));
            }
            TableMessage::InsertColRight => {
                if self.read_only {
                    return;
                }
                let Some((fr, fc)) = self.focused_cell else { return };
                let at = (fc + 1).min(self.col_count());
                self.insert_column_at(at);
                self.focused_cell = Some((fr, at));
            }
            TableMessage::DeleteCol => {
                if self.read_only {
                    return;
                }
                let Some((fr, fc)) = self.focused_cell else { return };
                if self.col_count() <= 1 {
                    return;
                }
                for row in &mut self.rows {
                    if fc < row.len() {
                        row.remove(fc);
                    }
                }
                if fc < self.col_widths.len() {
                    self.col_widths.remove(fc);
                }
                let new_col = fc.min(self.col_count().saturating_sub(1));
                self.focused_cell = Some((fr, new_col));
            }
            TableMessage::CursorMove(x, y) => {
                self.last_cursor_x = x;
                self.last_cursor_y = y;
                if let Some((col, start_w, start_x)) = self.resize_drag {
                    let delta = x - start_x;
                    let new_w = (start_w + delta).max(MIN_COL_WIDTH);
                    if col < self.col_widths.len() {
                        self.col_widths[col] = new_w;
                    }
                }
                if let Some((row, start_h, start_y)) = self.row_resize_drag {
                    let delta = y - start_y;
                    let new_h = (start_h + delta).max(MIN_ROW_HEIGHT);
                    if row < self.rows.len() {
                        if self.row_heights.len() <= row {
                            self.row_heights.resize(row + 1, None);
                        }
                        self.row_heights[row] = Some(new_h);
                    }
                }
                if let Some(drag) = self.reorder_drag {
                    match drag {
                        ReorderDrag::Column { from, start_x, .. } => {
                            let target = self.target_column_for_drag(from, x - start_x);
                            self.reorder_drag = Some(ReorderDrag::Column { from, target, start_x });
                        }
                        ReorderDrag::Row { from, start_y, .. } => {
                            let target = self.target_row_for_drag(from, y - start_y);
                            self.reorder_drag = Some(ReorderDrag::Row { from, target, start_y });
                        }
                    }
                }
            }
            TableMessage::BeginColResize(col) => {
                if self.read_only {
                    return;
                }
                if let Some(w) = self.col_widths.get(col).copied() {
                    self.resize_drag = Some((col, w, self.last_cursor_x));
                }
            }
            TableMessage::BeginRowResize(row) => {
                if self.read_only || row >= self.rows.len() {
                    return;
                }
                let current_h = self.row_heights.get(row).copied().flatten().unwrap_or(ROW_HEIGHT_ESTIMATE);
                self.row_resize_drag = Some((row, current_h, self.last_cursor_y));
            }
            TableMessage::BeginColReorder(col) => {
                if self.read_only || col >= self.col_widths.len() {
                    return;
                }
                self.reorder_drag = Some(ReorderDrag::Column {
                    from: col,
                    target: col,
                    start_x: self.last_cursor_x,
                });
            }
            TableMessage::BeginRowReorder(row) => {
                if self.read_only || row == 0 || row >= self.rows.len() || self.rows.len() <= 2 {
                    return;
                }
                self.reorder_drag = Some(ReorderDrag::Row {
                    from: row,
                    target: row,
                    start_y: self.last_cursor_y,
                });
            }
            TableMessage::AutoFitCol(col, font_size) => {
                if self.read_only || col >= self.col_widths.len() {
                    return;
                }
                self.auto_fit_col(col, font_size);
            }
            TableMessage::ToggleWrap => {
                if self.read_only { return; }
                self.wrap = !self.wrap;
                // Switching to wrap-on auto-closes any open spillover —
                // wrapped content is no longer clipped, so the popup is moot.
                if self.wrap { self.spillover = None; }
            }
            TableMessage::OpenSpillover(row, col) => {
                if self.wrap { return; }
                if row < self.rows.len() && col < self.col_widths.len() {
                    self.spillover = Some((row, col));
                }
            }
            TableMessage::CloseSpillover => {
                self.spillover = None;
            }
            TableMessage::CycleSort(col) => {
                if self.read_only || col >= self.col_widths.len() {
                    return;
                }
                self.cycle_sort(col);
            }
            TableMessage::EndDrag => {
                self.resize_drag = None;
                self.row_resize_drag = None;
                if let Some(drag) = self.reorder_drag.take() {
                    match drag {
                        ReorderDrag::Column { from, target, .. } => {
                            if from != target {
                                self.move_column(from, target);
                            }
                        }
                        ReorderDrag::Row { from, target, .. } => {
                            if from != target {
                                self.move_row(from, target);
                            }
                        }
                    }
                }
                // Also tear down the cell drag-select. The selection state
                // has already been committed by the last `apply_drag_to`,
                // so we just clear the bookkeeping.
                self.end_drag_select();
            }
        }
    }

    fn target_column_for_drag(&self, from: usize, delta_x: f32) -> usize {
        let n = self.col_widths.len();
        if n == 0 || from >= n {
            return from;
        }
        if delta_x > 0.0 {
            let mut accumulated = 0.0;
            let mut target = from;
            let mut i = from + 1;
            while i < n {
                let w = self.col_widths[i];
                if delta_x > accumulated + w / 2.0 {
                    target = i;
                    accumulated += w;
                    i += 1;
                } else {
                    break;
                }
            }
            target
        } else if delta_x < 0.0 {
            let mut accumulated = 0.0;
            let mut target = from;
            let mut i = from;
            let abs_d = -delta_x;
            while i > 0 {
                i -= 1;
                let w = self.col_widths[i];
                if abs_d > accumulated + w / 2.0 {
                    target = i;
                    accumulated += w;
                } else {
                    break;
                }
            }
            target
        } else {
            from
        }
    }

    fn target_row_for_drag(&self, from: usize, delta_y: f32) -> usize {
        let n = self.rows.len();
        if n <= 2 || from == 0 {
            return from;
        }
        let row_h = ROW_HEIGHT_ESTIMATE;
        let raw_steps = (delta_y / row_h).round() as i32;
        let target_signed = (from as i32) + raw_steps;
        target_signed.max(1).min(n as i32 - 1) as usize
    }

    pub fn move_column(&mut self, from: usize, to: usize) {
        let n = self.col_widths.len();
        if n == 0 || from >= n || from == to {
            return;
        }
        let to = to.min(n - 1);
        let w = self.col_widths.remove(from);
        self.col_widths.insert(to, w);
        for row in &mut self.rows {
            if from < row.len() {
                let cell = row.remove(from);
                let to_in_row = to.min(row.len());
                row.insert(to_in_row, cell);
            }
        }
        if let Some((r, c)) = self.focused_cell {
            let new_c = if c == from {
                to
            } else if from < to && c > from && c <= to {
                c - 1
            } else if from > to && c >= to && c < from {
                c + 1
            } else {
                c
            };
            self.focused_cell = Some((r, new_c));
        }
    }

    pub fn move_row(&mut self, from: usize, to: usize) {
        let n = self.rows.len();
        if n <= 2 || from == 0 || to == 0 || from >= n || from == to {
            return;
        }
        let to = to.min(n - 1).max(1);
        let row = self.rows.remove(from);
        self.rows.insert(to, row);
        if from < self.row_heights.len() {
            let h = self.row_heights.remove(from);
            let to_h = to.min(self.row_heights.len());
            self.row_heights.insert(to_h, h);
        }
        if let Some((r, c)) = self.focused_cell {
            let new_r = if r == from {
                to
            } else if from < to && r > from && r <= to {
                r - 1
            } else if from > to && r >= to && r < from {
                r + 1
            } else {
                r
            };
            self.focused_cell = Some((new_r, c));
        }
    }

    fn insert_column_at(&mut self, at: usize) {
        let at = at.min(self.col_count());
        for row in &mut self.rows {
            if at <= row.len() {
                row.insert(at, String::new());
            } else {
                row.push(String::new());
            }
        }
        if at <= self.col_widths.len() {
            self.col_widths.insert(at, DEFAULT_COL_WIDTH);
        } else {
            self.col_widths.push(DEFAULT_COL_WIDTH);
        }
    }

    pub fn next_cell(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let cols = self.col_count();
        let rows = self.row_count();
        if col + 1 < cols {
            Some((row, col + 1))
        } else if row + 1 < rows {
            Some((row + 1, 0))
        } else {
            None
        }
    }

    pub fn prev_cell(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let cols = self.col_count();
        if col > 0 {
            Some((row, col - 1))
        } else if row > 0 {
            Some((row - 1, cols.saturating_sub(1)))
        } else {
            None
        }
    }

    pub fn cell_below(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        if row + 1 < self.row_count() {
            Some((row + 1, col))
        } else {
            None
        }
    }

    pub fn pending_focus_id(&self) -> Option<WidgetId> {
        self.focused_cell.map(|(r, c)| cell_id(self.id, r, c))
    }

    /// True if this table carries metadata that markdown alone can't
    /// represent — non-default column widths, explicit row heights, or
    /// cell formulas (markdown would otherwise serialize the raw `/=...`
    /// text into the cell, but round-tripping via the sidecar keeps the
    /// formula label separate from the computed display).
    pub fn has_persistent_metadata(&self) -> bool {
        if self.col_widths.iter().any(|w| (*w - DEFAULT_COL_WIDTH).abs() > f32::EPSILON) {
            return true;
        }
        if self.row_heights.iter().any(|h| h.is_some()) {
            return true;
        }
        if self.rows.iter().any(|row| row.iter().any(|c| c.trim_start().starts_with("/="))) {
            return true;
        }
        false
    }
}

impl<Message: Clone + 'static> Block<Message> for TableBlock {
    fn id(&self) -> BlockId {
        self.id
    }

    fn kind_tag(&self) -> &'static str {
        "table"
    }

    fn start_line(&self) -> usize {
        self.start_line
    }

    fn set_start_line(&mut self, line: usize) {
        self.start_line = line;
    }

    fn line_count(&self) -> usize {
        // Header + separator + (rows-1) data lines = rows.len() + 1.
        if self.rows.is_empty() {
            0
        } else {
            self.rows.len() + 1
        }
    }

    fn is_eval_result(&self) -> bool {
        self.is_eval_result
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn view<'a>(&'a self, ctx: &ViewCtx<'_, Message>) -> LayeredView<'a, Message> {
        let block_idx = ctx.block_index;
        let on_table = ctx.on_table_msg;
        let on_msg = move |tmsg: TableMessage| on_table(block_idx, tmsg);

        // Pull the (row, col) currently in edit mode out of the central
        // `editing` path, but only if it points at THIS table block.
        let editing_cell: Option<(usize, usize)> = match ctx.editing {
            Some(path) if path.block_id == self.id => match &path.inner {
                InnerPath::Cell { row, col } => Some((*row, *col)),
                _ => None,
            },
            _ => None,
        };

        let element = table_view(self, editing_cell, ctx.font_size, ctx.computed_cells, on_msg);
        LayeredView::just(element)
    }

    fn to_md(&self) -> String {
        if self.is_eval_result || self.rows.is_empty() {
            return String::new();
        }
        let mut lines: Vec<String> = Vec::new();
        if let Some(header) = self.rows.first() {
            let cells: Vec<&str> = header.iter().map(|s| s.as_str()).collect();
            lines.push(format!("| {} |", cells.join(" | ")));
            let sep = header
                .iter()
                .map(|c| "-".repeat(c.len().max(3)))
                .collect::<Vec<_>>()
                .join(" | ");
            lines.push(format!("| {} |", sep));
            for row in self.rows.iter().skip(1) {
                let cells: Vec<&str> = row.iter().map(|s| s.as_str()).collect();
                lines.push(format!("| {} |", cells.join(" | ")));
            }
        }
        lines.join("\n")
    }

    fn hit_test(&self, _point: Point) -> Option<InnerPath> {
        Some(InnerPath::Whole)
    }

    fn apply(&mut self, cmd: BlockCommand) {
        match cmd {
            BlockCommand::InsertRowAbove(_row) => {
                self.handle(TableMessage::InsertRowAbove);
            }
            BlockCommand::InsertRowBelow(_row) => {
                self.handle(TableMessage::InsertRowBelow);
            }
            BlockCommand::DeleteRow(_row) => {
                self.handle(TableMessage::DeleteRow);
            }
            BlockCommand::InsertColLeft(_col) => {
                self.handle(TableMessage::InsertColLeft);
            }
            BlockCommand::InsertColRight(_col) => {
                self.handle(TableMessage::InsertColRight);
            }
            BlockCommand::DeleteCol(_col) => {
                self.handle(TableMessage::DeleteCol);
            }
            BlockCommand::SetCellValue { row, col, value } => {
                self.handle(TableMessage::CellChanged(row, col, value));
            }
            BlockCommand::ResizeCol { col, width } => {
                if col < self.col_widths.len() {
                    self.col_widths[col] = width;
                }
            }
            BlockCommand::ResizeRow { row, height } => {
                if row < self.row_heights.len() {
                    self.row_heights[row] = Some(height);
                }
            }
            BlockCommand::MoveCol { from, to } => {
                self.move_column(from, to);
            }
            BlockCommand::MoveRow { from, to } => {
                self.move_row(from, to);
            }
            _ => {}
        }
    }

    fn selectable_paths(&self) -> Box<dyn Iterator<Item = InnerPath> + '_> {
        let rows = self.rows.len();
        let cols = self.col_count();
        Box::new((0..rows).flat_map(move |r| {
            (0..cols).map(move |c| InnerPath::Cell { row: r, col: c })
        }))
    }
}

pub fn cell_id(block_id: u64, row: usize, col: usize) -> WidgetId {
    WidgetId::from(format!("table_cell_{}_{}_{}", block_id, row, col))
}

fn cell_border_for(is_eval: bool) -> Border {
    let ws = palette::widget_surface();
    Border {
        color: if is_eval { ws.eval_border } else { ws.border },
        width: 1.0,
        radius: 0.0.into(),
    }
}

fn cell_input_style_for(is_eval: bool) -> text_input::Style {
    let p = palette::current();
    let ws = palette::widget_surface();
    let fill = if is_eval { ws.eval_fill } else { ws.fill };
    text_input::Style {
        background: Background::Color(fill),
        border: cell_border_for(is_eval),
        icon: p.overlay2,
        placeholder: p.overlay0,
        value: ws.body_text,
        selection: Color { a: 0.4, ..p.blue },
    }
}

fn header_cell_style_for(is_eval: bool) -> text_input::Style {
    let p = palette::current();
    let ws = palette::widget_surface();
    let fill = if is_eval { ws.eval_fill } else { ws.fill };
    text_input::Style {
        background: Background::Color(fill),
        border: cell_border_for(is_eval),
        icon: p.overlay2,
        placeholder: p.overlay0,
        value: ws.header_accent,
        selection: Color { a: 0.4, ..p.blue },
    }
}

const RESIZE_HANDLE_WIDTH: f32 = 4.0;

pub fn table_view<'a, Message, F>(
    block: &'a TableBlock,
    editing_cell: Option<(usize, usize)>,
    font_size: f32,
    computed_cells: &std::collections::HashMap<(BlockId, u32, u32), acord_core::interp::Value>,
    on_msg: F,
) -> Element<'a, Message, Theme, iced_wgpu::Renderer>
where
    Message: Clone + 'a,
    F: Fn(TableMessage) -> Message + 'a + Copy,
{
    let block_id = block.id;
    let mut col_elements: Vec<Element<'a, Message, Theme, iced_wgpu::Renderer>> = Vec::new();
    let read_only = block.read_only;
    let reserve_chrome = !read_only;
    // Derived sizes that scale with the editor's zoom level.
    let chrome_font = font_size * 0.77;
    let corner_font = font_size * 0.69;
    let plus_font = font_size * 0.85;
    let row_h = font_size * 1.3 + CELL_PADDING.top + CELL_PADDING.bottom + 2.0;
    let header_h = chrome_font * 1.3;
    // Chrome (ABCD column letters, 123 row numbers) appears whenever the table
    // has a selected cell, not just when iced widget focus is in a cell. Without
    // the focused_cell branch the chrome would vanish the moment selection mode
    // takes over from edit mode, hiding the visual cue that the table is yours
    // to manipulate.
    let chrome_active = !read_only
        && (block.focused_cell.is_some()
            || block.is_active
            || block.reorder_drag.is_some());
    let drag = block.reorder_drag;

    if reserve_chrome {
        let mut header_row_cells: Vec<Element<'a, Message, Theme, iced_wgpu::Renderer>> = Vec::new();
        // Corner cell at (row-numbers ⨉ column-letters). The "select-all"
        // affordance — click to mark the whole table as selected. With the
        // table selected, plain Backspace clears every cell, Cmd+Backspace
        // deletes the table outright. Eventually this same handle will also
        // be the drag origin for moving the table around the document, in
        // line with the broader plan to make every chunk-level node draggable.
        // Visible whenever the chrome is active so the user always has a
        // reachable affordance once they've touched the table once.
        let corner: Element<'a, Message, Theme, iced_wgpu::Renderer> = if chrome_active {
            iced_widget::button(
                text("\u{25A0}")
                    .size(corner_font)
                    .font(EDITOR_FONT)
            )
            .width(Length::Fixed(ROW_NUMBER_WIDTH))
            .height(Length::Fixed(header_h))
            .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 })
            .style(plus_button_style)
            .on_press(on_msg(TableMessage::SelectAll))
            .into()
        } else {
            container(text(""))
                .width(Length::Fixed(ROW_NUMBER_WIDTH))
                .height(Length::Fixed(header_h))
                .into()
        };
        header_row_cells.push(corner);
        let p = palette::current();
        for (ci, w) in block.col_widths.iter().enumerate() {
            let letter = if chrome_active { column_letter(ci) } else { String::new() };
            let bg_color: Option<Color> = if let Some(ReorderDrag::Column { from, target, .. }) = drag {
                if from == ci {
                    Some(p.surface1)
                } else if target == ci && ci != from {
                    Some(Color { a: 0.4, ..p.blue })
                } else {
                    None
                }
            } else {
                None
            };
            let sort_state = block.sort_state_for(ci);
            let arrow_color = |active: bool| -> Color {
                if active { p.text } else { Color { a: 0.25, ..p.overlay0 } }
            };
            let (up_active, down_active) = match sort_state {
                Some((SortDir::Asc, _)) => (true, false),
                Some((SortDir::Desc, _)) => (false, true),
                None => (false, false),
            };
            let arrows: Element<'a, Message, Theme, iced_wgpu::Renderer> = if chrome_active {
                let up_glyph = text("\u{25B2}")
                    .size(chrome_font * 0.7)
                    .font(EDITOR_FONT)
                    .color(arrow_color(up_active));
                let down_glyph = text("\u{25BC}")
                    .size(chrome_font * 0.7)
                    .font(EDITOR_FONT)
                    .color(arrow_color(down_active));
                let stack = iced_widget::row![up_glyph, down_glyph]
                    .spacing(2.0)
                    .align_y(iced_wgpu::core::Alignment::Center);
                MouseArea::new(
                    container(stack)
                        .padding(Padding { top: 0.0, right: 4.0, bottom: 0.0, left: 4.0 })
                )
                    .on_press(on_msg(TableMessage::CycleSort(ci)))
                    .into()
            } else {
                container(text("")).width(Length::Fixed(0.0)).into()
            };

            let letter_inner: Element<'a, Message, Theme, iced_wgpu::Renderer> = iced_widget::row![
                MouseArea::new(
                    container(
                        text(letter)
                            .size(chrome_font)
                            .font(EDITOR_FONT)
                            .color(oklab::lighten_for_size(p.overlay0, chrome_font))
                    )
                    .width(Length::Fill)
                    .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 6.0 })
                )
                    .on_press(on_msg(TableMessage::BeginColReorder(ci))),
                arrows,
            ]
            .spacing(0.0)
            .align_y(iced_wgpu::core::Alignment::Center)
            .into();

            let letter_container = container(letter_inner)
                .width(Length::Fixed(*w))
                .height(Length::Fixed(header_h))
                .style(move |_theme: &Theme| container::Style {
                    background: bg_color.map(Background::Color),
                    border: Border::default(),
                    text_color: None,
                    shadow: Shadow::default(),
                    snap: false,
                });
            let letter_cell: Element<'a, Message, Theme, iced_wgpu::Renderer> =
                letter_container.into();
            header_row_cells.push(letter_cell);
            header_row_cells.push(
                container(text(""))
                    .width(Length::Fixed(RESIZE_HANDLE_WIDTH))
                    .height(Length::Fixed(header_h))
                    .into()
            );
        }
        col_elements.push(iced_widget::row(header_row_cells).spacing(0.0).into());
    }

    for (ri, row) in block.rows.iter().enumerate() {
        let is_header = ri == 0;
        let row_h = compute_row_height(block, ri, row, font_size, row_h);
        let mut row_cells: Vec<Element<'a, Message, Theme, iced_wgpu::Renderer>> = Vec::new();

        if reserve_chrome {
            let p = palette::current();
            let label = if chrome_active { format!("{}", ri + 1) } else { String::new() };
            let bg_color: Option<Color> = if let Some(ReorderDrag::Row { from, target, .. }) = drag {
                if from == ri {
                    Some(p.surface1)
                } else if target == ri && ri != from {
                    Some(Color { a: 0.4, ..p.blue })
                } else {
                    None
                }
            } else {
                None
            };
            let row_num_container = container(
                text(label)
                    .size(chrome_font)
                    .font(EDITOR_FONT)
                    .color(oklab::lighten_for_size(p.overlay0, chrome_font))
            )
            .width(Length::Fixed(ROW_NUMBER_WIDTH))
            .padding(Padding { top: 4.0, right: 6.0, bottom: 0.0, left: 0.0 })
            .style(move |_theme: &Theme| container::Style {
                background: bg_color.map(Background::Color),
                border: Border::default(),
                text_color: None,
                shadow: Shadow::default(),
                snap: false,
            });
            let row_num_cell: Element<'a, Message, Theme, iced_wgpu::Renderer> =
                if chrome_active && ri > 0 {
                    MouseArea::new(row_num_container)
                        .on_press(on_msg(TableMessage::BeginRowReorder(ri)))
                        .into()
                } else {
                    row_num_container.into()
                };
            row_cells.push(row_num_cell);
        }

        for (ci, cell) in row.iter().enumerate() {
            let width = block.col_widths.get(ci).copied().unwrap_or(DEFAULT_COL_WIDTH);
            let r = ri;
            let c = ci;

            let is_editing_this = editing_cell == Some((ri, ci));
            // A cell renders selected ONLY because it's in the selection set
            // (or the table-wide select-all mode is on). The HashSet is the
            // sole source of truth — `focused_cell` is preserved across blur
            // for keyboard targeting, so it's not a valid selection signal.
            let is_focused_this = block.selection.contains(&(ri, ci))
                || block.table_selected;

            let font = if is_header {
                Font { weight: iced_wgpu::core::font::Weight::Bold, ..EDITOR_FONT }
            } else {
                EDITOR_FONT
            };

            let cell_element: Element<'a, Message, Theme, iced_wgpu::Renderer> = if is_editing_this
                || read_only
            {
                // Edit mode (or eval-result table that the user can still
                // copy from) — use the real text_input.
                let is_eval = read_only;
                let style_fn = move |_theme: &Theme, _status: text_input::Status| -> text_input::Style {
                    if is_header { header_cell_style_for(is_eval) } else { cell_input_style_for(is_eval) }
                };
                let mut input = text_input::TextInput::new("", cell)
                    .id(cell_id(block_id, ri, ci))
                    .font(font)
                    .size(font_size)
                    .padding(CELL_PADDING)
                    .width(Length::Fixed(width))
                    .style(style_fn);
                if !read_only {
                    input = input.on_input(move |val| on_msg(TableMessage::CellChanged(r, c, val)));
                }
                // Pin the wrapper to row_h so a manually-resized row keeps its
                // height when the user double-clicks to enter edit mode —
                // text_input alone would snap back to its natural font-size height.
                container(input)
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(row_h))
                    .into()
            } else {
                // Selected-but-not-editing or fully unfocused cell. Renders
                // as a static text widget inside a container styled to match
                // the text_input's bounds — visually identical to the edit
                // form modulo a tinted background when this cell is the
                // current selection.
                let label_color = if is_header {
                    palette::widget_surface().header_accent
                } else {
                    palette::widget_surface().body_text
                };
                // Show the computed formula value when this cell is a
                // `/=...` formula and the eval loop produced a result.
                // Any cell without a computed entry (plain values, eval
                // errors during parse/topo pre-pass) falls back to raw.
                let display_text: String = if cell.trim_start().starts_with("/=") {
                    match computed_cells.get(&(block_id, ci as u32, ri as u32)) {
                        Some(v) => v.display(),
                        None => cell.clone(),
                    }
                } else {
                    cell.clone()
                };
                let display = text(display_text)
                    .size(font_size)
                    .font(font)
                    .color(oklab::lighten_for_size(label_color, font_size))
                    .wrapping(if block.wrap { Wrapping::Word } else { Wrapping::None });

                let is_eval = read_only;
                let container_style = move |_theme: &Theme| {
                    let ws = palette::widget_surface();
                    let p = palette::current();
                    let surface_fill = if is_eval { ws.eval_fill } else { ws.fill };
                    let background = if is_focused_this {
                        Some(Background::Color(Color { a: 0.45, ..p.blue }))
                    } else {
                        Some(Background::Color(surface_fill))
                    };
                    container::Style {
                        background,
                        border: cell_border_for(is_eval),
                        text_color: Some(oklab::lighten_for_size(label_color, font_size)),
                        shadow: Shadow::default(),
                        snap: false,
                    }
                };
                let cell_container = container(display)
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(row_h))
                    .padding(CELL_PADDING)
                    .style(container_style);

                MouseArea::new(cell_container)
                    .on_press(on_msg(TableMessage::SelectCell(r, c)))
                    .on_double_click(on_msg(TableMessage::EditCell(r, c)))
                    .on_right_release(on_msg(TableMessage::ContextMenu(r, c)))
                    .on_enter(on_msg(TableMessage::CellEnter(r, c)))
                    .into()
            };

            row_cells.push(cell_element);

            if is_header && !read_only {
                let handle_col = ci;
                let handle: Element<'a, Message, Theme, iced_wgpu::Renderer> =
                    MouseArea::new(
                        container(text(" "))
                            .width(Length::Fixed(RESIZE_HANDLE_WIDTH))
                            .height(Length::Shrink)
                    )
                    .interaction(Interaction::ResizingHorizontally)
                    .on_press(on_msg(TableMessage::BeginColResize(handle_col)))
                    .on_double_click(on_msg(TableMessage::AutoFitCol(handle_col, font_size)))
                    .into();
                row_cells.push(handle);
            } else {
                let spacer: Element<'a, Message, Theme, iced_wgpu::Renderer> =
                    container(text(" "))
                        .width(Length::Fixed(RESIZE_HANDLE_WIDTH))
                        .height(Length::Shrink)
                        .into();
                row_cells.push(spacer);
            }
        }

        let row_el: Element<'a, Message, Theme, iced_wgpu::Renderer> =
            iced_widget::row(row_cells).spacing(0.0).into();
        col_elements.push(row_el);

        // Row resize band — 3px hit area below each row, drags row height.
        // Skipped for read_only tables (eval results aren't meant to be
        // structurally edited).
        if !read_only {
            let resize_row = ri;
            let band_w: f32 = (if reserve_chrome { ROW_NUMBER_WIDTH } else { 0.0 })
                + block.col_widths.iter().sum::<f32>()
                + RESIZE_HANDLE_WIDTH * block.col_widths.len() as f32;
            let band: Element<'a, Message, Theme, iced_wgpu::Renderer> =
                MouseArea::new(
                    container(text(" "))
                        .width(Length::Fixed(band_w))
                        .height(Length::Fixed(ROW_RESIZE_HANDLE_HEIGHT))
                )
                .interaction(Interaction::ResizingVertically)
                .on_press(on_msg(TableMessage::BeginRowResize(resize_row)))
                .into();
            col_elements.push(band);
        }
    }

    let table: Element<'a, Message, Theme, iced_wgpu::Renderer> =
        iced_widget::column(col_elements).spacing(CELL_GAP_Y).into();

    let with_plus: Element<'a, Message, Theme, iced_wgpu::Renderer> = if read_only {
        table
    } else {
        let right_plus = iced_widget::button(
            text("+")
                .size(plus_font)
                .font(EDITOR_FONT)
        )
        .width(Length::Fixed(PLUS_BUTTON_THICKNESS))
        .height(Length::Fill)
        .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 })
        .style(plus_button_style)
        .on_press(on_msg(TableMessage::AddColumn));

        let table_with_right: Element<'a, Message, Theme, iced_wgpu::Renderer> =
            iced_widget::row(vec![table, right_plus.into()])
                .spacing(CELL_GAP_Y)
                .into();

        let bottom_plus = iced_widget::button(
            text("+")
                .size(plus_font)
                .font(EDITOR_FONT)
        )
        .width(Length::Fill)
        .height(Length::Fixed(PLUS_BUTTON_THICKNESS))
        .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 })
        .style(plus_button_style)
        .on_press(on_msg(TableMessage::AddRow));

        iced_widget::column(vec![table_with_right, bottom_plus.into()])
            .spacing(CELL_GAP_Y)
            .into()
    };

    let outer: Element<'a, Message, Theme, iced_wgpu::Renderer> = if read_only {
        iced_widget::container(with_plus)
            .padding(Padding { top: 6.0, right: 6.0, bottom: 6.0, left: 12.0 })
            .width(Length::Shrink)
            .style(|_theme: &Theme| {
                let ws = palette::widget_surface();
                container::Style {
                    background: Some(Background::Color(ws.eval_fill)),
                    border: Border {
                        color: ws.eval_accent,
                        width: 0.0,
                        radius: 4.0.into(),
                    },
                    text_color: None,
                    shadow: Shadow::default(),
                    snap: false,
                }
            })
            .into()
    } else {
        let wrapper = iced_widget::container(with_plus)
            .padding(Padding { top: 2.0, right: 0.0, bottom: 2.0, left: 8.0 })
            .width(Length::Shrink)
            .style(|_theme: &Theme| container::Style {
                background: None,
                border: Border::default(),
                text_color: None,
                shadow: Shadow::default(),
                snap: false,
            });

        MouseArea::new(wrapper)
            .on_move(move |pt| on_msg(TableMessage::CursorMove(pt.x, pt.y)))
            .on_release(on_msg(TableMessage::EndDrag))
            .into()
    };

    outer
}

/// Wrap-aware row height. Manual override wins. Then if wrap is on, fit to
/// the tallest wrapped cell (chars × char_w / col_width gives an approximate
/// line count). Otherwise fall back to the default single-line row height.
fn compute_row_height(
    block: &TableBlock,
    ri: usize,
    row: &[String],
    font_size: f32,
    default_h: f32,
) -> f32 {
    if let Some(h) = block.row_heights.get(ri).copied().flatten() {
        return h;
    }
    if !block.wrap {
        return default_h;
    }
    let line_h = font_size * 1.3;
    let char_w = font_size * 0.6;
    let pad_h = CELL_PADDING.top + CELL_PADDING.bottom + 2.0;
    let max_lines = row.iter().enumerate()
        .map(|(ci, cell)| {
            let w = block.col_widths.get(ci).copied().unwrap_or(DEFAULT_COL_WIDTH);
            let usable_w = (w - CELL_PADDING.left - CELL_PADDING.right).max(1.0);
            let chars_per_line = (usable_w / char_w).floor().max(1.0) as usize;
            // Honor explicit \n in addition to wrap-driven breaks.
            cell.lines()
                .map(|line| {
                    let n = line.chars().count().max(1);
                    (n + chars_per_line - 1) / chars_per_line
                })
                .sum::<usize>()
                .max(1)
        })
        .max()
        .unwrap_or(1);
    (max_lines as f32 * line_h + pad_h).max(default_h)
}

pub fn column_letter(mut idx: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (idx % 26) as u8) as char);
        if idx < 26 { break; }
        idx = idx / 26 - 1;
    }
    s
}

/// Natural alphanumeric comparison: contiguous digit runs compare as
/// integers so `R10` sorts after `R2`. Letter runs compare case-insensitive.
fn compare_alphanumeric(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek(), bi.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let mut an = 0u64;
                    let mut bn = 0u64;
                    while let Some(&c) = ai.peek() {
                        if !c.is_ascii_digit() { break; }
                        an = an.saturating_mul(10) + (c as u64 - b'0' as u64);
                        ai.next();
                    }
                    while let Some(&c) = bi.peek() {
                        if !c.is_ascii_digit() { break; }
                        bn = bn.saturating_mul(10) + (c as u64 - b'0' as u64);
                        bi.next();
                    }
                    match an.cmp(&bn) {
                        Ordering::Equal => continue,
                        non_eq => return non_eq,
                    }
                } else {
                    let la = ca.to_ascii_lowercase();
                    let lb = cb.to_ascii_lowercase();
                    match la.cmp(&lb) {
                        Ordering::Equal => { ai.next(); bi.next(); continue; }
                        non_eq => return non_eq,
                    }
                }
            }
        }
    }
}

fn plus_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let p = palette::current();
    let ws = palette::widget_surface();
    let (bg, text_color) = match status {
        button::Status::Hovered => (Some(Background::Color(ws.fill)), p.text),
        button::Status::Pressed => (Some(Background::Color(ws.border)), p.text),
        _ => (None, p.overlay0),
    };
    button::Style {
        background: bg,
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

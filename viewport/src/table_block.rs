use iced_wgpu::core::widget::Id as WidgetId;
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
use crate::palette;
use crate::selection::{BlockId, InnerPath};
use crate::syntax::EDITOR_FONT;

const MIN_COL_WIDTH: f32 = 60.0;
const DEFAULT_COL_WIDTH: f32 = 120.0;
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
#[allow(dead_code)]
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
}

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
            // tables, use a uniform default width.
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
                vec![DEFAULT_COL_WIDTH; col_count]
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
        }
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
            }
            TableMessage::EditCell(row, col) => {
                // Double click — selected AND editing. The editor's
                // `editing` field is set, and `pending_focus` is queued so
                // iced moves keyboard focus to the cell's text_input on the
                // next frame.
                self.focused_cell = Some((row, col));
                self.is_active = true;
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

fn cell_border() -> Border {
    let ws = palette::widget_surface();
    Border {
        color: ws.border,
        width: 1.0,
        radius: 0.0.into(),
    }
}

fn cell_input_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    let p = palette::current();
    let ws = palette::widget_surface();
    text_input::Style {
        background: Background::Color(ws.fill),
        border: cell_border(),
        icon: p.overlay2,
        placeholder: p.overlay0,
        value: ws.body_text,
        selection: Color { a: 0.4, ..p.blue },
    }
}

fn header_cell_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    let p = palette::current();
    let ws = palette::widget_surface();
    text_input::Style {
        background: Background::Color(ws.fill),
        border: cell_border(),
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
            let letter_container = container(
                text(letter)
                    .size(chrome_font)
                    .font(EDITOR_FONT)
                    .color(p.overlay0)
            )
            .width(Length::Fixed(*w))
            .height(Length::Fixed(header_h))
            .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 6.0 })
            .style(move |_theme: &Theme| container::Style {
                background: bg_color.map(Background::Color),
                border: Border::default(),
                text_color: None,
                shadow: Shadow::default(),
                snap: false,
            });
            let letter_cell: Element<'a, Message, Theme, iced_wgpu::Renderer> = if chrome_active {
                MouseArea::new(letter_container)
                    .on_press(on_msg(TableMessage::BeginColReorder(ci)))
                    .into()
            } else {
                letter_container.into()
            };
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
                    .color(p.overlay0)
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
                let style_fn: fn(&Theme, text_input::Status) -> text_input::Style = if is_header {
                    header_cell_style
                } else {
                    cell_input_style
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
                input.into()
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
                    .color(label_color);

                let container_style = move |_theme: &Theme| {
                    let ws = palette::widget_surface();
                    let p = palette::current();
                    let background = if is_focused_this {
                        // Tinted blue background — Excel/Numbers selection look.
                        // Heavier alpha than the default tint so selection is
                        // unmistakably visible against the cell fill.
                        Some(Background::Color(Color { a: 0.45, ..p.blue }))
                    } else {
                        Some(Background::Color(ws.fill))
                    };
                    container::Style {
                        background,
                        border: cell_border(),
                        text_color: Some(label_color),
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
                    .on_right_press(on_msg(TableMessage::ContextMenu(r, c)))
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
            .padding(Padding { top: 2.0, right: 0.0, bottom: 2.0, left: 8.0 })
            .width(Length::Shrink)
            .style(|_theme: &Theme| container::Style {
                background: None,
                border: Border::default(),
                text_color: None,
                shadow: Shadow::default(),
                snap: false,
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

fn column_letter(mut idx: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (idx % 26) as u8) as char);
        if idx < 26 { break; }
        idx = idx / 26 - 1;
    }
    s
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

//! Black-and-white PDF print of the open document.
//!
//! No styling beyond what print needs: Helvetica body, Helvetica-Bold headings,
//! Courier code/tables, simple grid borders, page numbers at the bottom.

use printpdf::{
    BuiltinFont, Color, Greyscale, Line, LinePoint, Mm, Op, PdfDocument, PdfFontHandle,
    PdfPage, PdfSaveOptions, Point, Pt, TextItem,
};

use crate::block::Block as BlockTrait;
use crate::editor::{EditorState, Message};
use crate::heading_block::{HeadingBlock, HeadingLevel};
use crate::hr_block::HrBlock;
use crate::table_block::TableBlock;
use crate::text_block::TextBlock;
use crate::tree_block::TreeBlock;

/// US Letter with 1-inch margins.
const PAGE_W_MM: f32 = 215.9;
const PAGE_H_MM: f32 = 279.4;
const MARGIN_MM: f32 = 19.05; // 0.75 inch — slightly tighter than 1" so notes fit

const BODY_PT: f32 = 10.5;
const CODE_PT: f32 = 9.5;
const LINE_GAP: f32 = 1.35; // line-height multiplier
const PARA_GAP_PT: f32 = 4.0;
const BLOCK_GAP_PT: f32 = 8.0;

const TABLE_PAD_PT: f32 = 4.0;
const TABLE_LINE_PT: f32 = 0.5;

/// Approximate glyph widths in em-units. Built-in font metrics aren't exposed
/// when `text_layout` is off; these are conservative averages so wrapping
/// under-fills slightly rather than overflowing.
fn avg_em(font: BuiltinFont) -> f32 {
    match font {
        BuiltinFont::Courier
        | BuiltinFont::CourierBold
        | BuiltinFont::CourierOblique
        | BuiltinFont::CourierBoldOblique => 0.6,
        BuiltinFont::HelveticaBold | BuiltinFont::HelveticaBoldOblique => 0.55,
        _ => 0.5,
    }
}

fn approx_text_width_pt(text: &str, font: BuiltinFont, size_pt: f32) -> f32 {
    text.chars().count() as f32 * avg_em(font) * size_pt
}

#[derive(Clone)]
enum PrintBlock {
    Heading { level: u8, text: String },
    Paragraph { lines: Vec<String> },
    Code { lines: Vec<String> },
    Table { rows: Vec<Vec<String>> },
    Hr,
}

/// pulls printable blocks out of the editor's live block tree.
fn collect_print_blocks(editor: &EditorState) -> Vec<PrintBlock> {
    let mut out: Vec<PrintBlock> = Vec::new();
    for block in editor.iter_blocks() {
        if let Some(h) = block.as_any().downcast_ref::<HeadingBlock>() {
            out.push(PrintBlock::Heading {
                level: heading_level_to_u8(h.level),
                text: h.text.clone(),
            });
        } else if block.as_any().downcast_ref::<HrBlock>().is_some() {
            out.push(PrintBlock::Hr);
        } else if let Some(t) = block.as_any().downcast_ref::<TableBlock>() {
            out.push(PrintBlock::Table { rows: t.rows.clone() });
        } else if let Some(t) = block.as_any().downcast_ref::<TextBlock>() {
            push_text_block(&mut out, t);
        } else if let Some(t) = block.as_any().downcast_ref::<TreeBlock>() {
            let md = <TreeBlock as BlockTrait<Message>>::to_md(t);
            push_paragraph(&mut out, &md);
        }
    }
    out
}

fn heading_level_to_u8(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
    }
}

/// splits a TextBlock into runs of code-fenced and plain paragraphs.
fn push_text_block(out: &mut Vec<PrintBlock>, t: &TextBlock) {
    let md = <TextBlock as BlockTrait<Message>>::to_md(t);
    let mut buf: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in md.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_fence {
                if !buf.is_empty() {
                    out.push(PrintBlock::Code { lines: std::mem::take(&mut buf) });
                }
                in_fence = false;
            } else {
                if !buf.is_empty() {
                    push_paragraph(out, &buf.join("\n"));
                    buf.clear();
                }
                in_fence = true;
            }
            continue;
        }
        buf.push(line.to_string());
    }
    if in_fence {
        if !buf.is_empty() {
            out.push(PrintBlock::Code { lines: buf });
        }
    } else if !buf.is_empty() {
        push_paragraph(out, &buf.join("\n"));
    }
}

/// drops trailing blanks and pushes a paragraph if non-empty.
fn push_paragraph(out: &mut Vec<PrintBlock>, text: &str) {
    let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let trimmed = trim_blank_edges(&lines);
    if trimmed.is_empty() { return; }
    out.push(PrintBlock::Paragraph { lines: trimmed });
}

fn trim_blank_edges(lines: &[String]) -> Vec<String> {
    let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|l| !l.trim().is_empty()).map(|i| i + 1).unwrap_or(0);
    if start >= end { Vec::new() } else { lines[start..end].to_vec() }
}

/// breaks a paragraph string into wrap-fit lines for the given font/width.
fn wrap_lines(text: &str, font: BuiltinFont, size_pt: f32, max_w_pt: f32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.lines() {
        if raw.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current, word)
            };
            if approx_text_width_pt(&candidate, font, size_pt) <= max_w_pt {
                current = candidate;
            } else if current.is_empty() {
                // single word too long for the line — let it overflow rather than truncate
                out.push(word.to_string());
                current = String::new();
            } else {
                out.push(current);
                current = word.to_string();
            }
        }
        if !current.is_empty() { out.push(current); }
    }
    out
}

/// strips inline markdown markers (`**`, `*`, `_`, backticks) so plain text prints clean.
fn strip_inline_md(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' => {
                // collapse runs of the same marker
                while chars.peek() == Some(&c) { chars.next(); }
            }
            '`' => {
                while chars.peek() == Some(&'`') { chars.next(); }
            }
            _ => out.push(c),
        }
    }
    out
}

struct Layout {
    page_w_pt: f32,
    page_h_pt: f32,
    margin_pt: f32,
    pages: Vec<Vec<Op>>,
    cur: Vec<Op>,
    /// y-cursor in PDF coordinates (origin at bottom-left, growing upward).
    y_pt: f32,
    page_count: usize,
}

impl Layout {
    fn new() -> Self {
        let page_w_pt = mm_to_pt(PAGE_W_MM);
        let page_h_pt = mm_to_pt(PAGE_H_MM);
        let margin_pt = mm_to_pt(MARGIN_MM);
        let mut me = Self {
            page_w_pt,
            page_h_pt,
            margin_pt,
            pages: Vec::new(),
            cur: Vec::new(),
            y_pt: page_h_pt - margin_pt,
            page_count: 0,
        };
        me.start_page();
        me
    }

    fn body_width_pt(&self) -> f32 {
        self.page_w_pt - 2.0 * self.margin_pt
    }

    fn bottom_limit_pt(&self) -> f32 {
        self.margin_pt
    }

    fn start_page(&mut self) {
        self.cur = Vec::new();
        self.cur.push(Op::SetFillColor { col: Color::Greyscale(Greyscale { percent: 0.0, icc_profile: None }) });
        self.cur.push(Op::SetOutlineColor { col: Color::Greyscale(Greyscale { percent: 0.0, icc_profile: None }) });
        self.y_pt = self.page_h_pt - self.margin_pt;
        self.page_count += 1;
    }

    fn finish_page(&mut self) {
        let footer = format!("{}", self.page_count);
        let w = approx_text_width_pt(&footer, BuiltinFont::Helvetica, 9.0);
        let x = (self.page_w_pt - w) / 2.0;
        let y = self.margin_pt / 2.0;
        self.cur.push(Op::StartTextSection);
        self.cur.push(Op::SetFont {
            font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
            size: Pt(9.0),
        });
        self.cur.push(Op::SetTextCursor { pos: Point { x: Pt(x), y: Pt(y) } });
        self.cur.push(Op::ShowText { items: vec![TextItem::Text(footer)] });
        self.cur.push(Op::EndTextSection);
        self.pages.push(std::mem::take(&mut self.cur));
    }

    fn ensure_space(&mut self, needed_pt: f32) {
        if self.y_pt - needed_pt < self.bottom_limit_pt() {
            self.finish_page();
            self.start_page();
        }
    }

    fn advance(&mut self, dy_pt: f32) {
        self.y_pt -= dy_pt;
    }

    fn draw_text_line(&mut self, line: &str, font: BuiltinFont, size_pt: f32) {
        let line_h = size_pt * LINE_GAP;
        self.ensure_space(line_h);
        self.advance(line_h);
        if line.is_empty() { return; }
        self.cur.push(Op::StartTextSection);
        self.cur.push(Op::SetFont {
            font: PdfFontHandle::Builtin(font),
            size: Pt(size_pt),
        });
        self.cur.push(Op::SetTextCursor {
            pos: Point { x: Pt(self.margin_pt), y: Pt(self.y_pt) },
        });
        self.cur.push(Op::ShowText { items: vec![TextItem::Text(line.to_string())] });
        self.cur.push(Op::EndTextSection);
    }

    fn draw_hr(&mut self) {
        let h = 6.0;
        self.ensure_space(h);
        self.advance(h / 2.0);
        let y = self.y_pt;
        self.cur.push(Op::SetOutlineThickness { pt: Pt(0.5) });
        self.cur.push(Op::DrawLine {
            line: Line {
                points: vec![
                    LinePoint { p: Point { x: Pt(self.margin_pt), y: Pt(y) }, bezier: false },
                    LinePoint { p: Point { x: Pt(self.page_w_pt - self.margin_pt), y: Pt(y) }, bezier: false },
                ],
                is_closed: false,
            },
        });
        self.advance(h / 2.0);
    }

    fn draw_table(&mut self, rows: &[Vec<String>]) {
        if rows.is_empty() { return; }
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if cols == 0 { return; }
        let body_w = self.body_width_pt();
        let col_w = body_w / cols as f32;

        let font_body = BuiltinFont::Helvetica;
        let font_head = BuiltinFont::HelveticaBold;
        let cell_inner_w = col_w - 2.0 * TABLE_PAD_PT;

        // pre-wrap each cell into lines so we can compute row heights up front.
        let wrapped: Vec<Vec<Vec<String>>> = rows
            .iter()
            .enumerate()
            .map(|(ri, row)| {
                let f = if ri == 0 { font_head } else { font_body };
                row.iter()
                    .map(|c| wrap_lines(&strip_inline_md(c), f, BODY_PT, cell_inner_w))
                    .collect()
            })
            .collect();

        for (ri, row) in rows.iter().enumerate() {
            let line_h = BODY_PT * LINE_GAP;
            let max_lines = wrapped[ri].iter().map(|c| c.len().max(1)).max().unwrap_or(1);
            let row_h = max_lines as f32 * line_h + 2.0 * TABLE_PAD_PT;

            self.ensure_space(row_h);

            let top_y = self.y_pt;
            let bottom_y = top_y - row_h;

            // borders
            self.cur.push(Op::SetOutlineThickness { pt: Pt(TABLE_LINE_PT) });
            // top
            self.cur.push(Op::DrawLine { line: hline(self.margin_pt, top_y, self.margin_pt + body_w, top_y) });
            // bottom
            self.cur.push(Op::DrawLine { line: hline(self.margin_pt, bottom_y, self.margin_pt + body_w, bottom_y) });
            // left + verticals + right
            for c in 0..=cols {
                let x = self.margin_pt + c as f32 * col_w;
                self.cur.push(Op::DrawLine { line: vline(x, top_y, x, bottom_y) });
            }

            // cell text
            let font = if ri == 0 { font_head } else { font_body };
            for (ci, lines) in wrapped[ri].iter().enumerate() {
                if ci >= row.len() { break; }
                let cell_x = self.margin_pt + ci as f32 * col_w + TABLE_PAD_PT;
                let mut text_y = top_y - TABLE_PAD_PT - BODY_PT * 0.85;
                for line in lines {
                    self.cur.push(Op::StartTextSection);
                    self.cur.push(Op::SetFont {
                        font: PdfFontHandle::Builtin(font),
                        size: Pt(BODY_PT),
                    });
                    self.cur.push(Op::SetTextCursor {
                        pos: Point { x: Pt(cell_x), y: Pt(text_y) },
                    });
                    self.cur.push(Op::ShowText { items: vec![TextItem::Text(line.clone())] });
                    self.cur.push(Op::EndTextSection);
                    text_y -= line_h;
                }
            }

            self.y_pt = bottom_y;
        }
    }

    fn finish(mut self) -> Vec<Vec<Op>> {
        self.finish_page();
        self.pages
    }
}

fn hline(x1: f32, y1: f32, x2: f32, y2: f32) -> Line {
    Line {
        points: vec![
            LinePoint { p: Point { x: Pt(x1), y: Pt(y1) }, bezier: false },
            LinePoint { p: Point { x: Pt(x2), y: Pt(y2) }, bezier: false },
        ],
        is_closed: false,
    }
}

fn vline(x1: f32, y1: f32, x2: f32, y2: f32) -> Line {
    Line {
        points: vec![
            LinePoint { p: Point { x: Pt(x1), y: Pt(y1) }, bezier: false },
            LinePoint { p: Point { x: Pt(x2), y: Pt(y2) }, bezier: false },
        ],
        is_closed: false,
    }
}

fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 18.0,
        2 => 15.0,
        3 => 13.0,
        _ => 12.0,
    }
}

fn render_blocks(blocks: &[PrintBlock], layout: &mut Layout) {
    let body_w = layout.body_width_pt();
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 { layout.advance(BLOCK_GAP_PT); }
        match block {
            PrintBlock::Heading { level, text } => {
                let size = heading_size(*level);
                let lines = wrap_lines(&strip_inline_md(text), BuiltinFont::HelveticaBold, size, body_w);
                for line in &lines {
                    layout.draw_text_line(line, BuiltinFont::HelveticaBold, size);
                }
            }
            PrintBlock::Paragraph { lines } => {
                for raw in lines {
                    let stripped = strip_inline_md(raw);
                    if stripped.trim().is_empty() {
                        layout.advance(PARA_GAP_PT);
                        continue;
                    }
                    let wrapped = wrap_lines(&stripped, BuiltinFont::Helvetica, BODY_PT, body_w);
                    for w in &wrapped {
                        layout.draw_text_line(w, BuiltinFont::Helvetica, BODY_PT);
                    }
                }
            }
            PrintBlock::Code { lines } => {
                for raw in lines {
                    let wrapped = wrap_lines(raw, BuiltinFont::Courier, CODE_PT, body_w);
                    if wrapped.is_empty() {
                        layout.draw_text_line("", BuiltinFont::Courier, CODE_PT);
                    } else {
                        for w in &wrapped {
                            layout.draw_text_line(w, BuiltinFont::Courier, CODE_PT);
                        }
                    }
                }
            }
            PrintBlock::Table { rows } => {
                layout.draw_table(rows);
            }
            PrintBlock::Hr => {
                layout.draw_hr();
            }
        }
    }
}

/// renders the current document as a printable PDF and returns the bytes.
pub fn render_pdf(editor: &EditorState, title: &str) -> Vec<u8> {
    let blocks = collect_print_blocks(editor);
    let mut layout = Layout::new();
    render_blocks(&blocks, &mut layout);
    let pages_ops = layout.finish();

    let mut doc = PdfDocument::new(title);
    for ops in pages_ops {
        doc.pages.push(PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), ops));
    }
    let mut warnings = Vec::new();
    doc.save(&PdfSaveOptions::default(), &mut warnings)
}

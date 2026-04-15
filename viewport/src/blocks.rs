//! Document parsing and block-list utilities.
//!
//! Owns the markdown -> `Vec<BoxedBlock>` parser plus the round-trip helpers
//! (serialize, recount lines, locate by line). Every block kind lives in its
//! own module behind the `Block` trait; this file deals with document-wide
//! concerns only.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::block::Block;
use crate::editor::Message;
use crate::heading_block::{HeadingBlock, HeadingLevel};
use crate::hr_block::HrBlock;
use crate::table_block::TableBlock;
use crate::text_block::TextBlock;

pub type BoxedBlock = Box<dyn Block<Message>>;

static NEXT_BLOCK_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed)
}

/// Split text into lines, preserving trailing empty lines that `str::lines()` drops.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.lines().collect();
    if text.ends_with('\n') {
        lines.push("");
    }
    lines
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanKind {
    Text,
    Table,
    HorizontalRule,
    Heading,
}

fn is_hr_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-')
}

fn heading_prefix(line: &str) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || bytes[0] != b'#' {
        return None;
    }
    let mut level = 0u8;
    while (level as usize) < bytes.len() && bytes[level as usize] == b'#' {
        level += 1;
    }
    if level > 4 || (level as usize) >= bytes.len() || bytes[level as usize] != b' ' {
        return None;
    }
    Some((level, &trimmed[level as usize + 1..]))
}

fn is_table_start(lines: &[&str], idx: usize) -> bool {
    if idx + 1 >= lines.len() {
        return false;
    }
    let line = lines[idx].trim();
    let next = lines[idx + 1].trim();
    if !line.starts_with('|') || !next.starts_with('|') {
        return false;
    }
    let inner = next.strip_prefix('|').unwrap_or(next);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').all(|seg| {
        let s = seg.trim();
        !s.is_empty() && s.chars().all(|c| c == '-' || c == ':')
    })
}

fn consume_table(lines: &[&str], start: usize) -> (Vec<Vec<String>>, usize) {
    let parse_row = |line: &str| -> Vec<String> {
        let trimmed = line.trim();
        let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
        let inner = inner.strip_suffix('|').unwrap_or(inner);
        inner.split('|').map(|c| c.trim().to_string()).collect()
    };

    let mut rows = vec![parse_row(lines[start])];
    let mut end = start + 2; // skip header + separator
    while end < lines.len() {
        let trimmed = lines[end].trim();
        if trimmed.is_empty() || !trimmed.contains('|') {
            break;
        }
        rows.push(parse_row(lines[end]));
        end += 1;
    }
    (rows, end)
}

struct BlockSpan {
    kind: SpanKind,
    start: usize,
    end: usize, // exclusive
    heading_level: u8,
    heading_text: String,
    table_rows: Vec<Vec<String>>,
}

impl BlockSpan {
    fn line_count(&self) -> usize {
        self.end - self.start
    }
}

fn classify_spans(lines: &[&str]) -> Vec<BlockSpan> {
    let mut spans = Vec::new();
    let mut i = 0;
    let mut text_start: Option<usize> = None;

    let flush_text = |text_start: &mut Option<usize>, i: usize, spans: &mut Vec<BlockSpan>| {
        if let Some(s) = text_start.take() {
            if s < i {
                spans.push(BlockSpan {
                    kind: SpanKind::Text,
                    start: s,
                    end: i,
                    heading_level: 0,
                    heading_text: String::new(),
                    table_rows: Vec::new(),
                });
            }
        }
    };

    while i < lines.len() {
        if is_hr_line(lines[i]) {
            flush_text(&mut text_start, i, &mut spans);
            spans.push(BlockSpan {
                kind: SpanKind::HorizontalRule,
                start: i,
                end: i + 1,
                heading_level: 0,
                heading_text: String::new(),
                table_rows: Vec::new(),
            });
            i += 1;
        } else if let Some((level, text)) = heading_prefix(lines[i]) {
            flush_text(&mut text_start, i, &mut spans);
            spans.push(BlockSpan {
                kind: SpanKind::Heading,
                start: i,
                end: i + 1,
                heading_level: level,
                heading_text: text.to_string(),
                table_rows: Vec::new(),
            });
            i += 1;
        } else if is_table_start(lines, i) {
            flush_text(&mut text_start, i, &mut spans);
            let (rows, end) = consume_table(lines, i);
            spans.push(BlockSpan {
                kind: SpanKind::Table,
                start: i,
                end,
                heading_level: 0,
                heading_text: String::new(),
                table_rows: rows,
            });
            i = end;
        } else {
            if text_start.is_none() {
                text_start = Some(i);
            }
            i += 1;
        }
    }
    flush_text(&mut text_start, lines.len(), &mut spans);

    if spans.is_empty() {
        spans.push(BlockSpan {
            kind: SpanKind::Text,
            start: 0,
            end: 0,
            heading_level: 0,
            heading_text: String::new(),
            table_rows: Vec::new(),
        });
    }

    spans
}

fn build_block(span: &BlockSpan, lines: &[&str], lang: &str) -> BoxedBlock {
    match span.kind {
        SpanKind::Text => {
            let block_text = lines[span.start..span.end].join("\n");
            Box::new(TextBlock::new(next_id(), &block_text, span.start, lang.to_string()))
        }
        SpanKind::HorizontalRule => Box::new(HrBlock::new(next_id(), span.start)),
        SpanKind::Heading => Box::new(HeadingBlock::new(
            next_id(),
            HeadingLevel::from_u8(span.heading_level),
            span.heading_text.clone(),
            span.start,
        )),
        SpanKind::Table => {
            Box::new(TableBlock::new(
                next_id(),
                span.table_rows.clone(),
                span.start,
            ))
        }
    }
}

pub fn parse_blocks(text: &str, lang: &str) -> Vec<BoxedBlock> {
    if text.is_empty() {
        return vec![Box::new(TextBlock::new(next_id(), "", 0, lang.to_string()))];
    }

    let lines: Vec<&str> = split_lines(text);
    let spans = classify_spans(&lines);
    let mut blocks: Vec<BoxedBlock> = Vec::with_capacity(spans.len());
    for span in &spans {
        blocks.push(build_block(span, &lines, lang));
    }
    if blocks.is_empty() {
        blocks.push(Box::new(TextBlock::new(next_id(), "", 0, lang.to_string())));
    }
    blocks
}

/// Incremental reparse: compare existing block kinds + spans to the new
/// classification, reuse boxed instances where the slot matches, rebuild the
/// rest. Preserves cursor state for unchanged text blocks because the
/// `text_editor::Content` instance is moved (not recreated) when both the
/// kind tag and the line span match.
pub fn reparse_incremental(old_blocks: &mut Vec<BoxedBlock>, text: &str, lang: &str) {
    let lines: Vec<&str> = if text.is_empty() {
        Vec::new()
    } else {
        split_lines(text)
    };
    let spans = classify_spans(&lines);

    let old_descriptors: Vec<(&'static str, usize, usize)> = old_blocks
        .iter()
        .map(|b| (b.kind_tag(), b.start_line(), b.line_count()))
        .collect();

    let new_descriptors: Vec<(&'static str, usize, usize)> = spans
        .iter()
        .map(|s| (span_kind_tag(s.kind), s.start, s.line_count()))
        .collect();

    if old_descriptors == new_descriptors {
        // Same structure: update text content in place for text blocks; align
        // start_line for all kinds. Cursor preserved (Content not recreated
        // unless serialized text actually changed).
        for (block, span) in old_blocks.iter_mut().zip(spans.iter()) {
            block.set_start_line(span.start);
            if matches!(span.kind, SpanKind::Text) {
                if let Some(tb) = block.as_any_mut().downcast_mut::<TextBlock>() {
                    let block_text = lines[span.start..span.end].join("\n");
                    let current = tb.content.text();
                    if current != block_text {
                        tb.content =
                            crate::text_widget::Content::with_text(&block_text);
                    }
                }
            }
        }
        return;
    }

    // Structure changed: rebuild, reusing blocks at matching positions. Match
    // is by (kind_tag, start_line); the boxed instance is `mem::replace`'d so
    // any preserved state (text editor cursor, table focus, drag) survives.
    let placeholder = || -> BoxedBlock {
        Box::new(TextBlock::new(0, "", 0, String::new()))
    };
    let mut new_blocks: Vec<BoxedBlock> = Vec::with_capacity(spans.len());
    for (i, span) in spans.iter().enumerate() {
        let span_tag = span_kind_tag(span.kind);
        let reuse = i < old_blocks.len()
            && old_blocks[i].kind_tag() == span_tag
            && old_blocks[i].start_line() == span.start;
        if reuse {
            let mut b = std::mem::replace(&mut old_blocks[i], placeholder());
            b.set_start_line(span.start);
            if matches!(span.kind, SpanKind::Text) {
                if let Some(tb) = b.as_any_mut().downcast_mut::<TextBlock>() {
                    let block_text = lines[span.start..span.end].join("\n");
                    if tb.content.text() != block_text {
                        tb.content =
                            crate::text_widget::Content::with_text(&block_text);
                    }
                }
            }
            new_blocks.push(b);
        } else {
            new_blocks.push(build_block(span, &lines, lang));
        }
    }

    if new_blocks.is_empty() {
        new_blocks.push(Box::new(TextBlock::new(next_id(), "", 0, lang.to_string())));
    }

    *old_blocks = new_blocks;
}

fn span_kind_tag(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::Text => "text",
        SpanKind::Table => "table",
        SpanKind::HorizontalRule => "hr",
        SpanKind::Heading => "heading",
    }
}

/// Serialize blocks back to document text. Eval-result tables and trees are
/// skipped — they're regenerated from source on every load.
pub fn serialize_blocks(blocks: &[BoxedBlock]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        let tag = block.kind_tag();
        if tag == "tree" {
            continue;
        }
        if tag == "table" && block.is_eval_result() {
            continue;
        }
        let md = block.to_md();
        // For text blocks, push even empty strings — they preserve the empty
        // line gap between adjacent non-text blocks.
        if tag == "text" || !md.is_empty() {
            parts.push(md);
        }
    }
    parts.join("\n")
}

/// Update document-relative start_line on every block based on its line_count.
pub fn recount_lines(blocks: &mut [BoxedBlock]) {
    let mut line = 0;
    for block in blocks.iter_mut() {
        block.set_start_line(line);
        line += block.line_count();
    }
}

/// Find the block index containing a given global line number.
pub fn block_at_line(blocks: &[BoxedBlock], global_line: usize) -> Option<usize> {
    for (i, block) in blocks.iter().enumerate() {
        let start = block.start_line();
        let end = start + block.line_count();
        if global_line >= start && global_line < end {
            return Some(i);
        }
    }
    None
}

pub fn line_count_with_trailing(text: &str) -> usize {
    split_lines(text).len().max(1)
}

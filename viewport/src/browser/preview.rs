use std::ops::Range;

use iced_wgpu::core::text::highlighter::Highlighter;

use crate::syntax::{SyntaxHighlight, SyntaxHighlighter, SyntaxSettings};

/// a single highlighted preview line with byte-range spans and a markdown heading level.
#[derive(Debug, Clone)]
pub struct PreviewLine {
    pub text: String,
    pub spans: Vec<(Range<usize>, u8)>,
    pub heading: Option<u8>,
}

/// highlights source line-by-line with a fresh per-preview user-ident rainbow.
pub fn highlight_preview(source: &str) -> Vec<PreviewLine> {
    let settings = SyntaxSettings {
        lang: "rust".to_string(),
        source: source.to_string(),
    };
    let mut highlighter = SyntaxHighlighter::new(&settings);

    let mut out = Vec::new();
    for line in source.split('\n') {
        let spans: Vec<(Range<usize>, u8)> = highlighter
            .highlight_line(line)
            .map(|(range, SyntaxHighlight { kind })| (range, kind))
            .collect();
        let heading = parse_heading_level(line);
        out.push(PreviewLine {
            text: line.to_string(),
            spans,
            heading,
        });
    }
    out
}

/// returns the markdown heading level of the line, capped at 3, or none.
fn parse_heading_level(line: &str) -> Option<u8> {
    let trimmed = line.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || bytes[0] != b'#' {
        return None;
    }
    let mut level = 0usize;
    while level < bytes.len() && bytes[level] == b'#' {
        level += 1;
    }
    if level == 0 || level > 3 {
        return None;
    }
    if level < bytes.len() && bytes[level] == b' ' {
        Some(level as u8)
    } else {
        None
    }
}

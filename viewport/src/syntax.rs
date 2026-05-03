use std::collections::HashMap;
use std::ops::Range;

use iced_wgpu::core::text::highlighter;
use iced_wgpu::core::{Color, Font};
use iced_wgpu::core::font::{Weight, Style as FontStyle};
use acord_core::highlight::{highlight_source, HighlightSpan};
use acord_core::doc::{classify_document, LineKind};
use crate::editor::{RESULT_PREFIX, ERROR_PREFIX};
use crate::palette;

pub const EVAL_RESULT_KIND: u8 = 24;
pub const EVAL_ERROR_KIND: u8 = 25;

// --- Cordial (eval-line) tokens. Start at 50 to leave room above the
// markdown range. A single hand-rolled scanner (`highlight_cordial`) dispatches
// on these so every Cordial visual element — the `/=` sigil, the `@` ref
// prefix, `::`, table/block names, cell addresses, keywords, builtins,
// numbers, strings, comments — gets its own color.
const COR_EVAL_SIGIL: u8 = 50;
const COR_AT_SIGIL: u8 = 51;
const COR_COLON_COLON: u8 = 52;
const COR_REF_COLON: u8 = 53;
const COR_TABLE_NAME: u8 = 54;
const COR_BLOCK_NAME: u8 = 55;
const COR_CELL_ADDR: u8 = 56;
const COR_KEYWORD: u8 = 57;
const COR_BUILTIN_FN: u8 = 58;
const COR_NUMBER: u8 = 59;
const COR_STRING: u8 = 60;
const COR_COMMENT: u8 = 61;
const COR_OPERATOR: u8 = 62;
const COR_BRACKET: u8 = 63;
const COR_TYPE_ANN: u8 = 64;

// Per-identifier rainbow. Each user-introduced name (let, fn, params, for var,
// math-form fn def) gets one of eight palette slots, picked with a stride
// that avoids adjacent colors landing on consecutive identifiers. Subsequent
// references resolve to the same slot so the name reads the same color
// throughout the document.
const USER_IDENT_BASE: u8 = 70;
pub const USER_IDENT_PALETTE_SIZE: u8 = 8;
pub const USER_IDENT_HOP: u32 = 3;

/// The 8-slot rainbow shared by user-identifier highlighting and the gutter
/// line-number rainbow. Same hop-of-3 walk through the same palette so the
/// two systems read as one design.
pub fn rainbow_color(idx: u32) -> Color {
    let slot = ((idx * USER_IDENT_HOP) % USER_IDENT_PALETTE_SIZE as u32) as u8;
    highlight_color(USER_IDENT_BASE + slot)
}

const MD_HEADING_MARKER: u8 = 26;
const MD_H1: u8 = 27;
const MD_H2: u8 = 28;
const MD_H3: u8 = 29;
const MD_BOLD: u8 = 30;
const MD_ITALIC: u8 = 31;
const MD_INLINE_CODE: u8 = 32;
const MD_FORMAT_MARKER: u8 = 33;
const MD_LINK_TEXT: u8 = 34;
const MD_LINK_URL: u8 = 35;
const MD_BLOCKQUOTE_MARKER: u8 = 36;
const MD_BLOCKQUOTE: u8 = 37;
const MD_LIST_MARKER: u8 = 38;
const MD_FENCE_MARKER: u8 = 39;
const MD_CODE_BLOCK: u8 = 40;
const MD_HR: u8 = 41;
const MD_TASK_OPEN: u8 = 42;
const MD_TASK_DONE: u8 = 43;
const MD_BOLD_ITALIC: u8 = 44;

/// The monospace family used for the editor body and every inline highlight
/// span. Naming the family explicitly (rather than `Family::Monospace`) forces
/// cosmic-text / fontdb to resolve real Bold, Italic and BoldItalic faces,
/// which the generic monospace fallback does not reliably do on macOS because
/// cosmic-text hardcodes its default monospace family to "Noto Sans Mono".
#[cfg(target_os = "macos")]
pub const EDITOR_FONT: Font = Font::with_name("Menlo");
#[cfg(target_os = "windows")]
pub const EDITOR_FONT: Font = Font::with_name("Consolas");
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const EDITOR_FONT: Font = Font::with_name("DejaVu Sans Mono");

#[derive(Clone, PartialEq)]
pub struct SyntaxSettings {
    pub lang: String,
    pub source: String,
}

#[derive(Clone, Copy, Debug)]
pub struct SyntaxHighlight {
    pub kind: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineDecor {
    None,
    CodeBlock,
    Blockquote,
    HorizontalRule,
    FenceMarker,
}

pub struct SyntaxHighlighter {
    lang: String,
    spans: Vec<HighlightSpan>,
    line_offsets: Vec<usize>,
    line_kinds: Vec<LineKind>,
    in_fenced_code: bool,
    current_line: usize,
    line_decors: Vec<LineDecor>,
    user_idents: HashMap<String, u8>,
    /// per-line tree-sitter spans for fenced code body lines, by absolute line index.
    code_block_spans: HashMap<usize, Vec<(Range<usize>, SyntaxHighlight)>>,
}

impl SyntaxHighlighter {
    fn rebuild(&mut self, source: &str) {
        self.spans = highlight_source(source, &self.lang);
        self.line_offsets.clear();
        let mut offset = 0;
        for line in source.split('\n') {
            self.line_offsets.push(offset);
            offset += line.len() + 1;
        }
        let classified = classify_document(source);
        self.line_kinds = classified.into_iter().map(|cl| cl.kind).collect();

        self.line_decors.clear();
        let mut in_fence = false;
        for (i, raw_line) in source.split('\n').enumerate() {
            let is_md = i < self.line_kinds.len() && self.line_kinds[i] == LineKind::Markdown;
            if is_md {
                let trimmed = raw_line.trim_start();
                if trimmed.starts_with("```") {
                    in_fence = !in_fence;
                    self.line_decors.push(LineDecor::FenceMarker);
                } else if in_fence {
                    self.line_decors.push(LineDecor::CodeBlock);
                } else if is_horizontal_rule(trimmed) {
                    self.line_decors.push(LineDecor::HorizontalRule);
                } else if trimmed.starts_with("> ") || trimmed == ">" {
                    self.line_decors.push(LineDecor::Blockquote);
                } else {
                    self.line_decors.push(LineDecor::None);
                }
            } else {
                if in_fence { in_fence = false; }
                self.line_decors.push(LineDecor::None);
            }
        }

        self.in_fenced_code = false;
        self.current_line = 0;

        self.scan_user_idents(source);
        self.scan_fenced_code_blocks(source);
    }

    /// runs each language-tagged fenced block through tree-sitter and stashes per-line spans.
    fn scan_fenced_code_blocks(&mut self, source: &str) {
        self.code_block_spans.clear();
        let lines: Vec<&str> = source.split('\n').collect();
        let mut i = 0;
        while i < lines.len() {
            let is_md = i < self.line_kinds.len()
                && self.line_kinds[i] == LineKind::Markdown;
            if !is_md {
                i += 1;
                continue;
            }
            let trimmed = lines[i].trim_start();
            if !trimmed.starts_with("```") {
                i += 1;
                continue;
            }
            let lang_label = trimmed.strip_prefix("```").unwrap_or("").trim();

            let mut j = i + 1;
            while j < lines.len() {
                let jis_md = j < self.line_kinds.len()
                    && self.line_kinds[j] == LineKind::Markdown;
                if jis_md && lines[j].trim_start().starts_with("```") {
                    break;
                }
                j += 1;
            }

            let body_start_line = i + 1;
            let body_end_line = j; // exclusive
            if body_end_line > body_start_line {
                if let Some(canonical) = canonical_code_lang(lang_label) {
                    let body_start_byte = self.line_offsets[body_start_line];
                    let body_end_byte = if body_end_line < self.line_offsets.len() {
                        self.line_offsets[body_end_line].saturating_sub(1)
                    } else {
                        source.len()
                    };
                    let body_end_byte = body_end_byte.min(source.len());
                    if body_end_byte > body_start_byte {
                        let body = &source[body_start_byte..body_end_byte];
                        let spans = highlight_source(body, &canonical);
                        for span in spans {
                            let abs_start = body_start_byte + span.start;
                            let abs_end = body_start_byte + span.end;
                            for line_idx in body_start_line..body_end_line {
                                let line_byte_start = self.line_offsets[line_idx];
                                let line_byte_end = if line_idx + 1 < self.line_offsets.len() {
                                    self.line_offsets[line_idx + 1].saturating_sub(1)
                                } else {
                                    source.len()
                                };
                                if abs_start >= line_byte_end { continue; }
                                if abs_end <= line_byte_start { break; }
                                let local_start = abs_start.saturating_sub(line_byte_start);
                                let local_end = abs_end.min(line_byte_end) - line_byte_start;
                                if local_end > local_start {
                                    self.code_block_spans
                                        .entry(line_idx)
                                        .or_default()
                                        .push((
                                            local_start..local_end,
                                            SyntaxHighlight { kind: span.kind },
                                        ));
                                }
                            }
                        }
                    }
                }
            }
            i = j + 1; // skip past closing fence (or break out if unclosed)
        }
    }

    /// Walk the source, find every identifier introduction site (let, fn,
    /// for, math-form fn def, params), and assign each unique name a slot
    /// in the user-ident rainbow. Subsequent references in `highlight_cordial`
    /// look the name up here.
    fn scan_user_idents(&mut self, source: &str) {
        self.user_idents.clear();
        let mut next_slot: u32 = 0;

        for line in source.split('\n') {
            let trimmed = line.trim_start();
            let bytes = trimmed.as_bytes();

            // `let IDENT...`
            if let Some(rest) = trimmed.strip_prefix("let ") {
                let mut i = 0;
                let rb = rest.as_bytes();
                while i < rb.len() && rb[i] == b' ' { i += 1; }
                let name_start = i;
                while i < rb.len() && (rb[i].is_ascii_alphanumeric() || rb[i] == b'_') { i += 1; }
                if i > name_start {
                    assign_user_ident(&mut self.user_idents, &mut next_slot, &rest[name_start..i]);
                }
                while i < rb.len() && rb[i] == b' ' { i += 1; }
                if i < rb.len() && rb[i] == b'(' {
                    extract_paren_idents(&rest[i + 1..], &mut self.user_idents, &mut next_slot);
                }
                continue;
            }

            // `fn IDENT(...)`
            if let Some(rest) = trimmed.strip_prefix("fn ") {
                let mut i = 0;
                let rb = rest.as_bytes();
                while i < rb.len() && rb[i] == b' ' { i += 1; }
                let name_start = i;
                while i < rb.len() && (rb[i].is_ascii_alphanumeric() || rb[i] == b'_') { i += 1; }
                if i > name_start {
                    assign_user_ident(&mut self.user_idents, &mut next_slot, &rest[name_start..i]);
                }
                while i < rb.len() && rb[i] == b' ' { i += 1; }
                if i < rb.len() && rb[i] == b'(' {
                    extract_paren_idents(&rest[i + 1..], &mut self.user_idents, &mut next_slot);
                }
                continue;
            }

            // `for IDENT in ...`
            if let Some(rest) = trimmed.strip_prefix("for ") {
                let rb = rest.as_bytes();
                let mut i = 0;
                while i < rb.len() && rb[i] == b' ' { i += 1; }
                let name_start = i;
                while i < rb.len() && (rb[i].is_ascii_alphanumeric() || rb[i] == b'_') { i += 1; }
                if i > name_start {
                    assign_user_ident(&mut self.user_idents, &mut next_slot, &rest[name_start..i]);
                }
                continue;
            }

            // `IDENT(...) = ...` math-form fn def, OR `IDENT = ...` assignment
            let mut i = 0;
            let name_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') { i += 1; }
            if i > name_start {
                let name = &trimmed[name_start..i];
                let mut j = i;
                while j < bytes.len() && bytes[j] == b' ' { j += 1; }
                if j < bytes.len() {
                    if bytes[j] == b'(' {
                        assign_user_ident(&mut self.user_idents, &mut next_slot, name);
                        extract_paren_idents(&trimmed[j + 1..], &mut self.user_idents, &mut next_slot);
                    } else if bytes[j] == b'=' && (j + 1 >= bytes.len() || bytes[j + 1] != b'=') {
                        assign_user_ident(&mut self.user_idents, &mut next_slot, name);
                    }
                }
            }
        }
    }

    /// byte ranges of inline format markers on a non-fenced markdown line, empty otherwise.
    pub fn line_marker_ranges(&self, line_idx: usize, line_text: &str) -> Vec<Range<usize>> {
        if line_idx >= self.line_kinds.len() {
            return Vec::new();
        }
        if self.line_kinds[line_idx] != LineKind::Markdown {
            return Vec::new();
        }
        if line_idx < self.line_decors.len() {
            match self.line_decors[line_idx] {
                LineDecor::CodeBlock
                | LineDecor::FenceMarker
                | LineDecor::HorizontalRule => return Vec::new(),
                _ => {}
            }
        }
        parse_inline(line_text, 0)
            .into_iter()
            .filter(|(_, h)| h.kind == MD_FORMAT_MARKER)
            .map(|(r, _)| r)
            .collect()
    }

    fn highlight_markdown(&self, line: &str) -> Vec<(Range<usize>, SyntaxHighlight)> {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();

        if is_horizontal_rule(trimmed) {
            return vec![(0..line.len(), SyntaxHighlight { kind: MD_HR })];
        }

        if let Some(level) = heading_level(trimmed) {
            let marker_end = leading + level + 1;
            let kind = match level {
                1 => MD_H1,
                2 => MD_H2,
                _ => MD_H3,
            };
            let mut spans = vec![
                (0..marker_end, SyntaxHighlight { kind: MD_HEADING_MARKER }),
            ];
            if marker_end < line.len() {
                spans.push((marker_end..line.len(), SyntaxHighlight { kind }));
            }
            return spans;
        }

        if trimmed.starts_with("> ") || trimmed == ">" {
            let marker_end = leading + if trimmed.len() > 1 { 2 } else { 1 };
            let mut spans = vec![
                (0..marker_end, SyntaxHighlight { kind: MD_BLOCKQUOTE_MARKER }),
            ];
            if marker_end < line.len() {
                let content = &line[marker_end..];
                let inner = parse_inline(content, marker_end);
                if inner.is_empty() {
                    spans.push((marker_end..line.len(), SyntaxHighlight { kind: MD_BLOCKQUOTE }));
                } else {
                    spans.extend(inner);
                }
            }
            return spans;
        }

        if let Some(list_info) = list_marker_info(trimmed) {
            let (marker_len, marker_kind) = match list_info {
                ListKind::TaskOpen(n) => (n, MD_TASK_OPEN),
                ListKind::TaskDone(n) => (n, MD_TASK_DONE),
                ListKind::Plain(n) => (n, MD_LIST_MARKER),
            };
            let marker_end = leading + marker_len;
            let mut spans = vec![
                (0..marker_end, SyntaxHighlight { kind: marker_kind }),
            ];
            if marker_end < line.len() {
                let content = &line[marker_end..];
                let inner = parse_inline(content, marker_end);
                if inner.is_empty() {
                    return spans;
                }
                spans.extend(inner);
            }
            return spans;
        }

        parse_inline(line, 0)
    }
}

/// Scan a Cordial line (or an eval line) and emit per-token highlight
/// spans. Idempotent, single-pass; each branch either consumes a whole
/// token or advances one byte. Unknown bytes get no highlight (they fall
/// through to the editor's default text color).
fn assign_user_ident(map: &mut HashMap<String, u8>, slot: &mut u32, name: &str) {
    if name.is_empty()
        || is_cordial_keyword(name)
        || is_cordial_builtin(name)
        || is_cordial_type_annotation(name)
        || name == "pi"
        || name == "where"
        || name == "from"
        || name == "solve"
        || map.contains_key(name)
    {
        return;
    }
    let color_idx = ((*slot * USER_IDENT_HOP) % USER_IDENT_PALETTE_SIZE as u32) as u8;
    map.insert(name.to_string(), color_idx);
    *slot += 1;
}

fn extract_paren_idents(s: &str, map: &mut HashMap<String, u8>, slot: &mut u32) {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut depth: i32 = 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => { depth += 1; i += 1; }
            b')' => { depth -= 1; i += 1; }
            b':' => {
                // Skip the type identifier that follows; type names belong
                // to the type-annotation color, not the user-ident rainbow.
                i += 1;
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') { i += 1; }
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') { i += 1; }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') { i += 1; }
                assign_user_ident(map, slot, &s[start..i]);
            }
            _ => i += 1,
        }
    }
}

fn highlight_cordial(line: &str, user_idents: &HashMap<String, u8>) -> Vec<(Range<usize>, SyntaxHighlight)> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut spans: Vec<(Range<usize>, SyntaxHighlight)> = Vec::new();
    let mut i = 0;

    // Opening `/=`, `/=|`, `/=\` sigil (with optional leading whitespace).
    let leading = line.len() - line.trim_start().len();
    if leading + 2 <= len && &bytes[leading..leading + 2] == b"/=" {
        let sigil_end = if leading + 3 <= len
            && (bytes[leading + 2] == b'|' || bytes[leading + 2] == b'\\')
        {
            leading + 3
        } else {
            leading + 2
        };
        spans.push((leading..sigil_end, SyntaxHighlight { kind: COR_EVAL_SIGIL }));
        i = sigil_end;
    }

    while i < len {
        let c = bytes[i];

        // Whitespace: skip.
        if c == b' ' || c == b'\t' || c == b'\r' {
            i += 1;
            continue;
        }

        // Line comment: `// …` — rest of line.
        if c == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            spans.push((i..len, SyntaxHighlight { kind: COR_COMMENT }));
            break;
        }

        // String literal.
        if c == b'"' {
            let start = i;
            i += 1;
            while i < len && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < len { i += 2; } else { i += 1; }
            }
            if i < len { i += 1; }
            spans.push((start..i, SyntaxHighlight { kind: COR_STRING }));
            continue;
        }

        // `@` cell reference: @[Block::]Table[:A1[:B4]] or @T[A1:B4].
        if c == b'@' {
            spans.push((i..i + 1, SyntaxHighlight { kind: COR_AT_SIGIL }));
            i += 1;
            // First ident.
            let n1_start = i;
            while i < len && is_ident_byte(bytes[i]) { i += 1; }
            let n1_end = i;
            // Is it a block qualifier? Look for `::` after.
            if i + 1 < len && bytes[i] == b':' && bytes[i + 1] == b':' {
                if n1_end > n1_start {
                    spans.push((n1_start..n1_end, SyntaxHighlight { kind: COR_BLOCK_NAME }));
                }
                spans.push((i..i + 2, SyntaxHighlight { kind: COR_COLON_COLON }));
                i += 2;
                let t_start = i;
                while i < len && is_ident_byte(bytes[i]) { i += 1; }
                if i > t_start {
                    spans.push((t_start..i, SyntaxHighlight { kind: COR_TABLE_NAME }));
                }
            } else if n1_end > n1_start {
                spans.push((n1_start..n1_end, SyntaxHighlight { kind: COR_TABLE_NAME }));
            }
            // Optional `:A1` or `:A1:B2` cell/range target.
            if i < len && bytes[i] == b':' {
                spans.push((i..i + 1, SyntaxHighlight { kind: COR_REF_COLON }));
                i += 1;
                i = consume_cell_addr(bytes, i, &mut spans);
                if i < len && bytes[i] == b':' {
                    spans.push((i..i + 1, SyntaxHighlight { kind: COR_REF_COLON }));
                    i += 1;
                    i = consume_cell_addr(bytes, i, &mut spans);
                }
            } else if i < len && bytes[i] == b'[' {
                // Bracket range: `[A1:B2]`.
                spans.push((i..i + 1, SyntaxHighlight { kind: COR_BRACKET }));
                i += 1;
                i = consume_cell_addr(bytes, i, &mut spans);
                if i < len && bytes[i] == b':' {
                    spans.push((i..i + 1, SyntaxHighlight { kind: COR_REF_COLON }));
                    i += 1;
                    i = consume_cell_addr(bytes, i, &mut spans);
                }
                if i < len && bytes[i] == b']' {
                    spans.push((i..i + 1, SyntaxHighlight { kind: COR_BRACKET }));
                    i += 1;
                }
            }
            continue;
        }

        // Numeric literal (integer or decimal, with optional leading `-`
        // in operator-valid position — keep it simple: only recognise as
        // a number when we're right after an operator or at the start of
        // whitespace, otherwise leave `-` to the operator scanner).
        if c.is_ascii_digit()
            || (c == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            spans.push((start..i, SyntaxHighlight { kind: COR_NUMBER }));
            continue;
        }

        // Identifier → keyword / builtin / type-annotation / user-rainbow.
        if is_ident_byte(c) && !c.is_ascii_digit() {
            let start = i;
            while i < len && is_ident_byte(bytes[i]) { i += 1; }
            let word = &line[start..i];
            if is_cordial_keyword(word) {
                spans.push((start..i, SyntaxHighlight { kind: COR_KEYWORD }));
            } else if is_cordial_builtin(word) {
                spans.push((start..i, SyntaxHighlight { kind: COR_BUILTIN_FN }));
            } else if is_cordial_type_annotation(word) && last_token_is_colon(&spans) {
                spans.push((start..i, SyntaxHighlight { kind: COR_TYPE_ANN }));
            } else if let Some(&slot) = user_idents.get(word) {
                spans.push((start..i, SyntaxHighlight { kind: USER_IDENT_BASE + slot }));
            }
            continue;
        }

        // `::` as a namespace separator outside of a ref (e.g. `use mod::item`).
        if c == b':' && i + 1 < len && bytes[i + 1] == b':' {
            spans.push((i..i + 2, SyntaxHighlight { kind: COR_COLON_COLON }));
            i += 2;
            continue;
        }

        // Plain `:` — likely a type annotation colon in `let x: T = …`.
        if c == b':' {
            spans.push((i..i + 1, SyntaxHighlight { kind: COR_REF_COLON }));
            i += 1;
            continue;
        }

        // Bracket / brace / paren — separate color from operators.
        if matches!(c, b'(' | b')' | b'{' | b'}' | b'[' | b']' | b',') {
            spans.push((i..i + 1, SyntaxHighlight { kind: COR_BRACKET }));
            i += 1;
            continue;
        }

        // Operator run: consume a contiguous block of operator bytes.
        if is_operator_byte(c) {
            let start = i;
            while i < len && is_operator_byte(bytes[i]) { i += 1; }
            spans.push((start..i, SyntaxHighlight { kind: COR_OPERATOR }));
            continue;
        }

        i += 1;
    }

    spans
}

fn consume_cell_addr(
    bytes: &[u8],
    start: usize,
    spans: &mut Vec<(Range<usize>, SyntaxHighlight)>,
) -> usize {
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() { i += 1; }
    let letters_end = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    // Only tag as a cell address when we matched BOTH letters AND digits —
    // otherwise we're looking at a bare identifier or a digit run that
    // some other branch should have handled.
    if i > start && letters_end > start && i > letters_end {
        spans.push((start..i, SyntaxHighlight { kind: COR_CELL_ADDR }));
    }
    i
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_operator_byte(b: u8) -> bool {
    matches!(b, b'+' | b'-' | b'*' | b'/' | b'%' | b'^'
        | b'=' | b'<' | b'>' | b'!' | b'~' | b'&' | b'|' | b'.')
}

fn is_cordial_keyword(w: &str) -> bool {
    matches!(w, "let" | "fn" | "if" | "else" | "while" | "for" | "in"
        | "return" | "use" | "is" | "true" | "false" | "and" | "or" | "not"
        // Function-inversion DSL — two forms:
        //   programmer:  let lfreq = solve!(l, f0)   // or `solve!(l from f0)`
        //   math:        let lfreq(freq, c) = l where f0(l, c) = freq
        | "solve" | "where" | "from")
}

fn is_cordial_builtin(w: &str) -> bool {
    matches!(w,
        // math
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
        | "sqrt" | "abs" | "floor" | "ceil" | "round" | "ln" | "log"
        // collections
        | "len" | "range" | "push"
        // aggregates
        | "sum" | "avg" | "min" | "max" | "count" | "std_devp" | "std_devs"
        // constants
        | "pi"
    )
}

fn is_cordial_type_annotation(w: &str) -> bool {
    matches!(w, "int" | "float" | "bool" | "str" | "number" | "array" | "vec")
}

/// Did the scanner just emit a `:` span? Used so a type name following a
/// `:` picks up the type-annotation color only in the `let x: T = …` shape,
/// never when it happens to sit elsewhere on the line.
fn last_token_is_colon(spans: &[(Range<usize>, SyntaxHighlight)]) -> bool {
    matches!(spans.last(), Some((_, h)) if h.kind == COR_REF_COLON)
}

fn heading_level(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || bytes[0] != b'#' { return None; }
    let mut level = 0;
    while level < bytes.len() && bytes[level] == b'#' { level += 1; }
    if level > 3 { return None; }
    if level < bytes.len() && bytes[level] == b' ' {
        Some(level)
    } else {
        None
    }
}

fn is_horizontal_rule(trimmed: &str) -> bool {
    if trimmed.len() < 3 { return false; }
    let first = trimmed.as_bytes()[0];
    if !matches!(first, b'-' | b'*' | b'_') { return false; }
    trimmed.bytes().all(|b| b == first || b == b' ')
}

#[derive(Clone, Copy, PartialEq)]
enum ListKind {
    Plain(usize),
    TaskOpen(usize),
    TaskDone(usize),
}

fn list_marker_info(trimmed: &str) -> Option<ListKind> {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() { return None; }

    if matches!(bytes[0], b'-' | b'*' | b'+') && bytes.get(1) == Some(&b' ') {
        if trimmed.starts_with("- [ ] ") {
            return Some(ListKind::TaskOpen(6));
        }
        if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            return Some(ListKind::TaskDone(6));
        }
        return Some(ListKind::Plain(2));
    }

    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    if i > 0 && i < bytes.len() && matches!(bytes[i], b'.' | b')') {
        if bytes.get(i + 1) == Some(&b' ') {
            return Some(ListKind::Plain(i + 2));
        }
    }
    None
}

fn parse_inline(text: &str, base: usize) -> Vec<(Range<usize>, SyntaxHighlight)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut spans = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'\\' && i + 1 < len && is_md_punctuation(bytes[i + 1]) {
            i += 2;
            continue;
        }

        // cosmic-text's partial reshape (called by iced's text_editor after
        // add_span) drops the new run's attrs on the FIRST glyph of the new
        // attribute run, so `*hello*` would render with "h" plain and "ello"
        // italic. Workaround: emit the bold/italic span covering the opening
        // marker bytes too — the marker becomes the "lost first glyph" and
        // the first letter of the inner text gets the style. The marker span
        // pushed first is overridden by the bold/italic span that follows
        // because cosmic-text uses the LAST add_span to win on overlap.
        // Markers (`*`, `**`, `***`) end up italic/bold themselves, which is
        // imperceptible at typical font sizes.

        if i + 2 < len && bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
            if let Some(end) = find_triple_star(bytes, i + 3) {
                spans.push((base + i..base + i + 3, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                spans.push((base + end..base + end + 3, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                if i + 3 < end {
                    spans.push((base + i..base + end + 3, SyntaxHighlight { kind: MD_BOLD_ITALIC }));
                }
                i = end + 3;
                continue;
            }
        }

        if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some(end) = find_closing(bytes, i + 2, b'*', b'*') {
                spans.push((base + i..base + i + 2, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                spans.push((base + end..base + end + 2, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                if i + 2 < end {
                    let inner = parse_inline(&text[i + 2..end], base + i + 2);
                    if inner.is_empty() {
                        spans.push((base + i..base + end + 2, SyntaxHighlight { kind: MD_BOLD }));
                    } else {
                        spans.push((base + i..base + i + 2, SyntaxHighlight { kind: MD_BOLD }));
                        for (r, h) in inner {
                            let kind = if h.kind == MD_ITALIC { MD_BOLD_ITALIC } else { h.kind };
                            spans.push((r, SyntaxHighlight { kind }));
                        }
                        // Extend bold over closing marker for visual consistency.
                        spans.push((base + end..base + end + 2, SyntaxHighlight { kind: MD_BOLD }));
                    }
                }
                i = end + 2;
                continue;
            }
        }

        if bytes[i] == b'*' && (i + 1 >= len || bytes[i + 1] != b'*') {
            if let Some(end) = find_single_closing(bytes, i + 1, b'*') {
                if end > i + 1 && bytes[end - 1] != b'*' {
                    spans.push((base + i..base + i + 1, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                    spans.push((base + end..base + end + 1, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                    if i + 1 < end {
                        spans.push((base + i..base + end + 1, SyntaxHighlight { kind: MD_ITALIC }));
                    }
                    i = end + 1;
                    continue;
                }
            }
        }

        if bytes[i] == b'`' {
            let tick_count = count_backticks(bytes, i);
            if let Some(end) = find_backtick_close(bytes, i + tick_count, tick_count) {
                spans.push((base + i..base + i + tick_count, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                if i + tick_count < end {
                    spans.push((base + i + tick_count..base + end, SyntaxHighlight { kind: MD_INLINE_CODE }));
                }
                spans.push((base + end..base + end + tick_count, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                i = end + tick_count;
                continue;
            }
        }

        if bytes[i] == b'[' {
            if let Some((text_end, url_end)) = find_link(bytes, i) {
                spans.push((base + i..base + i + 1, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                if i + 1 < text_end {
                    spans.push((base + i + 1..base + text_end, SyntaxHighlight { kind: MD_LINK_TEXT }));
                }
                spans.push((base + text_end..base + text_end + 2, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                if text_end + 2 < url_end {
                    spans.push((base + text_end + 2..base + url_end, SyntaxHighlight { kind: MD_LINK_URL }));
                }
                spans.push((base + url_end..base + url_end + 1, SyntaxHighlight { kind: MD_FORMAT_MARKER }));
                i = url_end + 1;
                continue;
            }
        }

        i += 1;
    }

    spans
}

fn is_md_punctuation(b: u8) -> bool {
    matches!(b, b'\\' | b'`' | b'*' | b'_' | b'{' | b'}' | b'[' | b']'
        | b'(' | b')' | b'#' | b'+' | b'-' | b'.' | b'!' | b'|')
}

fn find_triple_star(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 2 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn count_backticks(bytes: &[u8], start: usize) -> usize {
    let mut n = 0;
    while start + n < bytes.len() && bytes[start + n] == b'`' { n += 1; }
    n
}

fn find_backtick_close(bytes: &[u8], start: usize, count: usize) -> Option<usize> {
    if count == 0 { return None; }
    let mut i = start;
    while i + count <= bytes.len() {
        if count_backticks(bytes, i) == count {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_closing(bytes: &[u8], start: usize, c1: u8, c2: u8) -> Option<usize> {
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == c1 && bytes[i + 1] == c2 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_single_closing(bytes: &[u8], start: usize, ch: u8) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == ch {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_link(bytes: &[u8], open: usize) -> Option<(usize, usize)> {
    let mut i = open + 1;
    while i < bytes.len() {
        if bytes[i] == b']' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'(' {
                let text_end = i;
                let mut j = i + 2;
                while j < bytes.len() {
                    if bytes[j] == b')' {
                        return Some((text_end, j));
                    }
                    j += 1;
                }
            }
            return None;
        }
        if bytes[i] == b'\n' { return None; }
        i += 1;
    }
    None
}

impl highlighter::Highlighter for SyntaxHighlighter {
    type Settings = SyntaxSettings;
    type Highlight = SyntaxHighlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, SyntaxHighlight)>;

    fn new(settings: &Self::Settings) -> Self {
        let mut h = SyntaxHighlighter {
            lang: settings.lang.clone(),
            spans: Vec::new(),
            line_offsets: Vec::new(),
            line_kinds: Vec::new(),
            in_fenced_code: false,
            current_line: 0,
            line_decors: Vec::new(),
            user_idents: HashMap::new(),
            code_block_spans: HashMap::new(),
        };
        h.rebuild(&settings.source);
        h
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.lang = new_settings.lang.clone();
        self.rebuild(&new_settings.source);
    }

    fn change_line(&mut self, line: usize) {
        self.current_line = self.current_line.min(line);
        if line == 0 {
            self.in_fenced_code = false;
        }
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let ln = self.current_line;
        self.current_line += 1;

        let trimmed = line.trim_start();
        if trimmed.starts_with(RESULT_PREFIX) {
            return vec![(0..line.len(), SyntaxHighlight { kind: EVAL_RESULT_KIND })].into_iter();
        }
        if trimmed.starts_with(ERROR_PREFIX) {
            return vec![(0..line.len(), SyntaxHighlight { kind: EVAL_ERROR_KIND })].into_iter();
        }

        let is_markdown = ln < self.line_kinds.len()
            && self.line_kinds[ln] == LineKind::Markdown;

        if is_markdown {
            if trimmed.starts_with("```") {
                self.in_fenced_code = !self.in_fenced_code;
                return vec![(0..line.len(), SyntaxHighlight { kind: MD_FENCE_MARKER })].into_iter();
            }

            if self.in_fenced_code {
                if let Some(spans) = self.code_block_spans.get(&ln) {
                    return spans.clone().into_iter();
                }
                return vec![(0..line.len(), SyntaxHighlight { kind: MD_CODE_BLOCK })].into_iter();
            }

            // Markdown lines always return md_spans, even when empty —
            // falling through to the code path would let plain prose pick up
            // Rust keyword highlighting on words like "let", "type", "return".
            return self.highlight_markdown(line).into_iter();
        } else if self.in_fenced_code {
            self.in_fenced_code = false;
        }

        // Non-markdown lines are Cordial / Eval / Comment — hand-rolled
        // Cordial scanner, not the generic tree-sitter path (which uses
        // the configured `lang`, wrong for Cordial). Each token gets its
        // own color: `/=`, `@`, `::`, table / block names, cell addresses,
        // keywords, builtins, numbers, strings, comments.
        if ln < self.line_kinds.len()
            && matches!(self.line_kinds[ln], LineKind::Cordial | LineKind::Eval | LineKind::Comment)
        {
            return highlight_cordial(line, &self.user_idents).into_iter();
        }

        if ln >= self.line_offsets.len() {
            return Vec::new().into_iter();
        }

        let line_start = self.line_offsets[ln];
        let line_end = if ln + 1 < self.line_offsets.len() {
            self.line_offsets[ln + 1] - 1
        } else {
            line_start + line.len()
        };

        let mut result = Vec::new();
        for span in &self.spans {
            if span.end <= line_start || span.start >= line_end {
                continue;
            }
            let start = span.start.max(line_start) - line_start;
            let end = span.end.min(line_end) - line_start;
            if start < end {
                result.push((start..end, SyntaxHighlight { kind: span.kind }));
            }
        }
        result.into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

pub fn highlight_color(kind: u8) -> Color {
    let p = palette::current();
    if kind >= USER_IDENT_BASE && kind < USER_IDENT_BASE + USER_IDENT_PALETTE_SIZE {
        return match kind - USER_IDENT_BASE {
            0 => p.red,
            1 => p.green,
            2 => p.peach,
            3 => p.blue,
            4 => p.mauve,
            5 => p.teal,
            6 => p.yellow,
            7 => p.pink,
            _ => p.text,
        };
    }
    match kind {
        0  => p.mauve,
        1  => p.blue,
        2  => p.teal,
        3  => p.yellow,
        4  => p.yellow,
        5  => p.teal,
        6  => p.peach,
        7  => p.peach,
        8  => p.green,
        9  => p.peach,
        10 => p.overlay0,
        11 => p.text,
        12 => p.red,
        13 => p.flamingo,
        14 => p.sky,
        15 => p.overlay2,
        16 => p.overlay2,
        17 => p.overlay2,
        18 => p.blue,
        19 => p.mauve,
        20 => p.yellow,
        21 => p.teal,
        22 => p.red,
        23 => p.text,
        24 => p.green,
        25 => p.maroon,
        COR_EVAL_SIGIL => p.teal,
        COR_AT_SIGIL => p.mauve,
        COR_COLON_COLON => p.flamingo,
        COR_REF_COLON => p.flamingo,
        COR_TABLE_NAME => p.blue,
        COR_BLOCK_NAME => p.lavender,
        COR_CELL_ADDR => p.yellow,
        COR_KEYWORD => p.mauve,
        COR_BUILTIN_FN => p.sky,
        COR_NUMBER => p.peach,
        COR_STRING => p.green,
        COR_COMMENT => p.overlay1,
        COR_OPERATOR => p.overlay2,
        COR_BRACKET => p.overlay2,
        COR_TYPE_ANN => p.yellow,
        MD_HEADING_MARKER => p.overlay0,
        MD_H1 => p.rosewater,
        MD_H2 => p.peach,
        MD_H3 => p.yellow,
        MD_BOLD => p.text,
        MD_ITALIC => p.text,
        MD_INLINE_CODE => p.green,
        MD_FORMAT_MARKER => p.overlay0,
        MD_LINK_TEXT => p.blue,
        MD_LINK_URL => p.overlay1,
        MD_BLOCKQUOTE_MARKER => p.overlay0,
        MD_BLOCKQUOTE => p.sky,
        MD_LIST_MARKER => p.sky,
        MD_FENCE_MARKER => p.overlay0,
        MD_CODE_BLOCK => p.text,
        MD_HR => p.overlay1,
        MD_TASK_OPEN => p.overlay2,
        MD_TASK_DONE => p.green,
        MD_BOLD_ITALIC => p.text,
        _  => p.text,
    }
}

pub fn highlight_font(kind: u8) -> Option<Font> {
    // Spans inherit the named family from EDITOR_FONT so fontdb can pick up
    // the real Bold, Italic and BoldItalic faces of the system monospace.
    match kind {
        MD_HEADING_MARKER => Some(Font { weight: Weight::Bold, ..EDITOR_FONT }),
        MD_H1 => Some(Font { weight: Weight::Black, ..EDITOR_FONT }),
        MD_H2 => Some(Font { weight: Weight::Bold, ..EDITOR_FONT }),
        MD_H3 => Some(Font { weight: Weight::Semibold, ..EDITOR_FONT }),
        MD_BOLD => Some(Font { weight: Weight::Bold, ..EDITOR_FONT }),
        MD_ITALIC => Some(Font { style: FontStyle::Italic, ..EDITOR_FONT }),
        MD_BOLD_ITALIC => Some(Font { weight: Weight::Bold, style: FontStyle::Italic, ..EDITOR_FONT }),
        MD_INLINE_CODE => Some(EDITOR_FONT),
        MD_FORMAT_MARKER => Some(EDITOR_FONT),
        MD_BLOCKQUOTE => Some(Font { style: FontStyle::Italic, ..EDITOR_FONT }),
        MD_FENCE_MARKER => Some(EDITOR_FONT),
        MD_CODE_BLOCK => Some(EDITOR_FONT),
        MD_TASK_DONE => Some(Font { weight: Weight::Bold, ..EDITOR_FONT }),
        _ => None,
    }
}

/// maps a fenced-code label to a tree-sitter language id, recursing on the trailing extension for dotted labels.
fn canonical_code_lang(label: &str) -> Option<String> {
    let label = label.trim().to_ascii_lowercase();
    if label.is_empty() {
        return None;
    }
    let direct = match label.as_str() {
        "rust" | "rs" => Some("rust"),
        "py" | "python" => Some("python"),
        "js" | "javascript" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "typescript" => Some("typescript"),
        "tsx" => Some("tsx"),
        "jsx" => Some("javascript"),
        "c" | "h" => Some("c"),
        "cpp" | "c++" | "cc" | "cxx" | "hpp" => Some("cpp"),
        "go" => Some("go"),
        "rb" | "ruby" => Some("ruby"),
        "sh" | "bash" | "shell" | "zsh" => Some("bash"),
        "java" => Some("java"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "scss" => Some("scss"),
        "json" => Some("json"),
        "lua" => Some("lua"),
        "php" => Some("php"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "swift" => Some("swift"),
        "zig" => Some("zig"),
        "sql" => Some("sql"),
        "make" | "makefile" => Some("make"),
        "md" | "markdown" => Some("markdown"),
        _ => None,
    };
    if let Some(d) = direct {
        return Some(d.to_string());
    }
    if let Some(idx) = label.rfind('.') {
        return canonical_code_lang(&label[idx + 1..]);
    }
    None
}

pub fn compute_line_decors(source: &str) -> Vec<LineDecor> {
    let classified = classify_document(source);
    let line_kinds: Vec<LineKind> = classified.into_iter().map(|cl| cl.kind).collect();
    let mut decors = Vec::new();
    let mut in_fence = false;
    for (i, raw_line) in source.split('\n').enumerate() {
        let is_md = i < line_kinds.len() && line_kinds[i] == LineKind::Markdown;
        if is_md {
            let trimmed = raw_line.trim_start();
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
                decors.push(LineDecor::FenceMarker);
            } else if in_fence {
                decors.push(LineDecor::CodeBlock);
            } else if is_horizontal_rule(trimmed) {
                decors.push(LineDecor::HorizontalRule);
            } else if trimmed.starts_with("> ") || trimmed == ">" {
                decors.push(LineDecor::Blockquote);
            } else {
                decors.push(LineDecor::None);
            }
        } else {
            if in_fence { in_fence = false; }
            decors.push(LineDecor::None);
        }
    }
    decors
}

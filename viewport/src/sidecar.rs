//! Embedded sidecar archive.
//!
//! Markdown is the floor — `.md` files stay readable in vim and on GitHub —
//! but Numbers-class table features (positional metadata, per-cell formatting,
//! formulas, references) don't fit in markdown.
//!
//! Acord stores rich metadata in the SAME `.md` file as a base64-encoded zip
//! wrapped in an HTML comment appended to the end of the document:
//!
//! ```text
//! ...the user's markdown content...
//!
//! <!-- acord-archive
//! UEsDBBQAAAAIA...base64...AAAA
//! -->
//! ```
//!
//! Why this shape:
//! - HTML comments are valid markdown — every renderer (GitHub, Bear, Obsidian)
//!   treats them as invisible. Vim shows them as a single comment block, not
//!   as binary garbage.
//! - Base64 stays text-clean — no `\0` bytes, vim won't flag the file as
//!   binary, `git diff` is still legible (modulo a wide line at the bottom).
//! - The zip's central directory makes it trivial to add more entries later
//!   (per-block scratch state, formula caches, embedded images) without
//!   changing the framing.
//!
//! Per-table linking is positional: the Nth non-eval table in document layout
//! order is sidecar key "N". No proprietary tags appear in the markdown body.
//! Identity is runtime state derived from the document, never written to disk.
//!
//! The archive is structured like a Rust crate — each block is a submodule
//! file under `src/`, and `config.toml` holds display-only metadata (col
//! widths, row heights, cell styles). Save direction only: the markdown is
//! always the source of truth; the archive is regenerated fresh on every save.
//! On load, only `config.toml` is read for display metadata. If missing or
//! malformed, start fresh — next save overwrites.
//!
//! Eval result tables are explicitly NOT persisted. Only the source `/= expr`
//! line goes into markdown; the result table re-renders fresh on load.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Sentinel that opens the embedded archive comment. Anything from this string
/// to the matching `-->` is the archive payload (base64-encoded zip).
const ARCHIVE_OPEN: &str = "<!-- acord-archive";
const ARCHIVE_CLOSE: &str = "-->";

/// Root-level display metadata file inside the zip. Holds col widths, row
/// heights, cell styles, formulas — things that don't affect evaluation.
const CONFIG_ENTRY: &str = "config.toml";
/// Directory inside the zip holding one `.cord` file per block. Each file
/// contains TOML front-matter + source, structured like a crate submodule.
const SRC_DIR: &str = "src/";

/// Top-level schema of a `<file>.acord.toml` companion. Versioned so we can
/// migrate later as the Numbers-class table feature set grows.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Sidecar {
    /// Schema version. Bump on incompatible changes.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Table metadata indexed by `[#id]` markers in the markdown.
    #[serde(default)]
    pub tables: HashMap<String, TableSidecar>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableSidecar {
    /// Per-column widths in pixels. Same length as the table's column count
    /// (or shorter; missing entries fall back to the editor's default width).
    #[serde(default)]
    pub col_widths: Vec<f32>,
    /// Sparse per-row explicit heights. Keys are row indices serialized as
    /// strings (TOML's native key type); convert with `parse::<usize>()` at
    /// the boundary. A table with a few resized rows doesn't carry the
    /// default for every other row.
    #[serde(default)]
    pub row_heights: HashMap<String, f32>,
    /// Per-cell metadata indexed by spreadsheet-style address ("A1", "D2", ...).
    #[serde(default)]
    pub cells: HashMap<String, CellSidecar>,
    /// Cell formulas indexed by spreadsheet address.
    #[serde(default)]
    pub formulas: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CellSidecar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
}

/// Reads sidecar TOML. Returns `Default` on parse error so a corrupt sidecar
/// never blocks opening a markdown file — the user just loses the rich metadata
/// until they re-save.
pub struct SidecarReader {
    inner: Sidecar,
}

impl SidecarReader {
    pub fn from_toml(text: &str) -> Self {
        let inner: Sidecar = toml::from_str(text).unwrap_or_default();
        Self { inner }
    }

    pub fn empty() -> Self {
        Self { inner: Sidecar::default() }
    }

    pub fn table(&self, id: &str) -> Option<&TableSidecar> {
        self.inner.tables.get(id)
    }
}

/// Accumulates sidecar entries during a save pass. Each block's `to_md` writes
/// its side-channel state into the writer; after the pass, `flush` produces the
/// TOML text to write to disk (or `None` if there's nothing to write — empty
/// sidecars should be deleted from disk to avoid littering).
pub struct SidecarWriter {
    inner: Sidecar,
}

impl SidecarWriter {
    pub fn new() -> Self {
        Self {
            inner: Sidecar {
                version: 1,
                tables: HashMap::new(),
            },
        }
    }

    pub fn put_table(&mut self, id: String, data: TableSidecar) {
        self.inner.tables.insert(id, data);
    }

    /// Returns the serialized TOML, or `None` if the sidecar has no entries.
    pub fn flush(self) -> Option<String> {
        if self.inner.tables.is_empty() {
            return None;
        }
        toml::to_string_pretty(&self.inner).ok()
    }
}

impl Default for SidecarWriter {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------------------------------------
// Embedded archive: split markdown text into (body, optional sidecar)
// ----------------------------------------------------------------------------

/// Result of pulling an archive out of an `.md` file. `markdown` is the user
/// content with the archive comment stripped; `sidecar` is the parsed config
/// (or `None` if the file had no archive).
pub struct LoadedDoc {
    pub markdown: String,
    pub sidecar: Option<Sidecar>,
}

/// Pull an embedded archive out of a markdown file. If the file has no
/// `<!-- acord-archive ... -->` comment, returns the text unchanged with
/// `sidecar = None`. Failure modes (truncated comment, bad base64, malformed
/// zip, malformed TOML) all degrade gracefully to "no sidecar" — the user
/// never loses access to their markdown content because of corrupted metadata.
pub fn extract_archive(text: &str) -> LoadedDoc {
    let Some(open_idx) = text.rfind(ARCHIVE_OPEN) else {
        return LoadedDoc {
            markdown: text.to_string(),
            sidecar: None,
        };
    };
    // The closing `-->` must come AFTER the opener.
    let after_open = open_idx + ARCHIVE_OPEN.len();
    let Some(rel_close) = text[after_open..].find(ARCHIVE_CLOSE) else {
        return LoadedDoc {
            markdown: text.to_string(),
            sidecar: None,
        };
    };
    let close_idx = after_open + rel_close;
    let payload = text[after_open..close_idx].trim();

    let body = strip_trailing_blank_lines(text[..open_idx].trim_end_matches('\n'));

    let parsed = decode_archive_payload(payload);
    LoadedDoc {
        markdown: body,
        sidecar: parsed,
    }
}

/// A single block's source file for the archive. Written to `src/<filename>`
/// inside the zip. Content is TOML front-matter + `---` separator + raw source.
pub struct BlockFile {
    pub filename: String,
    pub content: String,
}

/// builds archive zip bytes from sidecar metadata and block files, None when both empty.
pub fn build_archive_bytes(sidecar: &Sidecar, block_files: &[BlockFile]) -> Option<Vec<u8>> {
    if sidecar.tables.is_empty() && block_files.is_empty() {
        return None;
    }
    let toml_text = toml::to_string_pretty(sidecar).ok()?;
    write_zip(&toml_text, block_files).ok()
}

/// parses zip bytes back into a Sidecar.
pub fn extract_archive_bytes(bytes: &[u8]) -> Option<Sidecar> {
    let toml_text = read_zip(bytes)?;
    toml::from_str::<Sidecar>(&toml_text).ok()
}

/// magic separating the markdown body from the appended raw zip; the surrounding NULs
/// trip text editors into "binary mode" so the archive shows up as garbage, not as
/// readable base64.
pub const BINARY_SENTINEL: &[u8] = b"\n\x00ACORD-ARCHIVE\x00\n";

/// appends raw zip bytes after the markdown body, separated by BINARY_SENTINEL.
pub fn embed_in_md(markdown: &[u8], archive: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(markdown.len() + BINARY_SENTINEL.len() + archive.len());
    out.extend_from_slice(markdown);
    if !markdown.ends_with(b"\n") {
        out.push(b'\n');
    }
    out.extend_from_slice(BINARY_SENTINEL);
    out.extend_from_slice(archive);
    out
}

/// splits raw file bytes on BINARY_SENTINEL, returning (text_bytes, optional zip bytes).
pub fn extract_from_md(bytes: &[u8]) -> (Vec<u8>, Option<Vec<u8>>) {
    if let Some(idx) = rfind_subslice(bytes, BINARY_SENTINEL) {
        let text = bytes[..idx].to_vec();
        let archive_start = idx + BINARY_SENTINEL.len();
        let archive = bytes[archive_start..].to_vec();
        return (text, Some(archive));
    }
    (bytes.to_vec(), None)
}

fn rfind_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// legacy embed format: base64-encoded zip inside an HTML comment. Kept for the
/// round-trip tests and the load-side back-compat path; new saves go through embed_in_md.
pub fn embed_archive(markdown: &str, sidecar: &Sidecar, block_files: &[BlockFile]) -> String {
    let Some(zip_bytes) = build_archive_bytes(sidecar, block_files) else {
        return markdown.to_string();
    };
    let encoded = B64.encode(&zip_bytes);
    let wrapped = wrap_base64(&encoded, 76);

    let mut out = markdown.trim_end_matches('\n').to_string();
    out.push_str("\n\n");
    out.push_str(ARCHIVE_OPEN);
    out.push('\n');
    out.push_str(&wrapped);
    out.push('\n');
    out.push_str(ARCHIVE_CLOSE);
    out.push('\n');
    out
}

fn strip_trailing_blank_lines(s: &str) -> String {
    // Walk back over consecutive trailing newlines / whitespace lines so that
    // round-tripping a doc with an archive doesn't accumulate blank lines.
    let mut end = s.len();
    let bytes = s.as_bytes();
    while end > 0 {
        let line_end = end;
        let mut line_start = end;
        while line_start > 0 && bytes[line_start - 1] != b'\n' {
            line_start -= 1;
        }
        let line = &s[line_start..line_end];
        if line.trim().is_empty() {
            end = if line_start == 0 { 0 } else { line_start - 1 };
        } else {
            break;
        }
    }
    s[..end].to_string()
}

fn decode_archive_payload(payload: &str) -> Option<Sidecar> {
    // Strip whitespace inside the comment so the wrapping is invisible to the
    // decoder.
    let cleaned: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    let zip_bytes = B64.decode(cleaned.as_bytes()).ok()?;
    let toml_text = read_zip(&zip_bytes)?;
    toml::from_str::<Sidecar>(&toml_text).ok()
}

fn write_zip(toml_text: &str, block_files: &[BlockFile]) -> Result<Vec<u8>, String> {
    let total_bytes = toml_text.len()
        + block_files.iter().map(|f| f.filename.len() + f.content.len()).sum::<usize>();
    let mut buf: Vec<u8> = Vec::with_capacity(total_bytes + 512);
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts: SimpleFileOptions =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        if !toml_text.is_empty() {
            zip.start_file(CONFIG_ENTRY, opts)
                .map_err(|e| format!("zip start_file config: {}", e))?;
            zip.write_all(toml_text.as_bytes())
                .map_err(|e| format!("zip write config: {}", e))?;
        }

        for file in block_files {
            let path = format!("{}{}", SRC_DIR, file.filename);
            zip.start_file(path, opts)
                .map_err(|e| format!("zip start_file {}: {}", file.filename, e))?;
            zip.write_all(file.content.as_bytes())
                .map_err(|e| format!("zip write {}: {}", file.filename, e))?;
        }

        zip.finish()
            .map_err(|e| format!("zip finish: {}", e))?;
    }
    Ok(buf)
}

fn read_zip(bytes: &[u8]) -> Option<String> {
    let cursor = Cursor::new(bytes);
    let mut zip = ZipArchive::new(cursor).ok()?;
    let mut entry = zip.by_name(CONFIG_ENTRY).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    Some(text)
}

fn wrap_base64(s: &str, width: usize) -> String {
    if width == 0 || s.len() <= width {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + s.len() / width);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let end = (i + width).min(bytes.len());
        // Base64 is ASCII, slicing by byte == slicing by char.
        out.push_str(&s[i..end]);
        if end < bytes.len() {
            out.push('\n');
        }
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sidecar() -> Sidecar {
        let mut tables = HashMap::new();
        tables.insert(
            "t1".to_string(),
            TableSidecar {
                col_widths: vec![100.0, 200.0, 150.0],
                row_heights: HashMap::new(),
                cells: HashMap::new(),
                formulas: HashMap::new(),
            },
        );
        Sidecar { version: 1, tables }
    }

    #[test]
    fn round_trip_embed_extract() {
        let body = "# Hello\n\nSome text.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let sidecar = sample_sidecar();
        let with_archive = embed_archive(body, &sidecar, &[]);
        assert!(with_archive.contains(ARCHIVE_OPEN));
        assert!(with_archive.contains(ARCHIVE_CLOSE));

        let loaded = extract_archive(&with_archive);
        assert_eq!(loaded.markdown.trim_end(), body.trim_end());
        let parsed = loaded.sidecar.expect("sidecar should round-trip");
        assert_eq!(parsed.tables.len(), 1);
        let t1 = &parsed.tables["t1"];
        assert_eq!(t1.col_widths, vec![100.0, 200.0, 150.0]);
    }

    #[test]
    fn empty_sidecar_skips_embed() {
        let body = "Just some markdown.\n";
        let empty = Sidecar::default();
        let out = embed_archive(body, &empty, &[]);
        assert_eq!(out, body);
        assert!(!out.contains("acord-archive"));
    }

    #[test]
    fn extract_with_no_archive() {
        let body = "# Plain doc\n\nNo archive here.";
        let loaded = extract_archive(body);
        assert_eq!(loaded.markdown, body);
        assert!(loaded.sidecar.is_none());
    }

    #[test]
    fn extract_with_corrupt_payload_recovers_markdown() {
        // Garbage in the comment body must NOT eat the user's markdown — they
        // get the body back, sidecar None.
        let doc = "# Body\n\nstuff\n\n<!-- acord-archive\nnot-actually-base64!!!\n-->\n";
        let loaded = extract_archive(doc);
        assert!(loaded.markdown.contains("# Body"));
        assert!(loaded.markdown.contains("stuff"));
        assert!(loaded.sidecar.is_none());
    }

    #[test]
    fn round_trip_preserves_complex_metadata() {
        let mut tables = HashMap::new();
        let mut cells = HashMap::new();
        cells.insert(
            "A1".to_string(),
            CellSidecar {
                background: Some("#ff0000".into()),
                foreground: Some("#ffffff".into()),
                font_weight: Some("bold".into()),
                align: Some("center".into()),
            },
        );
        let mut row_heights = HashMap::new();
        row_heights.insert("2".to_string(), 48.0);
        let mut formulas = HashMap::new();
        formulas.insert("B3".to_string(), "=SUM(A1:A10)".to_string());
        tables.insert(
            "t1".to_string(),
            TableSidecar {
                col_widths: vec![80.0, 120.0],
                row_heights,
                cells,
                formulas,
            },
        );
        let sc = Sidecar { version: 1, tables };

        let body = "# Doc\n";
        let embedded = embed_archive(body, &sc, &[]);
        let loaded = extract_archive(&embedded);
        let parsed = loaded.sidecar.unwrap();

        let t = &parsed.tables["t1"];
        assert_eq!(t.col_widths, vec![80.0, 120.0]);
        assert_eq!(t.row_heights["2"], 48.0);
        assert_eq!(t.cells["A1"].background.as_deref(), Some("#ff0000"));
        assert_eq!(t.formulas["B3"], "=SUM(A1:A10)");
    }

    #[test]
    fn embed_does_not_double_blank_line() {
        // Body that already ends with newlines should round-trip cleanly.
        let body = "Line\n\n\n";
        let sc = sample_sidecar();
        let embedded = embed_archive(body, &sc, &[]);
        let loaded = extract_archive(&embedded);
        // Trailing blank lines around the archive should not accumulate.
        assert_eq!(loaded.markdown.trim_end(), "Line");
    }
}

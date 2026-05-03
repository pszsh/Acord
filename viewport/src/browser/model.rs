use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::preview::{highlight_preview, PreviewLine};

const SUPPORTED_EXTS: &[&str] = &["md", "txt", "markdown", "mdown"];
const PREVIEW_LINES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserItemKind {
    File,
    Folder,
}

#[derive(Debug, Clone)]
pub struct BrowserItem {
    pub path: PathBuf,
    pub name: String,
    pub kind: BrowserItemKind,
    pub modified: SystemTime,
    pub preview: String,
    pub preview_lines: Vec<PreviewLine>,
}

/// Folders first, then files; both in date-modified descending order.
pub fn scan_directory(dir: &Path) -> Vec<BrowserItem> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };

    let mut folders: Vec<BrowserItem> = Vec::new();
    let mut files: Vec<BrowserItem> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') { continue; }

        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        if meta.is_dir() {
            folders.push(BrowserItem {
                path: path.clone(),
                name,
                kind: BrowserItemKind::Folder,
                modified,
                preview: folder_summary(&path),
                preview_lines: Vec::new(),
            });
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if !SUPPORTED_EXTS.iter().any(|e| *e == ext) { continue; }

            let display = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .unwrap_or(name);

            let preview = file_preview(&path);
            let preview_lines = highlight_preview(&preview);

            files.push(BrowserItem {
                path: path.clone(),
                name: display,
                kind: BrowserItemKind::File,
                modified,
                preview,
                preview_lines,
            });
        }
    }

    folders.sort_by(|a, b| b.modified.cmp(&a.modified));
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    folders.extend(files);
    folders
}

pub fn file_preview(path: &Path) -> String {
    // bytes-first so the binary archive trailer doesn't trip the utf-8 decode.
    let Ok(bytes) = std::fs::read(path) else { return String::new() };
    let (text_bytes, _) = crate::sidecar::extract_from_md(&bytes);
    let text = String::from_utf8_lossy(&text_bytes);
    let body = strip_sidecar_archive(&text);
    if body_looks_blank(body) {
        return "(empty note)".to_string();
    }
    body.lines().take(PREVIEW_LINES).collect::<Vec<_>>().join("\n")
}

pub fn folder_summary(dir: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(dir) else { return "Empty".to_string() };
    let mut files = 0usize;
    let mut folders = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s.starts_with('.') { continue; }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            folders += 1;
        } else {
            let ext = entry.path()
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if SUPPORTED_EXTS.iter().any(|e| *e == ext) { files += 1; }
        }
    }
    let mut parts: Vec<String> = Vec::new();
    if files > 0 {
        parts.push(format!("{} file{}", files, if files == 1 { "" } else { "s" }));
    }
    if folders > 0 {
        parts.push(format!("{} folder{}", folders, if folders == 1 { "" } else { "s" }));
    }
    if parts.is_empty() { "Empty".to_string() } else { parts.join(", ") }
}

/// Cuts the file at the start of the embedded base64 archive comment.
fn strip_sidecar_archive(text: &str) -> &str {
    match text.find("<!-- acord-archive") {
        Some(idx) => &text[..idx],
        None => text,
    }
}

/// True when the body has nothing but whitespace, separator rows, or default-header tables.
fn body_looks_blank(body: &str) -> bool {
    let trimmed = body.trim();
    if trimmed.is_empty() { return true; }
    for raw in trimmed.lines() {
        let t = raw.trim();
        if t.is_empty() { continue; }
        if !t.starts_with('|') { return false; }
        let cells: Vec<&str> = t
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        let separator = !cells.is_empty()
            && cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'));
        if separator { continue; }
        let default_header = cells.iter().enumerate().all(|(i, c)| *c == format!("Header {}", i + 1));
        if cells.iter().all(|c| c.is_empty()) || default_header { continue; }
        return false;
    }
    true
}

pub fn rename(item_path: &Path, new_name: &str, is_file: bool) -> std::io::Result<PathBuf> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty name"));
    }
    let parent = item_path.parent().unwrap_or_else(|| Path::new(""));
    let dest = if is_file {
        let ext = item_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext.is_empty() {
            parent.join(trimmed)
        } else {
            parent.join(format!("{}.{}", trimmed, ext))
        }
    } else {
        parent.join(trimmed)
    };
    if dest.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "destination exists"));
    }
    std::fs::rename(item_path, &dest)?;
    Ok(dest)
}

/// Copies the file to a sibling with a `name N.ext` suffix, picking the lowest free N.
pub fn duplicate(item_path: &Path) -> std::io::Result<PathBuf> {
    let parent = item_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = item_path.file_stem().and_then(|s| s.to_str()).unwrap_or("copy");
    let ext = item_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut n = 1usize;
    let dest = loop {
        let candidate = if ext.is_empty() {
            parent.join(format!("{} {}", stem, n))
        } else {
            parent.join(format!("{} {}.{}", stem, n, ext))
        };
        if !candidate.exists() { break candidate; }
        n += 1;
    };
    std::fs::copy(item_path, &dest)?;
    Ok(dest)
}

pub fn move_into(item_path: &Path, folder: &Path) -> std::io::Result<PathBuf> {
    let name = item_path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name")
    })?;
    let dest = folder.join(name);
    if dest.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "destination exists"));
    }
    std::fs::rename(item_path, &dest)?;
    Ok(dest)
}

/// Bumps each path's mtime so it sorts immediately above `anchor` in date-descending order.
/// Items keep their relative order: the first path in `items` lands closest above the anchor.
pub fn reorder_before(items: &[PathBuf], anchor: &Path) -> std::io::Result<()> {
    let anchor_meta = std::fs::metadata(anchor)?;
    let anchor_mtime = anchor_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    // Spread the dragged items across one second of mtime above the anchor.
    // The earliest in `items` gets the highest mtime so it sorts first under
    // descending order — matches the user's drag-order expectation.
    let n = items.len();
    if n == 0 { return Ok(()); }
    let step_ms: u64 = (1000 / n.max(1) as u64).max(1);
    for (i, path) in items.iter().enumerate() {
        let offset_ms = (n - i) as u64 * step_ms;
        let new_time = anchor_mtime + std::time::Duration::from_millis(offset_ms);
        let ft = filetime::FileTime::from_system_time(new_time);
        let _ = filetime::set_file_mtime(path, ft);
    }
    Ok(())
}

/// Bumps each path's mtime above every existing item in `parent`, preserving drag order.
/// Used when dropping items at the very top of the grid.
pub fn reorder_to_top(items: &[PathBuf], parent: &Path) -> std::io::Result<()> {
    let mut max_mtime = SystemTime::UNIX_EPOCH;
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(t) = meta.modified() {
                    if t > max_mtime { max_mtime = t; }
                }
            }
        }
    }
    let base = max_mtime.max(SystemTime::now() - std::time::Duration::from_secs(1));
    let n = items.len();
    if n == 0 { return Ok(()); }
    for (i, path) in items.iter().enumerate() {
        let new_time = base + std::time::Duration::from_millis((n - i) as u64 * 10 + 100);
        let ft = filetime::FileTime::from_system_time(new_time);
        let _ = filetime::set_file_mtime(path, ft);
    }
    Ok(())
}

pub fn create_folder(parent: &Path) -> std::io::Result<PathBuf> {
    let mut name = "New Folder".to_string();
    let mut n = 1usize;
    while parent.join(&name).exists() {
        n += 1;
        name = format!("New Folder {}", n);
    }
    let dest = parent.join(name);
    std::fs::create_dir(&dest)?;
    Ok(dest)
}

/// Creates a fresh folder next to `items` and moves each one inside.
/// Items already living in the destination are skipped to avoid same-name self-moves.
pub fn create_folder_with_items(parent: &Path, items: &[PathBuf]) -> std::io::Result<PathBuf> {
    let folder = create_folder(parent)?;
    for item in items {
        if item.parent() == Some(folder.as_path()) { continue; }
        let _ = move_into(item, &folder);
    }
    Ok(folder)
}

/// Sends the path to the OS trash; falls back to permanent delete on platforms without trash support.
pub fn trash(item_path: &Path) -> std::io::Result<()> {
    match trash_crate_remove(item_path) {
        Ok(()) => Ok(()),
        Err(_) => {
            if item_path.is_dir() {
                std::fs::remove_dir_all(item_path)
            } else {
                std::fs::remove_file(item_path)
            }
        }
    }
}

fn trash_crate_remove(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    trash::delete(path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

pub fn path_segments(current: &Path, root: &Path) -> Vec<(String, PathBuf)> {
    let mut segments: Vec<(String, PathBuf)> = Vec::new();
    let mut p = current.to_path_buf();
    while p != root && p.starts_with(root) {
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        segments.insert(0, (name, p.clone()));
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    segments.insert(0, ("Documents".to_string(), root.to_path_buf()));
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_body_detection() {
        assert!(body_looks_blank(""));
        assert!(body_looks_blank("   \n\n  "));
        assert!(body_looks_blank("| | |\n| - | - |\n| | |"));
        assert!(body_looks_blank("| Header 1 | Header 2 |\n| -------- | -------- |"));
        assert!(!body_looks_blank("hello"));
        assert!(!body_looks_blank("| a | b |\n| - | - |\n| 1 | 2 |"));
    }

    #[test]
    fn sidecar_strip() {
        let text = "body line\n\n<!-- acord-archive\nABCDEF\n-->\n";
        assert_eq!(strip_sidecar_archive(text), "body line\n\n");
        assert_eq!(strip_sidecar_archive("plain"), "plain");
    }
}

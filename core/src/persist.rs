use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMeta {
    pub uuid: String,
    pub title: String,
    pub path: String,
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateIndex {
    pub notes: HashMap<String, NoteMeta>,
}

impl StateIndex {
    pub fn new() -> Self {
        StateIndex {
            notes: HashMap::new(),
        }
    }

    pub fn load() -> io::Result<Self> {
        let path = state_path();
        if !path.exists() {
            return Ok(Self::new());
        }
        let data = fs::read_to_string(&path)?;
        serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self) -> io::Result<()> {
        let path = state_path();
        ensure_dir(path.parent().unwrap())?;
        let data = serde_json::to_string_pretty(self)?;
        fs::write(&path, data)
    }

    pub fn upsert(&mut self, meta: NoteMeta) {
        self.notes.insert(meta.uuid.clone(), meta);
    }

    pub fn remove(&mut self, uuid: &str) {
        self.notes.remove(uuid);
    }

    pub fn list(&self) -> Vec<&NoteMeta> {
        let mut notes: Vec<&NoteMeta> = self.notes.values().collect();
        notes.sort_by(|a, b| b.modified.cmp(&a.modified));
        notes
    }
}

fn acord_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".acord")
}

fn cache_dir() -> PathBuf {
    acord_dir().join("cache")
}

fn state_path() -> PathBuf {
    acord_dir().join("state.json")
}

fn ensure_dir(dir: &Path) -> io::Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn title_from_text(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let title = trimmed.trim_start_matches('#').trim();
        if !title.is_empty() {
            return title.chars().take(80).collect();
        }
    }
    "Untitled".into()
}

pub fn save_to_file(text: &str, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, text)
}

pub fn load_from_file(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

pub fn cache_save(uuid: &str, text: &str) -> io::Result<PathBuf> {
    let dir = cache_dir();
    ensure_dir(&dir)?;

    let filename = format!("{}.sw", uuid);
    let path = dir.join(&filename);
    fs::write(&path, text)?;

    let mut index = StateIndex::load().unwrap_or_else(|_| StateIndex::new());
    index.upsert(NoteMeta {
        uuid: uuid.to_string(),
        title: title_from_text(text),
        path: path.to_string_lossy().into_owned(),
        modified: now_epoch(),
    });
    index.save()?;

    Ok(path)
}

pub fn cache_load(uuid: &str) -> io::Result<String> {
    let filename = format!("{}.sw", uuid);
    let path = cache_dir().join(filename);
    fs::read_to_string(path)
}

pub fn list_notes() -> Vec<NoteMeta> {
    StateIndex::load()
        .unwrap_or_else(|_| StateIndex::new())
        .list()
        .into_iter()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_extraction() {
        assert_eq!(title_from_text("# My Note\nSome content"), "My Note");
        assert_eq!(title_from_text("Hello world"), "Hello world");
        assert_eq!(title_from_text(""), "Untitled");
        assert_eq!(title_from_text("\n\n## Section\nstuff"), "Section");
    }

    #[test]
    fn state_index_round_trip() {
        let mut idx = StateIndex::new();
        idx.upsert(NoteMeta {
            uuid: "abc".into(),
            title: "Test".into(),
            path: "/tmp/test.sw".into(),
            modified: 1000,
        });
        assert_eq!(idx.list().len(), 1);
        idx.remove("abc");
        assert_eq!(idx.list().len(), 0);
    }
}

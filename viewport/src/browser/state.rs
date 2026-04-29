use std::path::PathBuf;

use super::model::{self, BrowserItem, BrowserItemKind};

pub struct BrowserState {
    pub root: PathBuf,
    pub current: PathBuf,
    pub items: Vec<BrowserItem>,
    pub selected: Option<PathBuf>,
    pub scale: f32,
    pub renaming: Option<PathBuf>,
    pub rename_text: String,
    /// Set when an item should be opened; the host shell drains this each frame.
    pub pending_open: Option<PathBuf>,
    pub context_menu: Option<ContextMenu>,
}

#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub anchor: iced_wgpu::core::Point,
    pub item_path: PathBuf,
    pub is_file: bool,
}

#[derive(Debug, Clone)]
pub enum BrowserMessage {
    NavigateTo(PathBuf),
    Open(PathBuf),
    Select(PathBuf),
    StartRename(PathBuf),
    UpdateRename(String),
    CommitRename,
    CancelRename,
    Duplicate(PathBuf),
    Trash(PathBuf),
    NewFolder,
    ScaleUp,
    ScaleDown,
    Refresh,
    ShowContextMenu { anchor: iced_wgpu::core::Point, path: PathBuf, is_file: bool },
    HideContextMenu,
}

impl BrowserState {
    pub fn new(root: PathBuf) -> Self {
        let current = root.clone();
        let items = model::scan_directory(&current);
        Self {
            root,
            current,
            items,
            selected: None,
            scale: 1.0,
            renaming: None,
            rename_text: String::new(),
            pending_open: None,
            context_menu: None,
        }
    }

    pub fn refresh(&mut self) {
        self.items = model::scan_directory(&self.current);
    }

    pub fn update(&mut self, msg: BrowserMessage) {
        match msg {
            BrowserMessage::NavigateTo(path) => {
                self.current = path;
                self.selected = None;
                self.renaming = None;
                self.context_menu = None;
                self.refresh();
            }
            BrowserMessage::Open(path) => {
                self.pending_open = Some(path);
                self.context_menu = None;
            }
            BrowserMessage::Select(path) => {
                self.selected = Some(path);
                self.context_menu = None;
            }
            BrowserMessage::StartRename(path) => {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
                    .unwrap_or_default();
                self.rename_text = stem;
                self.renaming = Some(path);
                self.context_menu = None;
            }
            BrowserMessage::UpdateRename(text) => {
                self.rename_text = text;
            }
            BrowserMessage::CommitRename => {
                if let Some(path) = self.renaming.take() {
                    let is_file = path.is_file();
                    let _ = model::rename(&path, &self.rename_text, is_file);
                    self.rename_text.clear();
                    self.refresh();
                }
            }
            BrowserMessage::CancelRename => {
                self.renaming = None;
                self.rename_text.clear();
            }
            BrowserMessage::Duplicate(path) => {
                let _ = model::duplicate(&path);
                self.context_menu = None;
                self.refresh();
            }
            BrowserMessage::Trash(path) => {
                let _ = model::trash(&path);
                if self.selected.as_deref() == Some(&path) {
                    self.selected = None;
                }
                self.context_menu = None;
                self.refresh();
            }
            BrowserMessage::NewFolder => {
                let _ = model::create_folder(&self.current);
                self.refresh();
            }
            BrowserMessage::ScaleUp => {
                self.scale = (self.scale + 0.1).min(3.0);
            }
            BrowserMessage::ScaleDown => {
                self.scale = (self.scale - 0.1).max(0.4);
            }
            BrowserMessage::Refresh => {
                self.refresh();
            }
            BrowserMessage::ShowContextMenu { anchor, path, is_file } => {
                self.context_menu = Some(ContextMenu { anchor, item_path: path, is_file });
            }
            BrowserMessage::HideContextMenu => {
                self.context_menu = None;
            }
        }
    }

    pub fn take_pending_open(&mut self) -> Option<PathBuf> {
        self.pending_open.take()
    }

    pub fn path_segments(&self) -> Vec<(String, PathBuf)> {
        model::path_segments(&self.current, &self.root)
    }

    pub fn is_renaming(&self, item: &BrowserItem) -> bool {
        self.renaming.as_deref() == Some(&item.path)
    }

    pub fn is_selected(&self, item: &BrowserItem) -> bool {
        self.selected.as_deref() == Some(&item.path)
    }

    pub fn item_kind_is_file(item: &BrowserItem) -> bool {
        item.kind == BrowserItemKind::File
    }
}

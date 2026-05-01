use std::collections::HashSet;
use std::path::PathBuf;

use iced_wgpu::core::keyboard::Modifiers;
use iced_wgpu::core::Point;

use super::model::{self, BrowserItem, BrowserItemKind};

pub struct BrowserState {
    pub root: PathBuf,
    pub current: PathBuf,
    pub items: Vec<BrowserItem>,
    pub selected: HashSet<PathBuf>,
    pub selection_anchor: Option<PathBuf>,
    pub scale: f32,
    pub renaming: Option<PathBuf>,
    pub rename_text: String,
    /// holds the next path the host shell should open; drained each frame.
    pub pending_open: Option<PathBuf>,
    pub context_menu: Option<ContextMenu>,
    pub current_modifiers: Modifiers,
    pub cursor_pos: Point,
}

#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub anchor: Point,
    /// None when the right-click landed between cards.
    pub target: Option<ContextTarget>,
}

#[derive(Debug, Clone)]
pub struct ContextTarget {
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
    NewFolderWithSelection,
    ScaleUp,
    ScaleDown,
    ScaleReset,
    Refresh,
    ShowContextMenu { path: PathBuf, is_file: bool },
    ShowEmptyContextMenu,
    HideContextMenu,
    ContextOpen,
    ContextRename,
    ContextDuplicate,
    ContextTrash,
}

impl BrowserState {
    pub fn new(root: PathBuf) -> Self {
        let current = root.clone();
        let items = model::scan_directory(&current);
        Self {
            root,
            current,
            items,
            selected: HashSet::new(),
            selection_anchor: None,
            scale: 1.0,
            renaming: None,
            rename_text: String::new(),
            pending_open: None,
            context_menu: None,
            current_modifiers: Modifiers::empty(),
            cursor_pos: Point::ORIGIN,
        }
    }

    pub fn refresh(&mut self) {
        self.items = model::scan_directory(&self.current);
        self.prune_selection();
    }

    pub fn update(&mut self, msg: BrowserMessage) {
        match msg {
            BrowserMessage::NavigateTo(path) => {
                self.current = path;
                self.selected.clear();
                self.selection_anchor = None;
                self.renaming = None;
                self.context_menu = None;
                self.refresh();
            }
            BrowserMessage::Open(path) => {
                self.pending_open = Some(path);
                self.context_menu = None;
            }
            BrowserMessage::Select(path) => {
                self.apply_selection(path);
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
                self.selected.remove(&path);
                if self.selection_anchor.as_deref() == Some(&path) {
                    self.selection_anchor = None;
                }
                self.context_menu = None;
                self.refresh();
            }
            BrowserMessage::NewFolder => {
                self.context_menu = None;
                if let Ok(folder) = model::create_folder(&self.current) {
                    self.refresh();
                    self.start_renaming(folder);
                }
            }
            BrowserMessage::NewFolderWithSelection => {
                self.context_menu = None;
                let items: Vec<PathBuf> = self.selected.iter().cloned().collect();
                if items.is_empty() {
                    return;
                }
                if let Ok(folder) = model::create_folder_with_items(&self.current, &items) {
                    self.selected.clear();
                    self.selection_anchor = None;
                    self.refresh();
                    self.start_renaming(folder);
                }
            }
            BrowserMessage::ScaleUp => {
                self.scale = (self.scale * 14.0 / 13.0).min(3.0);
            }
            BrowserMessage::ScaleDown => {
                self.scale = (self.scale * 13.0 / 14.0).max(0.4);
            }
            BrowserMessage::ScaleReset => {
                self.scale = 1.0;
            }
            BrowserMessage::Refresh => {
                self.refresh();
            }
            BrowserMessage::ShowContextMenu { path, is_file } => {
                self.context_menu = Some(ContextMenu {
                    anchor: self.cursor_pos,
                    target: Some(ContextTarget { item_path: path, is_file }),
                });
            }
            BrowserMessage::ShowEmptyContextMenu => {
                self.context_menu = Some(ContextMenu {
                    anchor: self.cursor_pos,
                    target: None,
                });
            }
            BrowserMessage::HideContextMenu => {
                self.context_menu = None;
            }
            BrowserMessage::ContextOpen => {
                self.context_menu = None;
                if let Some(path) = self.single_selected() {
                    if path.is_dir() {
                        self.current = path;
                        self.selected.clear();
                        self.selection_anchor = None;
                        self.refresh();
                    } else {
                        self.pending_open = Some(path);
                    }
                }
            }
            BrowserMessage::ContextRename => {
                self.context_menu = None;
                if let Some(path) = self.single_selected() {
                    self.start_renaming(path);
                }
            }
            BrowserMessage::ContextDuplicate => {
                self.context_menu = None;
                let targets: Vec<PathBuf> = self.selected.iter().cloned().collect();
                for path in targets {
                    let _ = model::duplicate(&path);
                }
                self.refresh();
            }
            BrowserMessage::ContextTrash => {
                self.context_menu = None;
                let targets: Vec<PathBuf> = self.selected.iter().cloned().collect();
                for path in &targets {
                    let _ = model::trash(path);
                }
                for path in &targets {
                    self.selected.remove(path);
                }
                if let Some(anchor) = &self.selection_anchor {
                    if targets.contains(anchor) {
                        self.selection_anchor = None;
                    }
                }
                self.refresh();
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
        self.selected.contains(&item.path)
    }

    pub fn item_kind_is_file(item: &BrowserItem) -> bool {
        item.kind == BrowserItemKind::File
    }

    /// True when a context menu was opened on an item that's part of the live selection.
    pub fn context_acts_on_selection(&self) -> bool {
        match self.context_menu.as_ref().and_then(|m| m.target.as_ref()) {
            Some(t) => self.selected.contains(&t.item_path),
            None => false,
        }
    }

    /// Returns the lone selected path when selection size is exactly one.
    pub fn single_selected(&self) -> Option<PathBuf> {
        if self.selected.len() == 1 {
            self.selected.iter().next().cloned()
        } else {
            None
        }
    }

    /// applies command/shift/plain selection rules to the clicked path.
    fn apply_selection(&mut self, path: PathBuf) {
        let mods = self.current_modifiers;
        if mods.command() {
            if !self.selected.insert(path.clone()) {
                self.selected.remove(&path);
            }
            self.selection_anchor = Some(path);
        } else if mods.shift() {
            self.select_range_from_anchor(&path);
        } else {
            self.selected.clear();
            self.selected.insert(path.clone());
            self.selection_anchor = Some(path);
        }
    }

    /// extends the selection from the current anchor to the given path, replacing existing selection.
    fn select_range_from_anchor(&mut self, path: &PathBuf) {
        let Some(anchor) = self.selection_anchor.clone() else {
            self.selected.clear();
            self.selected.insert(path.clone());
            self.selection_anchor = Some(path.clone());
            return;
        };
        let a = self.items.iter().position(|i| i.path == anchor);
        let b = self.items.iter().position(|i| i.path == *path);
        let (Some(a), Some(b)) = (a, b) else {
            self.selected.clear();
            self.selected.insert(path.clone());
            self.selection_anchor = Some(path.clone());
            return;
        };
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        self.selected.clear();
        for i in lo..=hi {
            self.selected.insert(self.items[i].path.clone());
        }
    }

    fn start_renaming(&mut self, path: PathBuf) {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_default();
        self.rename_text = stem;
        self.renaming = Some(path);
    }

    /// drops selection entries that no longer exist after a refresh.
    fn prune_selection(&mut self) {
        let live: HashSet<PathBuf> = self.items.iter().map(|i| i.path.clone()).collect();
        self.selected.retain(|p| live.contains(p));
        if let Some(anchor) = &self.selection_anchor {
            if !live.contains(anchor) {
                self.selection_anchor = None;
            }
        }
    }
}

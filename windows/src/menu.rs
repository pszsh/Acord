use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, accelerator::Accelerator};
use muda::accelerator::{Code, Modifiers};

pub struct AppMenu {
    pub menu: Menu,
}

pub enum MenuAction {
    NewNote,
    Open,
    Save,
    SaveAs,
    Quit,
    Undo,
    Redo,
    Bold,
    Italic,
    InsertTable,
    Evaluate,
    LiveMode,
    EditorMode,
    ViewMode,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Find,
    ExportCrate,
}

impl AppMenu {
    pub fn new() -> Self {
        let menu = Menu::new();

        let file = Submenu::new("File", true);
        file.append(&MenuItem::with_id("new", "New Note", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyN)))).ok();
        file.append(&MenuItem::with_id("open", "Open...", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyO)))).ok();
        file.append(&PredefinedMenuItem::separator()).ok();
        file.append(&MenuItem::with_id("save", "Save", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyS)))).ok();
        file.append(&MenuItem::with_id("save_as", "Save As...", true, Some(Accelerator::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS)))).ok();
        file.append(&PredefinedMenuItem::separator()).ok();
        file.append(&MenuItem::with_id("export_crate", "Export as Rust Library", true, Some(Accelerator::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyE)))).ok();
        file.append(&PredefinedMenuItem::separator()).ok();
        file.append(&MenuItem::with_id("quit", "Quit", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyQ)))).ok();

        let edit = Submenu::new("Edit", true);
        edit.append(&MenuItem::with_id("undo", "Undo", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyZ)))).ok();
        edit.append(&MenuItem::with_id("redo", "Redo", true, Some(Accelerator::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyZ)))).ok();
        edit.append(&PredefinedMenuItem::separator()).ok();
        edit.append(&PredefinedMenuItem::cut(None)).ok();
        edit.append(&PredefinedMenuItem::copy(None)).ok();
        edit.append(&PredefinedMenuItem::paste(None)).ok();
        edit.append(&PredefinedMenuItem::select_all(None)).ok();
        edit.append(&PredefinedMenuItem::separator()).ok();
        edit.append(&MenuItem::with_id("find", "Find...", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyF)))).ok();
        edit.append(&PredefinedMenuItem::separator()).ok();
        edit.append(&MenuItem::with_id("bold", "Bold", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyB)))).ok();
        edit.append(&MenuItem::with_id("italic", "Italic", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyI)))).ok();
        edit.append(&MenuItem::with_id("table", "Insert Table", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::KeyT)))).ok();

        let render = Submenu::new("Render", true);
        render.append(&MenuItem::with_id("live", "Live", true, None)).ok();
        render.append(&MenuItem::with_id("editor", "Editor", true, None)).ok();
        render.append(&MenuItem::with_id("view", "View", true, None)).ok();
        render.append(&PredefinedMenuItem::separator()).ok();
        render.append(&MenuItem::with_id("eval", "Evaluate", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::Enter)))).ok();

        let view = Submenu::new("View", true);
        view.append(&MenuItem::with_id("zoom_in", "Zoom In", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::Equal)))).ok();
        view.append(&MenuItem::with_id("zoom_out", "Zoom Out", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::Minus)))).ok();
        view.append(&MenuItem::with_id("zoom_reset", "Reset Zoom", true, Some(Accelerator::new(Some(Modifiers::CONTROL), Code::Digit0)))).ok();

        menu.append(&file).ok();
        menu.append(&edit).ok();
        menu.append(&render).ok();
        menu.append(&view).ok();

        Self { menu }
    }

    pub fn poll() -> Option<MenuAction> {
        MenuEvent::receiver().try_recv().ok().and_then(|e| {
            match e.id().0.as_str() {
                "new" => Some(MenuAction::NewNote),
                "open" => Some(MenuAction::Open),
                "save" => Some(MenuAction::Save),
                "save_as" => Some(MenuAction::SaveAs),
                "quit" => Some(MenuAction::Quit),
                "undo" => Some(MenuAction::Undo),
                "redo" => Some(MenuAction::Redo),
                "bold" => Some(MenuAction::Bold),
                "italic" => Some(MenuAction::Italic),
                "table" => Some(MenuAction::InsertTable),
                "eval" => Some(MenuAction::Evaluate),
                "live" => Some(MenuAction::LiveMode),
                "editor" => Some(MenuAction::EditorMode),
                "view" => Some(MenuAction::ViewMode),
                "zoom_in" => Some(MenuAction::ZoomIn),
                "zoom_out" => Some(MenuAction::ZoomOut),
                "zoom_reset" => Some(MenuAction::ZoomReset),
                "find" => Some(MenuAction::Find),
                "export_crate" => Some(MenuAction::ExportCrate),
                _ => None,
            }
        })
    }
}

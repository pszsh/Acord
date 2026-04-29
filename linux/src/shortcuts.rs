use winit::keyboard::{Key, ModifiersState, SmolStr};

#[derive(Clone, Copy)]
#[allow(dead_code)]
// LiveMode/EditorMode/ViewMode are dispatched but not yet bound to a shortcut
// — Linux has no menu bar to expose them via, so they wait for either a key
// binding decision or an iced-rendered menu inside the viewport.
pub enum MenuAction {
    NewNote,
    Open,
    Save,
    SaveAs,
    Quit,
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
    Settings,
    ExportCrate,
    ToggleBrowser,
}

/// Matches an app-level shortcut. Returns Some(action) for combos that should
/// fire a MenuAction; None for combos that should fall through to the
/// viewport (cut/copy/paste/undo/redo/select-all are handled inside iced via
/// the Ctrl→LOGO modifier alias, plain typing, navigation, etc.).
pub fn match_shortcut(modifiers: ModifiersState, key: &Key) -> Option<MenuAction> {
    // Alt+B mirrors macOS Ctrl+B for the document browser. Mac-Cmd maps to
    // Ctrl on Linux/Windows, so Mac-Ctrl gets bumped to Alt to avoid collision.
    if modifiers.alt_key() && !modifiers.control_key() && !modifiers.super_key() {
        if let Key::Character(s) = key {
            if ascii_lower(s) == 'b' {
                return Some(MenuAction::ToggleBrowser);
            }
        }
    }

    if !modifiers.control_key() {
        return None;
    }
    let shift = modifiers.shift_key();

    match key {
        Key::Character(s) => match (shift, ascii_lower(s)) {
            (false, 'n') => Some(MenuAction::NewNote),
            (false, 'o') => Some(MenuAction::Open),
            (false, 's') => Some(MenuAction::Save),
            (true,  's') => Some(MenuAction::SaveAs),
            (false, 'q') => Some(MenuAction::Quit),
            (false, 'b') => Some(MenuAction::Bold),
            (false, 'i') => Some(MenuAction::Italic),
            (false, 't') => Some(MenuAction::InsertTable),
            (false, 'f') => Some(MenuAction::Find),
            (false, 'e') => Some(MenuAction::Evaluate),
            (true,  'e') => Some(MenuAction::ExportCrate),
            (false, ',') => Some(MenuAction::Settings),
            (false, '=') | (false, '+') => Some(MenuAction::ZoomIn),
            (false, '-') => Some(MenuAction::ZoomOut),
            (true,  '0') => Some(MenuAction::ZoomReset),
            _ => None,
        },
        _ => None,
    }
}

fn ascii_lower(s: &SmolStr) -> char {
    s.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0')
}

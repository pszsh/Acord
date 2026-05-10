import SwiftUI

/// Bridges SwiftUI menu buttons to the Rust viewport handle.
/// Holds a weak reference to the IcedViewportView so the menu can dispatch
/// commands without owning the rendering surface.
final class ViewportController: ObservableObject {
    weak var view: IcedViewportView?

    func send(_ code: UInt32) {
        guard let h = view?.viewportHandle else {
            dlog("send(\(code)): no handle")
            return
        }
        dlog("send(\(code))")
        viewport_send_command(h, code)
    }

    /// Editor commands (mirror viewport/src/lib.rs::viewport_send_command codes).
    func toggleBold()        { send(1) }
    func toggleItalic()      { send(2) }
    func insertTable()       { send(3) }
    func evaluate()          { send(5) }
    func zoomIn()            { send(7) }
    func zoomOut()           { send(8) }
    func resetZoom()         { send(9) }
    func setLiveMode()       { send(11) }
    func setEditorMode()     { send(12) }
    func setViewMode()       { send(13) }
    func toggleSettings()    { send(16) }

    /// Hand-rolled key events for shortcuts that flow through iced's text bindings
    /// rather than the cmd dispatcher (Find, Undo, Redo, etc.).
    private func sendKey(keyCode: UInt32, modifiers: UInt32, character: String) {
        guard let h = view?.viewportHandle else {
            dlog("sendKey: no handle (key=\(character.debugDescription) mods=\(modifiers))")
            return
        }
        dlog("sendKey: char=\(character.debugDescription) keyCode=\(keyCode) mods=\(modifiers)")
        character.withCString { cstr in
            viewport_key_event(h, keyCode, modifiers, true, cstr)
            viewport_key_event(h, keyCode, modifiers, false, cstr)
        }
    }

    /// `f` / cmd. The viewport reads .super_key as cmd via iced's modifier mapping.
    /// keycode 3 is the macOS keycode for `f`; iced doesn't actually use the
    /// platform keycode on macOS — it pulls the Key from the characters string.
    /// So we pass 0 and let the character drive it.
    func toggleFind() { sendKey(keyCode: 0, modifiers: cmdMask, character: "f") }
    func undo()       { sendKey(keyCode: 0, modifiers: cmdMask, character: "z") }
    func redo()       { sendKey(keyCode: 0, modifiers: cmdMask | shiftMask, character: "Z") }

    // UIKeyModifierFlags bits; copied here so the controller doesn't import UIKit.
    private var cmdMask: UInt32   { 1 << 20 }
    private var shiftMask: UInt32 { 1 << 17 }

    /// File operations — Open and Save go through UIDocumentPicker so iOS
    /// prompts the user to grant per-file access.
    func newNote() {
        guard let h = view?.viewportHandle else {
            dlog("newNote: no handle")
            return
        }
        dlog("newNote")
        let stub = "# "
        stub.withCString { viewport_set_text(h, $0) }
    }

    func openDocument() {
        guard let h = view?.viewportHandle else {
            dlog("openDocument: no handle")
            return
        }
        dlog("openDocument: dispatching to picker")
        DocumentPicker.presentOpen(handle: h)
    }

    func saveDocument() {
        dlog("saveDocument (routed to saveDocumentAs)")
        saveDocumentAs()
    }

    func saveDocumentAs() {
        guard let h = view?.viewportHandle else {
            dlog("saveDocumentAs: no handle")
            return
        }
        dlog("saveDocumentAs: dispatching to picker")
        DocumentPicker.presentSave(handle: h, defaultName: "Acord")
    }

    func printDocument() {
        dlog("printDocument: not implemented")
        // TODO: call viewport_render_pdf, hand to UIPrintInteractionController
    }
}

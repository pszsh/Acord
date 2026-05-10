import SwiftUI

/// Top toolbar with File / Edit / Render / View menus, mirroring the
/// editor's MenuCategory layout. Uses SwiftUI Menu so each label opens a
/// dropdown of buttons; each button dispatches through ViewportController.
struct MenuBar: View {
    @ObservedObject var controller: ViewportController

    var body: some View {
        HStack(spacing: 0) {
            fileMenu
            editMenu
            renderMenu
            viewMenu
            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Color(white: 0.12))
        .foregroundColor(.white)
        .font(.system(size: 13))
    }

    private var fileMenu: some View {
        Menu("File") {
            Button("New Note") { controller.newNote() }
            Button("Open…")     { controller.openDocument() }
            Divider()
            Button("Save")      { controller.saveDocument() }
            Button("Save As…")  { controller.saveDocumentAs() }
            Divider()
            Button("Print…")    { controller.printDocument() }
            Divider()
            Button("Settings…") { controller.toggleSettings() }
        }
        .menuLabel
    }

    private var editMenu: some View {
        Menu("Edit") {
            Button("Undo")          { controller.undo() }
            Button("Redo")          { controller.redo() }
            Divider()
            Button("Bold")          { controller.toggleBold() }
            Button("Italic")        { controller.toggleItalic() }
            Button("Insert Table")  { controller.insertTable() }
            Divider()
            Button("Find…")         { controller.toggleFind() }
        }
        .menuLabel
    }

    private var renderMenu: some View {
        Menu("Render") {
            Button("Live")     { controller.setLiveMode() }
            Button("Editor")   { controller.setEditorMode() }
            Button("View")     { controller.setViewMode() }
            Divider()
            Button("Evaluate") { controller.evaluate() }
        }
        .menuLabel
    }

    private var viewMenu: some View {
        Menu("View") {
            Button("Zoom In")    { controller.zoomIn() }
            Button("Zoom Out")   { controller.zoomOut() }
            Button("Reset Zoom") { controller.resetZoom() }
        }
        .menuLabel
    }
}

private extension View {
    /// Consistent Menu chrome — slightly padded, light hit target.
    var menuLabel: some View {
        self
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
    }
}

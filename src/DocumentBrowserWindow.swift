import Cocoa
import SwiftUI
import Combine
import UniformTypeIdentifiers

// MARK: - Model

enum BrowserItemKind {
    case file
    case folder
}

struct BrowserItem: Identifiable, Hashable {
    let id: String
    let url: URL
    let name: String
    let kind: BrowserItemKind
    let modified: Date
    var preview: String

    static func == (lhs: BrowserItem, rhs: BrowserItem) -> Bool {
        lhs.url == rhs.url
    }
    func hash(into hasher: inout Hasher) {
        hasher.combine(url)
    }
}

// MARK: - Controller

class DocumentBrowserController {
    static var shared: DocumentBrowserController?

    let window: NSWindow
    let browserState: BrowserState
    private let hostingView: NSHostingView<DocumentBrowserView>

    init(appState: AppState) {
        browserState = BrowserState(appState: appState)

        let view = DocumentBrowserView(state: browserState)
        hostingView = NSHostingView(rootView: view)

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 600),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Documents"
        window.backgroundColor = Theme.current.base
        window.contentView = hostingView
        window.setFrameAutosaveName("AcordBrowser")
        window.center()
        window.isReleasedWhenClosed = false
    }

    func toggle() {
        if window.isVisible {
            window.orderOut(nil)
        } else {
            browserState.refresh()
            window.makeKeyAndOrderFront(nil)
        }
    }
}

// MARK: - State

class BrowserState: ObservableObject {
    @Published var items: [BrowserItem] = []
    @Published var cardScale: CGFloat = 1.0
    @Published var selectedURL: URL?
    @Published var currentPath: URL

    let appState: AppState
    private let fm = FileManager.default
    private static let supportedExtensions: Set<String> = ["md", "txt", "markdown", "mdown"]

    var rootPath: URL {
        URL(fileURLWithPath: ConfigManager.shared.autoSaveDirectory)
    }

    var pathSegments: [(name: String, url: URL)] {
        var segments: [(String, URL)] = []
        var path = currentPath.standardizedFileURL
        let root = rootPath.standardizedFileURL

        while path != root && path.path.hasPrefix(root.path) {
            segments.insert((path.lastPathComponent, path), at: 0)
            path = path.deletingLastPathComponent().standardizedFileURL
        }
        segments.insert(("Documents", root), at: 0)
        return segments
    }

    init(appState: AppState) {
        self.appState = appState
        self.currentPath = URL(fileURLWithPath: ConfigManager.shared.autoSaveDirectory)
        refresh()
    }

    func refresh() {
        items = scanDirectory(currentPath)
    }

    func navigate(to url: URL) {
        currentPath = url
        selectedURL = nil
        refresh()
    }

    private func scanDirectory(_ dir: URL) -> [BrowserItem] {
        guard let contents = try? fm.contentsOfDirectory(
            at: dir,
            includingPropertiesForKeys: [.contentModificationDateKey, .isDirectoryKey],
            options: [.skipsHiddenFiles]
        ) else { return [] }

        var folders: [BrowserItem] = []
        var files: [BrowserItem] = []

        for url in contents {
            guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .contentModificationDateKey]) else { continue }
            let mtime = values.contentModificationDate ?? .distantPast

            if values.isDirectory == true {
                folders.append(BrowserItem(
                    id: url.path,
                    url: url,
                    name: url.lastPathComponent,
                    kind: .folder,
                    modified: mtime,
                    preview: folderSummary(url)
                ))
            } else {
                let ext = url.pathExtension.lowercased()
                guard Self.supportedExtensions.contains(ext) else { continue }
                files.append(BrowserItem(
                    id: url.path,
                    url: url,
                    name: url.deletingPathExtension().lastPathComponent,
                    kind: .file,
                    modified: mtime,
                    preview: filePreview(url)
                ))
            }
        }

        folders.sort { $0.modified > $1.modified }
        files.sort { $0.modified > $1.modified }
        return folders + files
    }

    private func filePreview(_ url: URL) -> String {
        guard let data = try? Data(contentsOf: url, options: .mappedIfSafe),
              let text = String(data: data, encoding: .utf8) else { return "" }
        let body = Self.stripSidecarArchive(text)
        if Self.bodyLooksBlank(body) {
            return "(empty note)"
        }
        let lines = body.components(separatedBy: "\n")
        return lines.prefix(20).joined(separator: "\n")
    }

    /// Remove the `<!-- acord-archive … -->` base64 sidecar comment before
    /// previewing. Without this, phantom notes that were saved with only
    /// an empty default table render their archive blob as tile text.
    private static func stripSidecarArchive(_ text: String) -> String {
        guard let marker = text.range(of: "<!-- acord-archive") else { return text }
        return String(text[..<marker.lowerBound])
    }

    /// `true` when the body contains no real content — either all whitespace
    /// or nothing but an empty default-header table with no user data. These
    /// show up for notes the user opened but never filled in; calling them
    /// out as `(empty note)` beats rendering three rows of `| | |`.
    private static func bodyLooksBlank(_ body: String) -> Bool {
        let trimmed = body.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return true }
        let meaningful = trimmed.components(separatedBy: "\n").filter { line in
            let t = line.trimmingCharacters(in: .whitespaces)
            if t.isEmpty { return false }
            if !t.hasPrefix("|") { return true }
            let cells = t
                .trimmingCharacters(in: CharacterSet(charactersIn: "|"))
                .components(separatedBy: "|")
                .map { $0.trimmingCharacters(in: .whitespaces) }
            // Separator row: cells are all dashes/colons.
            if cells.allSatisfy({ !$0.isEmpty && $0.allSatisfy { "-:".contains($0) } }) {
                return false
            }
            // All cells empty or the default `Header N` placeholder.
            let isDefaultHeader = cells.enumerated().allSatisfy { (i, cell) in
                cell == "Header \(i + 1)"
            }
            if cells.allSatisfy({ $0.isEmpty }) || isDefaultHeader {
                return false
            }
            return true
        }
        return meaningful.isEmpty
    }

    private func folderSummary(_ url: URL) -> String {
        let contents = (try? fm.contentsOfDirectory(
            at: url,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        )) ?? []
        let fileCount = contents.filter {
            Self.supportedExtensions.contains($0.pathExtension.lowercased())
        }.count
        let folderCount = contents.filter {
            (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
        }.count
        var parts: [String] = []
        if fileCount > 0 { parts.append("\(fileCount) file\(fileCount == 1 ? "" : "s")") }
        if folderCount > 0 { parts.append("\(folderCount) folder\(folderCount == 1 ? "" : "s")") }
        return parts.isEmpty ? "Empty" : parts.joined(separator: ", ")
    }

    // MARK: - Actions

    func openFile(_ item: BrowserItem) {
        guard item.kind == .file else { return }
        appState.loadNoteFromFile(item.url)
        DocumentBrowserController.shared?.window.orderOut(nil)
    }

    func renameItem(_ item: BrowserItem, to newName: String) {
        let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        let ext = item.kind == .file ? "." + item.url.pathExtension : ""
        let dest = item.url.deletingLastPathComponent().appendingPathComponent(trimmed + ext)
        guard !fm.fileExists(atPath: dest.path) else { return }
        try? fm.moveItem(at: item.url, to: dest)
        refresh()
    }

    func duplicateItem(_ item: BrowserItem) {
        guard item.kind == .file else { return }
        let dir = item.url.deletingLastPathComponent()
        let base = item.url.deletingPathExtension().lastPathComponent
        let ext = item.url.pathExtension
        var n = 1
        var dest: URL
        repeat {
            dest = dir.appendingPathComponent("\(base) \(n).\(ext)")
            n += 1
        } while fm.fileExists(atPath: dest.path)
        try? fm.copyItem(at: item.url, to: dest)
        refresh()
    }

    func trashItem(_ item: BrowserItem) {
        try? fm.trashItem(at: item.url, resultingItemURL: nil)
        if selectedURL == item.url { selectedURL = nil }
        refresh()
    }

    func revealInFinder(_ item: BrowserItem) {
        NSWorkspace.shared.activateFileViewerSelecting([item.url])
    }

    func createFolder() {
        var name = "New Folder"
        var n = 1
        while fm.fileExists(atPath: currentPath.appendingPathComponent(name).path) {
            n += 1
            name = "New Folder \(n)"
        }
        let url = currentPath.appendingPathComponent(name)
        try? fm.createDirectory(at: url, withIntermediateDirectories: false)
        refresh()
    }

    func moveItem(_ item: BrowserItem, into folder: BrowserItem) {
        guard folder.kind == .folder else { return }
        let dest = folder.url.appendingPathComponent(item.url.lastPathComponent)
        guard !fm.fileExists(atPath: dest.path) else { return }
        try? fm.moveItem(at: item.url, to: dest)
        refresh()
    }

    func scaleUp() {
        cardScale = min(cardScale + 0.1, 3.0)
    }

    func scaleDown() {
        cardScale = max(cardScale - 0.1, 0.4)
    }
}

// MARK: - Browser View

struct DocumentBrowserView: View {
    @ObservedObject var state: BrowserState

    var body: some View {
        VStack(spacing: 0) {
            BreadcrumbBar(state: state)
            Divider().background(Color(ns: Theme.current.surface1))

            ScrollView {
                if state.items.isEmpty {
                    emptyState
                } else {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(
                            minimum: 200 * state.cardScale,
                            maximum: 400 * state.cardScale
                        ))],
                        spacing: 16 * state.cardScale
                    ) {
                        ForEach(state.items) { item in
                            BrowserCardView(item: item, state: state)
                                .onDrag {
                                    NSItemProvider(object: item.url as NSURL)
                                }
                        }
                    }
                    .padding(16 * state.cardScale)
                }
            }
            .background(Color(ns: Theme.current.base))
            .contextMenu {
                Button("New Folder") { state.createFolder() }
                Divider()
                Button("Reveal in Finder") {
                    NSWorkspace.shared.open(state.currentPath)
                }
            }
        }
        .background(Color(ns: Theme.current.base))
        .frame(minWidth: 400, minHeight: 300)
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Text("No documents")
                .font(.system(size: 16, weight: .medium))
                .foregroundColor(Color(ns: Theme.current.subtext0))
            Text("Create a new note or add files to this folder")
                .font(.system(size: 12))
                .foregroundColor(Color(ns: Theme.current.overlay0))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.top, 100)
    }
}

// MARK: - Breadcrumb Bar

struct BreadcrumbBar: View {
    @ObservedObject var state: BrowserState

    var body: some View {
        HStack(spacing: 4) {
            ForEach(Array(state.pathSegments.enumerated()), id: \.offset) { index, segment in
                if index > 0 {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundColor(Color(ns: Theme.current.overlay0))
                }
                Button(action: { state.navigate(to: segment.url) }) {
                    Text(segment.name)
                        .font(.system(size: 12, weight: isLast(index) ? .semibold : .regular))
                        .foregroundColor(Color(ns: isLast(index) ? Theme.current.text : Theme.current.subtext0))
                }
                .buttonStyle(.plain)
            }
            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(Color(ns: Theme.current.mantle))
    }

    private func isLast(_ index: Int) -> Bool {
        index == state.pathSegments.count - 1
    }
}

// MARK: - Card View

struct BrowserCardView: View {
    let item: BrowserItem
    @ObservedObject var state: BrowserState
    @State private var isRenaming = false
    @State private var renameText = ""
    @State private var isDropTarget = false

    private var isSelected: Bool {
        state.selectedURL == item.url
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6 * state.cardScale) {
            previewArea
            titleArea
        }
        .padding(10 * state.cardScale)
        .background(
            RoundedRectangle(cornerRadius: 8 * state.cardScale)
                .fill(Color(ns: isSelected ? Theme.current.surface1 : Theme.current.surface0))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8 * state.cardScale)
                .stroke(
                    isDropTarget ? Color(ns: Theme.current.green) :
                    isSelected ? Color(ns: Theme.current.blue) : Color.clear,
                    lineWidth: 2
                )
        )
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            switch item.kind {
            case .folder: state.navigate(to: item.url)
            case .file: state.openFile(item)
            }
        }
        .onTapGesture(count: 1) {
            state.selectedURL = item.url
        }
        .contextMenu { contextMenuItems }
        .onDrop(of: [.fileURL], isTargeted: item.kind == .folder ? $isDropTarget : .constant(false)) { providers in
            guard item.kind == .folder else { return false }
            for provider in providers {
                provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { data, _ in
                    guard let urlData = data as? Data,
                          let sourceURL = URL(dataRepresentation: urlData, relativeTo: nil) else { return }
                    DispatchQueue.main.async {
                        let source = BrowserItem(
                            id: sourceURL.path, url: sourceURL,
                            name: sourceURL.lastPathComponent,
                            kind: .file, modified: .now, preview: ""
                        )
                        state.moveItem(source, into: item)
                    }
                }
            }
            return true
        }
    }

    @ViewBuilder
    private var previewArea: some View {
        if item.kind == .folder {
            HStack(spacing: 8 * state.cardScale) {
                Image(systemName: "folder.fill")
                    .font(.system(size: 28 * state.cardScale))
                    .foregroundColor(Color(ns: Theme.current.blue))
                Text(item.preview)
                    .font(.system(size: 10 * state.cardScale))
                    .foregroundColor(Color(ns: Theme.current.subtext0))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8 * state.cardScale)
            .background(Color(ns: Theme.current.mantle))
            .cornerRadius(4 * state.cardScale)
        } else {
            Text(item.preview)
                .font(.system(size: 10 * state.cardScale, design: .monospaced))
                .foregroundColor(Color(ns: Theme.current.subtext0))
                .lineLimit(nil)
                .frame(maxWidth: .infinity, alignment: .topLeading)
                .padding(8 * state.cardScale)
                .background(Color(ns: Theme.current.mantle))
                .cornerRadius(4 * state.cardScale)
        }
    }

    @ViewBuilder
    private var titleArea: some View {
        if isRenaming {
            TextField("Name", text: $renameText, onCommit: {
                state.renameItem(item, to: renameText)
                isRenaming = false
            })
            .textFieldStyle(.plain)
            .font(.system(size: 12 * state.cardScale, weight: .semibold))
            .foregroundColor(Color(ns: Theme.current.text))
            .padding(.horizontal, 4)
        } else {
            Text(item.name)
                .font(.system(size: 12 * state.cardScale, weight: .semibold))
                .foregroundColor(Color(ns: Theme.current.text))
                .lineLimit(2)
                .padding(.horizontal, 4)
        }
    }

    @ViewBuilder
    private var contextMenuItems: some View {
        switch item.kind {
        case .file:
            Button("Open") { state.openFile(item) }
            Button("Rename") {
                renameText = item.name
                isRenaming = true
            }
            Button("Duplicate") { state.duplicateItem(item) }
            Divider()
            Button("Move to Trash") { state.trashItem(item) }
            Divider()
            Button("Reveal in Finder") { state.revealInFinder(item) }
        case .folder:
            Button("Open") { state.navigate(to: item.url) }
            Button("Rename") {
                renameText = item.name
                isRenaming = true
            }
            Divider()
            Button("Move to Trash") { state.trashItem(item) }
            Divider()
            Button("Reveal in Finder") { state.revealInFinder(item) }
        }
    }
}

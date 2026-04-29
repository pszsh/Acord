import Foundation
import Combine

enum FileFormat: String, CaseIterable {
    case markdown, csv, json, toml, yaml, xml, svg
    case rust, c, cpp, objc
    case javascript, typescript, jsx, tsx
    case html, css, scss, less
    case python, go, ruby, php, lua
    case shell, java, kotlin, swift, zig, sql
    case makefile, dockerfile
    case config, lock, plainText
    case unknown

    static func from(extension ext: String) -> FileFormat {
        switch ext.lowercased() {
        case "md", "markdown", "mdown": return .markdown
        case "csv": return .csv
        case "json": return .json
        case "toml": return .toml
        case "yaml", "yml": return .yaml
        case "xml": return .xml
        case "svg": return .svg
        case "rs": return .rust
        case "c": return .c
        case "cpp", "cc", "cxx": return .cpp
        case "h", "hpp", "hxx": return .cpp
        case "m": return .objc
        case "js": return .javascript
        case "jsx": return .jsx
        case "ts": return .typescript
        case "tsx": return .tsx
        case "html", "htm": return .html
        case "css": return .css
        case "scss": return .scss
        case "less": return .less
        case "py": return .python
        case "go": return .go
        case "rb": return .ruby
        case "php": return .php
        case "lua": return .lua
        case "sh", "bash", "zsh", "fish": return .shell
        case "java": return .java
        case "kt", "kts": return .kotlin
        case "swift": return .swift
        case "zig": return .zig
        case "sql": return .sql
        case "mk": return .makefile
        case "ini", "cfg", "conf", "env": return .config
        case "lock": return .lock
        case "txt", "text", "log": return .plainText
        default: return .unknown
        }
    }

    static func from(filename: String) -> FileFormat {
        let lower = filename.lowercased()
        if lower == "makefile" { return .makefile }
        if lower == "dockerfile" { return .dockerfile }
        let ext = (filename as NSString).pathExtension
        if ext.isEmpty { return .unknown }
        return from(extension: ext)
    }

    var isCode: Bool {
        switch self {
        case .rust, .c, .cpp, .objc, .javascript, .typescript, .jsx, .tsx,
             .html, .css, .scss, .less, .python, .go, .ruby, .php, .lua,
             .shell, .java, .kotlin, .swift, .zig, .sql, .makefile, .dockerfile,
             .json, .toml, .yaml, .xml, .svg:
            return true
        default:
            return false
        }
    }

    var isMarkdown: Bool { self == .markdown }
    var isCSV: Bool { self == .csv }

    var treeSitterLang: String? {
        switch self {
        case .rust: return "rust"
        case .c: return "c"
        case .cpp: return "cpp"
        case .javascript: return "javascript"
        case .jsx: return "jsx"
        case .typescript: return "typescript"
        case .tsx: return "tsx"
        case .python: return "python"
        case .go: return "go"
        case .ruby: return "ruby"
        case .php: return "php"
        case .lua: return "lua"
        case .shell: return "bash"
        case .java: return "java"
        case .kotlin: return "kotlin"
        case .swift: return "swift"
        case .zig: return "zig"
        case .sql: return "sql"
        case .html: return "html"
        case .css, .scss, .less: return "css"
        case .json: return "json"
        case .toml: return "toml"
        case .yaml: return "yaml"
        case .makefile: return "make"
        case .dockerfile: return "dockerfile"
        default: return nil
        }
    }
}

class AppState: ObservableObject {
    @Published var documentText: String = "" {
        didSet {
            if documentText != oldValue {
                modified = true
                bridge.setText(currentNoteID, text: documentText)
                scheduleAutoSave()
            }
        }
    }
    @Published var evalResults: [Int: EvalEntry] = [:]
    @Published var noteList: [NoteInfo] = []
    @Published var currentNoteID: UUID
    @Published var selectedNoteIDs: Set<UUID> = []
    @Published var modified: Bool = false
    @Published var currentFileURL: URL? = nil
    @Published var currentFileFormat: FileFormat = .markdown

    private let bridge = RustBridge.shared
    private var autoSaveTimer: DispatchSourceTimer?
    private var autoSaveDirty = false
    private var autoSaveCoolingDown = false
    private let autoSaveQueue = DispatchQueue(label: "com.acord.autosave")
    /// Per-note autosave file path, established on the first write and never
    /// changed for the rest of the session. Stops the title-derived filename
    /// from re-deriving on every keystroke and littering the notes directory
    /// with `u.md`, `us.md`, `use.md`, ...
    private var autoSavePaths: [UUID: URL] = [:]

    init() {
        let id = bridge.newDocument()
        self.currentNoteID = id
        self.selectedNoteIDs = [id]
        refreshNoteList()
    }

    // MARK: - Auto-save

    private func scheduleAutoSave() {
        if autoSaveCoolingDown {
            autoSaveDirty = true
            return
        }
        performAutoSave()
    }

    private func performAutoSave() {
        guard shouldAutoSave() else { return }

        autoSaveCoolingDown = true
        autoSaveDirty = false

        let text = documentText
        let noteID = currentNoteID
        let url = resolveAutoSaveURL(noteID: noteID, text: text)

        autoSaveQueue.async { [weak self] in
            Self.writeAutoSaveFile(at: url, text: text)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
                guard let self = self else { return }
                self.autoSaveCoolingDown = false
                if self.autoSaveDirty {
                    self.autoSaveDirty = false
                    self.performAutoSave()
                }
            }
        }

        bridge.setText(currentNoteID, text: documentText)
        let _ = bridge.cacheSave(currentNoteID)
        modified = false
        refreshNoteList()
    }

    private func shouldAutoSave() -> Bool {
        // Autosave only when the note has real user content. A freshly-
        // created doc that picked up the default `Header 1 | Header 2 |
        // Header 3` table from Cmd+T without the user typing anything
        // still reads as "blank" by this check — that's what stops the
        // ~/.acord/notes directory from accumulating `{uuid}.md` phantoms.
        //
        // Explicit saves (Cmd+S → `saveNote`) skip this gate, so a user
        // who genuinely wants to keep a note with only an empty table
        // can still force it.
        !AppState.isEffectivelyBlank(documentText)
    }

    /// Shared blank-detection used by both the autosave gate and (via its
    /// `static` form) the browser's `(empty note)` preview label. A note
    /// is "blank" when, after the `<!-- acord-archive … -->` sidecar is
    /// stripped, nothing remains except whitespace or default empty-table
    /// scaffolding (all-empty cells or the `Header N` placeholder row).
    static func isEffectivelyBlank(_ text: String) -> Bool {
        let body = stripSidecarArchive(text)
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
            if cells.allSatisfy({ !$0.isEmpty && $0.allSatisfy { "-:".contains($0) } }) {
                return false
            }
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

    private static func stripSidecarArchive(_ text: String) -> String {
        guard let marker = text.range(of: "<!-- acord-archive") else { return text }
        return String(text[..<marker.lowerBound])
    }

    private func extractTitle(from text: String) -> String {
        let firstLine = text.components(separatedBy: "\n").first?
            .trimmingCharacters(in: .whitespaces) ?? ""
        let clean = firstLine.replacingOccurrences(
            of: "^#+\\s*", with: "", options: .regularExpression
        )
        return clean.isEmpty ? "Untitled" : String(clean.prefix(60))
    }

    private func sanitizeFilename(_ name: String) -> String {
        let illegal = CharacterSet(charactersIn: "/\\:*?\"<>|")
        let parts = name.unicodeScalars.filter { !illegal.contains($0) }
        let cleaned = String(String.UnicodeScalarView(parts))
            .trimmingCharacters(in: .whitespaces)
        return cleaned.isEmpty ? UUID().uuidString : cleaned
    }

    /// Resolve the autosave file URL for `noteID`. First call for a noteID
    /// derives a filename from the title (or the UUID when there's no title);
    /// the resulting path is then locked in for the rest of the session, so
    /// later keystrokes can't spawn a fresh file each time the title grows.
    /// Must be called on the main thread (mutates `autoSavePaths`).
    private func resolveAutoSaveURL(noteID: UUID, text: String) -> URL {
        if let url = autoSavePaths[noteID] {
            return url
        }
        let dirURL = URL(fileURLWithPath: ConfigManager.shared.autoSaveDirectory)
        try? FileManager.default.createDirectory(at: dirURL, withIntermediateDirectories: true)
        let title = extractTitle(from: text)
        let filename: String
        if title == "Untitled" {
            filename = noteID.uuidString.lowercased()
        } else {
            filename = sanitizeFilename(title)
        }
        let url = dirURL.appendingPathComponent(filename + ".md")
        autoSavePaths[noteID] = url
        return url
    }

    /// Background-safe atomic write. No path resolution here — the URL was
    /// resolved on the main thread before dispatch.
    private static func writeAutoSaveFile(at url: URL, text: String) {
        try? text.write(to: url, atomically: true, encoding: .utf8)
    }

    /// Strip the `<!-- acord-archive ... -->` sidecar comment from `text`.
    /// The markdown body before the comment is the user's actual content;
    /// non-markdown destinations (.rs, .json, .csv-source, etc.) must not
    /// inherit the comment because it isn't valid syntax in those formats.
    private static func stripArchiveForExternalSave(_ text: String) -> String {
        var body = stripSidecarArchive(text)
        // `stripSidecarArchive` keeps trailing whitespace — trim so we don't
        // leave a flapping blank line where the comment used to be.
        while body.hasSuffix("\n\n") {
            body.removeLast()
        }
        return body
    }

    // MARK: - Note operations

    func newNote() {
        saveCurrentIfNeeded()
        cleanupBlankNote(currentNoteID)
        let id = bridge.newDocument()
        currentNoteID = id
        selectedNoteIDs = [id]
        documentText = "# "
        evalResults = [:]
        modified = false
        currentFileURL = nil
        currentFileFormat = .markdown
        refreshNoteList()
    }

    func selectNote(_ id: UUID, extend: Bool = false, range: Bool = false) {
        if range, let anchor = selectedNoteIDs.first {
            guard let anchorIdx = noteList.firstIndex(where: { $0.id == anchor }),
                  let targetIdx = noteList.firstIndex(where: { $0.id == id }) else {
                selectedNoteIDs = [id]
                return
            }
            let lo = min(anchorIdx, targetIdx)
            let hi = max(anchorIdx, targetIdx)
            selectedNoteIDs = Set(noteList[lo...hi].map(\.id))
        } else if extend {
            if selectedNoteIDs.contains(id) {
                selectedNoteIDs.remove(id)
            } else {
                selectedNoteIDs.insert(id)
            }
        } else {
            selectedNoteIDs = [id]
        }
    }

    func openNote(_ id: UUID) {
        saveCurrentIfNeeded()
        cleanupBlankNote(currentNoteID)
        if bridge.cacheLoad(id) {
            currentNoteID = id
            selectedNoteIDs = [id]
            documentText = bridge.getText(id)
            modified = false
            evaluate()
        }
    }

    func loadNote(_ id: UUID) {
        openNote(id)
    }

    func saveNote() {
        bridge.setText(currentNoteID, text: documentText)
        if let url = currentFileURL {
            let textToSave = textForExternalSave(format: currentFileFormat)
            try? textToSave.write(to: url, atomically: true, encoding: .utf8)
        }
        let _ = bridge.cacheSave(currentNoteID)
        modified = false
        refreshNoteList()
    }

    func saveNoteToFile(_ url: URL) {
        let format = FileFormat.from(filename: url.lastPathComponent)
        let textToSave = textForExternalSave(format: format)
        try? textToSave.write(to: url, atomically: true, encoding: .utf8)
        currentFileURL = url
        currentFileFormat = format
        // An explicit save-to-disk locks the autosave path to the same file
        // for the rest of the session — keystrokes after Save As shouldn't
        // start a fresh autosave file under the old name.
        if format.isMarkdown {
            autoSavePaths[currentNoteID] = url
        }
        modified = false
    }

    /// Project the in-memory `documentText` onto the right shape for an
    /// external file format. CSV gets converted from the markdown table,
    /// non-markdown formats get the sidecar archive comment stripped (the
    /// HTML comment isn't valid in .rs/.json/etc.), markdown passes through.
    private func textForExternalSave(format: FileFormat) -> String {
        if format.isCSV { return markdownTableToCSV(documentText) }
        if format.isMarkdown { return documentText }
        return AppState.stripArchiveForExternalSave(documentText)
    }

    func loadNoteFromFile(_ url: URL) {
        let format = FileFormat.from(filename: url.lastPathComponent)
        if let (id, text) = bridge.loadNote(path: url.path) {
            currentNoteID = id
            currentFileURL = url
            currentFileFormat = format
            if format.isCSV {
                documentText = csvToMarkdownTable(text)
            } else {
                documentText = text
            }
            // Lock the autosave path to the loaded file when it lives in the
            // notes dir. Outside that dir, the user picked their own path —
            // we won't shadow it with an autosave duplicate.
            let dir = URL(fileURLWithPath: ConfigManager.shared.autoSaveDirectory)
                .standardizedFileURL
            let parent = url.deletingLastPathComponent().standardizedFileURL
            if format.isMarkdown && parent == dir {
                autoSavePaths[id] = url
            }
            modified = false
            let _ = bridge.cacheSave(id)
            evaluate()
            refreshNoteList()
        }
    }

    // MARK: - CSV conversion

    private func csvToMarkdownTable(_ csv: String) -> String {
        let rows = parseCSVRows(csv)
        guard let header = rows.first, !header.isEmpty else { return csv }

        var lines: [String] = []
        lines.append("| " + header.joined(separator: " | ") + " |")
        lines.append("| " + header.map { _ in "---" }.joined(separator: " | ") + " |")
        for row in rows.dropFirst() {
            var cells = row
            while cells.count < header.count { cells.append("") }
            lines.append("| " + cells.prefix(header.count).joined(separator: " | ") + " |")
        }
        return lines.joined(separator: "\n")
    }

    private func markdownTableToCSV(_ markdown: String) -> String {
        let lines = markdown.components(separatedBy: "\n").filter { !$0.isEmpty }
        var csvRows: [String] = []

        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard trimmed.hasPrefix("|") else { continue }
            if isTableSeparatorLine(trimmed) { continue }
            let cells = extractTableCells(trimmed)
            csvRows.append(cells.map { escapeCSVField($0) }.joined(separator: ","))
        }

        return csvRows.joined(separator: "\n") + "\n"
    }

    private func parseCSVRows(_ csv: String) -> [[String]] {
        var rows: [[String]] = []
        var current: [String] = []
        var field = ""
        var inQuotes = false
        let chars = Array(csv)
        var i = 0

        while i < chars.count {
            let ch = chars[i]
            if inQuotes {
                if ch == "\"" {
                    if i + 1 < chars.count && chars[i + 1] == "\"" {
                        field.append("\"")
                        i += 2
                        continue
                    }
                    inQuotes = false
                } else {
                    field.append(ch)
                }
            } else {
                if ch == "\"" {
                    inQuotes = true
                } else if ch == "," {
                    current.append(field.trimmingCharacters(in: .whitespaces))
                    field = ""
                } else if ch == "\n" || ch == "\r" {
                    current.append(field.trimmingCharacters(in: .whitespaces))
                    field = ""
                    if !current.isEmpty {
                        rows.append(current)
                    }
                    current = []
                    if ch == "\r" && i + 1 < chars.count && chars[i + 1] == "\n" {
                        i += 1
                    }
                } else {
                    field.append(ch)
                }
            }
            i += 1
        }

        if !field.isEmpty || !current.isEmpty {
            current.append(field.trimmingCharacters(in: .whitespaces))
            rows.append(current)
        }

        return rows
    }

    private func isTableSeparatorLine(_ line: String) -> Bool {
        let stripped = line.replacingOccurrences(of: " ", with: "")
        return stripped.allSatisfy { "|:-".contains($0) } && stripped.contains("-")
    }

    private func extractTableCells(_ line: String) -> [String] {
        var trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("|") { trimmed = String(trimmed.dropFirst()) }
        if trimmed.hasSuffix("|") { trimmed = String(trimmed.dropLast()) }
        return trimmed.components(separatedBy: "|").map { $0.trimmingCharacters(in: .whitespaces) }
    }

    private func escapeCSVField(_ field: String) -> String {
        if field.contains(",") || field.contains("\"") || field.contains("\n") {
            return "\"" + field.replacingOccurrences(of: "\"", with: "\"\"") + "\""
        }
        return field
    }

    func deleteNote(_ id: UUID) {
        bridge.deleteNote(id)
        if let url = autoSavePaths.removeValue(forKey: id) {
            try? FileManager.default.removeItem(at: url)
        }
        if id == currentNoteID {
            newNote()
        }
        refreshNoteList()
    }

    func deleteNotes(_ ids: Set<UUID>) {
        for id in ids {
            bridge.deleteNote(id)
            if let url = autoSavePaths.removeValue(forKey: id) {
                try? FileManager.default.removeItem(at: url)
            }
        }
        if ids.contains(currentNoteID) {
            let remaining = noteList.first { !ids.contains($0.id) }
            if let next = remaining {
                currentNoteID = next.id
                if bridge.cacheLoad(next.id) {
                    documentText = bridge.getText(next.id)
                }
            } else {
                let id = bridge.newDocument()
                currentNoteID = id
                documentText = ""
            }
            evalResults = [:]
            modified = false
        }
        refreshNoteList()
    }

    func evaluate() {
        evalResults = bridge.evaluate(currentNoteID)
    }

    /// Write a caller-provided text snapshot to the notes directory,
    /// bypassing the `documentText` pipeline entirely. Used by the
    /// AppDelegate's 100ms autosave timer, which reads text directly
    /// from the viewport — routing through `documentText.didSet` would
    /// trip the Combine → `vp.setText` round-trip and wipe viewport
    /// state (including visible eval results).
    func writeAutosavedCopy(text: String) {
        let noteID = currentNoteID
        let url = resolveAutoSaveURL(noteID: noteID, text: text)
        autoSaveQueue.async {
            Self.writeAutoSaveFile(at: url, text: text)
        }
    }

    func refreshNoteList() {
        var notes = bridge.listNotes()
        notes.removeAll { note in
            let trimmed = note.title.trimmingCharacters(in: .whitespacesAndNewlines)
            let isBlank = trimmed.isEmpty || trimmed == "Untitled"
            return isBlank && note.id != currentNoteID
        }
        noteList = notes
    }

    private func saveCurrentIfNeeded() {
        if modified {
            saveNote()
        }
    }

    private func cleanupBlankNote(_ id: UUID) {
        let text = bridge.getText(id)
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            bridge.deleteNote(id)
            if let url = autoSavePaths.removeValue(forKey: id) {
                try? FileManager.default.removeItem(at: url)
            }
        }
    }
}

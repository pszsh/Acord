import Cocoa
import Combine
import SwiftUI
import UniformTypeIdentifiers

extension Notification.Name {
    static let focusEditor = Notification.Name("focusEditor")
    static let focusTitle = Notification.Name("focusTitle")
}

class WindowController {
    let window: NSWindow
    let appState: AppState
    init(window: NSWindow, appState: AppState) {
        self.window = window
        self.appState = appState
    }
}

class AppDelegate: NSObject, NSApplicationDelegate, NSMenuItemValidation {
    var window: NSWindow!
    var appState: AppState!
    private var titleCancellable: AnyCancellable?
    private var textCancellable: AnyCancellable?
    private var titleBarView: TitleBarView?
    private var focusTitleObserver: NSObjectProtocol?
    private var windowControllers: [WindowController] = []
    /// Writes the viewport's current text to the notes directory on a
    /// tight interval. Deliberately bypasses `appState.documentText` — the
    /// Combine sink on that property pushes text back into the viewport
    /// via `vp.setText`, which rebuilds viewport state and clears the
    /// eval overlay. By writing straight to disk, autosave can't disturb
    /// what the user sees.
    private var autosaveTimer: Timer?

    private var viewport: IcedViewportView? {
        window?.contentView as? IcedViewportView
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = ConfigManager.shared
        appState = AppState()

        let viewport = IcedViewportView(frame: NSRect(x: 0, y: 0, width: 1200, height: 800))
        viewport.autoresizingMask = [.width, .height]

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.backgroundColor = Theme.current.base
        window.title = "Acord"
        window.contentView = viewport
        window.center()
        window.setFrameAutosaveName("AcordMainWindow")
        window.makeKeyAndOrderFront(nil)

        applyThemeAppearance()
        setupTitleBar()
        setupMenuBar()
        observeDocumentTitle()

        observeDocumentText()
        syncThemeToViewport()
        startAutosaveTimer()

        DocumentBrowserController.shared = DocumentBrowserController(appState: appState)

        NotificationCenter.default.addObserver(
            self, selector: #selector(settingsDidChange),
            name: .settingsChanged, object: nil
        )

        if let url = pendingOpenURLs.first {
            pendingOpenURLs = []
            appState.loadNoteFromFile(url)
        }
    }

    private var pendingOpenURLs: [URL] = []

    func application(_ application: NSApplication, open urls: [URL]) {
        guard let url = urls.first else { return }
        if appState != nil {
            appState.loadNoteFromFile(url)
        } else {
            pendingOpenURLs = [url]
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }

    // Runs before AppKit tears the window down. We must front-run the window
    // teardown so the Rust-backed viewport releases its wgpu/Metal resources
    // while the NSView + CAMetalLayer it holds raw pointers to are still
    // alive. `applicationWillTerminate` is too late: by the time that fires,
    // AppKit has already started deallocating the window/contentView graph
    // and the delegate can no longer safely read `self.window`.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        // Pull out any unsaved text before tearing down. `getText` refreshes
        // the viewport's own `cachedText`, so later reads during teardown
        // can fall back to it if the handle is already gone.
        syncTextFromViewport()
        appState.saveNote()

        // Explicit, ordered teardown of every viewport we own, while the
        // views + window graph are still fully alive.
        if let vp = viewport {
            vp.teardown()
        }
        for controller in windowControllers {
            if let vp = controller.window.contentView as? IcedViewportView {
                vp.teardown()
            }
        }

        // Drop strong refs so AppKit doesn't try to replay anything through
        // the delegate during its own terminate phases.
        titleCancellable = nil
        textCancellable = nil
        if let observer = focusTitleObserver {
            NotificationCenter.default.removeObserver(observer)
            focusTitleObserver = nil
        }
        NotificationCenter.default.removeObserver(self)

        return .terminateNow
    }

    // MARK: - Menu bar

    private func setupMenuBar() {
        let mainMenu = NSMenu()

        mainMenu.addItem(buildAppMenu())
        mainMenu.addItem(buildFileMenu())
        mainMenu.addItem(buildEditMenu())
        mainMenu.addItem(buildRenderMenu())
        mainMenu.addItem(buildViewMenu())
        mainMenu.addItem(buildWindowMenu())

        NSApp.mainMenu = mainMenu
    }

    private func buildAppMenu() -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu()
        menu.addItem(withTitle: "About Acord", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        menu.addItem(.separator())
        let settingsItem = NSMenuItem(title: "Settings...", action: #selector(openSettings), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit Acord", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        item.submenu = menu
        return item
    }

    private func buildFileMenu() -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "File")

        let newWindowItem = NSMenuItem(title: "New Window", action: #selector(newWindow), keyEquivalent: "n")
        newWindowItem.target = self
        menu.addItem(newWindowItem)

        let newNoteItem = NSMenuItem(title: "New Note", action: #selector(newNote), keyEquivalent: "N")
        newNoteItem.keyEquivalentModifierMask = [.command, .shift]
        newNoteItem.target = self
        menu.addItem(newNoteItem)

        let openItem = NSMenuItem(title: "Open...", action: #selector(openNote), keyEquivalent: "o")
        openItem.target = self
        menu.addItem(openItem)

        menu.addItem(.separator())

        let saveItem = NSMenuItem(title: "Save", action: #selector(saveNote), keyEquivalent: "s")
        saveItem.target = self
        menu.addItem(saveItem)

        let saveAsItem = NSMenuItem(title: "Save As...", action: #selector(saveNoteAs), keyEquivalent: "S")
        saveAsItem.target = self
        menu.addItem(saveAsItem)

        menu.addItem(.separator())

        let exportCrateItem = NSMenuItem(title: "Export as Rust Library...", action: #selector(exportCrate), keyEquivalent: "E")
        exportCrateItem.keyEquivalentModifierMask = [.command, .shift]
        exportCrateItem.target = self
        menu.addItem(exportCrateItem)

        menu.addItem(.separator())

        let openStorageItem = NSMenuItem(title: "Open Storage Directory", action: #selector(openStorageDirectory), keyEquivalent: "")
        openStorageItem.target = self
        menu.addItem(openStorageItem)

        item.submenu = menu
        return item
    }

    private func buildEditMenu() -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "Edit")
        menu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        menu.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
        menu.addItem(.separator())
        menu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        menu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        menu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        menu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        menu.addItem(.separator())

        let boldItem = NSMenuItem(title: "Bold", action: #selector(boldSelection), keyEquivalent: "b")
        boldItem.target = self
        menu.addItem(boldItem)

        let italicItem = NSMenuItem(title: "Italic", action: #selector(italicizeSelection), keyEquivalent: "i")
        italicItem.target = self
        menu.addItem(italicItem)

        menu.addItem(.separator())

        let tableItem = NSMenuItem(title: "Insert Table", action: #selector(insertTable), keyEquivalent: "t")
        tableItem.target = self
        menu.addItem(tableItem)

        let evalItem = NSMenuItem(title: "Smart Eval", action: #selector(smartEval), keyEquivalent: "e")
        evalItem.target = self
        menu.addItem(evalItem)

        menu.addItem(.separator())

        let findItem = NSMenuItem(title: "Find...", action: #selector(NSTextView.performFindPanelAction(_:)), keyEquivalent: "f")
        findItem.tag = Int(NSTextFinder.Action.showFindInterface.rawValue)
        menu.addItem(findItem)

        menu.addItem(.separator())

        let formatItem = NSMenuItem(title: "Format Document", action: #selector(formatDocument), keyEquivalent: "F")
        formatItem.keyEquivalentModifierMask = [.command, .shift]
        formatItem.target = self
        menu.addItem(formatItem)

        item.submenu = menu
        return item
    }

    private func buildRenderMenu() -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "Render")

        let modesHeader = NSMenuItem(title: "Modes", action: nil, keyEquivalent: "")
        modesHeader.isEnabled = false
        menu.addItem(modesHeader)

        let liveItem = NSMenuItem(title: "Live", action: #selector(setLiveMode), keyEquivalent: "")
        liveItem.target = self
        menu.addItem(liveItem)

        let editorItem = NSMenuItem(title: "Editor", action: #selector(setEditorMode), keyEquivalent: "")
        editorItem.target = self
        menu.addItem(editorItem)

        let viewItem = NSMenuItem(title: "View", action: #selector(setViewMode), keyEquivalent: "")
        viewItem.target = self
        menu.addItem(viewItem)

        item.submenu = menu
        return item
    }

    private func buildViewMenu() -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "View")
        let toggleItem = NSMenuItem(title: "Document Browser", action: #selector(toggleBrowser), keyEquivalent: "b")
        toggleItem.keyEquivalentModifierMask = .control
        toggleItem.target = self
        menu.addItem(toggleItem)

        menu.addItem(.separator())

        let zoomInItem = NSMenuItem(title: "Zoom In", action: #selector(zoomIn), keyEquivalent: "=")
        zoomInItem.target = self
        menu.addItem(zoomInItem)

        let zoomOutItem = NSMenuItem(title: "Zoom Out", action: #selector(zoomOut), keyEquivalent: "-")
        zoomOutItem.target = self
        menu.addItem(zoomOutItem)

        let actualSizeItem = NSMenuItem(title: "Actual Size", action: #selector(zoomReset), keyEquivalent: "0")
        actualSizeItem.target = self
        menu.addItem(actualSizeItem)

        item.submenu = menu
        return item
    }

    private func buildWindowMenu() -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "Window")
        menu.addItem(withTitle: "Minimize", action: #selector(NSWindow.miniaturize(_:)), keyEquivalent: "m")
        menu.addItem(withTitle: "Zoom", action: #selector(NSWindow.zoom(_:)), keyEquivalent: "")
        item.submenu = menu
        NSApp.windowsMenu = menu
        return item
    }

    // MARK: - Actions

    @objc private func newNote() {
        appState.newNote()
    }

    @objc private func newWindow() {
        let state = AppState()
        let viewport = IcedViewportView(frame: NSRect(x: 0, y: 0, width: 1200, height: 800))
        viewport.autoresizingMask = [.width, .height]

        let win = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        win.isReleasedWhenClosed = false
        win.titlebarAppearsTransparent = true
        win.titleVisibility = .hidden
        win.backgroundColor = Theme.current.base
        win.title = "Acord"
        win.contentView = viewport
        win.center()
        win.makeKeyAndOrderFront(nil)

        let controller = WindowController(window: win, appState: state)
        windowControllers.append(controller)
    }

    @objc private func openStorageDirectory() {
        let dir = ConfigManager.shared.autoSaveDirectory
        let url = URL(fileURLWithPath: dir, isDirectory: true)
        NSWorkspace.shared.open(url)
    }

    @objc private func boldSelection() {
        viewport?.sendCommand(1)
    }

    @objc private func italicizeSelection() {
        viewport?.sendCommand(2)
    }

    @objc private func insertTable() {
        viewport?.sendCommand(3)
    }

    @objc private func smartEval() {
        viewport?.sendCommand(4)
    }

    @objc private func openNote() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = Self.supportedContentTypes
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.appState.loadNoteFromFile(url)
        }
    }

    @objc private func saveNote() {
        syncTextFromViewport()
        if appState.currentFileURL != nil {
            appState.saveNote()
        } else {
            saveNoteAs()
        }
    }

    @objc private func saveNoteAs() {
        syncTextFromViewport()
        let panel = NSSavePanel()
        panel.allowedContentTypes = Self.supportedContentTypes
        panel.nameFieldStringValue = defaultFilename()
        if let url = appState.currentFileURL {
            panel.directoryURL = url.deletingLastPathComponent()
            panel.nameFieldStringValue = url.lastPathComponent
        }
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.appState.saveNoteToFile(url)
        }
    }

    @objc private func exportCrate() {
        syncTextFromViewport()
        guard let w = window, let vp = w.contentView as? IcedViewportView,
              let handle = vp.viewportHandle else { return }

        let panel = NSSavePanel()
        panel.title = "Export as Rust Library"
        panel.message = "Choose a location and name for your exported crate"
        panel.prompt = "Export"
        panel.nameFieldLabel = "Crate name:"
        panel.nameFieldStringValue = defaultCrateName()
        panel.canCreateDirectories = true

        panel.beginSheetModal(for: w) { response in
            guard response == .OK, let url = panel.url else { return }
            let parentDir = url.deletingLastPathComponent().path
            let name = url.lastPathComponent
            parentDir.withCString { pd in
                name.withCString { n in
                    if let cstr = viewport_export_crate(handle, pd, n) {
                        let resultPath = String(cString: cstr)
                        viewport_free_string(cstr)
                        self.notifyExportComplete(at: resultPath)
                    } else {
                        self.notifyExportFailed()
                    }
                }
            }
        }
    }

    private func defaultCrateName() -> String {
        let firstLine = appState.documentText
            .components(separatedBy: "\n").first?
            .trimmingCharacters(in: .whitespaces) ?? ""
        let stripped = firstLine.replacingOccurrences(
            of: "^#+\\s*", with: "", options: .regularExpression
        )
        let words = stripped.split(separator: " ").prefix(2).joined(separator: "-")
        let sanitized = words.lowercased()
            .map { $0.isLetter || $0.isNumber || $0 == "-" ? String($0) : "" }.joined()
        return sanitized.isEmpty ? "my-note" : sanitized
    }

    private func notifyExportComplete(at path: String) {
        let alert = NSAlert()
        alert.messageText = "Export complete"
        alert.informativeText = "Crate written to:\n\(path)\n\nCheck the README for build and install instructions."
        alert.addButton(withTitle: "Reveal in Finder")
        alert.addButton(withTitle: "OK")
        if alert.runModal() == .alertFirstButtonReturn {
            NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
        }
    }

    private func notifyExportFailed() {
        let alert = NSAlert()
        alert.messageText = "Export failed"
        alert.informativeText = "Could not export the note. Check the folder permissions and that the crate name doesn't collide with an existing folder."
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    private func defaultFilename() -> String {
        if let url = appState.currentFileURL {
            return url.lastPathComponent
        }
        let firstLine = appState.documentText
            .components(separatedBy: "\n").first?
            .trimmingCharacters(in: .whitespaces) ?? ""
        let stripped = firstLine.replacingOccurrences(
            of: "^#+\\s*", with: "", options: .regularExpression
        )
        let trimmed = stripped.trimmingCharacters(in: .whitespaces)
        let ext = extensionForFormat(appState.currentFileFormat)
        guard !trimmed.isEmpty, trimmed != "Untitled" else { return "note.\(ext)" }
        let sanitized = trimmed.map { "/:\\\\".contains($0) ? "-" : String($0) }.joined()
        return sanitized.prefix(80) + ".\(ext)"
    }

    private func extensionForFormat(_ format: FileFormat) -> String {
        switch format {
        case .markdown: return "md"
        case .csv: return "csv"
        case .json: return "json"
        case .toml: return "toml"
        case .yaml: return "yaml"
        case .xml: return "xml"
        case .svg: return "svg"
        case .rust: return "rs"
        case .c: return "c"
        case .cpp: return "cpp"
        case .objc: return "m"
        case .javascript: return "js"
        case .typescript: return "ts"
        case .jsx: return "jsx"
        case .tsx: return "tsx"
        case .html: return "html"
        case .css: return "css"
        case .scss: return "scss"
        case .less: return "less"
        case .python: return "py"
        case .go: return "go"
        case .ruby: return "rb"
        case .php: return "php"
        case .lua: return "lua"
        case .shell: return "sh"
        case .java: return "java"
        case .kotlin: return "kt"
        case .swift: return "swift"
        case .zig: return "zig"
        case .sql: return "sql"
        case .makefile: return "mk"
        case .dockerfile: return "Dockerfile"
        case .config: return "conf"
        case .lock: return "lock"
        case .plainText, .unknown: return "txt"
        }
    }

    private static let supportedContentTypes: [UTType] = {
        let extensions = [
            "md", "markdown", "mdown",
            "csv", "json", "toml", "yaml", "yml", "xml", "svg",
            "rs", "c", "cpp", "cc", "cxx", "h", "hpp", "hxx",
            "js", "jsx", "ts", "tsx",
            "html", "htm", "css", "scss", "less",
            "py", "go", "rb", "php", "lua",
            "sh", "bash", "zsh", "fish",
            "java", "kt", "kts", "swift", "zig", "sql",
            "mk", "ini", "cfg", "conf", "env",
            "lock", "txt", "text", "log"
        ]
        var types: [UTType] = [.plainText]
        for ext in extensions {
            if let t = UTType(filenameExtension: ext) {
                types.append(t)
            }
        }
        return Array(Set(types))
    }()

    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        let mode = viewport?.renderMode() ?? 0
        switch menuItem.action {
        case #selector(setLiveMode):
            menuItem.state = mode == 0 ? .on : .off
        case #selector(setEditorMode):
            menuItem.state = mode == 1 ? .on : .off
        case #selector(setViewMode):
            menuItem.state = mode == 2 ? .on : .off
        default:
            break
        }
        return true
    }

    @objc private func setLiveMode() {
        viewport?.sendCommand(11)
    }

    @objc private func setEditorMode() {
        viewport?.sendCommand(12)
    }

    @objc private func setViewMode() {
        viewport?.sendCommand(13)
    }

    @objc private func formatDocument() {
        viewport?.sendCommand(10)
    }

    @objc private func openSettings() {
        SettingsWindowController.show()
    }

    @objc private func settingsDidChange() {
        window.backgroundColor = Theme.current.base
        syncThemeToViewport()
        window.contentView?.needsDisplay = true
    }

    private func syncThemeToViewport() {
        let mode = ConfigManager.shared.themeMode
        let name: String
        switch mode {
        case "dark": name = "kicad"
        case "light": name = "latte"
        default:
            let appearance = NSApp.effectiveAppearance
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            name = isDark ? "kicad" : "latte"
        }
        viewport?.setTheme(name)
    }

    @objc private func toggleBrowser() {
        DocumentBrowserController.shared?.toggle()
    }

    @objc private func zoomIn() {
        if let browser = DocumentBrowserController.shared, browser.window.isKeyWindow {
            browser.browserState.scaleUp()
            return
        }
        ConfigManager.shared.zoomLevel += 1
        NotificationCenter.default.post(name: .settingsChanged, object: nil)
    }

    @objc private func zoomOut() {
        if let browser = DocumentBrowserController.shared, browser.window.isKeyWindow {
            browser.browserState.scaleDown()
            return
        }
        let current = ConfigManager.shared.zoomLevel
        if 11 + current > 8 {
            ConfigManager.shared.zoomLevel -= 1
            NotificationCenter.default.post(name: .settingsChanged, object: nil)
        }
    }

    @objc private func zoomReset() {
        ConfigManager.shared.zoomLevel = 0
        NotificationCenter.default.post(name: .settingsChanged, object: nil)
    }

    private func setupTitleBar() {
        let accessory = TitleBarAccessoryController()
        window.addTitlebarAccessoryViewController(accessory)

        let tbv = accessory.titleView
        tbv.onCommit = { [weak self] rawTitle in
            guard let self = self else { return }
            // Only drop the document's first line if it actually IS a title
            // (starts with `#`). Normalize whatever the user typed in the
            // title bar to a `# ` prefix so the saved markdown is valid.
            let trimmed = rawTitle.trimmingCharacters(in: .whitespaces)
            let normalizedTitle: String
            if trimmed.isEmpty {
                normalizedTitle = ""
            } else if trimmed.hasPrefix("#") {
                normalizedTitle = trimmed
            } else {
                normalizedTitle = "# " + trimmed
            }

            let lines = self.appState.documentText.components(separatedBy: "\n")
            let firstIsTitle = lines.first
                .map { $0.trimmingCharacters(in: .whitespaces).hasPrefix("#") }
                ?? false
            let body: [String] = firstIsTitle ? Array(lines.dropFirst()) : lines

            let newLines: [String]
            if normalizedTitle.isEmpty {
                newLines = body
            } else {
                newLines = [normalizedTitle] + body
            }
            self.appState.documentText = newLines.joined(separator: "\n")
        }

        titleBarView = tbv

        focusTitleObserver = NotificationCenter.default.addObserver(
            forName: .focusTitle, object: nil, queue: .main
        ) { [weak self] _ in
            self?.titleBarView?.beginEditing()
        }
    }

    private func observeDocumentText() {
        textCancellable = appState.$documentText
            .receive(on: RunLoop.main)
            .sink { [weak self] text in
                guard let self = self, let vp = self.viewport else { return }
                // Idempotent: when the sync timer pulls text FROM the
                // viewport and assigns it to `documentText`, this sink
                // fires again and would push the identical text back in —
                // and `vp.setText` rebuilds viewport state, clearing eval
                // results. Skip the round-trip when vp already has it.
                if vp.getText() == text { return }
                vp.setText(text)
            }
    }

    private func syncTextFromViewport() {
        guard let w = window, let vp = w.contentView as? IcedViewportView else { return }
        let text = vp.getText()
        if !text.isEmpty || appState.documentText.isEmpty {
            appState.documentText = text
        }
    }

    /// 100ms autosave loop. Reads straight from the viewport and writes a
    /// file in the notes directory — no Combine publishers, no `setText`,
    /// no viewport-state rebuilds. The existing explicit flows (Cmd+S,
    /// note switch, quit) still route through `syncTextFromViewport` so
    /// `appState.documentText` stays current when Swift actually needs it.
    private func startAutosaveTimer() {
        autosaveTimer?.invalidate()
        autosaveTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            self?.persistViewportToNotesDir()
        }
    }

    private func persistViewportToNotesDir() {
        guard let w = window, let vp = w.contentView as? IcedViewportView else { return }
        let text = vp.getText()
        guard !AppState.isEffectivelyBlank(text) else { return }
        appState.writeAutosavedCopy(text: text)
    }

    private func observeDocumentTitle() {
        titleCancellable = appState.$documentText
            .receive(on: RunLoop.main)
            .sink { [weak self] text in
                guard let self = self else { return }
                let firstLine = text.components(separatedBy: "\n").first?
                    .trimmingCharacters(in: .whitespaces) ?? ""
                let clean = firstLine.replacingOccurrences(
                    of: "^#+\\s*", with: "", options: .regularExpression
                )
                let displayTitle = clean.isEmpty ? "Acord" : String(clean.prefix(60))
                self.window.title = displayTitle
                self.titleBarView?.title = firstLine
            }
    }
}

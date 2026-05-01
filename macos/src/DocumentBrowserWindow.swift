import Cocoa

class DocumentBrowserController {
    static var shared: DocumentBrowserController?

    let window: NSWindow
    private let view: IcedBrowserView
    private let appState: AppState

    init(appState: AppState) {
        self.appState = appState
        let dir = ConfigManager.shared.autoSaveDirectory
        let frame = NSRect(x: 0, y: 0, width: 900, height: 650)
        view = IcedBrowserView(frame: frame, notesDir: dir)
        view.autoresizingMask = [.width, .height]

        window = NSWindow(
            contentRect: frame,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Documents"
        window.backgroundColor = Theme.current.base
        window.contentView = view
        window.setFrameAutosaveName("AcordBrowser")
        window.center()
        window.isReleasedWhenClosed = false

        view.onOpenPath = { [weak self] path in
            guard let self = self else { return }
            let url = URL(fileURLWithPath: path)
            DispatchQueue.main.async {
                self.appState.loadNoteFromFile(url)
                self.window.orderOut(nil)
            }
        }
    }

    func toggle() {
        if window.isVisible {
            window.orderOut(nil)
        } else {
            view.refresh()
            window.makeKeyAndOrderFront(nil)
        }
    }

    /// true while the browser window is the focused window.
    var isKeyWindow: Bool { window.isKeyWindow }

    /// forwards a numeric command to the embedded browser view.
    func sendCommand(_ command: UInt32) {
        view.sendCommand(command)
    }
}

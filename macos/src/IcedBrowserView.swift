import AppKit

class IcedBrowserView: NSView {
    private(set) var browserHandle: OpaquePointer?
    private var displayLink: CVDisplayLink?
    private var isTornDown = false
    private let notesDir: String
    var onOpenPath: ((String) -> Void)?

    init(frame: NSRect, notesDir: String) {
        self.notesDir = notesDir
        super.init(frame: frame)
        wantsLayer = true
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not used") }

    override var isFlipped: Bool { true }
    override var wantsUpdateLayer: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window != nil && browserHandle == nil && !isTornDown {
            createBrowser()
            startDisplayLink()
            window?.makeFirstResponder(self)
        } else if window == nil {
            teardown()
        }
    }

    private func createBrowser() {
        let scale = Float(window?.backingScaleFactor ?? 2.0)
        let w = Float(bounds.width)
        let h = Float(bounds.height)
        let nsviewPtr = Unmanaged.passUnretained(self).toOpaque()
        notesDir.withCString { cstr in
            browserHandle = browser_create(nsviewPtr, w, h, scale, cstr)
        }
    }

    func teardown() {
        if isTornDown { return }
        isTornDown = true
        stopDisplayLink()
        if let h = browserHandle {
            browser_destroy(h)
            browserHandle = nil
        }
    }

    deinit { teardown() }

    private func startDisplayLink() {
        guard displayLink == nil else { return }
        var link: CVDisplayLink?
        CVDisplayLinkCreateWithActiveCGDisplays(&link)
        guard let link = link else { return }
        let selfPtr = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        CVDisplayLinkSetOutputCallback(link, { _, _, _, _, _, userInfo -> CVReturn in
            guard let userInfo = userInfo else { return kCVReturnSuccess }
            let view = Unmanaged<IcedBrowserView>.fromOpaque(userInfo).takeUnretainedValue()
            DispatchQueue.main.async { view.renderFrame() }
            return kCVReturnSuccess
        }, selfPtr)
        CVDisplayLinkStart(link)
        displayLink = link
    }

    private func stopDisplayLink() {
        guard let link = displayLink else { return }
        CVDisplayLinkStop(link)
        displayLink = nil
    }

    private func renderFrame() {
        if isTornDown { return }
        guard let h = browserHandle else { return }
        browser_render(h)
        if let cstr = browser_take_pending_open(h) {
            let path = String(cString: cstr)
            viewport_free_string(cstr)
            onOpenPath?(path)
        }
    }

    func refresh() {
        guard let h = browserHandle else { return }
        browser_refresh(h)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        guard let h = browserHandle else { return }
        let scale = Float(window?.backingScaleFactor ?? 2.0)
        browser_resize(h, Float(bounds.width), Float(bounds.height), scale)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        guard let h = browserHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        browser_mouse_event(h, Float(pt.x), Float(pt.y), 0, true)
    }

    override func mouseUp(with event: NSEvent) {
        guard let h = browserHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        browser_mouse_event(h, Float(pt.x), Float(pt.y), 0, false)
    }

    override func mouseMoved(with event: NSEvent) {
        guard let h = browserHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        browser_mouse_event(h, Float(pt.x), Float(pt.y), 255, false)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let h = browserHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        browser_mouse_event(h, Float(pt.x), Float(pt.y), 255, false)
    }

    override func scrollWheel(with event: NSEvent) {
        guard let h = browserHandle else { return }
        browser_scroll_event(h, Float(event.scrollingDeltaX), Float(event.scrollingDeltaY))
    }

    override func keyDown(with event: NSEvent) {
        guard let h = browserHandle else { return }
        let text = event.characters ?? ""
        text.withCString { cstr in
            browser_key_event(h, UInt32(event.keyCode), UInt32(event.modifierFlags.rawValue), true, cstr)
        }
    }

    override func keyUp(with event: NSEvent) {
        guard let h = browserHandle else { return }
        let text = event.characters ?? ""
        text.withCString { cstr in
            browser_key_event(h, UInt32(event.keyCode), UInt32(event.modifierFlags.rawValue), false, cstr)
        }
    }

    override func flagsChanged(with event: NSEvent) {
        guard let h = browserHandle else { return }
        browser_key_event(h, UInt32(event.keyCode), UInt32(event.modifierFlags.rawValue), true, nil)
    }
}

import AppKit
import SwiftUI

class IcedViewportView: NSView {
    private(set) var viewportHandle: OpaquePointer?
    private var displayLink: CVDisplayLink?
    private var isTornDown = false
    // Last text pulled out of Rust. Refreshed on every edit tick via the
    // render loop, so terminate/save paths can read a current-enough value
    // without touching the viewport once teardown has begun.
    private var cachedText: String = ""

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
    }

    override var isFlipped: Bool { true }
    override var wantsUpdateLayer: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    // MARK: - Lifecycle

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window != nil && viewportHandle == nil && !isTornDown {
            createViewport()
            startDisplayLink()
            window?.makeFirstResponder(self)
        } else if window == nil {
            teardown()
        }
    }

    private func createViewport() {
        let scale = Float(window?.backingScaleFactor ?? 2.0)
        let w = Float(bounds.width)
        let h = Float(bounds.height)
        let nsviewPtr = Unmanaged.passUnretained(self).toOpaque()
        viewportHandle = viewport_create(nsviewPtr, w, h, scale)
    }

    private func destroyViewport() {
        guard let handle = viewportHandle else { return }
        viewportHandle = nil
        viewport_destroy(handle)
    }

    /// Ordered shutdown: stop the display link first (joins in-flight CV
    /// callbacks), then snapshot the text into `cachedText`, then drop the
    /// Rust handle. Idempotent — safe to call from both the terminate hook
    /// and `deinit`.
    func teardown() {
        if isTornDown { return }
        isTornDown = true
        stopDisplayLink()
        if let h = viewportHandle, let cstr = viewport_get_text(h) {
            cachedText = String(cString: cstr)
            viewport_free_string(cstr)
        }
        destroyViewport()
    }

    deinit {
        teardown()
    }

    // MARK: - Display Link

    private func startDisplayLink() {
        guard displayLink == nil else { return }
        var link: CVDisplayLink?
        CVDisplayLinkCreateWithActiveCGDisplays(&link)
        guard let link = link else { return }

        let selfPtr = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        CVDisplayLinkSetOutputCallback(link, { _, _, _, _, _, userInfo -> CVReturn in
            guard let userInfo = userInfo else { return kCVReturnSuccess }
            let view = Unmanaged<IcedViewportView>.fromOpaque(userInfo).takeUnretainedValue()
            DispatchQueue.main.async {
                view.renderFrame()
            }
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
        guard let handle = viewportHandle else { return }
        viewport_render(handle)
    }

    // MARK: - Resize

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        resizeViewport()
    }

    override func setBoundsSize(_ newSize: NSSize) {
        super.setBoundsSize(newSize)
        resizeViewport()
    }

    private func resizeViewport() {
        guard let handle = viewportHandle else { return }
        let scale = Float(window?.backingScaleFactor ?? 2.0)
        viewport_resize(handle, Float(bounds.width), Float(bounds.height), scale)
    }

    // MARK: - Mouse Events

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        guard let h = viewportHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        viewport_mouse_event(h, Float(pt.x), Float(pt.y), 0, true)
    }

    override func mouseUp(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        viewport_mouse_event(h, Float(pt.x), Float(pt.y), 0, false)
    }

    override func mouseMoved(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        viewport_mouse_event(h, Float(pt.x), Float(pt.y), 255, false)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        // Use the 255 sentinel — pointer move only, no button event. mouseDown
        // already fired ButtonPressed; sending another one per drag tick would
        // restart iced's selection on every frame and make click+drag twitch.
        viewport_mouse_event(h, Float(pt.x), Float(pt.y), 255, false)
    }

    override func rightMouseDown(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        viewport_mouse_event(h, Float(pt.x), Float(pt.y), 1, true)
    }

    override func rightMouseUp(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        viewport_mouse_event(h, Float(pt.x), Float(pt.y), 1, false)
    }

    override func scrollWheel(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let pt = convert(event.locationInWindow, from: nil)
        viewport_scroll_event(h, Float(pt.x), Float(pt.y), Float(event.scrollingDeltaX), Float(event.scrollingDeltaY))
    }

    // MARK: - Key Events

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard viewportHandle != nil else { return super.performKeyEquivalent(with: event) }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let cmd = flags.contains(.command)
        let shift = flags.contains(.shift)
        let chars = event.charactersIgnoringModifiers ?? ""

        if cmd && !shift {
            switch chars {
            case "a", "b", "c", "e", "f", "g", "i", "r", "v", "x", "z", "p", "t",
                 "=", "+", "-", "0":
                keyDown(with: event)
                return true
            default: break
            }
        }
        if cmd && shift {
            switch chars {
            case "g", "z":
                keyDown(with: event)
                return true
            default: break
            }
        }
        return super.performKeyEquivalent(with: event)
    }

    override func keyDown(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let text = event.characters ?? ""
        text.withCString { cstr in
            viewport_key_event(h, UInt32(event.keyCode), UInt32(event.modifierFlags.rawValue), true, cstr)
        }
    }

    override func keyUp(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        let text = event.characters ?? ""
        text.withCString { cstr in
            viewport_key_event(h, UInt32(event.keyCode), UInt32(event.modifierFlags.rawValue), false, cstr)
        }
    }

    override func flagsChanged(with event: NSEvent) {
        guard let h = viewportHandle else { return }
        viewport_key_event(h, UInt32(event.keyCode), UInt32(event.modifierFlags.rawValue), true, nil)
    }

    // MARK: - Text Bridge

    func setText(_ text: String) {
        if isTornDown { return }
        guard let h = viewportHandle else { return }
        cachedText = text
        text.withCString { cstr in
            viewport_set_text(h, cstr)
        }
    }

    func getText() -> String {
        if isTornDown { return cachedText }
        guard let h = viewportHandle else { return cachedText }
        guard let cstr = viewport_get_text(h) else { return cachedText }
        let result = String(cString: cstr)
        viewport_free_string(cstr)
        cachedText = result
        return result
    }

    func sendCommand(_ command: UInt32) {
        guard let h = viewportHandle else { return }
        viewport_send_command(h, command)
    }

    func setTheme(_ name: String) {
        guard let h = viewportHandle else { return }
        name.withCString { cstr in
            viewport_set_theme(h, cstr)
        }
    }

    func setLineIndicator(_ mode: String) {
        guard let h = viewportHandle else { return }
        mode.withCString { cstr in
            viewport_set_line_indicator(h, cstr)
        }
    }

    func setGutterRainbow(_ enabled: Bool) {
        guard let h = viewportHandle else { return }
        viewport_set_gutter_rainbow(h, enabled)
    }

    func setAutoPairFlags(_ flags: UInt32) {
        guard let h = viewportHandle else { return }
        viewport_set_auto_pair_flags(h, flags)
    }

    /// Returns 0 = Live, 1 = Editor, 2 = View.
    func renderMode() -> UInt32 {
        guard let h = viewportHandle else { return 0 }
        return viewport_render_mode(h)
    }

    func setSettingsView(themeMode: String, lineIndicator: String, gutterRainbow: Bool, autoSaveDir: String) {
        guard let h = viewportHandle else { return }
        themeMode.withCString { t in
            lineIndicator.withCString { l in
                autoSaveDir.withCString { d in
                    viewport_set_settings_view(h, t, l, gutterRainbow, d)
                }
            }
        }
    }

    func takeShellAction() -> String? {
        guard let h = viewportHandle else { return nil }
        guard let cstr = viewport_take_shell_action(h) else { return nil }
        let s = String(cString: cstr)
        viewport_free_string(cstr)
        return s
    }
}

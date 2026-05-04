import UIKit
import QuartzCore

/// CAMetalLayer-backed UIView that owns the Rust viewport handle and pumps
/// CADisplayLink ticks into `viewport_render`.
class IcedViewportView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    private(set) var viewportHandle: OpaquePointer?
    private var displayLink: CADisplayLink?
    private var isTornDown = false
    private var cachedText: String = ""

    override init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        commonInit()
    }

    private func commonInit() {
        backgroundColor = .black
        isMultipleTouchEnabled = false
        if let metalLayer = layer as? CAMetalLayer {
            metalLayer.contentsScale = UIScreen.main.scale
            metalLayer.framebufferOnly = true
            metalLayer.pixelFormat = .bgra8Unorm
            metalLayer.isOpaque = true
        }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil && viewportHandle == nil && !isTornDown {
            createViewport()
            startDisplayLink()
            becomeFirstResponder()
        } else if window == nil {
            teardown()
        }
    }

    override var canBecomeFirstResponder: Bool { true }

    private func createViewport() {
        let scale = Float(window?.screen.scale ?? UIScreen.main.scale)
        let w = Float(bounds.width)
        let h = Float(bounds.height)
        let viewPtr = Unmanaged.passUnretained(self).toOpaque()
        viewportHandle = viewport_create(viewPtr, w, h, scale)
    }

    private func destroyViewport() {
        guard let handle = viewportHandle else { return }
        viewportHandle = nil
        viewport_destroy(handle)
    }

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

    deinit { teardown() }

    // MARK: - Display link

    private func startDisplayLink() {
        guard displayLink == nil else { return }
        let link = CADisplayLink(target: self, selector: #selector(renderFrame))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    private func stopDisplayLink() {
        displayLink?.invalidate()
        displayLink = nil
    }

    @objc private func renderFrame() {
        if isTornDown { return }
        guard let handle = viewportHandle else { return }
        viewport_render(handle)
    }

    // MARK: - Resize

    override func layoutSubviews() {
        super.layoutSubviews()
        let scale = Float(window?.screen.scale ?? UIScreen.main.scale)
        if let metalLayer = layer as? CAMetalLayer {
            metalLayer.contentsScale = CGFloat(scale)
            metalLayer.drawableSize = CGSize(
                width: bounds.width * CGFloat(scale),
                height: bounds.height * CGFloat(scale)
            )
        }
        guard let handle = viewportHandle else { return }
        viewport_resize(handle, Float(bounds.width), Float(bounds.height), scale)
    }

    // MARK: - Touches

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let h = viewportHandle, let touch = touches.first else { return }
        let p = touch.location(in: self)
        viewport_mouse_event(h, Float(p.x), Float(p.y), 0, true)
        becomeFirstResponder()
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let h = viewportHandle, let touch = touches.first else { return }
        let p = touch.location(in: self)
        viewport_mouse_event(h, Float(p.x), Float(p.y), 255, false)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let h = viewportHandle, let touch = touches.first else { return }
        let p = touch.location(in: self)
        viewport_mouse_event(h, Float(p.x), Float(p.y), 0, false)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let h = viewportHandle, let touch = touches.first else { return }
        let p = touch.location(in: self)
        viewport_mouse_event(h, Float(p.x), Float(p.y), 0, false)
    }

    // MARK: - Hardware keyboard (Magic Keyboard / Smart Keyboard)

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard let h = viewportHandle else {
            super.pressesBegan(presses, with: event)
            return
        }
        for press in presses {
            forwardKey(press, pressed: true, handle: h)
        }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard let h = viewportHandle else {
            super.pressesEnded(presses, with: event)
            return
        }
        for press in presses {
            forwardKey(press, pressed: false, handle: h)
        }
    }

    private func forwardKey(_ press: UIPress, pressed: Bool, handle: OpaquePointer) {
        guard let key = press.key else { return }
        let chars = pressed ? key.characters : ""
        chars.withCString { cstr in
            viewport_key_event(handle, UInt32(key.keyCode.rawValue), UInt32(key.modifierFlags.rawValue), pressed, cstr)
        }
    }
}

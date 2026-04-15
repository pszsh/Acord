import Cocoa

class TitleBarAccessoryController: NSTitlebarAccessoryViewController {
    let titleView = TitleBarView()

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 28))
        titleView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(titleView)
        NSLayoutConstraint.activate([
            titleView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            titleView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            titleView.topAnchor.constraint(equalTo: container.topAnchor),
            titleView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        self.view = container
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        layoutAttribute = .top
        fullScreenMinHeight = 28
    }
}

class TitleBarView: NSView {
    private let label = NSTextField(labelWithString: "")
    private let editor = NSTextField()
    private(set) var isEditing = false

    var title: String = "" {
        didSet {
            if !isEditing {
                let dt = displayTitle
                label.stringValue = dt.isEmpty ? "Untitled" : dt
                label.textColor = dt.isEmpty ? Theme.current.overlay0 : Theme.current.text
            }
        }
    }

    var onCommit: ((String) -> Void)?

    private var displayTitle: String {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("# ") {
            return String(trimmed.dropFirst(2))
        }
        return trimmed
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    private func setup() {
        label.font = .systemFont(ofSize: 13, weight: .semibold)
        label.textColor = Theme.current.overlay0
        label.backgroundColor = .clear
        label.isBezeled = false
        label.isEditable = false
        label.isSelectable = false
        label.alignment = .center
        label.lineBreakMode = .byTruncatingTail
        label.cell?.truncatesLastVisibleLine = true
        label.translatesAutoresizingMaskIntoConstraints = false
        label.stringValue = "Untitled"

        editor.font = .systemFont(ofSize: 13, weight: .semibold)
        editor.textColor = Theme.current.text
        editor.backgroundColor = Theme.current.surface0
        editor.isBezeled = false
        editor.isEditable = true
        editor.isSelectable = true
        editor.alignment = .center
        editor.focusRingType = .none
        editor.cell?.lineBreakMode = .byTruncatingTail
        editor.translatesAutoresizingMaskIntoConstraints = false
        editor.isHidden = true
        editor.delegate = self

        addSubview(label)
        addSubview(editor)

        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            label.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, multiplier: 0.5),

            editor.centerXAnchor.constraint(equalTo: centerXAnchor),
            editor.centerYAnchor.constraint(equalTo: centerYAnchor),
            editor.widthAnchor.constraint(equalTo: widthAnchor, multiplier: 0.5),
        ])

        let dblClick = NSClickGestureRecognizer(target: self, action: #selector(handleDoubleClick(_:)))
        dblClick.numberOfClicksRequired = 2
        dblClick.delaysPrimaryMouseButtonEvents = false
        addGestureRecognizer(dblClick)
    }

    @objc private func handleDoubleClick(_ sender: NSClickGestureRecognizer) {
        if !isEditing { beginEditing() }
    }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 && !isEditing {
            beginEditing()
            return
        }
        super.mouseDown(with: event)
    }

    func beginEditing() {
        isEditing = true
        editor.stringValue = title
        label.isHidden = true
        editor.isHidden = false
        window?.makeFirstResponder(editor)
        editor.currentEditor()?.selectAll(nil)
    }

    func endEditing() {
        guard isEditing else { return }
        isEditing = false
        let raw = editor.stringValue
        onCommit?(raw)
        let dt = displayTitle
        label.stringValue = dt.isEmpty ? "Untitled" : dt
        label.textColor = dt.isEmpty ? Theme.current.overlay0 : Theme.current.text
        editor.isHidden = true
        label.isHidden = false
    }

    func updateColors() {
        let dt = displayTitle
        label.textColor = dt.isEmpty ? Theme.current.overlay0 : Theme.current.text
        editor.textColor = Theme.current.text
        editor.backgroundColor = Theme.current.surface0
    }
}

extension TitleBarView: NSTextFieldDelegate {
    func controlTextDidEndEditing(_ obj: Notification) {
        endEditing()
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        if sel == #selector(NSResponder.insertNewline(_:)) {
            endEditing()
            NotificationCenter.default.post(name: .focusEditor, object: nil)
            return true
        }
        if sel == #selector(NSResponder.cancelOperation(_:)) {
            endEditing()
            return true
        }
        return false
    }
}

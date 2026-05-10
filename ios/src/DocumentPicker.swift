import UIKit
import UniformTypeIdentifiers

/// Bridges UIDocumentPickerViewController into the Rust viewport.
/// Open and Save flows rely on iOS's per-file permission grant — a
/// security-scoped URL is what the picker hands back, and we copy bytes in
/// or out under `startAccessingSecurityScopedResource` while it's in scope.
enum DocumentPicker {
    private static var openDelegate: OpenDelegate?
    private static var saveDelegate: SaveDelegate?

    static func presentOpen(handle: OpaquePointer) {
        dlog("presentOpen called")
        guard let root = topViewController() else {
            dlog("presentOpen: topViewController returned nil — picker NOT shown")
            return
        }
        // .item is the broadest "any file" UTI — without this, files whose UTI
        // doesn't exactly match get rendered grey/unselectable in the picker.
        // asCopy:true sidesteps the security-scoped-resource entitlement dance:
        // iOS hands us a copy in our sandbox tmp dir we can just read.
        var types: [UTType] = [.plainText, .utf8PlainText, .text, .sourceCode, .data, .item]
        if let md = UTType(filenameExtension: "md") { types.insert(md, at: 0) }
        if let md = UTType("net.daringfireball.markdown") { types.insert(md, at: 0) }
        dlog("presentOpen: types=\(types.map(\.identifier))")
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: true)
        let delegate = OpenDelegate(handle: handle)
        openDelegate = delegate
        picker.delegate = delegate
        picker.allowsMultipleSelection = false
        root.present(picker, animated: true) {
            dlog("presentOpen: picker presented from \(type(of: root))")
        }
    }

    static func presentSave(handle: OpaquePointer, defaultName: String) {
        dlog("presentSave called")
        guard let root = topViewController() else {
            dlog("presentSave: topViewController returned nil — picker NOT shown")
            return
        }

        guard let cstr = viewport_get_text(handle) else {
            dlog("presentSave: viewport_get_text returned null")
            return
        }
        let text = String(cString: cstr)
        viewport_free_string(cstr)
        dlog("presentSave: serialized \(text.utf8.count) bytes from viewport")

        let tmp = FileManager.default.temporaryDirectory.appendingPathComponent("\(defaultName).md")
        do {
            try text.data(using: .utf8)?.write(to: tmp)
            dlog("presentSave: wrote tmp \(tmp.path)")
        } catch {
            dlog("presentSave: tmp write failed: \(error)")
            return
        }

        let picker = UIDocumentPickerViewController(forExporting: [tmp], asCopy: true)
        let delegate = SaveDelegate(handle: handle, source: tmp)
        saveDelegate = delegate
        picker.delegate = delegate
        root.present(picker, animated: true) {
            dlog("presentSave: picker presented")
        }
    }

    private static func topViewController() -> UIViewController? {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            dlog("topViewController: no UIWindowScene")
            return nil
        }
        guard let window = scene.windows.first(where: { $0.isKeyWindow }) ?? scene.windows.first else {
            dlog("topViewController: no window in scene")
            return nil
        }
        var top = window.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        return top
    }
}

private final class OpenDelegate: NSObject, UIDocumentPickerDelegate {
    let handle: OpaquePointer
    init(handle: OpaquePointer) { self.handle = handle }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        dlog("open delegate fired with \(urls.count) urls")
        guard let url = urls.first else {
            dlog("open: no url in selection")
            return
        }
        dlog("open: url=\(url.path)")
        // asCopy:true means url is already in our sandbox tmp dir — no scoped access needed.
        do {
            let data = try Data(contentsOf: url)
            guard let text = String(data: data, encoding: .utf8) else {
                dlog("open: file at \(url.path) is not utf-8 (\(data.count) bytes)")
                return
            }
            text.withCString { cstr in
                viewport_set_text(handle, cstr)
            }
            dlog("open: loaded \(data.count) bytes (\(text.count) chars) from \(url.lastPathComponent)")
        } catch {
            dlog("open: read failed: \(error)")
        }
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        dlog("open: cancelled by user")
    }
}

private final class SaveDelegate: NSObject, UIDocumentPickerDelegate {
    let handle: OpaquePointer
    let source: URL
    init(handle: OpaquePointer, source: URL) {
        self.handle = handle
        self.source = source
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        dlog("save: picker resolved with destinations=\(urls.map(\.path))")
        try? FileManager.default.removeItem(at: source)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        dlog("save: cancelled by user")
        try? FileManager.default.removeItem(at: source)
    }
}

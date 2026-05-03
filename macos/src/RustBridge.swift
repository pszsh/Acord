import Foundation

struct NoteInfo: Identifiable {
    let id: UUID
    var title: String
    var lastModified: Date
}

enum EvalFormat: String {
    case inline
    case table
    case tree
}

struct EvalEntry {
    let result: String
    let format: EvalFormat
}

class RustBridge {
    static let shared = RustBridge()

    private var docs: [UUID: OpaquePointer] = [:]

    private init() {}

    func newDocument() -> UUID {
        let ptr = acord_doc_new()!
        let uuidStr = cacheSaveRaw(ptr)
        let id = UUID(uuidString: uuidStr) ?? UUID()
        docs[id] = ptr
        return id
    }

    func freeDocument(_ id: UUID) {
        guard let ptr = docs.removeValue(forKey: id) else { return }
        acord_doc_free(ptr)
    }

    func setText(_ id: UUID, text: String) {
        guard let ptr = docs[id] else { return }
        text.withCString { cstr in
            acord_doc_set_text(ptr, cstr)
        }
    }

    func getText(_ id: UUID) -> String {
        guard let ptr = docs[id] else { return "" }
        guard let cstr = acord_doc_get_text(ptr) else { return "" }
        let str = String(cString: cstr)
        acord_free_string(cstr)
        return str
    }

    func evaluate(_ id: UUID) -> [Int: EvalEntry] {
        guard let ptr = docs[id] else { return [:] }
        guard let cstr = acord_doc_evaluate(ptr) else { return [:] }
        let json = String(cString: cstr)
        acord_free_string(cstr)
        return parseEvalJSON(json)
    }

    func evaluateLine(_ line: String) -> String {
        guard let cstr = line.withCString({ acord_eval_line($0) }) else { return "" }
        let str = String(cString: cstr)
        acord_free_string(cstr)
        return str
    }

    func saveNote(_ id: UUID, path: String) -> Bool {
        guard let ptr = docs[id] else { return false }
        return path.withCString { cstr in
            acord_doc_save(ptr, cstr)
        }
    }

    func loadNote(path: String) -> (UUID, String)? {
        guard let ptr = path.withCString({ acord_doc_load($0) }) else { return nil }
        let uuidStr = cacheSaveRaw(ptr)
        guard let id = UUID(uuidString: uuidStr) else {
            acord_doc_free(ptr)
            return nil
        }
        if let old = docs[id] { acord_doc_free(old) }
        docs[id] = ptr

        guard let cstr = acord_doc_get_text(ptr) else { return (id, "") }
        let text = String(cString: cstr)
        acord_free_string(cstr)
        return (id, text)
    }

    /// installs a doc from already-decoded text — used when the shell read raw bytes
    /// itself (so it could split off the binary archive trailer) and just needs a UUID.
    func installDocument(text: String) -> UUID? {
        guard let ptr = acord_doc_new() else { return nil }
        text.withCString { cstr in
            acord_doc_set_text(ptr, cstr)
        }
        let uuidStr = cacheSaveRaw(ptr)
        guard let id = UUID(uuidString: uuidStr) else {
            acord_doc_free(ptr)
            return nil
        }
        if let old = docs[id] { acord_doc_free(old) }
        docs[id] = ptr
        return id
    }

    func cacheSave(_ id: UUID) -> Bool {
        guard let ptr = docs[id] else { return false }
        guard let cstr = acord_cache_save(ptr) else { return false }
        acord_free_string(cstr)
        return true
    }

    func cacheLoad(_ id: UUID) -> Bool {
        let uuidStr = id.uuidString.lowercased()
        guard let ptr = uuidStr.withCString({ acord_cache_load($0) }) else { return false }
        if let old = docs[id] { acord_doc_free(old) }
        docs[id] = ptr
        return true
    }

    func listNotes() -> [NoteInfo] {
        guard let cstr = acord_list_notes() else { return [] }
        let json = String(cString: cstr)
        acord_free_string(cstr)
        return parseNoteListJSON(json)
    }

    struct HighlightSpan {
        let start: Int
        let end: Int
        let kind: Int
    }

    func highlight(source: String, lang: String) -> [HighlightSpan] {
        guard let cstr = source.withCString({ src in
            lang.withCString({ lng in
                acord_highlight(src, lng)
            })
        }) else { return [] }
        let json = String(cString: cstr)
        acord_free_string(cstr)
        return parseHighlightJSON(json)
    }

    private func parseHighlightJSON(_ json: String) -> [HighlightSpan] {
        guard let data = json.data(using: .utf8) else { return [] }
        guard let arr = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return [] }
        var spans: [HighlightSpan] = []
        for item in arr {
            guard let start = item["start"] as? Int,
                  let end = item["end"] as? Int,
                  let kind = item["kind"] as? Int else { continue }
            spans.append(HighlightSpan(start: start, end: end, kind: kind))
        }
        return spans
    }

    func deleteNote(_ id: UUID) {
        freeDocument(id)
        let cacheDir = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".acord/cache")
        let cacheFile = cacheDir.appendingPathComponent("\(id.uuidString.lowercased()).sw")
        try? FileManager.default.removeItem(at: cacheFile)
    }

    // MARK: - Internal

    private func cacheSaveRaw(_ ptr: OpaquePointer) -> String {
        guard let cstr = acord_cache_save(ptr) else { return UUID().uuidString }
        let str = String(cString: cstr)
        acord_free_string(cstr)
        return str
    }

    private func parseEvalJSON(_ json: String) -> [Int: EvalEntry] {
        guard let data = json.data(using: .utf8) else { return [:] }
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return [:] }
        guard let results = obj["results"] as? [[String: Any]] else { return [:] }
        var dict: [Int: EvalEntry] = [:]
        for item in results {
            if let line = item["line"] as? Int, let result = item["result"] as? String {
                let fmt = EvalFormat(rawValue: item["format"] as? String ?? "inline") ?? .inline
                dict[line] = EvalEntry(result: result, format: fmt)
            }
        }
        return dict
    }

    private func parseNoteListJSON(_ json: String) -> [NoteInfo] {
        guard let data = json.data(using: .utf8) else { return [] }
        guard let arr = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return [] }
        var notes: [NoteInfo] = []
        for item in arr {
            guard let uuidStr = item["uuid"] as? String,
                  let uuid = UUID(uuidString: uuidStr),
                  let title = item["title"] as? String else { continue }
            let modified = item["modified"] as? Double ?? 0
            let date = Date(timeIntervalSince1970: modified)
            notes.append(NoteInfo(id: uuid, title: title, lastModified: date))
        }
        return notes.sorted { $0.lastModified > $1.lastModified }
    }
}

import Foundation

class ConfigManager {
    static let shared = ConfigManager()

    private let configDir: URL
    private let configFile: URL
    private let defaultNotesDir: URL
    private var config: [String: String]

    private init() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        configDir = home.appendingPathComponent(".acord")
        configFile = configDir.appendingPathComponent("config.json")
        defaultNotesDir = configDir.appendingPathComponent("notes")
        config = [:]
        ensureDirectories()
        load()
    }

    private func ensureDirectories() {
        let fm = FileManager.default
        try? fm.createDirectory(at: configDir, withIntermediateDirectories: true)
        try? fm.createDirectory(at: defaultNotesDir, withIntermediateDirectories: true)
    }

    private func load() {
        guard let data = try? Data(contentsOf: configFile),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: String]
        else { return }
        config = dict
    }

    private func save() {
        guard let data = try? JSONSerialization.data(
            withJSONObject: config, options: [.prettyPrinted, .sortedKeys]
        ) else { return }
        try? data.write(to: configFile, options: .atomic)
    }

    var autoSaveDirectory: String {
        get { config["autoSaveDirectory"] ?? defaultNotesDir.path }
        set { config["autoSaveDirectory"] = newValue; save() }
    }

    var themeMode: String {
        get { config["themeMode"] ?? "auto" }
        set { config["themeMode"] = newValue; save() }
    }

    var lineIndicatorMode: String {
        get { config["lineIndicatorMode"] ?? "on" }
        set { config["lineIndicatorMode"] = newValue; save() }
    }

    var zoomLevel: CGFloat {
        get { CGFloat(Double(config["zoomLevel"] ?? "0") ?? 0) }
        set { config["zoomLevel"] = String(Double(newValue)); save() }
    }
}

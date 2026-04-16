import SwiftUI
import Cocoa

enum ThemeMode: String, CaseIterable {
    case auto = "auto"
    case dark = "dark"
    case light = "light"

    var label: String {
        switch self {
        case .auto: return "Auto"
        case .dark: return "Dark"
        case .light: return "Light"
        }
    }
}

enum LineIndicatorMode: String, CaseIterable {
    case on = "on"
    case off = "off"
    case vim = "vim"

    var label: String {
        switch self {
        case .on: return "On"
        case .off: return "Off"
        case .vim: return "Vim"
        }
    }
}

struct SettingsView: View {
    @State private var themeMode: String = ConfigManager.shared.themeMode
    @State private var lineIndicatorMode: String = ConfigManager.shared.lineIndicatorMode
    @State private var gutterRainbow: Bool = ConfigManager.shared.gutterRainbow
    @State private var autoSaveDir: String = ConfigManager.shared.autoSaveDirectory

    var body: some View {
        let palette = Theme.current
        Form {
            Section("Theme") {
                Picker("Mode", selection: $themeMode) {
                    ForEach(ThemeMode.allCases, id: \.rawValue) { mode in
                        Text(mode.label).tag(mode.rawValue)
                    }
                }
                .pickerStyle(.segmented)
            }

            Section("Line Numbers") {
                Picker("Mode", selection: $lineIndicatorMode) {
                    ForEach(LineIndicatorMode.allCases, id: \.rawValue) { mode in
                        Text(mode.label).tag(mode.rawValue)
                    }
                }
                .pickerStyle(.segmented)
                Toggle("Gutter rainbow", isOn: $gutterRainbow)
            }

            Section("Auto-Save") {
                HStack {
                    TextField("Directory", text: $autoSaveDir)
                        .textFieldStyle(.roundedBorder)
                    Button("Choose...") {
                        let panel = NSOpenPanel()
                        panel.canChooseFiles = false
                        panel.canChooseDirectories = true
                        panel.allowsMultipleSelection = false
                        if panel.runModal() == .OK, let url = panel.url {
                            autoSaveDir = url.path
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 400, height: 300)
        .background(Color(ns: palette.base))
        .onChange(of: themeMode) {
            ConfigManager.shared.themeMode = themeMode
            applyThemeAppearance()
            NotificationCenter.default.post(name: .settingsChanged, object: nil)
        }
        .onChange(of: lineIndicatorMode) {
            ConfigManager.shared.lineIndicatorMode = lineIndicatorMode
            NotificationCenter.default.post(name: .settingsChanged, object: nil)
        }
        .onChange(of: gutterRainbow) {
            ConfigManager.shared.gutterRainbow = gutterRainbow
            NotificationCenter.default.post(name: .settingsChanged, object: nil)
        }
        .onChange(of: autoSaveDir) {
            ConfigManager.shared.autoSaveDirectory = autoSaveDir
        }
    }
}

func applyThemeAppearance() {
    let mode = ConfigManager.shared.themeMode
    switch mode {
    case "dark":
        NSApp.appearance = NSAppearance(named: .darkAqua)
    case "light":
        NSApp.appearance = NSAppearance(named: .aqua)
    default:
        NSApp.appearance = nil
    }
}

extension Notification.Name {
    static let settingsChanged = Notification.Name("settingsChanged")
}

class SettingsWindowController {
    private static var window: NSWindow?

    static func show() {
        if let existing = window, existing.isVisible {
            existing.makeKeyAndOrderFront(nil)
            return
        }

        let settingsView = SettingsView()
        let hostingView = NSHostingView(rootView: settingsView)
        hostingView.frame = NSRect(x: 0, y: 0, width: 400, height: 280)

        let w = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 280),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        w.title = "Settings"
        w.contentView = hostingView
        w.center()
        w.isReleasedWhenClosed = false
        w.makeKeyAndOrderFront(nil)
        window = w
    }
}

import Cocoa
import SwiftUI

struct CatppuccinPalette {
    let base: NSColor
    let mantle: NSColor
    let crust: NSColor
    let surface0: NSColor
    let surface1: NSColor
    let surface2: NSColor
    let overlay0: NSColor
    let overlay1: NSColor
    let overlay2: NSColor
    let text: NSColor
    let subtext0: NSColor
    let subtext1: NSColor
    let red: NSColor
    let maroon: NSColor
    let peach: NSColor
    let yellow: NSColor
    let green: NSColor
    let teal: NSColor
    let sky: NSColor
    let sapphire: NSColor
    let blue: NSColor
    let lavender: NSColor
    let mauve: NSColor
    let pink: NSColor
    let flamingo: NSColor
    let rosewater: NSColor
}

struct Theme {
    static let mocha = CatppuccinPalette(
        base:      NSColor(red: 0.118, green: 0.118, blue: 0.180, alpha: 1),
        mantle:    NSColor(red: 0.094, green: 0.094, blue: 0.149, alpha: 1),
        crust:     NSColor(red: 0.071, green: 0.071, blue: 0.118, alpha: 1),
        surface0:  NSColor(red: 0.188, green: 0.188, blue: 0.259, alpha: 1),
        surface1:  NSColor(red: 0.271, green: 0.271, blue: 0.353, alpha: 1),
        surface2:  NSColor(red: 0.353, green: 0.353, blue: 0.439, alpha: 1),
        overlay0:  NSColor(red: 0.427, green: 0.427, blue: 0.522, alpha: 1),
        overlay1:  NSColor(red: 0.506, green: 0.506, blue: 0.600, alpha: 1),
        overlay2:  NSColor(red: 0.584, green: 0.584, blue: 0.682, alpha: 1),
        text:      NSColor(red: 0.804, green: 0.839, blue: 0.957, alpha: 1),
        subtext0:  NSColor(red: 0.651, green: 0.686, blue: 0.820, alpha: 1),
        subtext1:  NSColor(red: 0.725, green: 0.761, blue: 0.886, alpha: 1),
        red:       NSColor(red: 0.953, green: 0.545, blue: 0.659, alpha: 1),
        maroon:    NSColor(red: 0.922, green: 0.600, blue: 0.659, alpha: 1),
        peach:     NSColor(red: 0.980, green: 0.702, blue: 0.529, alpha: 1),
        yellow:    NSColor(red: 0.976, green: 0.886, blue: 0.686, alpha: 1),
        green:     NSColor(red: 0.651, green: 0.890, blue: 0.631, alpha: 1),
        teal:      NSColor(red: 0.596, green: 0.878, blue: 0.816, alpha: 1),
        sky:       NSColor(red: 0.537, green: 0.863, blue: 0.922, alpha: 1),
        sapphire:  NSColor(red: 0.455, green: 0.784, blue: 0.890, alpha: 1),
        blue:      NSColor(red: 0.537, green: 0.706, blue: 0.980, alpha: 1),
        lavender:  NSColor(red: 0.710, green: 0.745, blue: 0.996, alpha: 1),
        mauve:     NSColor(red: 0.796, green: 0.651, blue: 0.969, alpha: 1),
        pink:      NSColor(red: 0.961, green: 0.710, blue: 0.898, alpha: 1),
        flamingo:  NSColor(red: 0.949, green: 0.710, blue: 0.765, alpha: 1),
        rosewater: NSColor(red: 0.961, green: 0.761, blue: 0.765, alpha: 1)
    )

    static let latte = CatppuccinPalette(
        base:      NSColor(red: 0.937, green: 0.929, blue: 0.961, alpha: 1),
        mantle:    NSColor(red: 0.906, green: 0.898, blue: 0.941, alpha: 1),
        crust:     NSColor(red: 0.863, green: 0.855, blue: 0.910, alpha: 1),
        surface0:  NSColor(red: 0.800, green: 0.796, blue: 0.863, alpha: 1),
        surface1:  NSColor(red: 0.737, green: 0.733, blue: 0.816, alpha: 1),
        surface2:  NSColor(red: 0.667, green: 0.663, blue: 0.757, alpha: 1),
        overlay0:  NSColor(red: 0.604, green: 0.596, blue: 0.706, alpha: 1),
        overlay1:  NSColor(red: 0.533, green: 0.529, blue: 0.647, alpha: 1),
        overlay2:  NSColor(red: 0.467, green: 0.463, blue: 0.592, alpha: 1),
        text:      NSColor(red: 0.298, green: 0.286, blue: 0.416, alpha: 1),
        subtext0:  NSColor(red: 0.376, green: 0.365, blue: 0.494, alpha: 1),
        subtext1:  NSColor(red: 0.337, green: 0.325, blue: 0.455, alpha: 1),
        red:       NSColor(red: 0.822, green: 0.294, blue: 0.345, alpha: 1),
        maroon:    NSColor(red: 0.906, green: 0.345, blue: 0.388, alpha: 1),
        peach:     NSColor(red: 0.996, green: 0.541, blue: 0.243, alpha: 1),
        yellow:    NSColor(red: 0.875, green: 0.627, blue: 0.086, alpha: 1),
        green:     NSColor(red: 0.251, green: 0.624, blue: 0.247, alpha: 1),
        teal:      NSColor(red: 0.090, green: 0.604, blue: 0.502, alpha: 1),
        sky:       NSColor(red: 0.016, green: 0.639, blue: 0.757, alpha: 1),
        sapphire:  NSColor(red: 0.125, green: 0.561, blue: 0.737, alpha: 1),
        blue:      NSColor(red: 0.118, green: 0.404, blue: 0.878, alpha: 1),
        lavender:  NSColor(red: 0.451, green: 0.420, blue: 0.878, alpha: 1),
        mauve:     NSColor(red: 0.529, green: 0.329, blue: 0.890, alpha: 1),
        pink:      NSColor(red: 0.918, green: 0.341, blue: 0.604, alpha: 1),
        flamingo:  NSColor(red: 0.867, green: 0.369, blue: 0.424, alpha: 1),
        rosewater: NSColor(red: 0.863, green: 0.443, blue: 0.439, alpha: 1)
    )

    static var current: CatppuccinPalette {
        let mode = ConfigManager.shared.themeMode
        switch mode {
        case "dark": return mocha
        case "light": return latte
        default:
            let appearance = NSApp.effectiveAppearance
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            return isDark ? mocha : latte
        }
    }

    static var editorFont: NSFont {
        NSFont.monospacedSystemFont(ofSize: max(8, 13 + ConfigManager.shared.zoomLevel), weight: .regular)
    }
    static var gutterFont: NSFont {
        NSFont.monospacedSystemFont(ofSize: max(8, 11 + ConfigManager.shared.zoomLevel), weight: .regular)
    }
    static var sidebarFont: NSFont {
        NSFont.systemFont(ofSize: max(8, 13 + ConfigManager.shared.zoomLevel), weight: .regular)
    }
    static var sidebarDateFont: NSFont {
        NSFont.systemFont(ofSize: max(8, 11 + ConfigManager.shared.zoomLevel), weight: .regular)
    }

    struct SyntaxColors {
        let keyword: NSColor
        let number: NSColor
        let string: NSColor
        let comment: NSColor
        let `operator`: NSColor
        let function: NSColor
        let result: NSColor
        let type: NSColor
        let boolean: NSColor
    }

    static var syntax: SyntaxColors {
        let p = current
        return SyntaxColors(
            keyword:  p.mauve,
            number:   p.peach,
            string:   p.green,
            comment:  p.overlay1,
            operator: p.sky,
            function: p.blue,
            result:   p.teal,
            type:     p.yellow,
            boolean:  p.peach
        )
    }
}

extension Color {
    init(ns: NSColor) {
        self.init(nsColor: ns)
    }
}

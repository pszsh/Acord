use iced_wgpu::core::Color;
use std::cell::RefCell;

#[derive(Clone, Copy)]
pub struct Palette {
    pub rosewater: Color,
    pub flamingo: Color,
    pub pink: Color,
    pub mauve: Color,
    pub red: Color,
    pub maroon: Color,
    pub peach: Color,
    pub yellow: Color,
    pub green: Color,
    pub teal: Color,
    pub sky: Color,
    pub sapphire: Color,
    pub blue: Color,
    pub lavender: Color,
    pub text: Color,
    pub subtext1: Color,
    pub subtext0: Color,
    pub overlay2: Color,
    pub overlay1: Color,
    pub overlay0: Color,
    pub surface2: Color,
    pub surface1: Color,
    pub surface0: Color,
    pub base: Color,
    pub mantle: Color,
    pub crust: Color,
}

pub static MOCHA: Palette = Palette {
    rosewater: Color::from_rgb(0.961, 0.878, 0.863),
    flamingo:  Color::from_rgb(0.949, 0.804, 0.804),
    pink:      Color::from_rgb(0.961, 0.761, 0.906),
    mauve:     Color::from_rgb(0.796, 0.651, 0.969),
    red:       Color::from_rgb(0.953, 0.545, 0.659),
    maroon:    Color::from_rgb(0.922, 0.627, 0.675),
    peach:     Color::from_rgb(0.980, 0.702, 0.529),
    yellow:    Color::from_rgb(0.976, 0.886, 0.686),
    green:     Color::from_rgb(0.651, 0.890, 0.631),
    teal:      Color::from_rgb(0.580, 0.886, 0.835),
    sky:       Color::from_rgb(0.537, 0.863, 0.922),
    sapphire:  Color::from_rgb(0.455, 0.780, 0.925),
    blue:      Color::from_rgb(0.537, 0.706, 0.980),
    lavender:  Color::from_rgb(0.706, 0.745, 0.996),
    text:      Color::from_rgb(0.804, 0.839, 0.957),
    subtext1:  Color::from_rgb(0.729, 0.761, 0.871),
    subtext0:  Color::from_rgb(0.651, 0.678, 0.784),
    overlay2:  Color::from_rgb(0.576, 0.600, 0.698),
    overlay1:  Color::from_rgb(0.498, 0.518, 0.612),
    overlay0:  Color::from_rgb(0.424, 0.439, 0.525),
    surface2:  Color::from_rgb(0.345, 0.357, 0.439),
    surface1:  Color::from_rgb(0.271, 0.278, 0.353),
    surface0:  Color::from_rgb(0.192, 0.196, 0.267),
    base:      Color::from_rgb(0.118, 0.118, 0.180),
    mantle:    Color::from_rgb(0.094, 0.094, 0.145),
    crust:     Color::from_rgb(0.067, 0.067, 0.106),
};

/// KiCad-inspired dark — near-black background, saturated accents, high
/// contrast. The signature KiCad schematic-editor feel: vivid greens,
/// bright cyans, punchy reds and yellows on a deep navy base.
pub static KICAD: Palette = Palette {
    rosewater: Color::from_rgb(0.984, 0.639, 0.757),
    flamingo:  Color::from_rgb(0.965, 0.533, 0.404),
    pink:      Color::from_rgb(0.973, 0.345, 0.718),
    mauve:     Color::from_rgb(0.635, 0.282, 0.980),
    red:       Color::from_rgb(0.914, 0.376, 0.376),
    maroon:    Color::from_rgb(0.949, 0.416, 0.584),
    peach:     Color::from_rgb(0.965, 0.533, 0.404),
    yellow:    Color::from_rgb(0.988, 0.831, 0.349),
    green:     Color::from_rgb(0.403, 0.972, 0.534),
    teal:      Color::from_rgb(0.310, 1.000, 0.882),
    sky:       Color::from_rgb(0.403, 0.813, 0.972),
    sapphire:  Color::from_rgb(0.384, 0.635, 0.949),
    blue:      Color::from_rgb(0.337, 0.475, 0.988),
    lavender:  Color::from_rgb(1.000, 0.718, 0.937),
    text:      Color::from_rgb(0.965, 0.954, 0.969),
    subtext1:  Color::from_rgb(0.824, 0.813, 0.852),
    subtext0:  Color::from_rgb(0.679, 0.668, 0.725),
    overlay2:  Color::from_rgb(0.548, 0.545, 0.598),
    overlay1:  Color::from_rgb(0.449, 0.453, 0.499),
    overlay0:  Color::from_rgb(0.361, 0.368, 0.418),
    surface2:  Color::from_rgb(0.133, 0.141, 0.149),
    surface1:  Color::from_rgb(0.122, 0.126, 0.141),
    surface0:  Color::from_rgb(0.102, 0.110, 0.125),
    base:      Color::from_rgb(0.090, 0.094, 0.114),
    mantle:    Color::from_rgb(0.075, 0.078, 0.102),
    crust:     Color::from_rgb(0.059, 0.059, 0.059),
};

pub static LATTE: Palette = Palette {
    rosewater: Color::from_rgb(0.863, 0.541, 0.471),
    flamingo:  Color::from_rgb(0.867, 0.471, 0.471),
    pink:      Color::from_rgb(0.918, 0.463, 0.796),
    mauve:     Color::from_rgb(0.533, 0.224, 0.937),
    red:       Color::from_rgb(0.824, 0.059, 0.224),
    maroon:    Color::from_rgb(0.902, 0.271, 0.325),
    peach:     Color::from_rgb(0.996, 0.392, 0.043),
    yellow:    Color::from_rgb(0.875, 0.557, 0.114),
    green:     Color::from_rgb(0.251, 0.627, 0.169),
    teal:      Color::from_rgb(0.090, 0.573, 0.600),
    sky:       Color::from_rgb(0.016, 0.647, 0.898),
    sapphire:  Color::from_rgb(0.125, 0.624, 0.710),
    blue:      Color::from_rgb(0.118, 0.400, 0.961),
    lavender:  Color::from_rgb(0.447, 0.529, 0.992),
    text:      Color::from_rgb(0.298, 0.310, 0.412),
    subtext1:  Color::from_rgb(0.361, 0.373, 0.467),
    subtext0:  Color::from_rgb(0.424, 0.435, 0.522),
    overlay2:  Color::from_rgb(0.486, 0.498, 0.576),
    overlay1:  Color::from_rgb(0.549, 0.561, 0.631),
    overlay0:  Color::from_rgb(0.612, 0.627, 0.690),
    surface2:  Color::from_rgb(0.675, 0.690, 0.745),
    surface1:  Color::from_rgb(0.737, 0.753, 0.800),
    surface0:  Color::from_rgb(0.800, 0.816, 0.855),
    base:      Color::from_rgb(0.937, 0.945, 0.961),
    mantle:    Color::from_rgb(0.902, 0.914, 0.937),
    crust:     Color::from_rgb(0.863, 0.878, 0.910),
};

thread_local! {
    static CURRENT: RefCell<&'static Palette> = const { RefCell::new(&MOCHA) };
    static IS_DARK: RefCell<bool> = const { RefCell::new(true) };
}

pub fn current() -> &'static Palette {
    CURRENT.with(|c| *c.borrow())
}

pub fn is_dark() -> bool {
    IS_DARK.with(|d| *d.borrow())
}

pub fn set_theme(name: &str) {
    let (pal, dark) = match name {
        "latte" | "light" => (&LATTE, false),
        "kicad" => (&KICAD, true),
        _ => (&KICAD, true),
    };
    CURRENT.with(|c| *c.borrow_mut() = pal);
    IS_DARK.with(|d| *d.borrow_mut() = dark);
}

/// Colors for bordered inline widgets (tables, trees). Shared so both
/// widget types render identical frosted-card surfaces in both themes.
pub struct WidgetSurface {
    pub fill: Color,
    pub border: Color,
    pub header_accent: Color,
    pub body_text: Color,
}

pub fn widget_surface() -> WidgetSurface {
    let p = current();
    // Dark: fill lifts above base (surface0) for a frosted-lighter card.
    // Light: fill recedes below base (mantle) for a frosted-cooler card.
    let fill = if is_dark() { p.surface0 } else { p.mantle };
    WidgetSurface {
        fill,
        border: p.surface2,
        header_accent: p.teal,
        body_text: p.text,
    }
}

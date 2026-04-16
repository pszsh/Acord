//! Perceptually uniform color operations.
//!
//! Wraps Björn Ottosson's OKLab. Used to compensate for the apparent dimming
//! of small glyphs and thin strokes that arises from antialiased coverage
//! blending against a near-black background. The compensation is calibrated
//! by rendered pixel size: smaller -> bigger boost, with a knee at
//! `THRESHOLD_PX`. Hue and chroma are preserved; only L is adjusted.

use crate::palette;
use iced_wgpu::core::Color;

pub const BASE_BOOST: f32 = 0.30;
pub const THRESHOLD_PX: f32 = 6.0;

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn linear_rgb_to_oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    [
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    ]
}

fn oklab_to_linear_rgb([l_, a, b]: [f32; 3]) -> [f32; 3] {
    let l = l_ + 0.3963377774 * a + 0.2158037573 * b;
    let m = l_ - 0.1055613458 * a - 0.0638541728 * b;
    let s = l_ - 0.0894841775 * a - 1.2914855480 * b;
    let l = l * l * l;
    let m = m * m * m;
    let s = s * s * s;
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
}

fn to_oklab(c: Color) -> [f32; 3] {
    linear_rgb_to_oklab([srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b)])
}

fn from_oklab(lab: [f32; 3], alpha: f32) -> Color {
    let [r, g, b] = oklab_to_linear_rgb(lab);
    Color {
        r: linear_to_srgb(r.clamp(0.0, 1.0)),
        g: linear_to_srgb(g.clamp(0.0, 1.0)),
        b: linear_to_srgb(b.clamp(0.0, 1.0)),
        a: alpha,
    }
}

/// Linear-with-knee curve: full boost at `size_px = 0`, zero at and above
/// `THRESHOLD_PX`. Returns the unsigned magnitude — the caller decides the
/// sign (positive = lighten on dark bg, negative = darken on light bg).
pub fn size_boost(size_px: f32) -> f32 {
    (BASE_BOOST * (1.0 - size_px / THRESHOLD_PX)).max(0.0)
}

/// Add `l_delta` to OKLab L while preserving chroma.
pub fn lighten(color: Color, l_delta: f32) -> Color {
    if l_delta == 0.0 {
        return color;
    }
    let mut lab = to_oklab(color);
    lab[0] = (lab[0] + l_delta).clamp(0.0, 1.0);
    from_oklab(lab, color.a)
}

/// Compensate for AA coverage blending: brighten on dark backgrounds
/// (where AA dims), darken on light backgrounds (where AA washes out).
/// Identity when `size_px >= THRESHOLD_PX`.
pub fn lighten_for_size(color: Color, size_px: f32) -> Color {
    let mag = size_boost(size_px);
    if mag == 0.0 {
        return color;
    }
    let delta = if palette::is_dark() { mag } else { -mag };
    lighten(color, delta)
}

/// Hue-rotate a color by 180° in OKLab while preserving lightness and
/// chroma magnitude — produces the perceptual complement (red→cyan,
/// blue→amber, yellow→indigo, green→magenta). Used by the gutter to render
/// "lines above the cursor" as the inverse of the rainbow used below.
pub fn invert_hue(color: Color) -> Color {
    let mut lab = to_oklab(color);
    lab[1] = -lab[1];
    lab[2] = -lab[2];
    from_oklab(lab, color.a)
}

/// Drain chroma toward zero by `t` (0.0 = identity, 1.0 = grey at the same
/// L). Lightness is preserved, so a "faded red" stays as bright as the red
/// it came from — it just stops being red. Used by the gutter rainbow to
/// dissolve into neutral without dimming.
pub fn desaturate(color: Color, t: f32) -> Color {
    let k = 1.0 - t.clamp(0.0, 1.0);
    if k == 1.0 { return color; }
    let mut lab = to_oklab(color);
    lab[1] *= k;
    lab[2] *= k;
    from_oklab(lab, color.a)
}

/// Perceptual interpolation between two colors. `t = 0` returns `a`,
/// `t = 1` returns `b`.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let la = to_oklab(a);
    let lb = to_oklab(b);
    let lab = [
        la[0] + (lb[0] - la[0]) * t,
        la[1] + (lb[1] - la[1]) * t,
        la[2] + (lb[2] - la[2]) * t,
    ];
    from_oklab(lab, a.a + (b.a - a.a) * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn color_eq(a: Color, b: Color, eps: f32) -> bool {
        approx_eq(a.r, b.r, eps)
            && approx_eq(a.g, b.g, eps)
            && approx_eq(a.b, b.b, eps)
            && approx_eq(a.a, b.a, eps)
    }

    fn kicad_palette() -> Vec<Color> {
        let p = &palette::KICAD;
        vec![
            p.rosewater, p.flamingo, p.pink, p.mauve, p.red, p.maroon, p.peach,
            p.yellow, p.green, p.teal, p.sky, p.sapphire, p.blue, p.lavender,
            p.text, p.subtext1, p.subtext0, p.overlay2, p.overlay1, p.overlay0,
            p.surface2, p.surface1, p.surface0, p.base, p.mantle, p.crust,
        ]
    }

    #[test]
    fn roundtrip_kicad() {
        for c in kicad_palette() {
            let lab = to_oklab(c);
            let back = from_oklab(lab, c.a);
            assert!(color_eq(c, back, 1e-3), "roundtrip failed: {:?} -> {:?}", c, back);
        }
    }

    #[test]
    fn size_boost_dark_theme_curve() {
        palette::set_theme("kicad");
        assert!(approx_eq(size_boost(0.0), BASE_BOOST, 1e-6));
        assert!(approx_eq(size_boost(THRESHOLD_PX), 0.0, 1e-6));
        assert!(approx_eq(size_boost(THRESHOLD_PX * 2.0), 0.0, 1e-6));
        assert!(approx_eq(size_boost(THRESHOLD_PX / 2.0), BASE_BOOST / 2.0, 1e-6));
    }

    #[test]
    fn size_boost_ignores_theme() {
        palette::set_theme("latte");
        assert!(approx_eq(size_boost(0.0), BASE_BOOST, 1e-6));
        palette::set_theme("kicad");
        assert!(approx_eq(size_boost(0.0), BASE_BOOST, 1e-6));
    }

    #[test]
    fn lighten_for_size_darkens_on_light() {
        palette::set_theme("latte");
        let c = palette::LATTE.text;
        let out = lighten_for_size(c, 1.0);
        let lab_in = to_oklab(c);
        let lab_out = to_oklab(out);
        assert!(lab_out[0] < lab_in[0], "L should decrease on light theme");
        palette::set_theme("kicad");
    }

    #[test]
    fn lighten_for_size_identity_above_threshold() {
        palette::set_theme("kicad");
        let c = palette::KICAD.red;
        // Above threshold: function short-circuits, returns input verbatim.
        assert_eq!(lighten_for_size(c, THRESHOLD_PX + 1.0), c);
        assert_eq!(lighten_for_size(c, THRESHOLD_PX), c);
    }

    #[test]
    fn lighten_preserves_chroma() {
        // Use a mid-gamut swatch so an L+ bump doesn't clip in sRGB.
        let c = palette::KICAD.overlay1;
        let lab = to_oklab(c);
        let bright = lighten(c, 0.10);
        let lab2 = to_oklab(bright);
        assert!(approx_eq(lab2[0], lab[0] + 0.10, 5e-3), "L: {} vs {}", lab2[0], lab[0] + 0.10);
        assert!(approx_eq(lab2[1], lab[1], 5e-3), "a drift: {} vs {}", lab2[1], lab[1]);
        assert!(approx_eq(lab2[2], lab[2], 5e-3), "b drift: {} vs {}", lab2[2], lab[2]);
    }

    #[test]
    fn mix_endpoints() {
        let a = palette::KICAD.red;
        let b = palette::KICAD.blue;
        assert!(color_eq(mix(a, b, 0.0), a, 1e-3));
        assert!(color_eq(mix(a, b, 1.0), b, 1e-3));
    }
}

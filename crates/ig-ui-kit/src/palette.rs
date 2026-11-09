//! Graph branch colors: maps an `ig-log` color id onto a color.
//!
//! Port of `platform/vcs-log/impl/src/com/intellij/vcs/log/graph/DefaultColorGenerator.kt`, with the
//! `java.awt.Color.RGBtoHSB` / `HSBtoRGB` conversions it relies on.
//!
//! The color id is what `ig_log::data::GraphColorManagerImpl` computes (a Java string hash of the
//! branch name, or [`DEFAULT_COLOR_ID`]). Saturation and brightness come from the theme keys
//! `VersionControl.Log.Graph.saturation` / `.brightness` (upstream defaults 0.4 / 0.65).
//!
//! Deviations: no cache (`colorMap`): the computation is a few float operations and the UI calls it
//! for visible rows only. Colors are returned as `0xRRGGBB`; the default color is resolved by the
//! caller ([`GraphPalette::default_color`]) because it depends on the theme (`JBColor.BLACK` is
//! black in light themes and the label foreground in dark ones).

/// `GraphColorManagerImpl.DEFAULT_COLOR`.
pub const DEFAULT_COLOR_ID: i32 = 0;

/// Branch color generator (`DefaultColorGenerator`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphPalette {
    /// `VersionControl.Log.Graph.saturation`, clamped to `0..=1`.
    pub saturation: f32,
    /// `VersionControl.Log.Graph.brightness`, clamped to `0..=1`.
    pub brightness: f32,
    /// The color of [`DEFAULT_COLOR_ID`] (`JBColor.BLACK`), `0xRRGGBB`.
    pub default_color: u32,
}

impl GraphPalette {
    pub fn new(saturation: f32, brightness: f32, default_color: u32) -> GraphPalette {
        GraphPalette {
            saturation: saturation.clamp(0.0, 1.0),
            brightness: brightness.clamp(0.0, 1.0),
            default_color,
        }
    }

    /// `getColor(colorId)` as `0xRRGGBB`.
    pub fn color(&self, color_id: i32) -> u32 {
        if color_id == DEFAULT_COLOR_ID {
            return self.default_color;
        }
        // `calcColor`: Java int arithmetic wraps on overflow
        let r = color_id.wrapping_mul(200).wrapping_add(30);
        let g = color_id.wrapping_mul(130).wrapping_add(50);
        let b = color_id.wrapping_mul(90).wrapping_add(100);
        let [hue, _, _] = rgb_to_hsb(range_fix(r), range_fix(g), range_fix(b));
        hsb_to_rgb(hue, self.saturation, self.brightness) & 0x00FF_FFFF
    }

    /// The color of [`DEFAULT_COLOR_ID`].
    pub fn default_color(&self) -> u32 {
        self.default_color
    }
}

/// `rangeFix`: `abs(n % 100) + 70` (Java `%` keeps the dividend's sign).
fn range_fix(n: i32) -> i32 {
    (n % 100).abs() + 70
}

/// `java.awt.Color.RGBtoHSB`.
pub fn rgb_to_hsb(r: i32, g: i32, b: i32) -> [f32; 3] {
    let cmax = r.max(g).max(b);
    let cmin = r.min(g).min(b);
    let brightness = cmax as f32 / 255.0;
    let saturation = if cmax != 0 {
        (cmax - cmin) as f32 / cmax as f32
    } else {
        0.0
    };
    let hue = if saturation == 0.0 {
        0.0
    } else {
        let span = (cmax - cmin) as f32;
        let redc = (cmax - r) as f32 / span;
        let greenc = (cmax - g) as f32 / span;
        let bluec = (cmax - b) as f32 / span;
        let mut hue = if r == cmax {
            bluec - greenc
        } else if g == cmax {
            2.0 + redc - bluec
        } else {
            4.0 + greenc - redc
        };
        hue /= 6.0;
        if hue < 0.0 {
            hue += 1.0;
        }
        hue
    };
    [hue, saturation, brightness]
}

/// `java.awt.Color.HSBtoRGB`: `0xFFRRGGBB`.
pub fn hsb_to_rgb(hue: f32, saturation: f32, brightness: f32) -> u32 {
    let to_byte = |v: f32| (v * 255.0 + 0.5) as u32;
    let (r, g, b) = if saturation == 0.0 {
        let v = to_byte(brightness);
        (v, v, v)
    } else {
        let h = (hue - hue.floor()) * 6.0;
        let f = h - h.floor();
        let p = brightness * (1.0 - saturation);
        let q = brightness * (1.0 - saturation * f);
        let t = brightness * (1.0 - saturation * (1.0 - f));
        match h as i32 {
            0 => (to_byte(brightness), to_byte(t), to_byte(p)),
            1 => (to_byte(q), to_byte(brightness), to_byte(p)),
            2 => (to_byte(p), to_byte(brightness), to_byte(t)),
            3 => (to_byte(p), to_byte(q), to_byte(brightness)),
            4 => (to_byte(t), to_byte(p), to_byte(brightness)),
            5 => (to_byte(brightness), to_byte(p), to_byte(q)),
            _ => (0, 0, 0),
        }
    };
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_hsb_round_trip_matches_awt() {
        // Expected values worked out by hand from the JDK algorithm (not run against a JVM).
        assert_eq!(rgb_to_hsb(255, 0, 0), [0.0, 1.0, 1.0]);
        assert_eq!(hsb_to_rgb(0.0, 1.0, 1.0), 0xFFFF0000);
        assert_eq!(hsb_to_rgb(0.5, 0.5, 0.5), 0xFF408080);
        assert_eq!(hsb_to_rgb(0.0, 0.0, 0.65), 0xFFA6A6A6);
        let [h, s, b] = rgb_to_hsb(100, 120, 140);
        assert!((h - 0.583_333_3).abs() < 1e-6); // (4 + 0.5 - 1) / 6
        assert!((s - 0.285_714_3).abs() < 1e-6);
        assert!((b - 0.549_019_6).abs() < 1e-6);
    }

    #[test]
    fn range_fix_keeps_java_sign_semantics() {
        assert_eq!(range_fix(30), 100);
        assert_eq!(range_fix(-130), 100);
        assert_eq!(range_fix(i32::MIN), 118);
    }

    #[test]
    fn default_color_id_uses_theme_color() {
        let palette = GraphPalette::new(0.4, 0.65, 0x123456);
        assert_eq!(palette.color(DEFAULT_COLOR_ID), 0x123456);
    }

    #[test]
    fn colors_use_hue_of_the_id_and_theme_saturation_brightness() {
        let palette = GraphPalette::new(0.4, 0.65, 0);
        // colorId 1: r=230 -> 100, g=180 -> 150, b=190 -> 160; hue of (100,150,160) = 0.52777...
        let [h, _, _] = rgb_to_hsb(100, 150, 160);
        assert_eq!(palette.color(1), hsb_to_rgb(h, 0.4, 0.65) & 0xFFFFFF);
        // every generated color has the requested brightness (max channel = 0.65 * 255)
        for id in [
            1,
            -1,
            7,
            1_234_567,
            i32::MAX,
            i32::MIN + 1,
            "master".len() as i32,
        ] {
            let c = palette.color(id);
            let max = (c >> 16 & 0xFF).max(c >> 8 & 0xFF).max(c & 0xFF);
            assert_eq!(max, 166, "id {id} -> {c:06x}");
        }
    }

    #[test]
    fn saturation_and_brightness_are_clamped() {
        let palette = GraphPalette::new(2.0, -1.0, 0);
        assert_eq!((palette.saturation, palette.brightness), (1.0, 0.0));
        assert_eq!(palette.color(5), 0);
    }
}

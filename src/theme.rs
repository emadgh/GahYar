use crate::settings::{Settings, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ThemeColors {
    pub theme: Theme,
    pub primary: [u8; 3],
    pub accent: [u8; 3],
}

impl ThemeColors {
    pub fn preset(theme: Theme) -> Self {
        let (primary, accent) = match theme {
            Theme::Dark => ([30, 31, 33], [248, 211, 88]),
            Theme::Light => ([231, 233, 236], [230, 181, 43]),
        };
        Self {
            theme,
            primary,
            accent,
        }
    }

    pub fn from_settings(settings: &Settings) -> Self {
        let mut colors = Self::preset(settings.theme);
        colors.primary = settings.primary.unwrap_or(colors.primary);
        colors.accent = settings.accent.unwrap_or(colors.accent);
        colors
    }

    pub fn apply(self, settings: &mut Settings) {
        settings.theme = self.theme;
        settings.primary = Some(self.primary);
        settings.accent = Some(self.accent);
    }

    pub fn is_custom(self) -> bool {
        self != Self::preset(self.theme)
    }

    pub fn title(self) -> &'static str {
        if self.is_custom() {
            "سفارشی"
        } else {
            self.theme.title()
        }
    }
}

pub fn parse_hex(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let number = u32::from_str_radix(value, 16).ok()?;
    Some([(number >> 16) as u8, (number >> 8) as u8, number as u8])
}

pub fn hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

pub fn mix(a: [u8; 3], b: [u8; 3], weight: f32) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f32 * (1.0 - weight) + b[i] as f32 * weight).round() as u8)
}

// Use visible channel differences rather than tiny percentages of near-black/white.
// Move toward the text color when there is contrast headroom, otherwise away.
pub fn surfaces(primary: [u8; 3]) -> [[u8; 3]; 3] {
    let fg = foreground(primary);
    let shades = |pole: [u8; 3]| {
        let distance = (0..3).map(|i| primary[i].abs_diff(pole[i])).max().unwrap() as f32;
        [14.0, 26.0, 5.0].map(|delta| mix(primary, pole, (delta / distance).min(1.0)))
    };
    let toward = shades(fg);
    if toward.iter().all(|bg| contrast(*bg, fg) >= 4.5) {
        toward
    } else {
        shades(if fg == [255; 3] { [0; 3] } else { [255; 3] })
    }
}

fn luminance(color: [u8; 3]) -> f32 {
    let linear = color.map(|v| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

pub fn foreground(background: [u8; 3]) -> [u8; 3] {
    if contrast(background, [0; 3]) >= contrast(background, [255; 3]) {
        [0; 3]
    } else {
        [255; 3]
    }
}

// Preserve the chosen fill; only adapt text drawn on a different background.
pub fn readable(color: [u8; 3], backgrounds: &[[u8; 3]]) -> [u8; 3] {
    let target = foreground(backgrounds[0]);
    for step in 0..=100 {
        let candidate = mix(color, target, step as f32 / 100.0);
        if backgrounds.iter().all(|bg| contrast(candidate, *bg) >= 4.5) {
            return candidate;
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_validation_and_round_trip() {
        for value in ["#000000", "FFFFFF", " #aBc123 "] {
            let color = parse_hex(value).unwrap();
            assert_eq!(parse_hex(&hex(color)), Some(color));
        }
        for value in ["", "#123", "#12345678", "GG0000", "۱۲۳۴۵۶", "12345💛"] {
            assert_eq!(parse_hex(value), None);
        }
    }

    #[test]
    fn legacy_settings_keep_preset_and_customization_resets() {
        for theme in [Theme::Dark, Theme::Light] {
            let mut settings = Settings {
                theme,
                ..Settings::default()
            };
            assert_eq!(
                ThemeColors::from_settings(&settings),
                ThemeColors::preset(theme)
            );
            let custom = ThemeColors {
                primary: [20, 40, 60],
                accent: [0, 100, 250],
                ..ThemeColors::preset(theme)
            };
            custom.apply(&mut settings);
            assert_eq!(ThemeColors::from_settings(&settings), custom);
            assert!(custom.is_custom());
            ThemeColors::preset(theme).apply(&mut settings);
            assert!(!ThemeColors::from_settings(&settings).is_custom());
            let encoded = serde_json::to_string(&custom).unwrap();
            assert_eq!(
                serde_json::from_str::<ThemeColors>(&encoded).unwrap(),
                custom
            );
        }
    }

    #[test]
    fn custom_palette_preserves_fills_and_readable_text() {
        let unpack = |v: u32| [v as u8, (v >> 8) as u8, (v >> 16) as u8];
        for primary in [
            [0; 3],
            [255; 3],
            [117; 3],
            [118; 3],
            [30, 20, 90],
            [240, 200, 160],
        ] {
            for accent in [[0; 3], [255; 3], primary, [255, 0, 255], [0, 255, 0]] {
                let p = crate::Palette::from_colors(ThemeColors {
                    theme: Theme::Dark,
                    primary,
                    accent,
                });
                assert_eq!(unpack(p.background), primary);
                assert_eq!(unpack(p.accent), accent);
                assert!(contrast(unpack(p.accent_text), accent) >= 4.5);
                for background in [
                    p.background,
                    p.surface,
                    p.surface_alt,
                    p.calendar_panel,
                    p.selected,
                ] {
                    for foreground in [p.text, p.muted, p.accent_label, p.holiday, p.event] {
                        assert!(contrast(unpack(background), unpack(foreground)) >= 4.5);
                    }
                }
            }
        }
    }

    #[test]
    fn custom_surfaces_stay_distinct_and_readable_across_rgb_space() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let primary = [r, g, b];
                    let shades = surfaces(primary);
                    let all = [primary, shades[0], shades[1], shades[2]];
                    for (i, a) in all.iter().enumerate() {
                        assert!(contrast(*a, foreground(primary)) >= 4.5);
                        for b in &all[i + 1..] {
                            assert!(a.iter().zip(b).any(|(a, b)| a.abs_diff(*b) >= 5), "{all:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn accent_only_changes_preserve_preset_surfaces() {
        for theme in [Theme::Dark, Theme::Light] {
            let preset = crate::Palette::from_theme(theme);
            let custom = crate::Palette::from_colors(ThemeColors {
                accent: [40, 180, 120],
                ..ThemeColors::preset(theme)
            });
            assert_eq!(custom.background, preset.background);
            assert_eq!(custom.surface, preset.surface);
            assert_eq!(custom.surface_alt, preset.surface_alt);
            assert_eq!(custom.calendar_panel, preset.calendar_panel);
        }
    }

    #[test]
    fn foreground_is_readable_across_rgb_space() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let bg = [r, g, b];
                    assert!(contrast(bg, foreground(bg)) >= 4.5);
                }
            }
        }
    }
}

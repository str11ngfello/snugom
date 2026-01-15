use colored::Color;
use once_cell::sync::Lazy;

/// CLI color theme configuration
pub struct ColorTheme {
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub highlight: Color,
    pub muted: Color,
    pub primary: Color,
    pub secondary: Color,
    #[allow(dead_code)]
    pub header: Color,
    pub key: Color,
    pub value: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Blue,
            highlight: Color::Cyan,
            muted: Color::BrightBlack,
            primary: Color::BrightBlue,
            secondary: Color::Magenta,
            header: Color::BrightWhite,
            key: Color::BrightCyan,
            value: Color::White,
        }
    }
}

/// Global theme instance
pub static THEME: Lazy<ColorTheme> = Lazy::new(ColorTheme::default);

/// Icons for different message types
#[allow(dead_code)]
pub struct Icons {
    pub success: &'static str,
    pub error: &'static str,
    pub warning: &'static str,
    pub info: &'static str,
    pub arrow: &'static str,
    pub bullet: &'static str,
    pub loading: &'static str,
    pub plus: &'static str,
    pub minus: &'static str,
    pub check: &'static str,
    pub cross: &'static str,
    pub star: &'static str,
    pub lock: &'static str,
    pub unlock: &'static str,
    pub clock: &'static str,
    pub folder: &'static str,
    pub file: &'static str,
    pub changed: &'static str,
}

pub const ICONS: Icons = Icons {
    success: "✓",
    error: "✗",
    warning: "⚠",
    info: "ℹ",
    arrow: "→",
    bullet: "•",
    loading: "⟳",
    plus: "+",
    minus: "-",
    check: "✓",
    cross: "✗",
    star: "★",
    lock: "🔒",
    unlock: "🔓",
    clock: "🕐",
    folder: "📁",
    file: "📄",
    changed: "~",
};

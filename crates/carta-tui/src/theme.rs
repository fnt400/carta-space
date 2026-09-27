use std::env;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct MarkdownTheme {
    pub heading: Style,
    pub emphasis: Style,
    pub strong: Style,
    pub quote: Style,
    pub code: Style,
    pub link: Style,
}

impl MarkdownTheme {
    fn from_environment() -> Self {
        Self {
            heading: Style::default()
                .fg(theme_color("CARTA_MD_HEADING_COLOR", Color::LightBlue))
                .add_modifier(Modifier::BOLD),
            emphasis: Style::default()
                .fg(theme_color("CARTA_MD_EMPHASIS_COLOR", Color::LightMagenta))
                .add_modifier(Modifier::ITALIC),
            strong: Style::default()
                .fg(theme_color("CARTA_MD_STRONG_COLOR", Color::LightYellow))
                .add_modifier(Modifier::BOLD),
            quote: Style::default().fg(theme_color("CARTA_MD_QUOTE_COLOR", Color::Gray)),
            code: Style::default().fg(theme_color("CARTA_MD_CODE_COLOR", Color::LightGreen)),
            link: Style::default().fg(theme_color("CARTA_MD_LINK_COLOR", Color::LightCyan)),
        }
    }
}

pub fn markdown_theme() -> &'static MarkdownTheme {
    static THEME: OnceLock<MarkdownTheme> = OnceLock::new();
    THEME.get_or_init(MarkdownTheme::from_environment)
}

fn theme_color(variable: &str, fallback: Color) -> Color {
    env::var(variable)
        .ok()
        .and_then(|value| parse_color(&value))
        .unwrap_or(fallback)
}

fn parse_color(value: &str) -> Option<Color> {
    let normalized = value\n        .trim()\n        .to_ascii_lowercase()\n        .replace('_', "-")\n        .replace(' ', "-");
    match normalized.as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "dark-gray" | "dark-grey" => Some(Color::DarkGray),
        "light-red" => Some(Color::LightRed),
        "light-green" => Some(Color::LightGreen),
        "light-yellow" => Some(Color::LightYellow),
        "light-blue" => Some(Color::LightBlue),
        "light-magenta" => Some(Color::LightMagenta),
        "light-cyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => parse_rgb(&normalized),
    }
}

fn parse_rgb(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_and_rgb_theme_colors() {
        assert_eq!(parse_color("light-cyan"), Some(Color::LightCyan));
        assert_eq!(parse_color("Light Blue"), Some(Color::LightBlue));
        assert_eq!(parse_color("#102030"), Some(Color::Rgb(16, 32, 48)));
        assert_eq!(parse_color("not-a-color"), None);
    }
}

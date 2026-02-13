use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Span as RtSpan;
use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::Color as SyntectColor;
use syntect::highlighting::FontStyle as SyntectFontStyle;
use syntect::highlighting::Style as SyntectStyle;
use syntect::highlighting::Theme;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxReference;
use syntect::parsing::SyntaxSet;

pub(crate) const DEFAULT_SYNTAX_THEME: &str = "base16-ocean.dark";

pub(crate) struct SyntectHighlighter {
    inner: HighlightLines<'static>,
    syntax_set: &'static SyntaxSet,
}

impl SyntectHighlighter {
    pub(crate) fn from_path(path: &Path, theme_name: &str) -> Self {
        let syntax_set = syntect_syntax_set();
        let syntax = path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| syntax_set.find_syntax_by_extension(ext))
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
        Self::new(syntax, theme_name)
    }

    pub(crate) fn from_language(language: Option<&str>, theme_name: &str) -> Option<Self> {
        let syntax_set = syntect_syntax_set();
        let token = language
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .and_then(|token| token.split_whitespace().next())?;
        let syntax = syntax_set
            .find_syntax_by_token(token)
            .or_else(|| syntax_set.find_syntax_by_extension(token))?;
        Some(Self::new(syntax, theme_name))
    }

    fn new(syntax: &'static SyntaxReference, theme_name: &str) -> Self {
        let theme = syntect_theme(theme_name);
        Self {
            inner: HighlightLines::new(syntax, theme),
            syntax_set: syntect_syntax_set(),
        }
    }

    pub(crate) fn highlight_line(&mut self, line: &str) -> Vec<RtSpan<'static>> {
        if line.is_empty() {
            return Vec::new();
        }
        match self.inner.highlight_line(line, self.syntax_set) {
            Ok(ranges) => ranges
                .into_iter()
                .map(|(style, text)| {
                    RtSpan::styled(text.to_string(), syntect_style_to_ratatui(style))
                })
                .collect(),
            Err(_) => vec![line.to_string().into()],
        }
    }
}

fn syntect_syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntect_themes() -> &'static ThemeSet {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    THEMES.get_or_init(ThemeSet::load_defaults)
}

fn syntect_theme(theme_name: &str) -> &'static Theme {
    let theme_set = syntect_themes();
    #[expect(clippy::expect_used)]
    theme_set
        .themes
        .get(theme_name)
        .or_else(|| theme_set.themes.get(DEFAULT_SYNTAX_THEME))
        .or_else(|| theme_set.themes.values().next())
        .expect("syntect themes missing")
}

fn syntect_style_to_ratatui(style: SyntectStyle) -> Style {
    let mut out = Style::default();
    if style.foreground.a != 0 {
        out = out.fg(syntect_color_to_ratatui(style.foreground));
    }
    if style.font_style.contains(SyntectFontStyle::BOLD) {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(SyntectFontStyle::ITALIC) {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(SyntectFontStyle::UNDERLINE) {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    out
}

fn syntect_color_to_ratatui(color: SyntectColor) -> Color {
    let r = color.r;
    let g = color.g;
    let b = color.b;
    if r == g && g == b {
        return if r < 64 {
            Color::Black
        } else if r < 128 {
            Color::DarkGray
        } else if r < 192 {
            Color::Gray
        } else {
            Color::White
        };
    }

    let max = r.max(g).max(b);
    let bright = max > 200;
    if r >= g && r >= b {
        if g > b {
            if bright {
                Color::LightYellow
            } else {
                Color::Yellow
            }
        } else if bright {
            Color::LightRed
        } else {
            Color::Red
        }
    } else if g >= r && g >= b {
        if b > r {
            if bright {
                Color::LightCyan
            } else {
                Color::Cyan
            }
        } else if bright {
            Color::LightGreen
        } else {
            Color::Green
        }
    } else if r > g {
        if bright {
            Color::LightMagenta
        } else {
            Color::Magenta
        }
    } else if bright {
        Color::LightBlue
    } else {
        Color::Blue
    }
}

use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use std::sync::OnceLock;
use tree_sitter_highlight::Highlight;
use tree_sitter_highlight::HighlightConfiguration;
use tree_sitter_highlight::HighlightEvent;
use tree_sitter_highlight::Highlighter;

// Ref: https://github.com/tree-sitter/tree-sitter-bash/blob/master/queries/highlights.scm
#[derive(Copy, Clone)]
enum BashHighlight {
    Comment,
    Constant,
    Embedded,
    Function,
    Keyword,
    Number,
    Operator,
    Property,
    String,
}

impl BashHighlight {
    const ALL: [Self; 9] = [
        Self::Comment,
        Self::Constant,
        Self::Embedded,
        Self::Function,
        Self::Keyword,
        Self::Number,
        Self::Operator,
        Self::Property,
        Self::String,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Constant => "constant",
            Self::Embedded => "embedded",
            Self::Function => "function",
            Self::Keyword => "keyword",
            Self::Number => "number",
            Self::Operator => "operator",
            Self::Property => "property",
            Self::String => "string",
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Comment | Self::Operator | Self::String => Style::default().dim(),
            _ => Style::default(),
        }
    }
}

static HIGHLIGHT_CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();

fn highlight_names() -> &'static [&'static str] {
    static NAMES: OnceLock<[&'static str; BashHighlight::ALL.len()]> = OnceLock::new();
    NAMES
        .get_or_init(|| BashHighlight::ALL.map(BashHighlight::as_str))
        .as_slice()
}

fn highlight_config() -> &'static HighlightConfiguration {
    HIGHLIGHT_CONFIG.get_or_init(|| {
        let language = tree_sitter_bash::LANGUAGE.into();
        #[expect(clippy::expect_used)]
        let mut config = HighlightConfiguration::new(
            language,
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        )
        .expect("load bash highlight query");
        config.configure(highlight_names());
        config
    })
}

fn highlight_for(highlight: Highlight) -> BashHighlight {
    BashHighlight::ALL[highlight.0]
}

fn push_segment(lines: &mut Vec<Line<'static>>, segment: &str, style: Option<Style>) {
    for (i, part) in segment.split('\n').enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        if part.is_empty() {
            continue;
        }
        let span = match style {
            Some(style) => Span::styled(part.to_string(), style),
            None => part.to_string().into(),
        };
        if let Some(last) = lines.last_mut() {
            last.spans.push(span);
        }
    }
}

const CODE_HIGHLIGHT_NAMES: [&str; 43] = [
    "comment",
    "comment.documentation",
    "constant",
    "constant.builtin",
    "constant.character",
    "constant.macro",
    "string",
    "string.special",
    "string.escape",
    "character",
    "number",
    "float",
    "boolean",
    "keyword",
    "keyword.function",
    "keyword.operator",
    "keyword.return",
    "operator",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "function",
    "function.builtin",
    "function.macro",
    "method",
    "method.call",
    "constructor",
    "type",
    "type.builtin",
    "type.definition",
    "tag",
    "attribute",
    "property",
    "field",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "label",
    "namespace",
    "module",
    "macro",
    "embedded",
];

fn code_highlight_names() -> &'static [&'static str] {
    &CODE_HIGHLIGHT_NAMES
}

fn code_style_for(name: &str) -> Option<Style> {
    if name.starts_with("comment") {
        Some(Style::default().dim())
    } else if name.starts_with("string") || name == "character" {
        Some(Style::default().green())
    } else if matches!(name, "number" | "float" | "boolean") || name.starts_with("keyword") {
        Some(Style::default().magenta())
    } else if name.starts_with("function")
        || name.starts_with("method")
        || name == "constructor"
        || name.starts_with("type")
        || matches!(name, "tag" | "attribute")
    {
        Some(Style::default().cyan())
    } else if name.starts_with("constant") || name == "macro" {
        Some(Style::default().magenta())
    } else if name.starts_with("operator") || name.starts_with("punctuation") {
        Some(Style::default().dim())
    } else if name == "label" {
        Some(Style::default().magenta())
    } else if name == "embedded" {
        Some(Style::default().cyan())
    } else {
        None
    }
}

fn highlight_code_to_lines(script: &str, config: &HighlightConfiguration) -> Vec<Line<'static>> {
    let mut highlighter = Highlighter::new();
    let iterator = match highlighter.highlight(config, script.as_bytes(), None, |_| None) {
        Ok(iter) => iter,
        Err(_) => return plain_code_to_lines(script),
    };

    let mut lines: Vec<Line<'static>> = vec![Line::from("")];
    let mut highlight_stack: Vec<Highlight> = Vec::new();

    for event in iterator {
        match event {
            Ok(HighlightEvent::HighlightStart(highlight)) => highlight_stack.push(highlight),
            Ok(HighlightEvent::HighlightEnd) => {
                highlight_stack.pop();
            }
            Ok(HighlightEvent::Source { start, end }) => {
                if start == end {
                    continue;
                }
                let style = highlight_stack
                    .last()
                    .and_then(|highlight| code_highlight_names().get(highlight.0))
                    .and_then(|name| code_style_for(name));
                push_segment(&mut lines, &script[start..end], style);
            }
            Err(_) => return plain_code_to_lines(script),
        }
    }

    trim_trailing_empty_line(&mut lines, script);
    lines
}

fn plain_code_to_lines(script: &str) -> Vec<Line<'static>> {
    if script.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for line in script.split('\n') {
        if line.is_empty() {
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(line.to_string()));
        }
    }
    trim_trailing_empty_line(&mut lines, script);
    lines
}

fn trim_trailing_empty_line(lines: &mut Vec<Line<'static>>, script: &str) {
    if script.ends_with('\n') && lines.last().is_some_and(|line| line.spans.is_empty()) {
        lines.pop();
    }
}

static JSON_HIGHLIGHT_CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
static PYTHON_HIGHLIGHT_CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
static RUST_HIGHLIGHT_CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
static YAML_HIGHLIGHT_CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();

fn highlight_config_json() -> &'static HighlightConfiguration {
    JSON_HIGHLIGHT_CONFIG.get_or_init(|| {
        let language = tree_sitter_json::LANGUAGE.into();
        #[expect(clippy::expect_used)]
        let mut config = HighlightConfiguration::new(
            language,
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        )
        .expect("load json highlight query");
        config.configure(code_highlight_names());
        config
    })
}

fn highlight_config_python() -> &'static HighlightConfiguration {
    PYTHON_HIGHLIGHT_CONFIG.get_or_init(|| {
        let language = tree_sitter_python::LANGUAGE.into();
        #[expect(clippy::expect_used)]
        let mut config = HighlightConfiguration::new(
            language,
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        )
        .expect("load python highlight query");
        config.configure(code_highlight_names());
        config
    })
}

fn highlight_config_rust() -> &'static HighlightConfiguration {
    RUST_HIGHLIGHT_CONFIG.get_or_init(|| {
        let language = tree_sitter_rust::LANGUAGE.into();
        #[expect(clippy::expect_used)]
        let mut config = HighlightConfiguration::new(
            language,
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            "",
            "",
        )
        .expect("load rust highlight query");
        config.configure(code_highlight_names());
        config
    })
}

fn highlight_config_yaml() -> &'static HighlightConfiguration {
    YAML_HIGHLIGHT_CONFIG.get_or_init(|| {
        let language = tree_sitter_yaml::LANGUAGE.into();
        #[expect(clippy::expect_used)]
        let mut config = HighlightConfiguration::new(
            language,
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        )
        .expect("load yaml highlight query");
        config.configure(code_highlight_names());
        config
    })
}

fn normalized_language(lang: Option<&str>) -> Option<String> {
    let lang = lang?;
    let lang = lang.trim();
    if lang.is_empty() {
        return None;
    }
    let base = lang
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '{' || c == '}')
        .next()
        .unwrap_or("");
    let base = base.trim_start_matches(['.', '{', '}']);
    let base = base.trim();
    if base.is_empty() {
        None
    } else {
        Some(base.to_ascii_lowercase())
    }
}

pub(crate) fn highlight_code_block_to_lines(
    lang: Option<&str>,
    script: &str,
) -> Vec<Line<'static>> {
    let lang = normalized_language(lang);
    let Some(lang) = lang.as_deref() else {
        return plain_code_to_lines(script);
    };
    match lang {
        "bash" | "sh" | "shell" | "zsh" | "fish" | "shell-session" | "sh-session" => {
            let mut lines = highlight_bash_to_lines(script);
            trim_trailing_empty_line(&mut lines, script);
            lines
        }
        "json" | "jsonc" | "json5" => highlight_code_to_lines(script, highlight_config_json()),
        "python" | "py" => highlight_code_to_lines(script, highlight_config_python()),
        "rust" | "rs" => highlight_code_to_lines(script, highlight_config_rust()),
        "yaml" | "yml" => highlight_code_to_lines(script, highlight_config_yaml()),
        _ => plain_code_to_lines(script),
    }
}

/// Convert a bash script into per-line styled content using tree-sitter's
/// bash highlight query. The highlighter is streamed so multi-line content is
/// split into `Line`s while preserving style boundaries.
pub(crate) fn highlight_bash_to_lines(script: &str) -> Vec<Line<'static>> {
    let mut highlighter = Highlighter::new();
    let iterator =
        match highlighter.highlight(highlight_config(), script.as_bytes(), None, |_| None) {
            Ok(iter) => iter,
            Err(_) => return vec![script.to_string().into()],
        };

    let mut lines: Vec<Line<'static>> = vec![Line::from("")];
    let mut highlight_stack: Vec<Highlight> = Vec::new();

    for event in iterator {
        match event {
            Ok(HighlightEvent::HighlightStart(highlight)) => highlight_stack.push(highlight),
            Ok(HighlightEvent::HighlightEnd) => {
                highlight_stack.pop();
            }
            Ok(HighlightEvent::Source { start, end }) => {
                if start == end {
                    continue;
                }
                let style = highlight_stack.last().map(|h| highlight_for(*h).style());
                push_segment(&mut lines, &script[start..end], style);
            }
            Err(_) => return vec![script.to_string().into()],
        }
    }

    if lines.is_empty() {
        vec![Line::from("")]
    } else {
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ratatui::style::Modifier;

    fn reconstructed(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|sp| sp.content.clone())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn dimmed_tokens(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|sp| sp.style.add_modifier.contains(Modifier::DIM))
            .map(|sp| sp.content.clone().into_owned())
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty())
            .collect()
    }

    #[test]
    fn dims_expected_bash_operators() {
        let s = "echo foo && bar || baz | qux & (echo hi)";
        let lines = highlight_bash_to_lines(s);
        assert_eq!(reconstructed(&lines), s);

        let dimmed = dimmed_tokens(&lines);
        assert!(dimmed.contains(&"&&".to_string()));
        assert!(dimmed.contains(&"|".to_string()));
        assert!(!dimmed.contains(&"echo".to_string()));
    }

    #[test]
    fn dims_redirects_and_strings() {
        let s = "echo \"hi\" > out.txt; echo 'ok'";
        let lines = highlight_bash_to_lines(s);
        assert_eq!(reconstructed(&lines), s);

        let dimmed = dimmed_tokens(&lines);
        assert!(dimmed.contains(&">".to_string()));
        assert!(dimmed.contains(&"\"hi\"".to_string()));
        assert!(dimmed.contains(&"'ok'".to_string()));
    }

    #[test]
    fn highlights_command_and_strings() {
        let s = "echo \"hi\"";
        let lines = highlight_bash_to_lines(s);
        let mut echo_style = None;
        let mut string_style = None;
        for span in &lines[0].spans {
            let text = span.content.as_ref();
            if text == "echo" {
                echo_style = Some(span.style);
            }
            if text == "\"hi\"" {
                string_style = Some(span.style);
            }
        }
        let echo_style = echo_style.expect("echo span missing");
        let string_style = string_style.expect("string span missing");
        assert!(echo_style.fg.is_none());
        assert!(!echo_style.add_modifier.contains(Modifier::DIM));
        assert!(string_style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn highlights_heredoc_body_as_string() {
        let s = "cat <<EOF\nheredoc body\nEOF";
        let lines = highlight_bash_to_lines(s);
        let body_line = &lines[1];
        let mut body_style = None;
        for span in &body_line.spans {
            if span.content.as_ref() == "heredoc body" {
                body_style = Some(span.style);
            }
        }
        let body_style = body_style.expect("missing heredoc span");
        assert!(body_style.add_modifier.contains(Modifier::DIM));
    }
}

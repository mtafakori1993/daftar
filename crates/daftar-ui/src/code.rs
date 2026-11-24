//! Syntax highlighting for fenced code blocks using syntect (pure Rust, no JS).

use std::ops::Range;

use egui::Color32;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct CodeHighlighter {
    syntaxes: SyntaxSet,
    themes: ThemeSet,
}

#[derive(Debug, Clone)]
pub struct CodeToken {
    pub range: Range<usize>,
    pub color: Color32,
}

#[derive(Debug, Clone, Default)]
pub struct HighlightedCode {
    pub tokens: Vec<CodeToken>,
    pub fallback_color: Color32,
}

impl CodeHighlighter {
    pub fn new() -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            themes: ThemeSet::load_defaults(),
        }
    }

    pub fn syntaxes(&self) -> &[syntect::parsing::SyntaxReference] {
        self.syntaxes.syntaxes()
    }

    pub fn highlight(&self, code: &str, language: Option<&str>, dark: bool) -> HighlightedCode {
        let syntax = language
            .and_then(|lang| {
                self.syntaxes
                    .find_syntax_by_token(lang)
                    .or_else(|| self.syntaxes.find_syntax_by_extension(lang))
            })
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let theme_name = if dark { "base16-ocean.dark" } else { "InspiredGitHub" };
        let theme = self
            .themes
            .themes
            .get(theme_name)
            .or_else(|| self.themes.themes.values().next());
        let Some(theme) = theme else {
            return HighlightedCode {
                tokens: Vec::new(),
                fallback_color: if dark {
                    Color32::from_rgb(0xE1, 0xE1, 0xE1)
                } else {
                    Color32::from_rgb(0x2C, 0x2C, 0x2C)
                },
            };
        };
        let fallback = {
            let c = theme.settings.foreground.unwrap_or(syntect::highlighting::Color {
                r: 0x2C,
                g: 0x2C,
                b: 0x2C,
                a: 0xFF,
            });
            Color32::from_rgb(c.r, c.g, c.b)
        };
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut tokens = Vec::new();
        let mut offset = 0usize;
        for line in LinesWithEndings::from(code) {
            let ranges = match highlighter.highlight_line(line, &self.syntaxes) {
                Ok(ranges) => ranges,
                Err(_) => {
                    tokens.push(CodeToken {
                        range: offset..offset + line.len(),
                        color: fallback,
                    });
                    offset += line.len();
                    continue;
                }
            };
            for (style, text) in ranges {
                if text.is_empty() {
                    continue;
                }
                let start = offset;
                let end = offset + text.len();
                tokens.push(CodeToken {
                    range: start..end,
                    color: Color32::from_rgb(
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                    ),
                });
                offset = end;
            }
        }
        HighlightedCode {
            tokens,
            fallback_color: fallback,
        }
    }
}

impl Default for CodeHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_with_multiple_colors() {
        let highlighter = CodeHighlighter::new();
        let code = "fn main() {\n    println!(\"hi\");\n}\n";
        let result = highlighter.highlight(code, Some("rust"), false);
        assert!(!result.tokens.is_empty());
        assert!(result.tokens.iter().all(|t| t.range.end <= code.len()));
        let distinct: std::collections::BTreeSet<_> =
            result.tokens.iter().map(|t| t.color.to_array()).collect();
        assert!(distinct.len() >= 2, "expected multiple token colours");
    }

    #[test]
    fn unknown_language_falls_back_gracefully() {
        let highlighter = CodeHighlighter::new();
        let code = "some opaque text\n";
        let result = highlighter.highlight(code, Some("not-a-language"), true);
        assert!(!result.tokens.is_empty());
        assert!(result.tokens.iter().any(|t| !t.range.is_empty()));
    }

    #[test]
    fn handles_empty_code() {
        let highlighter = CodeHighlighter::new();
        let result = highlighter.highlight("", None, false);
        assert!(result.tokens.is_empty());
    }

    #[test]
    fn token_ranges_cover_the_code() {
        let highlighter = CodeHighlighter::new();
        let code = "let x = 42;\n// comment\n";
        let result = highlighter.highlight(code, Some("rs"), false);
        let covered: usize = result.tokens.iter().map(|t| t.range.len()).sum();
        assert_eq!(covered, code.len());
    }
}
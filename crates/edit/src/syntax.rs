// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::ops::Range;
use std::path::Path;

use tree_sitter::Language;
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

use crate::framebuffer::IndexedColor;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SyntaxLanguage {
    C,
    Cpp,
    CSharp,
    Css,
    D,
    Html,
    JavaScript,
    Ruby,
    Rust,
    TypeScript,
    Tsx,
}

#[derive(Clone)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub color: IndexedColor,
}

pub struct SyntaxHighlighter {
    highlighter: Highlighter,
    config: HighlightConfiguration,
    highlight_colors: Vec<IndexedColor>,
}

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "function",
    "function.builtin",
    "keyword",
    "keyword.function",
    "keyword.operator",
    "keyword.return",
    "label",
    "method",
    "module",
    "namespace",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "tag.delimiter",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.member",
    "variable.parameter",
];

impl SyntaxLanguage {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "c" | "h" => Some(Self::C),
            "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Some(Self::Cpp),
            "cs" => Some(Self::CSharp),
            "css" => Some(Self::Css),
            "d" => Some(Self::D),
            "htm" | "html" => Some(Self::Html),
            "js" | "mjs" | "cjs" => Some(Self::JavaScript),
            "rb" => Some(Self::Ruby),
            "rs" => Some(Self::Rust),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension().and_then(|ext| ext.to_str()).and_then(Self::from_extension)
    }
}

impl SyntaxHighlighter {
    pub fn new(language: SyntaxLanguage) -> Option<Self> {
        let config = language_config(language);
        let mut highlight_config = HighlightConfiguration::new(
            config.language,
            config.name,
            config.highlights,
            config.injections,
            config.locals,
        )
        .ok()?;
        highlight_config.configure(HIGHLIGHT_NAMES);
        let highlight_colors =
            HIGHLIGHT_NAMES.iter().map(|name| highlight_color(name)).collect::<Vec<_>>();
        Some(Self { highlighter: Highlighter::new(), config: highlight_config, highlight_colors })
    }

    pub fn highlight(&mut self, source: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let mut highlight_stack: Vec<Highlight> = Vec::new();

        let events = match self.highlighter.highlight(&self.config, source.as_bytes(), None, |_| None) {
            Ok(events) => events,
            Err(_) => return spans,
        };

        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(highlight)) => highlight_stack.push(highlight),
                Ok(HighlightEvent::HighlightEnd) => {
                    highlight_stack.pop();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    if start >= end {
                        continue;
                    }
                    let Some(highlight) = highlight_stack.last() else {
                        continue;
                    };
                    if let Some(&color) = self.highlight_colors.get(highlight.0) {
                        spans.push(HighlightSpan { range: start..end, color });
                    }
                }
                Err(_) => break,
            }
        }

        spans
    }
}

struct LanguageConfig {
    language: Language,
    name: &'static str,
    highlights: &'static str,
    injections: &'static str,
    locals: &'static str,
}

const C_SHARP_HIGHLIGHTS_QUERY: &str = r#"
(comment) @comment
(string_literal) @string
(verbatim_string_literal) @string
(raw_string_literal) @string
"#;

const D_HIGHLIGHTS_QUERY: &str = r#"
(comment) @comment
(string) @string
"#;

fn language_config(language: SyntaxLanguage) -> LanguageConfig {
    match language {
        SyntaxLanguage::C => LanguageConfig {
            language: tree_sitter_c::LANGUAGE.into(),
            name: "c",
            highlights: tree_sitter_c::HIGHLIGHT_QUERY,
            injections: "",
            locals: "",
        },
        SyntaxLanguage::Cpp => LanguageConfig {
            language: tree_sitter_cpp::LANGUAGE.into(),
            name: "cpp",
            highlights: tree_sitter_cpp::HIGHLIGHT_QUERY,
            injections: "",
            locals: "",
        },
        SyntaxLanguage::CSharp => LanguageConfig {
            language: tree_sitter_c_sharp::LANGUAGE.into(),
            name: "csharp",
            highlights: C_SHARP_HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        SyntaxLanguage::Css => LanguageConfig {
            language: tree_sitter_css::LANGUAGE.into(),
            name: "css",
            highlights: tree_sitter_css::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        SyntaxLanguage::D => LanguageConfig {
            language: tree_sitter_d::LANGUAGE.into(),
            name: "d",
            highlights: D_HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        SyntaxLanguage::Html => LanguageConfig {
            language: tree_sitter_html::LANGUAGE.into(),
            name: "html",
            highlights: tree_sitter_html::HIGHLIGHTS_QUERY,
            injections: tree_sitter_html::INJECTIONS_QUERY,
            locals: "",
        },
        SyntaxLanguage::JavaScript => LanguageConfig {
            language: tree_sitter_javascript::LANGUAGE.into(),
            name: "javascript",
            highlights: tree_sitter_javascript::HIGHLIGHT_QUERY,
            injections: tree_sitter_javascript::INJECTIONS_QUERY,
            locals: tree_sitter_javascript::LOCALS_QUERY,
        },
        SyntaxLanguage::Ruby => LanguageConfig {
            language: tree_sitter_ruby::LANGUAGE.into(),
            name: "ruby",
            highlights: tree_sitter_ruby::HIGHLIGHTS_QUERY,
            injections: "",
            locals: tree_sitter_ruby::LOCALS_QUERY,
        },
        SyntaxLanguage::Rust => LanguageConfig {
            language: tree_sitter_rust::LANGUAGE.into(),
            name: "rust",
            highlights: tree_sitter_rust::HIGHLIGHTS_QUERY,
            injections: tree_sitter_rust::INJECTIONS_QUERY,
            locals: "",
        },
        SyntaxLanguage::TypeScript => LanguageConfig {
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            name: "typescript",
            highlights: tree_sitter_typescript::HIGHLIGHTS_QUERY,
            injections: "",
            locals: tree_sitter_typescript::LOCALS_QUERY,
        },
        SyntaxLanguage::Tsx => LanguageConfig {
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            name: "tsx",
            highlights: tree_sitter_typescript::HIGHLIGHTS_QUERY,
            injections: "",
            locals: tree_sitter_typescript::LOCALS_QUERY,
        },
    }
}

fn highlight_color(name: &str) -> IndexedColor {
    match name {
        "comment" => IndexedColor::BrightBlack,
        "string" | "string.special" => IndexedColor::BrightGreen,
        "keyword" | "keyword.function" | "keyword.operator" | "keyword.return" => {
            IndexedColor::BrightBlue
        }
        "function" | "function.builtin" | "method" => IndexedColor::BrightCyan,
        "type" | "type.builtin" | "constructor" | "namespace" | "module" => {
            IndexedColor::BrightYellow
        }
        "constant" | "constant.builtin" | "number" | "boolean" => IndexedColor::BrightMagenta,
        "tag" | "tag.delimiter" => IndexedColor::BrightBlue,
        "attribute" => IndexedColor::BrightCyan,
        "variable.builtin" | "variable.parameter" => IndexedColor::BrightWhite,
        "label" => IndexedColor::BrightMagenta,
        _ => IndexedColor::Foreground,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(SyntaxLanguage::from_extension("c"), Some(SyntaxLanguage::C));
        assert_eq!(SyntaxLanguage::from_extension("cpp"), Some(SyntaxLanguage::Cpp));
        assert_eq!(SyntaxLanguage::from_extension("d"), Some(SyntaxLanguage::D));
        assert_eq!(SyntaxLanguage::from_extension("rs"), Some(SyntaxLanguage::Rust));
        assert_eq!(SyntaxLanguage::from_extension("html"), Some(SyntaxLanguage::Html));
        assert_eq!(SyntaxLanguage::from_extension("css"), Some(SyntaxLanguage::Css));
        assert_eq!(SyntaxLanguage::from_extension("js"), Some(SyntaxLanguage::JavaScript));
        assert_eq!(SyntaxLanguage::from_extension("ts"), Some(SyntaxLanguage::TypeScript));
        assert_eq!(SyntaxLanguage::from_extension("tsx"), Some(SyntaxLanguage::Tsx));
        assert_eq!(SyntaxLanguage::from_extension("rb"), Some(SyntaxLanguage::Ruby));
        assert_eq!(SyntaxLanguage::from_extension("cs"), Some(SyntaxLanguage::CSharp));
    }

    #[test]
    fn test_rust_highlight_smoke() {
        let mut highlighter =
            SyntaxHighlighter::new(SyntaxLanguage::Rust).expect("Rust highlighter config");
        let spans = highlighter.highlight("fn main() { let value = 42; }\n");
        assert!(!spans.is_empty());
    }
}

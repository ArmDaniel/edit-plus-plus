// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use tree_sitter::{Language, Parser, Tree};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, Highlighter, HighlightEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportedLanguage {
    Rust,
    Cpp,
    Python,
}

impl ToString for SupportedLanguage {
    fn to_string(&self) -> String {
        match self {
            SupportedLanguage::Rust => "rust".to_string(),
            SupportedLanguage::Cpp => "cpp".to_string(),
            SupportedLanguage::Python => "python".to_string(),
        }
    }
}

impl SupportedLanguage {
    pub fn to_language(self) -> Language {
        match self {
            SupportedLanguage::Rust => tree_sitter_rust::language(),
            SupportedLanguage::Cpp => tree_sitter_cpp::language(),
            SupportedLanguage::Python => tree_sitter_python::language(),
        }
    }

    fn to_highlight_config(self) -> HighlightConfiguration {
        let mut config = match self {
            SupportedLanguage::Rust => HighlightConfiguration::new(
                tree_sitter_rust::language(),
                "rust",
                tree_sitter_rust::HIGHLIGHTS_QUERY,
                "",
                "",
            )
            .unwrap(),
            SupportedLanguage::Cpp => HighlightConfiguration::new(
                tree_sitter_cpp::language(),
                "cpp",
                tree_sitter_cpp::HIGHLIGHT_QUERY,
                "",
                "",
            )
            .unwrap(),
            SupportedLanguage::Python => HighlightConfiguration::new(
                tree_sitter_python::language(),
                "python",
                tree_sitter_python::HIGHLIGHTS_QUERY,
                "",
                "",
            )
            .unwrap(),
        };

        let highlight_names = [
            "attribute",
            "constant",
            "function.builtin",
            "function",
            "keyword",
            "operator",
            "property",
            "punctuation",
            "punctuation.bracket",
            "punctuation.delimiter",
            "string",
            "string.special",
            "tag",
            "type",
            "type.builtin",
            "variable",
            "variable.builtin",
            "variable.parameter",
        ]
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<&str>>();

        config.configure(&highlight_names);
        config
    }
}

pub struct Syntax {
    parser: Parser,
    highlighter: Highlighter,
    configs: Vec<(SupportedLanguage, HighlightConfiguration)>,
}

impl Syntax {
    pub fn new() -> Self {
        let highlighter = Highlighter::new();
        let configs = vec![
            (
                SupportedLanguage::Rust,
                SupportedLanguage::Rust.to_highlight_config(),
            ),
            (
                SupportedLanguage::Cpp,
                SupportedLanguage::Cpp.to_highlight_config(),
            ),
            (
                SupportedLanguage::Python,
                SupportedLanguage::Python.to_highlight_config(),
            ),
        ];
        Self {
            parser: Parser::new(),
            highlighter,
            configs,
        }
    }

    pub fn parse(&mut self, code: &str, lang: SupportedLanguage) -> Option<Tree> {
        self.parser
            .set_language(&lang.to_language())
            .expect("Failed to set language");
        self.parser.parse(code, None)
    }

    pub fn highlight<'a>(
        &'a mut self,
        code: &'a str,
        lang: SupportedLanguage,
    ) -> impl Iterator<Item = (std::ops::Range<usize>, Highlight)> + 'a {
        let config = self
            .configs
            .iter()
            .find(|(l, _)| *l == lang)
            .map(|(_, c)| c)
            .unwrap();

        let mut highlight_stack = Vec::new();

        self.highlighter
            .highlight(config, code.as_bytes(), None, |lang_name| {
                self.configs
                    .iter()
                    .find(|(lang, _)| lang.to_string() == lang_name)
                    .map(|(_, c)| c)
            })
            .unwrap()
            .filter_map(move |event| match event.unwrap() {
                HighlightEvent::Source { start, end } => Some((
                    start..end,
                    highlight_stack.last().copied().unwrap_or(Highlight(0)),
                )),
                HighlightEvent::HighlightStart(h) => {
                    highlight_stack.push(h);
                    None
                }
                HighlightEvent::HighlightEnd => {
                    highlight_stack.pop();
                    None
                }
            })
    }
}

use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const HIGHLIGHT_NAMES: &[&str] = &[
    "keyword",
    "function",
    "function.builtin",
    "type",
    "type.builtin",
    "constructor",
    "constant",
    "constant.builtin",
    "string",
    "number",
    "comment",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "operator",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "property",
    "tag",
    "attribute",
    "label",
    "escape",
    "embedded",
];

#[derive(serde::Serialize)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub kind: u8,
}

struct LangDef {
    language: Language,
    highlights: &'static str,
    injections: &'static str,
    locals: &'static str,
}

fn lang_def(lang_id: &str) -> Option<LangDef> {
    let ld = match lang_id {
        "rust" => LangDef {
            language: tree_sitter_rust::LANGUAGE.into(),
            highlights: tree_sitter_rust::HIGHLIGHTS_QUERY,
            injections: tree_sitter_rust::INJECTIONS_QUERY,
            locals: "",
        },
        "c" => LangDef {
            language: tree_sitter_c::LANGUAGE.into(),
            highlights: tree_sitter_c::HIGHLIGHT_QUERY,
            injections: "",
            locals: "",
        },
        "cpp" => LangDef {
            language: tree_sitter_cpp::LANGUAGE.into(),
            highlights: tree_sitter_cpp::HIGHLIGHT_QUERY,
            injections: "",
            locals: "",
        },
        "javascript" | "jsx" => LangDef {
            language: tree_sitter_javascript::LANGUAGE.into(),
            highlights: tree_sitter_javascript::HIGHLIGHT_QUERY,
            injections: tree_sitter_javascript::INJECTIONS_QUERY,
            locals: tree_sitter_javascript::LOCALS_QUERY,
        },
        "typescript" => LangDef {
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            highlights: tree_sitter_typescript::HIGHLIGHTS_QUERY,
            injections: "",
            locals: tree_sitter_typescript::LOCALS_QUERY,
        },
        "tsx" => LangDef {
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            highlights: tree_sitter_typescript::HIGHLIGHTS_QUERY,
            injections: "",
            locals: tree_sitter_typescript::LOCALS_QUERY,
        },
        "python" => LangDef {
            language: tree_sitter_python::LANGUAGE.into(),
            highlights: tree_sitter_python::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "go" => LangDef {
            language: tree_sitter_go::LANGUAGE.into(),
            highlights: tree_sitter_go::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "ruby" => LangDef {
            language: tree_sitter_ruby::LANGUAGE.into(),
            highlights: tree_sitter_ruby::HIGHLIGHTS_QUERY,
            injections: "",
            locals: tree_sitter_ruby::LOCALS_QUERY,
        },
        "bash" | "shell" => LangDef {
            language: tree_sitter_bash::LANGUAGE.into(),
            highlights: tree_sitter_bash::HIGHLIGHT_QUERY,
            injections: "",
            locals: "",
        },
        "java" => LangDef {
            language: tree_sitter_java::LANGUAGE.into(),
            highlights: tree_sitter_java::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "html" => LangDef {
            language: tree_sitter_html::LANGUAGE.into(),
            highlights: tree_sitter_html::HIGHLIGHTS_QUERY,
            injections: tree_sitter_html::INJECTIONS_QUERY,
            locals: "",
        },
        "css" | "scss" | "less" => LangDef {
            language: tree_sitter_css::LANGUAGE.into(),
            highlights: tree_sitter_css::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "json" => LangDef {
            language: tree_sitter_json::LANGUAGE.into(),
            highlights: tree_sitter_json::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "lua" => LangDef {
            language: tree_sitter_lua::LANGUAGE.into(),
            highlights: tree_sitter_lua::HIGHLIGHTS_QUERY,
            injections: tree_sitter_lua::INJECTIONS_QUERY,
            locals: tree_sitter_lua::LOCALS_QUERY,
        },
        "php" => LangDef {
            language: tree_sitter_php::LANGUAGE_PHP.into(),
            highlights: tree_sitter_php::HIGHLIGHTS_QUERY,
            injections: tree_sitter_php::INJECTIONS_QUERY,
            locals: "",
        },
        "toml" => LangDef {
            language: tree_sitter_toml_ng::LANGUAGE.into(),
            highlights: tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "yaml" => LangDef {
            language: tree_sitter_yaml::language(),
            highlights: tree_sitter_yaml::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "swift" => LangDef {
            language: tree_sitter_swift::LANGUAGE.into(),
            highlights: tree_sitter_swift::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "zig" => LangDef {
            language: tree_sitter_zig::LANGUAGE.into(),
            highlights: tree_sitter_zig::HIGHLIGHTS_QUERY,
            injections: tree_sitter_zig::INJECTIONS_QUERY,
            locals: "",
        },
        "sql" => LangDef {
            language: tree_sitter_sequel::LANGUAGE.into(),
            highlights: tree_sitter_sequel::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        "make" | "makefile" => LangDef {
            language: tree_sitter_make::LANGUAGE.into(),
            highlights: tree_sitter_make::HIGHLIGHTS_QUERY,
            injections: "",
            locals: "",
        },
        _ => return None,
    };
    Some(ld)
}

fn make_config(def: LangDef, name: &str) -> Option<HighlightConfiguration> {
    let mut config = HighlightConfiguration::new(
        def.language,
        name,
        def.highlights,
        def.injections,
        def.locals,
    ).ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(config)
}

pub fn highlight_source(source: &str, lang_id: &str) -> Vec<HighlightSpan> {
    let def = match lang_def(lang_id) {
        Some(d) => d,
        None => return Vec::new(),
    };

    let config = match make_config(def, lang_id) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut highlighter = Highlighter::new();
    let events = match highlighter.highlight(&config, source.as_bytes(), None, |_| None) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut spans = Vec::new();
    let mut stack: Vec<u8> = Vec::new();

    for event in events {
        match event {
            Ok(HighlightEvent::Source { start, end }) => {
                if let Some(&kind) = stack.last() {
                    spans.push(HighlightSpan { start, end, kind });
                }
            }
            Ok(HighlightEvent::HighlightStart(h)) => {
                stack.push(h.0 as u8);
            }
            Ok(HighlightEvent::HighlightEnd) => {
                stack.pop();
            }
            Err(_) => break,
        }
    }

    spans
}

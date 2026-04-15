use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LineKind {
    Markdown,
    Cordial,
    Eval,
    Comment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedLine {
    pub index: usize,
    pub kind: LineKind,
    pub content: String,
}

pub fn classify_line(index: usize, raw: &str) -> ClassifiedLine {
    let trimmed = raw.trim();

    let kind = if trimmed.starts_with("/=") {
        LineKind::Eval
    } else if trimmed.starts_with("//") {
        LineKind::Comment
    } else if is_cordial(trimmed) {
        LineKind::Cordial
    } else {
        LineKind::Markdown
    };

    ClassifiedLine {
        index,
        kind,
        content: raw.to_string(),
    }
}

fn is_cordial(line: &str) -> bool {
    if line.starts_with("let ") {
        let rest = &line[4..];
        if let Some(colon_pos) = rest.find(':') {
            let before_colon = rest[..colon_pos].trim();
            if is_ident(before_colon) {
                let after_colon = &rest[colon_pos + 1..];
                if after_colon.contains('=') {
                    return true;
                }
            }
        }
        if let Some(eq_pos) = rest.find('=') {
            let after_eq = rest.as_bytes().get(eq_pos + 1);
            if after_eq != Some(&b'=') {
                let name = rest[..eq_pos].trim();
                // Plain binding: `let x = …`. Covers every RHS — plain
                // expressions, struct/macro-looking constructions like
                // `let lfreq = solve!(l, f0)`, and the function-inversion
                // math form `let f(a, b) = expr where …` (where the LHS
                // is a function-def-shaped name+params, same as Cordial's
                // existing top-level `f(x) = …` short form).
                if is_ident(name) || is_assignment_target(name) {
                    return true;
                }
            }
        }
        return false;
    }

    if line.starts_with("while ") || line.starts_with("while(") { return true; }
    if line.starts_with("fn ") { return true; }
    if line.starts_with("if ") || line.starts_with("if(") { return true; }
    if line.starts_with("else ") || line == "else" || line.starts_with("else{") { return true; }
    if line.starts_with("for ") { return true; }
    if line.starts_with("return ") || line == "return" { return true; }
    if line.starts_with("use ") {
        let rest = line[4..].trim();
        if is_ident(rest.split("::").next().unwrap_or("")) {
            return true;
        }
    }
    if line == "}" || line.starts_with("} ") { return true; }

    if let Some(eq_pos) = line.find('=') {
        if eq_pos > 0 {
            let before = &line[..eq_pos];
            let after_eq = line.as_bytes().get(eq_pos + 1);
            if after_eq != Some(&b'=') && !before.ends_with('!') && !before.ends_with('<') && !before.ends_with('>') {
                let candidate = before.trim();
                if is_assignment_target(candidate) {
                    return true;
                }
            }
        }
    }

    false
}

fn is_assignment_target(s: &str) -> bool {
    // simple variable: `x`
    if is_ident(s) {
        return true;
    }
    // function def: `f(x)` or `f(x, y)`
    if let Some(paren) = s.find('(') {
        let name = &s[..paren];
        if is_ident(name) && s.ends_with(')') {
            return true;
        }
    }
    // cell-ref target: `@Table:A1`, `@Block::Table:A1`, or even bare
    // `@Table` / `@Table:A1:B2`. The interpreter's parser surfaces
    // whole-table / range mis-assignments as errors, so the classifier
    // only needs to recognize the `@name…` shape here.
    if let Some(rest) = s.strip_prefix('@') {
        if let Some(first) = rest.chars().next() {
            if first.is_alphabetic() || first == '_' {
                return true;
            }
        }
    }
    false
}

fn is_ident(s: &str) -> bool {
    if s.is_empty() { return false; }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' { return false; }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

pub fn classify_document(text: &str) -> Vec<ClassifiedLine> {
    let mut result = Vec::new();
    let mut comment_depth: usize = 0;
    let mut brace_depth: i32 = 0;

    for (i, line) in text.lines().enumerate() {
        let was_in_comment = comment_depth > 0;
        comment_depth = scan_comment_depth(line, comment_depth);

        if was_in_comment || line.trim().starts_with("/*") {
            result.push(ClassifiedLine { index: i, kind: LineKind::Comment, content: line.to_string() });
        } else if brace_depth > 0 {
            let trimmed = line.trim();
            let opens = trimmed.matches('{').count() as i32;
            let closes = trimmed.matches('}').count() as i32;
            brace_depth += opens - closes;
            if brace_depth < 0 { brace_depth = 0; }
            result.push(ClassifiedLine { index: i, kind: LineKind::Cordial, content: line.to_string() });
        } else {
            let cl = classify_line(i, line);
            if cl.kind == LineKind::Cordial {
                let trimmed = line.trim();
                let opens = trimmed.matches('{').count() as i32;
                let closes = trimmed.matches('}').count() as i32;
                brace_depth += opens - closes;
                if brace_depth < 0 { brace_depth = 0; }
            }
            result.push(cl);
        }
    }

    result
}

fn scan_comment_depth(line: &str, mut depth: usize) -> usize {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len.saturating_sub(1) {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            depth += 1;
            i += 2;
        } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            depth = depth.saturating_sub(1);
            i += 2;
        } else {
            i += 1;
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_line() {
        let c = classify_line(0, "# Hello World");
        assert_eq!(c.kind, LineKind::Markdown);
    }

    #[test]
    fn eval_line() {
        let c = classify_line(0, "/= 2 + 3");
        assert_eq!(c.kind, LineKind::Eval);
    }

    #[test]
    fn comment_line() {
        let c = classify_line(0, "// this is a comment");
        assert_eq!(c.kind, LineKind::Comment);
    }

    #[test]
    fn let_binding() {
        let c = classify_line(0, "let x = 5");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn variable_assignment() {
        let c = classify_line(0, "x = 5");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn function_def() {
        let c = classify_line(0, "f(x) = x^2");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn plain_text() {
        let c = classify_line(0, "Some notes about the project");
        assert_eq!(c.kind, LineKind::Markdown);
    }

    #[test]
    fn let_prose_not_cordial() {
        let c = classify_line(0, "let us consider something");
        assert_eq!(c.kind, LineKind::Markdown);
    }

    #[test]
    fn let_without_equals_not_cordial() {
        let c = classify_line(0, "let me explain");
        assert_eq!(c.kind, LineKind::Markdown);
    }

    #[test]
    fn single_line_block_comment() {
        let lines = classify_document("/* hello */");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, LineKind::Comment);
    }

    #[test]
    fn multiline_block_comment() {
        let lines = classify_document("/* start\nmiddle\nend */\nlet x = 5");
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].kind, LineKind::Comment);
        assert_eq!(lines[1].kind, LineKind::Comment);
        assert_eq!(lines[2].kind, LineKind::Comment);
        assert_eq!(lines[3].kind, LineKind::Cordial);
    }

    #[test]
    fn block_comment_then_code() {
        let lines = classify_document("/* comment */\n/= 2 + 3");
        assert_eq!(lines[0].kind, LineKind::Comment);
        assert_eq!(lines[1].kind, LineKind::Eval);
    }

    #[test]
    fn nested_block_comments() {
        let lines = classify_document("/* outer /* inner */ still comment */\nlet x = 5");
        assert_eq!(lines[0].kind, LineKind::Comment);
        assert_eq!(lines[1].kind, LineKind::Cordial);
    }

    #[test]
    fn nested_multiline_block_comments() {
        let doc = "/* outer\n/* inner */\nstill in outer\n*/\nlet x = 5";
        let lines = classify_document(doc);
        assert_eq!(lines[0].kind, LineKind::Comment);
        assert_eq!(lines[1].kind, LineKind::Comment);
        assert_eq!(lines[2].kind, LineKind::Comment);
        assert_eq!(lines[3].kind, LineKind::Comment);
        assert_eq!(lines[4].kind, LineKind::Cordial);
    }

    #[test]
    fn while_line() {
        let c = classify_line(0, "while (i < 10) {");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn fn_line() {
        let c = classify_line(0, "fn add(a, b) {");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn closing_brace() {
        let c = classify_line(0, "}");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn while_block_body_classified() {
        let doc = "while (x > 0) {\n    x = x - 1\n}";
        let lines = classify_document(doc);
        assert_eq!(lines[0].kind, LineKind::Cordial);
        assert_eq!(lines[1].kind, LineKind::Cordial);
        assert_eq!(lines[2].kind, LineKind::Cordial);
    }

    #[test]
    fn fn_block_body_classified() {
        let doc = "fn add(a, b) {\n    a + b\n}";
        let lines = classify_document(doc);
        assert_eq!(lines[0].kind, LineKind::Cordial);
        assert_eq!(lines[1].kind, LineKind::Cordial);
        assert_eq!(lines[2].kind, LineKind::Cordial);
    }

    #[test]
    fn let_with_type_annotation() {
        let c = classify_line(0, "let x: int = 5");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn let_with_bool_type() {
        let c = classify_line(0, "let flag: bool = 1");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn if_line() {
        let c = classify_line(0, "if (x > 5) {");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn else_line() {
        let c = classify_line(0, "} else {");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn for_line() {
        let c = classify_line(0, "for i in arr {");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn return_line() {
        let c = classify_line(0, "return x");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn use_line() {
        let c = classify_line(0, "use calculations");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn use_with_item() {
        let c = classify_line(0, "use budget::ramp");
        assert_eq!(c.kind, LineKind::Cordial);
    }

    #[test]
    fn use_prose_not_cordial() {
        let c = classify_line(0, "use a fork to eat");
        assert_eq!(c.kind, LineKind::Markdown);
    }

    #[test]
    fn if_block_body_classified() {
        let doc = "if (x > 5) {\n    x = 1\n} else {\n    x = 0\n}";
        let lines = classify_document(doc);
        assert!(lines.iter().all(|l| l.kind == LineKind::Cordial));
    }
}

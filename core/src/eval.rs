use serde::Serialize;
use crate::doc::{classify_document, LineKind};
use crate::interp;

#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub line: usize,
    pub result: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalError {
    pub line: usize,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentResult {
    pub results: Vec<EvalResult>,
    pub errors: Vec<EvalError>,
}

pub fn evaluate_document(text: &str) -> DocumentResult {
    let classified = classify_document(text);
    let mut results = Vec::new();
    let mut errors = Vec::new();

    let mut lines: Vec<(usize, &str, bool)> = Vec::new();
    for cl in &classified {
        match cl.kind {
            LineKind::Cordial => lines.push((cl.index, &cl.content, false)),
            LineKind::Eval => lines.push((cl.index, &cl.content, true)),
            LineKind::Comment | LineKind::Markdown => {}
        }
    }

    let interp_results = interp::interpret_document(&lines);
    for ir in interp_results {
        let fmt = match ir.format {
            interp::EvalFormat::Inline => "inline",
            interp::EvalFormat::Table => "table",
            interp::EvalFormat::Tree => "tree",
        };
        match ir.value {
            Some(interp::Value::Error(e)) => {
                errors.push(EvalError { line: ir.line, error: e });
            }
            Some(v) => {
                let s = match ir.format {
                    interp::EvalFormat::Table => value_to_table_json(&v),
                    interp::EvalFormat::Tree => value_to_tree_json(&v),
                    interp::EvalFormat::Inline => v.display(),
                };
                if !s.is_empty() {
                    results.push(EvalResult { line: ir.line, result: s, format: fmt.to_string() });
                }
            }
            None => {}
        }
    }

    DocumentResult { results, errors }
}

fn value_to_table_json(val: &interp::Value) -> String {
    match val {
        interp::Value::Array(rows) => {
            let table: Vec<Vec<String>> = rows.iter().map(|row| {
                match row {
                    interp::Value::Array(cols) => cols.iter().map(|c| c.display()).collect(),
                    other => vec![other.display()],
                }
            }).collect();
            serde_json::to_string(&table).unwrap_or_else(|_| val.display())
        }
        _ => val.display(),
    }
}

fn value_to_tree_json(val: &interp::Value) -> String {
    fn to_json(v: &interp::Value) -> serde_json::Value {
        match v {
            interp::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(|i| to_json(i)).collect())
            }
            interp::Value::Number(n) => {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(*n)
                        .unwrap_or_else(|| serde_json::Number::from(0))
                )
            }
            interp::Value::Bool(b) => serde_json::Value::Bool(*b),
            interp::Value::Str(s) => serde_json::Value::String(s.clone()),
            other => serde_json::Value::String(other.display()),
        }
    }
    serde_json::to_string(&to_json(val)).unwrap_or_else(|_| val.display())
}

pub fn evaluate_line(text: &str) -> Result<String, String> {
    let mut interp = interp::Interpreter::new();
    match interp.eval_expr_str(text) {
        Ok(v) => Ok(v.display()),
        Err(_) => {
            // fall back to cord-expr/cord-trig for trig and CORDIC expressions
            let graph = cord_expr::parse_expr(text)?;
            let val = cord_trig::eval::evaluate(&graph, 0.0, 0.0, 0.0);
            Ok(format_value(val))
        }
    }
}

fn format_value(val: f64) -> String {
    if val == val.trunc() && val.abs() < 1e15 {
        format!("{}", val as i64)
    } else {
        let s = format!("{:.10}", val);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}

// --- Module evaluation pipeline ---

/// Source material for a single module (block).
pub struct ModuleSource {
    /// Module name (from heading text, normalized).
    pub name: String,
    /// Raw text content of all text blocks in this module, joined.
    pub text: String,
    /// True for the root module (H1 section). Its exports are auto-imported
    /// into every other module.
    pub is_root: bool,
}

/// Per-module evaluation result.
pub struct ModuleResult {
    pub name: String,
    pub doc_result: DocumentResult,
    pub exports: interp::ModuleExports,
}

/// Evaluate modules in dependency order. Root module is evaluated first
/// and its exports are auto-imported into every other module. `use`
/// declarations are resolved via topological sort. Failed `use` (module
/// name doesn't match any source) is silently dropped.
pub fn evaluate_modules(sources: &[ModuleSource]) -> Vec<ModuleResult> {
    use std::collections::HashMap;

    // Index modules by name
    let name_to_idx: HashMap<&str, usize> = sources.iter().enumerate()
        .map(|(i, s)| (s.name.as_str(), i))
        .collect();

    // Extract use declarations from each module
    let use_decls: Vec<Vec<interp::UseDecl>> = sources.iter()
        .map(|s| interp::extract_use_declarations(&s.text))
        .collect();

    // Build adjacency list for topo sort (dependency edges: module -> modules it depends on)
    let n = sources.len();
    let mut in_degree = vec![0usize; n];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n]; // dep -> modules that depend on it

    for (i, decls) in use_decls.iter().enumerate() {
        for decl in decls {
            if let Some(&dep_idx) = name_to_idx.get(decl.module.as_str()) {
                if dep_idx != i {
                    dependents[dep_idx].push(i);
                    in_degree[i] += 1;
                }
            }
            // Unknown module names are silently ignored (failed use = prose)
        }
    }

    // Kahn's algorithm for topological sort. Root modules get priority
    // (pushed to front of queue).
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for (i, s) in sources.iter().enumerate() {
        if in_degree[i] == 0 {
            if s.is_root {
                queue.push_front(i);
            } else {
                queue.push_back(i);
            }
        }
    }

    let mut order: Vec<usize> = Vec::with_capacity(n);
    while let Some(idx) = queue.pop_front() {
        order.push(idx);
        for &dep in &dependents[idx] {
            in_degree[dep] -= 1;
            if in_degree[dep] == 0 {
                queue.push_back(dep);
            }
        }
    }

    // Any modules not in `order` are part of a cycle. Append them at
    // the end — they'll evaluate without their cyclic dependencies
    // (which means their `use`d bindings won't be available, producing
    // natural "undefined variable" errors downstream).
    for i in 0..n {
        if !order.contains(&i) {
            order.push(i);
        }
    }

    // Evaluate in topological order
    let mut exports_by_name: HashMap<String, interp::ModuleExports> = HashMap::new();
    let mut root_exports: Option<interp::ModuleExports> = None;
    let mut results: Vec<Option<ModuleResult>> = (0..n).map(|_| None).collect();

    for &idx in &order {
        let source = &sources[idx];

        // Create interpreter with imported scope
        let mut interp = interp::Interpreter::new();

        // Auto-import root module exports (unless this IS the root)
        if !source.is_root {
            if let Some(ref root_exp) = root_exports {
                interp.import_all(root_exp);
            }
        }

        // Import use'd modules' exports
        for decl in &use_decls[idx] {
            if let Some(module_exports) = exports_by_name.get(&decl.module) {
                match &decl.item {
                    Some(s) if s == "*" => {
                        interp.import_all(module_exports);
                    }
                    None => {
                        interp.import_all(module_exports);
                    }
                    Some(item) => {
                        interp.import_item(module_exports, item);
                    }
                }
            }
        }

        // Evaluate this module's text
        let doc_result = evaluate_document_with_interp(&mut interp, &source.text);
        let module_exports = interp.exports();

        if source.is_root {
            root_exports = Some(module_exports.clone());
        }
        exports_by_name.insert(source.name.clone(), module_exports.clone());

        results[idx] = Some(ModuleResult {
            name: source.name.clone(),
            doc_result,
            exports: module_exports,
        });
    }

    results.into_iter().flatten().collect()
}

/// Evaluate a document's text using an existing (pre-populated) interpreter.
pub fn evaluate_document_with_interp(interp: &mut interp::Interpreter, text: &str) -> DocumentResult {
    let classified = classify_document(text);
    let mut results = Vec::new();
    let mut errors = Vec::new();

    let mut lines: Vec<(usize, &str, bool)> = Vec::new();
    for cl in &classified {
        match cl.kind {
            LineKind::Cordial => lines.push((cl.index, &cl.content, false)),
            LineKind::Eval => lines.push((cl.index, &cl.content, true)),
            LineKind::Comment | LineKind::Markdown => {}
        }
    }

    let interp_results = interp::interpret_document_with(interp, &lines);
    for ir in interp_results {
        let fmt = match ir.format {
            interp::EvalFormat::Inline => "inline",
            interp::EvalFormat::Table => "table",
            interp::EvalFormat::Tree => "tree",
        };
        match ir.value {
            Some(interp::Value::Error(e)) => {
                errors.push(EvalError { line: ir.line, error: e });
            }
            Some(v) => {
                let s = match ir.format {
                    interp::EvalFormat::Table => value_to_table_json(&v),
                    interp::EvalFormat::Tree => value_to_tree_json(&v),
                    interp::EvalFormat::Inline => v.display(),
                };
                if !s.is_empty() {
                    results.push(EvalResult { line: ir.line, result: s, format: fmt.to_string() });
                }
            }
            None => {}
        }
    }

    DocumentResult { results, errors }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_eval() {
        let result = evaluate_line("2 + 3").unwrap();
        assert_eq!(result, "5");
    }

    #[test]
    fn eval_with_variables() {
        let doc = "let a = 5\nlet b = 3\n/= a + b";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "8");
        assert_eq!(result.results[0].line, 2);
    }

    #[test]
    fn eval_with_markdown() {
        let doc = "# Title\nlet val = 10\nSome text\n/= val * 2";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "20");
    }

    #[test]
    fn eval_trig() {
        let result = evaluate_line("sin(0)").unwrap();
        assert_eq!(result, "0");
    }

    #[test]
    fn eval_function_def() {
        let doc = "f(a) = a * a\n/= f(5)";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "25");
    }

    #[test]
    fn multiple_evals() {
        let doc = "let a = 3\n/= a\nlet b = 7\n/= a + b";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.results[0].result, "3");
        assert_eq!(result.results[1].result, "10");
    }

    #[test]
    fn format_integer() {
        assert_eq!(format_value(42.0), "42");
    }

    #[test]
    fn format_float() {
        let s = format_value(3.14);
        assert!(s.starts_with("3.14"));
    }

    #[test]
    fn eval_x_plus_5() {
        let doc = "let x = 10\n/= x + 5";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "15");
    }

    #[test]
    fn eval_string_concat() {
        let doc = "let x = \"hello\"\nlet y = \"world\"\n/= x + \" \" + y";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "hello world");
    }

    #[test]
    fn eval_booleans() {
        let doc = "let x = true\n/= x\n/= 1 > 0";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.results[0].result, "true");
        assert_eq!(result.results[1].result, "true");
    }

    #[test]
    fn eval_while_loop() {
        let doc = "let i = 0\nlet sum = 0\nwhile (i < 10) {\n    sum = sum + i\n    i = i + 1\n}\n/= sum";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "45");
    }

    #[test]
    fn eval_fn_block() {
        let doc = "fn add(a, b) {\n    a + b\n}\n/= add(3, 4)";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "7");
    }

    #[test]
    fn eval_type_annotation_int_lossy_rejected() {
        // Round-trip rule: lossy coercion is rejected.
        let doc = "let x: int = 3.7\n/= x";
        let result = evaluate_document(doc);
        assert!(result.errors.len() >= 1, "should error on lossy int");
    }

    #[test]
    fn eval_type_annotation_int_exact_accepted() {
        let doc = "let x: int = 3.0\n/= x";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "3");
    }

    #[test]
    fn eval_type_annotation_bool_error() {
        let doc = "let x: bool = 2\n/= x";
        let result = evaluate_document(doc);
        assert!(result.errors.len() >= 1);
        let msg = &result.errors[0].error;
        assert!(
            msg.contains("clean conversion") || msg.contains("cannot bind"),
            "expected clean-conversion error, got: {}", msg
        );
    }

    #[test]
    fn eval_array() {
        let doc = "let arr = [1, \"two\", true]\n/= arr";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "[1, \"two\", true]");
    }

    #[test]
    fn eval_error_recovery() {
        let doc = "let x = undefined_var\nlet y = 5\n/= y";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "5");
        assert!(result.errors.len() >= 1);
    }

    #[test]
    fn eval_mixed_markdown_and_code() {
        let doc = "# Notes\nlet x = 10\nSome text here\nwhile (x > 0) {\n    x = x - 1\n}\n/= x";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "0");
    }

    #[test]
    fn eval_if_else() {
        let doc = "let x = 10\nif (x > 5) {\n    x = 1\n} else {\n    x = 0\n}\n/= x";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "1");
    }

    #[test]
    fn eval_for_loop() {
        let doc = "let sum = 0\nfor i in [1, 2, 3] {\n    sum = sum + i\n}\n/= sum";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "6");
    }

    #[test]
    fn eval_array_index() {
        let doc = "let arr = [10, 20, 30]\n/= arr[1]";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "20");
    }

    #[test]
    fn eval_fn_return() {
        let doc = "fn max(a, b) {\n    if (a > b) {\n        return a\n    }\n    return b\n}\n/= max(3, 7)";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result, "7");
    }

    #[test]
    fn eval_table_format() {
        let doc = "let data = [[\"Name\", \"Age\"], [\"Alice\", 30], [\"Bob\", 25]]\n/=| data";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].format, "table");
        let parsed: Vec<Vec<String>> = serde_json::from_str(&result.results[0].result).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], vec!["Name", "Age"]);
    }

    #[test]
    fn eval_tree_format() {
        let doc = "let tree = [1, [2, 3], [4, [5]]]\n/=\\ tree";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].format, "tree");
        let parsed: serde_json::Value = serde_json::from_str(&result.results[0].result).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn eval_inline_format_default() {
        let doc = "let x = 42\n/= x";
        let result = evaluate_document(doc);
        assert_eq!(result.results[0].format, "inline");
    }

    #[test]
    fn eval_table_flat_array() {
        let doc = "/=| [1, 2, 3]";
        let result = evaluate_document(doc);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].format, "table");
    }

    #[test]
    fn eval_document_json_has_format() {
        let doc = "let x = 42\n/= x\n/=| [[1, 2], [3, 4]]";
        let result = evaluate_document(doc);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"format\":\"inline\""));
        assert!(json.contains("\"format\":\"table\""));
    }

    #[test]
    fn module_eval_use_imports_binding() {
        let sources = vec![
            ModuleSource { name: "root".into(), text: "let pi = 3.14".into(), is_root: true },
            ModuleSource { name: "math".into(), text: "fn double(x) {\n    x * 2\n}".into(), is_root: false },
            ModuleSource { name: "main".into(), text: "use math\n/= double(pi)".into(), is_root: false },
        ];
        let results = evaluate_modules(&sources);
        assert_eq!(results.len(), 3);
        // "main" should see both root's `pi` (auto-import) and math's `double` (via use)
        let main_result = results.iter().find(|r| r.name == "main").unwrap();
        assert_eq!(main_result.doc_result.results.len(), 1);
        assert_eq!(main_result.doc_result.results[0].result, "6.28");
    }

    #[test]
    fn module_eval_root_auto_imported() {
        let sources = vec![
            ModuleSource { name: "root".into(), text: "let x = 5".into(), is_root: true },
            ModuleSource { name: "child".into(), text: "/= x".into(), is_root: false },
        ];
        let results = evaluate_modules(&sources);
        let child = results.iter().find(|r| r.name == "child").unwrap();
        assert_eq!(child.doc_result.results.len(), 1);
        assert_eq!(child.doc_result.results[0].result, "5");
    }

    #[test]
    fn module_eval_without_use_no_access() {
        let sources = vec![
            ModuleSource { name: "root".into(), text: "".into(), is_root: true },
            ModuleSource { name: "a".into(), text: "let secret = 42".into(), is_root: false },
            ModuleSource { name: "b".into(), text: "/= secret".into(), is_root: false },
        ];
        let results = evaluate_modules(&sources);
        let b = results.iter().find(|r| r.name == "b").unwrap();
        assert_eq!(b.doc_result.errors.len(), 1);
        assert!(b.doc_result.errors[0].error.contains("undefined"));
    }

    #[test]
    fn module_eval_use_specific_item() {
        let sources = vec![
            ModuleSource { name: "root".into(), text: "".into(), is_root: true },
            ModuleSource { name: "math".into(), text: "let a = 1\nlet b = 2".into(), is_root: false },
            ModuleSource { name: "main".into(), text: "use math::a\n/= a".into(), is_root: false },
        ];
        let results = evaluate_modules(&sources);
        let main = results.iter().find(|r| r.name == "main").unwrap();
        assert_eq!(main.doc_result.results.len(), 1);
        assert_eq!(main.doc_result.results[0].result, "1");
    }

    #[test]
    fn module_eval_failed_use_no_error() {
        let sources = vec![
            ModuleSource { name: "root".into(), text: "".into(), is_root: true },
            ModuleSource { name: "main".into(), text: "use nonexistent\nlet x = 1\n/= x".into(), is_root: false },
        ];
        let results = evaluate_modules(&sources);
        let main = results.iter().find(|r| r.name == "main").unwrap();
        assert!(main.doc_result.errors.is_empty());
        assert_eq!(main.doc_result.results[0].result, "1");
    }

    #[test]
    fn module_eval_cycle_handled() {
        let sources = vec![
            ModuleSource { name: "root".into(), text: "".into(), is_root: true },
            ModuleSource { name: "a".into(), text: "use b\nlet x = 1".into(), is_root: false },
            ModuleSource { name: "b".into(), text: "use a\nlet y = 2".into(), is_root: false },
        ];
        // Shouldn't panic. One of them evaluates without the other's exports.
        let results = evaluate_modules(&sources);
        assert_eq!(results.len(), 3);
    }
}

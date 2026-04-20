use crate::selection::BlockId;

/// A module groups consecutive blocks that share a scope.
/// H2 headings start named modules; HRs close the current module
/// and start an unnamed one; H1 marks the root module.
#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub heading_block: Option<BlockId>,
    pub block_ids: Vec<BlockId>,
    pub is_root: bool,
}

/// Lightweight descriptor used by compute_modules so it doesn't need
/// access to the full Block trait (which is generic over Message).
pub struct BlockInfo {
    pub id: BlockId,
    pub kind_tag: &'static str,
    /// For heading blocks: the level (1, 2, 3). Zero for non-headings.
    pub heading_level: u8,
    /// For heading blocks: the heading text. Empty for non-headings.
    pub heading_text: String,
    /// For text blocks: the raw markdown content (used to auto-name
    /// unnamed modules from first `fn`/`let`). Empty for non-text blocks.
    pub text_content: String,
}

/// Walk blocks in layout order and group them into modules based on
/// heading/HR boundaries.
///
/// Rules:
/// - H1 -> root module (is_root = true)
/// - H2 -> close current, start named module
/// - HR -> close current, start unnamed module
/// - HR immediately followed by H1/H2 -> absorbed into the heading module
///   so the divider counts as decoration, not its own dangling block.
/// - Everything else -> append to current module
///
/// Unnamed modules are auto-named from their first `fn` or `let`
/// declaration, falling back to `_unnamed_N`.
pub fn compute_modules(infos: &[BlockInfo]) -> Vec<Module> {
    let mut modules: Vec<Module> = Vec::new();
    let mut current = Module {
        name: String::new(),
        heading_block: None,
        block_ids: Vec::new(),
        is_root: false,
    };
    let mut unnamed_counter: usize = 0;
    let mut seen_any = false;

    for info in infos {
        match (info.kind_tag, info.heading_level) {
            ("heading", 1) | ("heading", 2) => {
                let absorbed_hr = take_dangling_hr(&current, infos);
                if absorbed_hr.is_none() && (seen_any || !current.block_ids.is_empty()) {
                    finalize_unnamed(&mut current, &mut unnamed_counter, infos);
                    modules.push(current);
                }
                let block_ids = match absorbed_hr {
                    Some(hr_id) => vec![hr_id, info.id],
                    None => vec![info.id],
                };
                current = Module {
                    name: normalize_name(&info.heading_text),
                    heading_block: Some(info.id),
                    block_ids,
                    is_root: info.heading_level == 1,
                };
                seen_any = true;
            }
            ("hr", _) => {
                if seen_any || !current.block_ids.is_empty() {
                    finalize_unnamed(&mut current, &mut unnamed_counter, infos);
                    modules.push(current);
                }
                current = Module {
                    name: String::new(),
                    heading_block: None,
                    block_ids: vec![info.id],
                    is_root: false,
                };
                seen_any = true;
            }
            _ => {
                current.block_ids.push(info.id);
            }
        }
    }

    if !current.block_ids.is_empty() || seen_any {
        finalize_unnamed(&mut current, &mut unnamed_counter, infos);
        modules.push(current);
    }

    modules
}

/// Returns the HR block id if `current` is a freshly-opened HR-only module
/// (one block, no heading) — meaning the HR immediately precedes the caller's
/// heading and should be folded into it. None otherwise.
fn take_dangling_hr(current: &Module, infos: &[BlockInfo]) -> Option<BlockId> {
    if current.block_ids.len() != 1 || current.heading_block.is_some() {
        return None;
    }
    let only_id = current.block_ids[0];
    infos.iter()
        .find(|i| i.id == only_id && i.kind_tag == "hr")
        .map(|i| i.id)
}

/// If a module has no name, derive one from its first `fn`/`let` declaration.
fn finalize_unnamed(module: &mut Module, counter: &mut usize, infos: &[BlockInfo]) {
    if !module.name.is_empty() {
        return;
    }

    for &id in &module.block_ids {
        let Some(info) = infos.iter().find(|i| i.id == id) else { continue };
        if info.kind_tag != "text" { continue; }
        for line in info.text_content.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("fn ") {
                if let Some(name) = extract_ident(rest) {
                    module.name = name;
                    return;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("let ") {
                if let Some(name) = extract_ident(rest) {
                    module.name = name;
                    return;
                }
            }
        }
    }

    *counter += 1;
    module.name = format!("_unnamed_{counter}");
}

fn extract_ident(s: &str) -> Option<String> {
    let s = s.trim_start();
    let ident: String = s.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() { None } else { Some(ident) }
}

/// Scope of a table name assigned by a heading above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableNameScope {
    /// H3 heading — name is globally visible across all modules.
    Global,
    /// H4 heading — name is only visible within the owning module.
    BlockScoped,
}

/// Result of scanning for heading-named tables.
#[derive(Debug, Clone)]
pub struct TableNameAssignment {
    pub table_id: BlockId,
    pub name: String,
    pub scope: TableNameScope,
}

/// Scan the layout for H3/H4 headings directly above a table (only
/// whitespace/empty blocks between) and return name assignments.
pub fn detect_table_names(infos: &[BlockInfo]) -> Vec<TableNameAssignment> {
    let mut assignments = Vec::new();
    let len = infos.len();

    for i in 0..len {
        let level = infos[i].heading_level;
        if infos[i].kind_tag != "heading" || (level != 3 && level != 4) {
            continue;
        }
        // Look ahead for the next non-heading block. Skip whitespace-only
        // text blocks between the heading and the table.
        for j in (i + 1)..len {
            match infos[j].kind_tag {
                "table" => {
                    let scope = if level == 3 {
                        TableNameScope::Global
                    } else {
                        TableNameScope::BlockScoped
                    };
                    assignments.push(TableNameAssignment {
                        table_id: infos[j].id,
                        name: infos[i].heading_text.trim().to_string(),
                        scope,
                    });
                    break;
                }
                "text" if infos[j].text_content.trim().is_empty() => {
                    continue;
                }
                _ => break,
            }
        }
    }

    assignments
}

/// Lowercase, spaces to underscores, strip non-ident characters.
pub fn normalize_name(heading_text: &str) -> String {
    heading_text
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '_' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Positional fallback names for every block and table in the document,
/// assigned globally in layout order (1-indexed: `block_1`, `table_1`, …).
/// Headings and HRs count as blocks; tables also get their own sequence.
/// Cross-block refs use `block_N::table_N`. Heading-derived names from
/// `detect_table_names` take precedence — positional names are always
/// available as an additional lookup key.
pub fn compute_positional_ids(infos: &[BlockInfo]) -> PositionalIds {
    let mut blocks = Vec::new();
    let mut tables = Vec::new();
    let mut block_counter: usize = 0;
    let mut table_counter: usize = 0;
    for info in infos {
        block_counter += 1;
        blocks.push((info.id, format!("block_{}", block_counter)));
        if info.kind_tag == "table" {
            table_counter += 1;
            tables.push((info.id, format!("table_{}", table_counter), block_counter));
        }
    }
    PositionalIds { blocks, tables }
}

/// Output of `compute_positional_ids`. `tables` entries also carry the
/// 1-indexed block position the table appears in, so the caller can build
/// the cross-block alias `block_N::table_M`.
pub struct PositionalIds {
    pub blocks: Vec<(BlockId, String)>,
    pub tables: Vec<(BlockId, String, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_heading_names() {
        assert_eq!(normalize_name("Budget"), "budget");
        assert_eq!(normalize_name("My Calculations"), "my_calculations");
        assert_eq!(normalize_name("  Phase 2 — Design  "), "phase_2__design");
    }

    #[test]
    fn extract_ident_basic() {
        assert_eq!(extract_ident("ramp(s, d)"), Some("ramp".into()));
        assert_eq!(extract_ident("x = 5"), Some("x".into()));
        assert_eq!(extract_ident("  my_var: int = 3"), Some("my_var".into()));
        assert_eq!(extract_ident(""), None);
    }

    fn info(id: BlockId, kind: &'static str, level: u8, heading: &str, text: &str) -> BlockInfo {
        BlockInfo {
            id,
            kind_tag: kind,
            heading_level: level,
            heading_text: heading.to_string(),
            text_content: text.to_string(),
        }
    }

    #[test]
    fn basic_module_structure() {
        let infos = vec![
            info(1, "heading", 1, "Title", ""),
            info(2, "text", 0, "", "let pi = 3.14"),
            info(3, "heading", 2, "Calculations", ""),
            info(4, "text", 0, "", "fn ramp(s, d) {\n    s * d\n}"),
            info(5, "hr", 0, "", ""),
            info(6, "text", 0, "", "some prose"),
        ];

        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 3);

        assert_eq!(modules[0].name, "title");
        assert!(modules[0].is_root);
        assert_eq!(modules[0].block_ids, vec![1, 2]);

        assert_eq!(modules[1].name, "calculations");
        assert!(!modules[1].is_root);
        assert_eq!(modules[1].block_ids, vec![3, 4]);

        // HR starts an unnamed module; subsequent text joins it
        assert_eq!(modules[2].name, "_unnamed_1");
        assert_eq!(modules[2].block_ids, vec![5, 6]);
    }

    #[test]
    fn unnamed_module_gets_fn_name() {
        let infos = vec![
            info(1, "hr", 0, "", ""),
            info(2, "text", 0, "", "fn helper(x) = x * 2\nlet y = 3"),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "helper");
    }

    #[test]
    fn unnamed_module_gets_let_name() {
        let infos = vec![
            info(1, "hr", 0, "", ""),
            info(2, "text", 0, "", "Just prose\nlet total = 100"),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "total");
    }

    #[test]
    fn h3_does_not_split_module() {
        let infos = vec![
            info(1, "heading", 2, "Budget", ""),
            info(2, "heading", 3, "Details", ""),
            info(3, "text", 0, "", "content"),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "budget");
        assert_eq!(modules[0].block_ids, vec![1, 2, 3]);
    }

    #[test]
    fn empty_input() {
        let modules = compute_modules(&[]);
        assert!(modules.is_empty());
    }

    #[test]
    fn hr_collapses_into_following_h2() {
        let infos = vec![
            info(1, "text", 0, "", "preamble"),
            info(2, "hr", 0, "", ""),
            info(3, "heading", 2, "Section", ""),
            info(4, "text", 0, "", "content"),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].block_ids, vec![1]);
        assert_eq!(modules[1].name, "section");
        assert_eq!(modules[1].block_ids, vec![2, 3, 4]);
    }

    #[test]
    fn hr_collapses_into_following_h1() {
        let infos = vec![
            info(1, "text", 0, "", "preamble"),
            info(2, "hr", 0, "", ""),
            info(3, "heading", 1, "Title", ""),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[1].name, "title");
        assert!(modules[1].is_root);
        assert_eq!(modules[1].block_ids, vec![2, 3]);
    }

    #[test]
    fn hr_does_not_collapse_when_followed_by_text() {
        let infos = vec![
            info(1, "hr", 0, "", ""),
            info(2, "text", 0, "", "let total = 1"),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].block_ids, vec![1, 2]);
        assert_eq!(modules[0].name, "total");
    }

    #[test]
    fn consecutive_hrs_only_last_is_absorbed() {
        let infos = vec![
            info(1, "hr", 0, "", ""),
            info(2, "hr", 0, "", ""),
            info(3, "heading", 2, "Section", ""),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].block_ids, vec![1]);
        assert_eq!(modules[1].name, "section");
        assert_eq!(modules[1].block_ids, vec![2, 3]);
    }

    #[test]
    fn text_before_any_heading() {
        let infos = vec![
            info(1, "text", 0, "", "some preamble"),
            info(2, "heading", 1, "Title", ""),
        ];
        let modules = compute_modules(&infos);
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].name, "_unnamed_1");
        assert!(!modules[0].is_root);
        assert_eq!(modules[1].name, "title");
        assert!(modules[1].is_root);
    }

    #[test]
    fn h3_names_table_globally() {
        let infos = vec![
            info(1, "heading", 3, "Revenue", ""),
            info(2, "table", 0, "", ""),
        ];
        let names = detect_table_names(&infos);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].table_id, 2);
        assert_eq!(names[0].name, "Revenue");
        assert_eq!(names[0].scope, TableNameScope::Global);
    }

    #[test]
    fn h4_names_table_block_scoped() {
        let infos = vec![
            info(1, "heading", 4, "Internal", ""),
            info(2, "table", 0, "", ""),
        ];
        let names = detect_table_names(&infos);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].scope, TableNameScope::BlockScoped);
    }

    #[test]
    fn h3_without_table_below_is_ignored() {
        let infos = vec![
            info(1, "heading", 3, "Just a heading", ""),
            info(2, "text", 0, "", "no table here"),
        ];
        let names = detect_table_names(&infos);
        assert!(names.is_empty());
    }

    #[test]
    fn h3_with_whitespace_gap_names_table() {
        let infos = vec![
            info(1, "heading", 3, "Revenue", ""),
            info(2, "text", 0, "", "   \n  "),
            info(3, "table", 0, "", ""),
        ];
        let names = detect_table_names(&infos);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].table_id, 3);
    }

    #[test]
    fn positional_ids_global_ordering() {
        let infos = vec![
            info(10, "heading", 1, "Doc", ""),    // block_1
            info(11, "table", 0, "", ""),         // block_2, table_1
            info(12, "heading", 2, "Section", ""),// block_3
            info(13, "text", 0, "", "prose"),     // block_4
            info(14, "table", 0, "", ""),         // block_5, table_2
        ];
        let ids = compute_positional_ids(&infos);
        assert_eq!(ids.blocks.len(), 5);
        assert_eq!(ids.blocks[0], (10, "block_1".into()));
        assert_eq!(ids.blocks[4], (14, "block_5".into()));
        assert_eq!(ids.tables.len(), 2);
        assert_eq!(ids.tables[0], (11, "table_1".into(), 2));
        assert_eq!(ids.tables[1], (14, "table_2".into(), 5));
    }
}

//! Export a note as a standalone Rust crate. The crate mirrors the sidecar
//! ZIP's structure (src/blocks/*.cord + config.toml) but is written to a
//! user-chosen folder on disk with the full Cargo scaffolding (Cargo.toml,
//! build.sh, install.sh, README.md, src/main.rs, src/lib.rs).
//!
//! The main module (src/main.rs) runs a REPL using acord-core's interpreter.
//! Each `.cord` block is a submodule loaded into the REPL's scope at startup.
//! AOT codegen (Cordial → Rust source) is planned separately — build.sh is a
//! stub that currently just does `cargo build --release` of the REPL binary.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::editor::EditorState;
use crate::heading_block::HeadingBlock;
use crate::text_block::TextBlock;

/// Convert a free-form string to hyphen-form for use as a crate/folder name.
/// Lowercase, spaces and underscores become `-`, non-alphanumeric stripped.
pub fn to_hyphen_name(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' || c == '_' { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Export the current note as a standalone Rust crate at `out_dir`. The
/// folder name is the crate name; spaces/underscores in the user-supplied
/// name get converted to hyphens. Returns Ok(path) on success.
pub fn export_crate(state: &EditorState, out_dir: &Path, name: &str) -> Result<PathBuf, String> {
    let crate_name = to_hyphen_name(name);
    if crate_name.is_empty() {
        return Err("crate name is empty after normalization".into());
    }
    let crate_dir = out_dir.join(&crate_name);
    let src_dir = crate_dir.join("src");
    let blocks_dir = src_dir.join("blocks");

    fs::create_dir_all(&blocks_dir)
        .map_err(|e| format!("create {}: {}", blocks_dir.display(), e))?;

    // Write per-block .cord files (reuses the same format as the sidecar)
    let block_files = state.build_block_files();
    for file in &block_files {
        let path = blocks_dir.join(&file.filename);
        write_file(&path, &file.content)?;
    }

    // Write the three scaffolding files: Cargo.toml, main.rs, lib.rs
    write_file(&crate_dir.join("Cargo.toml"), &cargo_toml(&crate_name))?;
    write_file(&src_dir.join("main.rs"), &main_rs(&crate_name, &block_files))?;
    write_file(&src_dir.join("lib.rs"), &lib_rs(&block_files))?;

    // Scripts + README + gitignore
    let build_path = crate_dir.join("build.sh");
    write_file(&build_path, &build_sh(&crate_name))?;
    make_executable(&build_path)?;
    let install_path = crate_dir.join("install.sh");
    write_file(&install_path, &install_sh(&crate_name))?;
    make_executable(&install_path)?;
    write_file(&crate_dir.join("README.md"), &readme_md(state, &crate_name))?;
    write_file(&crate_dir.join(".gitignore"), "target/\nCargo.lock\n")?;

    Ok(crate_dir)
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    let mut f = fs::File::create(path)
        .map_err(|e| format!("create {}: {}", path.display(), e))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("write {}: {}", path.display(), e))?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("metadata {}: {}", path.display(), e))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("chmod {}: {}", path.display(), e))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> { Ok(()) }

fn cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
acord-core = {{ git = "https://git.else-if.org/jess/Acord.git", package = "acord-core" }}
"#,
    )
}

fn main_rs(name: &str, block_files: &[crate::sidecar::BlockFile]) -> String {
    let mut includes = String::new();
    let mut init_lines = String::new();
    for file in block_files {
        let var = ident_from_filename(&file.filename);
        includes.push_str(&format!(
            "const {}: &str = include_str!(\"blocks/{}\");\n",
            var.to_uppercase(),
            file.filename
        ));
        init_lines.push_str(&format!(
            "    load_block(&mut interp, {});\n",
            var.to_uppercase()
        ));
    }
    format!(
        r#"//! {name} — exported Acord note running as a REPL.
//!
//! `cargo run` drops you into an interactive Cordial prompt with every block
//! from the note pre-loaded. It's the notepad experience, minus the UI.
//!
//! Type expressions, call functions, reference tables. `:list` to inspect,
//! `:q` to quit.

use acord_core::interp::Interpreter;
use std::io::{{self, BufRead, Write}};

{includes}

fn main() {{
    let mut interp = Interpreter::new();
{init_lines}
    println!("{{}} REPL — :list to show bindings, :q to quit", env!("CARGO_PKG_NAME"));
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let mut line = String::new();
    loop {{
        write!(out, "> ").ok();
        out.flush().ok();
        line.clear();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {{ break; }}
        let trimmed = line.trim();
        if trimmed.is_empty() {{ continue; }}
        if trimmed == ":q" || trimmed == ":quit" {{ break; }}
        if trimmed == ":list" {{
            list_bindings(&interp);
            continue;
        }}
        match interp.exec_line(trimmed) {{
            Ok(Some(v)) => println!("{{}}", v.display()),
            Ok(None) => {{}}
            Err(e) => eprintln!("error: {{}}", e),
        }}
    }}
}}

/// Strip the `.cord` front-matter (everything up to and including the first
/// standalone `---`) and evaluate the remaining source into `interp`.
/// Lines that fail to parse are silently skipped — same as in the notepad.
fn load_block(interp: &mut Interpreter, source: &str) {{
    let body = strip_front_matter(source);
    for line in body.lines() {{
        let _ = interp.exec_line(line);
    }}
}}

fn strip_front_matter(src: &str) -> &str {{
    if !src.starts_with("---") {{ return src; }}
    let mut lines = src.split_inclusive('\n');
    // skip opening ---
    lines.next();
    // skip through closing ---
    let mut consumed = 0;
    for line in &mut lines {{
        consumed += line.len();
        if line.trim_end_matches('\n').trim() == "---" {{ break; }}
    }}
    &src[3 + consumed..]
}}

fn list_bindings(_interp: &Interpreter) {{
    // TODO: when acord-core exposes a public bindings iterator, enumerate here.
    // For now, users discover bindings by referencing them.
    println!("(binding enumeration not yet implemented)");
}}
"#,
    )
}

fn lib_rs(block_files: &[crate::sidecar::BlockFile]) -> String {
    let mut includes = String::new();
    let mut init_lines = String::new();
    for file in block_files {
        let var = ident_from_filename(&file.filename);
        includes.push_str(&format!(
            "const {}: &str = include_str!(\"blocks/{}\");\n",
            var.to_uppercase(),
            file.filename
        ));
        init_lines.push_str(&format!(
            "    load_block(&mut interp, {});\n",
            var.to_uppercase()
        ));
    }
    format!(
        r#"//! Exposes this note's loaded interpreter for use from other Rust projects.
//!
//! Example:
//! ```no_run
//! let mut interp = my_note::load();
//! let v = interp.exec_line("my_fn(1, 2, 3)").unwrap();
//! ```

use acord_core::interp::Interpreter;

{includes}

pub fn load() -> Interpreter {{
    let mut interp = Interpreter::new();
{init_lines}
    interp
}}

fn load_block(interp: &mut Interpreter, source: &str) {{
    let body = strip_front_matter(source);
    for line in body.lines() {{
        let _ = interp.exec_line(line);
    }}
}}

fn strip_front_matter(src: &str) -> &str {{
    if !src.starts_with("---") {{ return src; }}
    let mut lines = src.split_inclusive('\n');
    lines.next();
    let mut consumed = 0;
    for line in &mut lines {{
        consumed += line.len();
        if line.trim_end_matches('\n').trim() == "---" {{ break; }}
    }}
    &src[3 + consumed..]
}}
"#,
    )
}

fn build_sh(_name: &str) -> String {
    r#"#!/usr/bin/env bash
set -e
# TODO: AOT codegen — compile .cord sources to Rust source, produce a static
# binary with zero interpreter overhead. See the cordial-to-rust-codegen plan
# for the design. Until then, this builds the REPL binary in release mode.
cargo build --release
echo "Built target/release/$(basename "$PWD")"
"#
    .into()
}

fn install_sh(name: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -e
NAME="{name}"
DEST="${{HOME}}/.acord/bin"

if [ ! -f "target/release/${{NAME}}" ]; then
    echo "No release binary found. Running ./build.sh first..."
    ./build.sh
fi

mkdir -p "$DEST"
cp "target/release/${{NAME}}" "$DEST/${{NAME}}"
chmod +x "$DEST/${{NAME}}"

echo "Installed: ${{DEST}}/${{NAME}}"
echo ""
echo "Add ~/.acord/bin to your PATH if you haven't already:"
echo ""
echo "  # zsh:"
echo "  echo 'export PATH=\"\$HOME/.acord/bin:\$PATH\"' >> ~/.zshrc && source ~/.zshrc"
echo ""
echo "  # bash:"
echo "  echo 'export PATH=\"\$HOME/.acord/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
"#,
    )
}

fn readme_md(state: &EditorState, name: &str) -> String {
    let mut inventory = String::new();
    for block_id in state.layout.iter() {
        let Some(block) = state.registry.get(block_id) else { continue };
        let kind = block.kind_tag();
        if let Some(hb) = block.as_any().downcast_ref::<HeadingBlock>() {
            inventory.push_str(&format!(
                "- **{kind}** (level {}) — `{}`\n",
                hb.level as u8 + 1,
                hb.text.trim()
            ));
        } else if let Some(tb) = block.as_any().downcast_ref::<TextBlock>() {
            let first_line = tb.content.text();
            let preview = first_line.lines().next().unwrap_or("").trim();
            if !preview.is_empty() {
                inventory.push_str(&format!("- **{kind}** — {}\n", truncate(preview, 60)));
            } else {
                inventory.push_str(&format!("- **{kind}**\n"));
            }
        } else {
            inventory.push_str(&format!("- **{kind}**\n"));
        }
    }

    format!(
        r#"# {name}

This is your Acord note, exported as a standalone Rust crate.

## Run

- `cargo run` — interactive Cordial REPL with every binding from your note pre-loaded. Call functions, reference tables, mutate variables — exactly like opening a new block below the existing ones in the notepad.
- `./build.sh` — build the release binary.
- `./install.sh` — install the binary to `~/.acord/bin` and print PATH setup instructions.

## Blocks

{inventory}
## Use from another Rust project

Add this crate to your `Cargo.toml` as a path dependency, then:

```rust
use {name}::load;
let mut interp = load();
let v = interp.exec_line("my_fn(1, 2, 3)").unwrap();
println!("{{}}", v.unwrap().display());
```

## Notes

- Future versions will AOT-compile your `.cord` sources into native Rust via `./build.sh`, producing binaries with zero interpreter overhead. Today the binary uses the interpreter at runtime.
- This crate depends on `acord-core` via git. Make sure the host has network access on first build, or switch to a path/vendored dep for offline environments.
"#,
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str("...");
        out
    }
}

fn ident_from_filename(filename: &str) -> String {
    let stem = filename.trim_end_matches(".cord");
    stem.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_start_matches(|c: char| !c.is_alphabetic() && c != '_')
        .to_string()
}

#[allow(dead_code)]
fn derive_name_from_first_line(text: &str) -> String {
    let first = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let cleaned = first.trim_start_matches('#').trim();
    let first_two: Vec<&str> = cleaned.split_whitespace().take(2).collect();
    to_hyphen_name(&first_two.join(" "))
}

use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const KNOWN_PLATFORMS: &[&str] = &["macos", "windows", "linux", "ios"];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");

    if cmd.is_empty() || cmd == "help" || cmd == "--help" || cmd == "-h" {
        print_help();
        return ExitCode::from(2);
    }

    let extra_args: Vec<&String> = args.iter().skip(1).collect();
    let (action, platform) = parse(cmd);

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest must have a parent")
        .to_path_buf();

    let (script, runner) = match platform.as_str() {
        "windows" => (
            repo_root.join(format!("scripts/windows/{action}.ps1")),
            vec![
                "powershell",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ],
        ),
        "linux" | "macos" | "ios" => (
            repo_root.join(format!("scripts/{platform}/{action}.sh")),
            vec!["bash"],
        ),
        other => {
            eprintln!("unknown platform: {other}");
            return ExitCode::from(2);
        }
    };

    if !script.exists() {
        eprintln!("script not found: {}", script.display());
        return ExitCode::from(1);
    }

    let extra_display = if extra_args.is_empty() {
        String::new()
    } else {
        format!(
            " {}",
            extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "),
        )
    };
    eprintln!("→ {} {}{}", runner.join(" "), script.display(), extra_display);

    let mut command = Command::new(runner[0]);
    for arg in &runner[1..] {
        command.arg(arg);
    }
    command.arg(&script);
    for a in &extra_args {
        command.arg(a.as_str());
    }
    command.current_dir(&repo_root);

    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(e) => {
            eprintln!("failed to run {}: {e}", script.display());
            ExitCode::from(1)
        }
    }
}

fn parse(cmd: &str) -> (String, String) {
    if let Some(idx) = cmd.rfind('-') {
        let suffix = &cmd[idx + 1..];
        if KNOWN_PLATFORMS.contains(&suffix) {
            return (cmd[..idx].to_string(), suffix.to_string());
        }
    }
    (cmd.to_string(), current_platform().to_string())
}

fn current_platform() -> &'static str {
    match env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => {
            eprintln!("unsupported OS: {other}");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    eprintln!("usage: cargo xtask <command>");
    eprintln!();
    eprintln!("commands:");
    eprintln!("  build              release build for the current platform");
    eprintln!("  install            release build + install (macOS: /Applications)");
    eprintln!("  debug              debug build + foreground launch");
    eprintln!("  build-universal    universal binary for the current platform");
    eprintln!("  package            cross-compile + zip distributables");
    eprintln!("                       --all              all six targets");
    eprintln!("                       --target <name>    e.g. macos-aarch64, windows-x86_64");
    eprintln!();
    eprintln!("append -macos / -windows / -linux / -ios to any command to force a platform.");
    eprintln!("  e.g. cargo xtask build-universal-macos");
    eprintln!();
    eprintln!("ios:");
    eprintln!("  cargo xtask build-ios     build the .app bundle for the iPad simulator");
    eprintln!("  cargo xtask install-ios   build + install + launch (paired device wins, else sim)");
    eprintln!("  cargo xtask xcodeproj-ios generate Acord.xcodeproj for finishing in Xcode");
}

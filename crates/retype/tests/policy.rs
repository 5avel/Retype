//! Repository policy checks (SPEC §4, `.claude/rules/security.md`).
//!
//! These scan the source tree instead of the binary: they are cheap, run on
//! any OS with `cargo test`, and fail loudly when someone adds networking or
//! `unsafe` in the wrong place.

use std::fs;
use std::path::{Path, PathBuf};

/// Substrings that must never appear in Rust sources or Cargo manifests.
const FORBIDDEN_NETWORK: &[&str] = &[
    "std::net",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "Win32_Networking",
    "Win32_NetworkManagement",
    "WSAStartup",
    "InternetConnect",
    "isahc",
    "async-std",
    "async_std",
    "WinHttp",
    "InternetOpen",
    "URLDownloadToFile",
    "reqwest",
    "ureq",
    "hyper",
];

/// The only directory where `unsafe` code may appear.
const UNSAFE_ALLOWED_DIR: &str = "crates/retype/src/win";

#[test]
fn no_networking_apis_or_crates() {
    let violations = scan(|rel, line| {
        FORBIDDEN_NETWORK
            .iter()
            .find(|pattern| line.contains(*pattern))
            .map(|pattern| format!("{rel}: forbidden networking reference `{pattern}`"))
    });
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn unsafe_only_in_win_module() {
    let violations = scan(|rel, line| {
        let is_rust = rel.ends_with(".rs");
        let code = line.split("//").next().unwrap_or_default();
        let has_unsafe = [
            "unsafe {",
            "unsafe{",
            "unsafe fn",
            "unsafe impl",
            "unsafe extern",
        ]
        .iter()
        .any(|pattern| code.contains(pattern));
        (is_rust && has_unsafe && !rel.starts_with(UNSAFE_ALLOWED_DIR))
            .then(|| format!("{rel}: `unsafe` outside {UNSAFE_ALLOWED_DIR}"))
    });
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

/// Applies `check` to every line of every `.rs` / `Cargo.toml` under `crates/`
/// except this file, returning the collected violations with line numbers.
fn scan(check: impl Fn(&str, &str) -> Option<String>) -> Vec<String> {
    let root = workspace_root();
    let mut violations = Vec::new();
    for path in source_files(&root.join("crates")) {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.ends_with("tests/policy.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap_or_default();
        for (index, line) in text.lines().enumerate() {
            if let Some(message) = check(&rel, line) {
                violations.push(format!("{message} (line {})", index + 1));
            }
        }
    }
    violations
}

fn source_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(source_files(&path));
        } else if is_checked_file(&path) {
            files.push(path);
        }
    }
    files
}

fn is_checked_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    name.ends_with(".rs") || name == "Cargo.toml"
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/retype
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

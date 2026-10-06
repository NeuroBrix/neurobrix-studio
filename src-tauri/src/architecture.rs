//! Mechanical boundary checks. This guard cannot see semantic duplication
//! (a re-implementation of engine behaviour); rejecting that remains a
//! code-review responsibility.
#[cfg(test)]
mod guard {
    use std::fs;
    use std::path::{Path, PathBuf};

    // Concatenated so this file does not itself contain the banned token.
    const PROCESS_MARKER: &str = concat!("std::", "process");
    const COMMAND_MARKER: &str = concat!("Command:", ":new");

    fn walk(dir: &Path, out: &mut Vec<PathBuf>, extensions: &[&str]) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "target" || name == "gen" || name == "node_modules"
            {
                continue;
            }
            if path.is_dir() {
                walk(&path, out, extensions)?;
            } else if extensions.iter().any(|e| name.ends_with(e)) {
                out.push(path);
            }
        }
        Ok(())
    }

    #[test]
    fn rust_process_execution_lives_only_in_the_engine_module() {
        let mut files = Vec::new();
        walk(Path::new("src"), &mut files, &[".rs"]).expect("rust sources must be readable");
        assert!(
            !files.is_empty(),
            "rust sources must be reachable from cargo test"
        );
        for path in files {
            if path.ends_with("architecture.rs") {
                continue; // this guard is the exception, not a violation
            }
            let source = fs::read_to_string(&path).expect("readable source");
            let inside_engine = path.starts_with("src/engine");
            for marker in [PROCESS_MARKER, COMMAND_MARKER] {
                assert!(
                    inside_engine || !source.contains(marker),
                    "{} uses process execution outside src/engine/",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn frontend_never_executes_processes_or_shells() {
        let frontend = Path::new("../src");
        let mut files = Vec::new();
        walk(frontend, &mut files, &[".ts", ".svelte", ".js"])
            .expect("frontend sources must be readable");
        assert!(
            !files.is_empty(),
            "frontend sources must be reachable from cargo test"
        );
        for path in files {
            let source = fs::read_to_string(&path).expect("readable source");
            for banned in [
                concat!("child_", "process"),
                concat!("@tauri-apps/plugin-", "shell"),
                concat!("node:", "fs"),
            ] {
                assert!(
                    !source.contains(banned),
                    "{} reaches {}",
                    path.display(),
                    banned
                );
            }
        }
    }

    #[test]
    fn the_engine_module_never_invokes_python_directly() {
        let mut files = Vec::new();
        walk(Path::new("src/engine"), &mut files, &[".rs"])
            .expect("engine sources must be readable");
        assert!(
            !files.is_empty(),
            "engine sources must be reachable from cargo test"
        );
        for path in files {
            let source = fs::read_to_string(&path).expect("readable source");
            assert!(
                !source.contains(concat!("py", "thon")) && !source.contains(concat!("p", "ip ")),
                "{} addresses the Python runtime directly; only the public CLI is allowed",
                path.display()
            );
        }
    }

    #[test]
    fn walk_fails_when_a_directory_cannot_be_read() {
        // read_dir on a regular file errors on every platform, so a file
        // is the cheapest portable unreadable directory.
        let mut files = Vec::new();
        assert!(
            walk(Path::new("Cargo.toml"), &mut files, &[".rs"]).is_err(),
            "walk must propagate read_dir errors instead of silently skipping them"
        );
    }
}

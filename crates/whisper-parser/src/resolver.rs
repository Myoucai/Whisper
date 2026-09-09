//! Module import resolver for Whisper.
//!
//! Resolves `import "path"` statements by reading and parsing the
//! referenced `.ws` file, extracting word definitions, and merging
//! them into the importing program's AST.
//!
//! Search order (first match wins):
//!   1. Relative to the importing file's directory
//!   2. `$WHISPER_HOME/stdlib/`
//!   3. `$WHISPER_HOME/packages/`
//!
//! Transitive imports are resolved recursively with cycle detection.

use crate::ast::AstNode;
use crate::Parser;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Search paths for module resolution.
fn search_paths(source_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![source_dir.to_path_buf()];

    // Binary-relative paths (development and installed layouts).
    // Try several common repo-root / install-prefix locations.
    // The "std/" import prefix maps to "stdlib/" under these roots.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // Cargo target/{release,debug}: repo root is exe_dir/../../
            if let Some(repo_root) = exe_dir.parent().and_then(|p| p.parent()) {
                paths.push(repo_root.to_path_buf());
            }
            // Installed layout: share dir next to bin
            if let Some(prefix) = exe_dir.parent() {
                paths.push(prefix.join("share").join("whisper"));
            }
        }
    }

    // WHISPER_HOME defaults to ~/.whisper.
    // Imports like "std/X" resolve to WHISPER_HOME/stdlib/X.ws.
    let whisper_home = std::env::var("WHISPER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let base = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".into());
            PathBuf::from(base).join(".whisper")
        });

    paths.push(whisper_home.clone());
    // Installed packages live in $WHISPER_HOME/packages/<name>/ —
    // `import "pkgname/module"` resolves against that root.
    paths.push(whisper_home.join("packages"));
    paths
}

/// Resolve an import path to a filesystem path.
///
/// The `std/` prefix maps to the `stdlib/` directory in search paths.
/// E.g. `"std/math"` → `{search_dir}/stdlib/math.ws`.
fn resolve_module(import_path: &str, source_dir: &Path) -> Option<PathBuf> {
    let clean = import_path.trim_matches('"');

    // Map "std/X" → "stdlib/X"
    let (dir_prefix, module_path) = if let Some(rest) = clean.strip_prefix("std/") {
        ("stdlib", rest)
    } else {
        ("", clean)
    };

    let filename = if module_path.ends_with(".ws") {
        module_path.to_string()
    } else {
        format!("{module_path}.ws")
    };

    for base in &search_paths(source_dir) {
        let candidate = if dir_prefix.is_empty() {
            base.join(&filename)
        } else {
            base.join(dir_prefix).join(&filename)
        };
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Result of resolving all imports in an AST.
#[derive(Debug)]
pub struct ResolvedModule {
    /// The combined AST: imported Def nodes + original non-import nodes.
    pub ast: Vec<AstNode>,
    /// Files that were loaded (for diagnostics).
    pub loaded: Vec<PathBuf>,
}

/// Resolve all `import` statements in the given AST.
///
/// Walks the AST recursively, loading imported files and collecting
/// their word definitions.  Import nodes are removed from the result.
/// Cycles are detected and skipped.
///
/// **Export enforcement**: Only definitions marked with `export` in the
/// imported module are included. Non-exported definitions are filtered out.
/// If a module has no `export` statements, all definitions are included
/// (backward compatibility).
///
/// **Conflict detection**: If two imported modules export the same word
/// name, a warning is printed to stderr (compilation continues).
pub fn resolve_imports(ast: Vec<AstNode>, source_dir: &Path) -> Result<ResolvedModule, String> {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut exported_names: HashMap<String, String> = HashMap::new();
    resolve_imports_inner(ast, source_dir, &mut visited, &mut exported_names)
}

/// Internal recursive resolver with shared cycle-tracking set.
fn resolve_imports_inner(
    ast: Vec<AstNode>,
    source_dir: &Path,
    visited: &mut HashSet<PathBuf>,
    exported_names: &mut HashMap<String, String>,
) -> Result<ResolvedModule, String> {
    let mut imported_defs: Vec<AstNode> = Vec::new();
    let mut loaded: Vec<PathBuf> = Vec::new();

    for node in &ast {
        if let AstNode::Import(path) = node {
            let resolved = resolve_module(path, source_dir).ok_or_else(|| {
                format!(
                    "Module not found: {path}\n  Searched: {:?}",
                    search_paths(source_dir)
                )
            })?;

            // Canonicalise to detect cycles
            let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

            if visited.contains(&canonical) {
                continue; // Already loaded — skip
            }
            visited.insert(canonical.clone());

            // Read and parse the module
            let source = std::fs::read_to_string(&resolved)
                .map_err(|e| format!("Cannot read module '{path}' ({resolved:?}): {e}"))?;

            let module_ast = Parser::parse_source(&source)
                .map_err(|e| format!("Parse error in module '{path}': {}", e.message))?;

            // Recursively resolve imports within the module (shared visited set).
            // The sub-result's own-defs filter (bottom of this function) has
            // already export-filtered the module's OWN defs, while its
            // transitive imports' defs pass through unfiltered (they were
            // filtered by their own modules) — this is what makes import
            // order irrelevant.
            let module_dir = resolved.parent().unwrap_or(source_dir);
            let sub = resolve_imports_inner(module_ast, module_dir, visited, exported_names)?;

            // Conflict detection over everything this module contributes
            for def_node in &sub.ast {
                if let AstNode::Def { name, .. } = def_node {
                    if let Some(prev_module) = exported_names.get(name) {
                        eprintln!(
                            "Warning: word '{name}' exported by both '{prev_module}' and '{path}'"
                        );
                    }
                    exported_names.insert(name.clone(), path.clone());
                }
            }

            imported_defs.extend(sub.ast);
            loaded.extend(sub.loaded);
            loaded.push(resolved);
        }
    }

    // Own defs of THIS compilation unit: if the unit declares exports, only
    // exported defs are visible to importers; transitive imports above are
    // never subject to this filter.
    let own_exports = collect_exports(&ast);
    let mut result: Vec<AstNode> = imported_defs;
    for node in ast {
        match node {
            AstNode::Import(_) | AstNode::Export(_) => {}
            AstNode::Def { .. } => {
                let exported = match &node {
                    AstNode::Def { name, .. } => {
                        own_exports.is_empty() || own_exports.contains(name)
                    }
                    _ => unreachable!(),
                };
                if exported {
                    result.push(node);
                }
            }
            _ => result.push(node),
        }
    }

    Ok(ResolvedModule {
        ast: result,
        loaded,
    })
}

/// Collect the set of exported word names from an AST.
fn collect_exports(ast: &[AstNode]) -> HashSet<String> {
    let mut exports = HashSet::new();
    for node in ast {
        if let AstNode::Export(name) = node {
            exports.insert(name.clone());
        }
    }
    exports
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        std::fs::write(&path, content).unwrap();
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_resolve_math_stdlib() {
        // Load math.ws from the stdlib directory.
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let stdlib_dir = repo_root.join("stdlib");
        // Import "math" relative to the stdlib dir
        let ast = vec![AstNode::Import("math".into())];
        let result = resolve_imports(ast, &stdlib_dir);
        assert!(result.is_ok(), "resolve failed: {:?}", result.err());
        let resolved = result.unwrap();
        // math.ws should provide 'sq', 'cube', 'factorial', 'fib', 'even', 'odd'
        let def_names: Vec<String> = resolved
            .ast
            .iter()
            .filter_map(|n| {
                if let AstNode::Def { name, .. } = n {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            def_names.contains(&"sq".into()),
            "should contain sq, got {def_names:?}"
        );
        assert!(def_names.contains(&"factorial".into()));
        assert!(!resolved.loaded.is_empty());
    }

    #[test]
    fn test_import_not_found() {
        let dir = std::env::temp_dir();
        let ast = vec![AstNode::Import("nonexistent/foo".into())];
        let result = resolve_imports(ast, &dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Module not found"));
    }

    #[test]
    fn test_import_cycle_detection() {
        let dir = make_temp_dir("whisper-cycle-test");
        write_file(
            &dir,
            "cycle.ws",
            r#"
            import "cycle"
            : foo { 42 } ;
            export foo
        "#,
        );
        let ast = vec![AstNode::Import("cycle".into())];
        let result = resolve_imports(ast, &dir);
        assert!(result.is_ok(), "cycle should not crash");
        let resolved = result.unwrap();
        let def_names: Vec<String> = resolved
            .ast
            .iter()
            .filter_map(|n| {
                if let AstNode::Def { name, .. } = n {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();
        // 'foo' should appear exactly once
        assert_eq!(def_names.iter().filter(|n| *n == "foo").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_import_strips_import_nodes() {
        let dir = make_temp_dir("whisper-import-test");
        write_file(&dir, "lib.ws", ": helper { 1 + } ; export helper");
        let ast = vec![
            AstNode::Import("lib".into()),
            AstNode::Literal(whisper_core::value::Value::I64(5)),
        ];
        let result = resolve_imports(ast, &dir).unwrap();
        // Should have helper def + the literal, no Import or Export nodes
        assert_eq!(result.ast.len(), 2, "got: {result:?}");
        assert!(!result.ast.iter().any(|n| matches!(n, AstNode::Import(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

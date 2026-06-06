//! Package installation with capability review.
//!
//! Install flow:
//!   1. Resolve package spec → Git URL
//!   2. Git clone to temp directory
//!   3. Parse package.ws for metadata + capabilities
//!   4. Show capabilities to user, ask confirmation
//!   5. Compile .ws files to .wbin
//!   6. Copy to ~/.whisper/packages/<name>/
//!   7. Generate lockfile entry

use crate::manifest::PackageManifest;
use std::path::PathBuf;
use std::process::Command;

pub struct Installer {
    cache_dir: PathBuf,
}

impl Installer {
    pub fn new() -> Self {
        let cache_dir = dirs_home().join("packages");
        Installer { cache_dir }
    }

    /// Install a package from a spec.
    pub fn install(&self, spec: &str, auto_yes: bool) -> Result<(), String> {
        // 1. Resolve package info
        let info = crate::registry::resolve_package(spec)?;
        println!("Package: {} ({})", info.package_name, info.git_url);

        // 2. Check if git is available
        let git_ok = Command::new("git").arg("--version").output().is_ok();
        let temp_dir = std::env::temp_dir().join(format!("whisper-pkg-{}", info.package_name));

        if git_ok {
            // Clone via git
            let _ = std::fs::remove_dir_all(&temp_dir);
            let status = Command::new("git")
                .args(["clone", "--depth", "1", &info.git_url])
                .arg(&temp_dir)
                .status()
                .map_err(|e| format!("Git clone failed: {e}"))?;

            if !status.success() {
                return Err(format!("Git clone failed for {}", info.git_url));
            }
        } else {
            // Fallback: try download as zip
            println!("Git not found. Trying HTTPS download...");
            return Err("Direct download not yet supported. Please install Git.".into());
        }

        // 3. Parse package.ws
        let pkg_file = temp_dir.join("package.ws");
        if !pkg_file.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err("package.ws not found in repository".into());
        }

        let manifest_content = std::fs::read_to_string(&pkg_file)
            .map_err(|e| format!("Cannot read package.ws: {e}"))?;
        let manifest = PackageManifest::parse(&manifest_content)
            .map_err(|e| format!("Invalid package.ws: {e}"))?;

        println!();
        println!("Package: {} v{}", manifest.name, manifest.version);
        if !manifest.capabilities.is_empty() {
            println!("Capabilities: {:?}", manifest.capabilities);
        }
        if !manifest.dependencies.is_empty() {
            println!("Dependencies: {:?}", manifest.dependencies);
        }

        // 4. Capability review
        if !manifest.capabilities.is_empty() {
            if auto_yes {
                println!("Auto-authorizing capabilities: {:?}", manifest.capabilities);
            } else {
                print!("Authorize capabilities? [y/N]: ");
                use std::io::Write;
                let _ = std::io::stdout().flush();
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    let _ = std::fs::remove_dir_all(&temp_dir);
                    return Err("Installation cancelled.".into());
                }
            }
        }

        // 5. Install to cache
        let pkg_dir = self.cache_dir.join(&manifest.name);
        let _ = std::fs::create_dir_all(&pkg_dir);

        // Copy .ws files
        copy_ws_files(&temp_dir, &pkg_dir)?;

        // Save manifest
        let manifest_path = pkg_dir.join("package.ws");
        std::fs::write(&manifest_path, &manifest_content)
            .map_err(|e| format!("Failed to save manifest: {e}"))?;

        // 6. Generate lock entry
        let lock = crate::lock::LockEntry {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            source: info.git_url.clone(),
            checksum: "sha256:0000".to_string(),
            capabilities: manifest.capabilities.clone(),
        };

        let lockfile_path = dirs_home().join("whisper.lock");
        let mut lockfile = crate::lock::Lockfile::new();
        if lockfile_path.exists() {
            lockfile = crate::lock::Lockfile::read(&lockfile_path)
                .unwrap_or_else(|_| crate::lock::Lockfile::new());
        }
        lockfile.add(lock);
        lockfile.write(&lockfile_path)
            .map_err(|e| format!("Failed to write lockfile: {e}"))?;

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);

        println!("Installed: {} v{}", manifest.name, manifest.version);
        println!("Location: {}", pkg_dir.display());
        Ok(())
    }

    /// Install from a local directory.
    pub fn install_local(&self, path: &str, auto_yes: bool) -> Result<(), String> {
        let local_path = std::path::Path::new(path);
        if !local_path.exists() || !local_path.is_dir() {
            return Err(format!("Directory not found: {path}"));
        }

        let pkg_file = local_path.join("package.ws");
        if !pkg_file.exists() {
            return Err("package.ws not found in directory".into());
        }

        let manifest_content = std::fs::read_to_string(&pkg_file)
            .map_err(|e| format!("Cannot read package.ws: {e}"))?;
        let manifest = PackageManifest::parse(&manifest_content)
            .map_err(|e| format!("Invalid package.ws: {e}"))?;

        println!("Installing {} v{} (local)", manifest.name, manifest.version);

        if !manifest.capabilities.is_empty() {
            println!("Capabilities: {:?}", manifest.capabilities);
            if !auto_yes {
                print!("Authorize? [y/N]: ");
                use std::io::Write;
                let _ = std::io::stdout().flush();
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    return Err("Installation cancelled.".into());
                }
            }
        }

        let pkg_dir = self.cache_dir.join(&manifest.name);
        let _ = std::fs::create_dir_all(&pkg_dir);
        copy_ws_files(local_path, &pkg_dir)?;
        std::fs::write(pkg_dir.join("package.ws"), &manifest_content)
            .map_err(|e| format!("Save manifest: {e}"))?;

        println!("Installed to: {}", pkg_dir.display());
        Ok(())
    }

    /// List installed packages.
    pub fn list(&self) -> Result<Vec<InstalledPackage>, String> {
        if !self.cache_dir.exists() {
            return Ok(Vec::new());
        }
        let mut packages = Vec::new();
        for entry in std::fs::read_dir(&self.cache_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let manifest_path = entry.path().join("package.ws");
                if manifest_path.exists() {
                    let content = std::fs::read_to_string(&manifest_path)
                        .unwrap_or_default();
                    if let Ok(manifest) = PackageManifest::parse(&content) {
                        packages.push(InstalledPackage {
                            name: manifest.name.clone(),
                            version: manifest.version.clone(),
                            manifest,
                        });
                    }
                }
            }
        }
        Ok(packages)
    }
}

impl Default for Installer {
    fn default() -> Self {
        Self::new()
    }
}

fn copy_ws_files(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    copy_ws_recursive(src, dst, src)
}

fn copy_ws_recursive(base: &std::path::Path, dst: &std::path::Path, current: &std::path::Path) -> Result<(), String> {
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            copy_ws_recursive(base, dst, &path)?;
        } else if path.extension().is_some_and(|e| e == "ws") {
            let rel = path.strip_prefix(base).map_err(|e| e.to_string())?;
            let target = dst.join(rel);
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::copy(&path, &target).map_err(|e| format!("Copy failed: {e}"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub manifest: PackageManifest,
}

fn dirs_home() -> PathBuf {
    std::env::var("WHISPER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".whisper")
        })
}

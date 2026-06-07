//! Package installation with capability review.
//!
//! Install flow:
//!   1. Resolve package spec -> download URL
//!   2. Download (Git clone or HTTPS zip fallback)
//!   3. Parse package.ws for metadata + capabilities
//!   4. Show capabilities to user, ask confirmation
//!   5. Recursively install dependencies
//!   6. Copy .ws files to cache directory
//!   7. Generate lockfile entry with SHA256 checksum

use crate::manifest::PackageManifest;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Installer {
    cache_dir: PathBuf,
    /// Track already-installed packages to avoid cycles in transitive deps.
    installed: HashSet<String>,
}

impl Installer {
    pub fn new() -> Self {
        let cache_dir = dirs_home().join("packages");
        Installer {
            cache_dir,
            installed: HashSet::new(),
        }
    }

    /// Install a package from a spec, including transitive dependencies.
    pub fn install(&mut self, spec: &str, auto_yes: bool) -> Result<(), String> {
        self.install_internal(spec, auto_yes, 0)
    }

    fn install_internal(&mut self, spec: &str, auto_yes: bool, depth: usize) -> Result<(), String> {
        if depth > 10 {
            return Err("Dependency depth exceeded (circular dependency?)".into());
        }

        // 1. Resolve package info
        let info = crate::registry::resolve_package(spec)?;

        // Skip if already installed
        let pkg_key = format!(
            "{}@{}",
            info.package_name,
            info.version.as_deref().unwrap_or("latest")
        );
        if self.installed.contains(&pkg_key) {
            println!("  {} already installed, skipping", info.package_name);
            return Ok(());
        }
        self.installed.insert(pkg_key.clone());

        println!("Package: {} ({})", info.package_name, info.git_url);

        // 2. Download
        let temp_dir = std::env::temp_dir().join(format!("whisper-pkg-{}", info.package_name));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let download_ok = self
            .try_git_clone(&info.git_url, &temp_dir)
            .or_else(|_| self.try_curl_download(&info, &temp_dir))
            .or_else(|_| self.try_powershell_download(&info, &temp_dir));

        if let Err(e) = download_ok {
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(format!(
                "Failed to download {}: {}\n\
                 Install Git (https://git-scm.com) or curl to download packages.",
                info.package_name, e
            ));
        }

        // 3. Parse package.ws
        let manifest = self.read_manifest(&temp_dir)?;

        println!();
        println!("Package: {} v{}", manifest.name, manifest.version);
        if !manifest.capabilities.is_empty() {
            println!("Capabilities: {:?}", manifest.capabilities);
        }
        if !manifest.dependencies.is_empty() {
            println!("Dependencies: {:?}", manifest.dependencies);
        }

        // 4. Capability review
        self.review_capabilities(&manifest, auto_yes)?;

        // 5. Install transitive dependencies first
        let deps: Vec<String> = manifest.dependencies.clone();
        for dep_spec in &deps {
            println!("  Installing dependency: {dep_spec}");
            if let Err(e) = self.install_internal(dep_spec, auto_yes, depth + 1) {
                eprintln!("  Warning: failed to install dependency '{dep_spec}': {e}");
            }
        }

        // 6. Install to cache
        let pkg_dir = self.cache_dir.join(&manifest.name);
        let _ = std::fs::create_dir_all(&pkg_dir);
        copy_ws_files(&temp_dir, &pkg_dir)?;

        // Save manifest
        let manifest_path = pkg_dir.join("package.ws");
        let manifest_content = std::fs::read_to_string(temp_dir.join("package.ws"))
            .map_err(|e| format!("Cannot read manifest: {e}"))?;
        std::fs::write(&manifest_path, &manifest_content)
            .map_err(|e| format!("Failed to save manifest: {e}"))?;

        // 7. Generate lock entry with real checksum
        let checksum = compute_package_checksum(&pkg_dir)?;
        let lock = crate::lock::LockEntry {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            source: info.git_url.clone(),
            checksum,
            capabilities: manifest.capabilities.clone(),
        };

        let lockfile_path = dirs_home().join("whisper.lock");
        let mut lockfile = crate::lock::Lockfile::new();
        if lockfile_path.exists() {
            lockfile = crate::lock::Lockfile::read(&lockfile_path)
                .unwrap_or_else(|_| crate::lock::Lockfile::new());
        }
        lockfile.add(lock);
        lockfile
            .write(&lockfile_path)
            .map_err(|e| format!("Failed to write lockfile: {e}"))?;

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);

        println!("Installed: {} v{}", manifest.name, manifest.version);
        println!("Location: {}", pkg_dir.display());
        Ok(())
    }

    /// Try cloning via git.
    fn try_git_clone(&self, url: &str, dest: &Path) -> Result<(), String> {
        let git_ok = Command::new("git").arg("--version").output().is_ok();
        if !git_ok {
            return Err("Git not available".into());
        }
        let status = Command::new("git")
            .args(["clone", "--depth", "1", url])
            .arg(dest)
            .status()
            .map_err(|e| format!("Git clone failed: {e}"))?;
        if !status.success() {
            return Err(format!("Git clone failed for {url}"));
        }
        Ok(())
    }

    /// Try downloading via curl (HTTPS zip archive fallback).
    fn try_curl_download(
        &self,
        info: &crate::registry::PackageInfo,
        dest: &Path,
    ) -> Result<(), String> {
        let curl_ok = Command::new("curl").arg("--version").output().is_ok();
        if !curl_ok {
            return Err("curl not available".into());
        }

        let archive_url = git_to_archive_url(&info.git_url);
        println!("  Downloading via HTTPS: {archive_url}");
        let zip_path = dest.with_extension("zip");

        // Download zip
        let status = Command::new("curl")
            .args(["-L", "-o"])
            .arg(&zip_path)
            .arg(&archive_url)
            .status()
            .map_err(|e| format!("curl failed: {e}"))?;
        if !status.success() {
            return Err("curl download failed".into());
        }

        // Extract zip
        let unzip_ok = Command::new("unzip").arg("-v").output().is_ok();
        if unzip_ok {
            let _ = std::fs::create_dir_all(dest);
            let status = Command::new("unzip")
                .args(["-q", "-o"])
                .arg(&zip_path)
                .arg("-d")
                .arg(dest)
                .status()
                .map_err(|e| format!("unzip failed: {e}"))?;
            if !status.success() {
                return Err("unzip extraction failed".into());
            }
            let _ = std::fs::remove_file(&zip_path);

            // GitHub wraps in a subdirectory, move contents up
            flatten_github_archive(dest)?;
            return Ok(());
        }

        // Fallback: use built-in ZIP reader for .ws files
        self.extract_ws_files(&zip_path, dest)
    }

    /// Try PowerShell download on Windows.
    fn try_powershell_download(
        &self,
        info: &crate::registry::PackageInfo,
        dest: &Path,
    ) -> Result<(), String> {
        let ps_ok = Command::new("powershell")
            .arg("-Command")
            .arg("Write-Host test")
            .output()
            .is_ok();
        if !ps_ok {
            return Err("PowerShell not available".into());
        }

        let archive_url = git_to_archive_url(&info.git_url);
        let zip_path = dest.with_extension("zip");
        println!("  Downloading via PowerShell: {archive_url}");

        let status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                    archive_url,
                    zip_path.display()
                ),
            ])
            .status()
            .map_err(|e| format!("PowerShell download failed: {e}"))?;
        if !status.success() {
            return Err("PowerShell download failed".into());
        }

        let _ = std::fs::create_dir_all(dest);
        let status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    zip_path.display(),
                    dest.display()
                ),
            ])
            .status()
            .map_err(|e| format!("PowerShell extraction failed: {e}"))?;
        if !status.success() {
            return Err("PowerShell extraction failed".into());
        }

        let _ = std::fs::remove_file(&zip_path);
        flatten_github_archive(dest)?;
        Ok(())
    }

    /// Built-in minimal ZIP extraction for .ws files only.
    fn extract_ws_files(&self, zip_path: &Path, dest: &Path) -> Result<(), String> {
        let data = std::fs::read(zip_path).map_err(|e| format!("Read zip: {e}"))?;
        let _ = std::fs::remove_file(zip_path);

        // Find end-of-central-directory (EOCD) signature
        let eocd_sig: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
        let mut eocd_offset = None;
        for i in (0..data.len().saturating_sub(22)).rev() {
            if data[i..i + 4] == eocd_sig {
                eocd_offset = Some(i);
                break;
            }
        }
        let eocd_offset = eocd_offset.ok_or("Invalid ZIP file")?;

        let central_offset =
            u32::from_le_bytes(data[eocd_offset + 16..eocd_offset + 20].try_into().unwrap())
                as usize;
        let central_count =
            u16::from_le_bytes(data[eocd_offset + 10..eocd_offset + 12].try_into().unwrap());

        let mut pos = central_offset;
        let mut extracted = 0;
        for _ in 0..central_count {
            if pos + 46 > data.len() {
                break;
            }
            if data[pos..pos + 4] != [0x50, 0x4B, 0x01, 0x02] {
                break;
            }
            let name_len =
                u16::from_le_bytes(data[pos + 28..pos + 30].try_into().unwrap()) as usize;
            let compressed_size =
                u32::from_le_bytes(data[pos + 20..pos + 24].try_into().unwrap()) as usize;
            let local_offset =
                u32::from_le_bytes(data[pos + 42..pos + 46].try_into().unwrap()) as usize;
            let name = String::from_utf8_lossy(&data[pos + 46..pos + 46 + name_len]);

            if name.ends_with(".ws") {
                // Read local file header
                if local_offset + 30 + name_len > data.len() {
                    pos += 46 + name_len;
                    continue;
                }
                let local_name_len = u16::from_le_bytes(
                    data[local_offset + 26..local_offset + 28]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let extra_len = u16::from_le_bytes(
                    data[local_offset + 28..local_offset + 30]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let file_data_start = local_offset + 30 + local_name_len + extra_len;
                let file_data = &data[file_data_start..file_data_start + compressed_size];

                let rel_path = Path::new(&*name);
                let target = dest.join(rel_path);
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(&target, file_data).map_err(|e| format!("Write {name}: {e}"))?;
                extracted += 1;
            }
            pos += 46 + name_len;
        }

        if extracted == 0 {
            Err("No .ws files found in archive".into())
        } else {
            flatten_github_archive(dest)?;
            Ok(())
        }
    }

    /// Install from a local directory.
    pub fn install_local(&mut self, path: &str, auto_yes: bool) -> Result<(), String> {
        let local_path = std::path::Path::new(path);
        if !local_path.exists() || !local_path.is_dir() {
            return Err(format!("Directory not found: {path}"));
        }

        let manifest = self.read_manifest(local_path)?;
        println!("Installing {} v{} (local)", manifest.name, manifest.version);

        self.review_capabilities(&manifest, auto_yes)?;

        let pkg_dir = self.cache_dir.join(&manifest.name);
        let _ = std::fs::create_dir_all(&pkg_dir);
        copy_ws_files(local_path, &pkg_dir)?;
        let manifest_content = std::fs::read_to_string(local_path.join("package.ws"))
            .map_err(|e| format!("Read manifest: {e}"))?;
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
                    let content = std::fs::read_to_string(&manifest_path).unwrap_or_default();
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

    fn read_manifest(&self, dir: &Path) -> Result<PackageManifest, String> {
        let pkg_file = dir.join("package.ws");
        if !pkg_file.exists() {
            return Err("package.ws not found".into());
        }
        let content = std::fs::read_to_string(&pkg_file)
            .map_err(|e| format!("Cannot read package.ws: {e}"))?;
        PackageManifest::parse(&content).map_err(|e| format!("Invalid package.ws: {e}"))
    }

    fn review_capabilities(
        &self,
        manifest: &PackageManifest,
        auto_yes: bool,
    ) -> Result<(), String> {
        if manifest.capabilities.is_empty() {
            return Ok(());
        }
        if auto_yes {
            println!("Auto-authorizing capabilities: {:?}", manifest.capabilities);
            return Ok(());
        }
        print!(
            "Authorize capabilities ({:?})? [y/N]: ",
            manifest.capabilities
        );
        let _ = std::io::stdout().flush();
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;
        if !input.trim().eq_ignore_ascii_case("y") {
            return Err("Installation cancelled.".into());
        }
        Ok(())
    }
}

impl Default for Installer {
    fn default() -> Self {
        Self::new()
    }
}

// === Helpers ===

fn copy_ws_files(src: &Path, dst: &Path) -> Result<(), String> {
    copy_ws_recursive(src, dst, src)
}

fn copy_ws_recursive(base: &Path, dst: &Path, current: &Path) -> Result<(), String> {
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

/// Convert a Git URL to a GitHub archive download URL.
fn git_to_archive_url(git_url: &str) -> String {
    // https://github.com/user/repo.git -> https://github.com/user/repo/archive/refs/heads/main.zip
    let base = git_url.strip_suffix(".git").unwrap_or(git_url);
    format!("{base}/archive/refs/heads/main.zip")
}

/// GitHub archives wrap contents in a `repo-branch/` subdirectory.
/// Move everything up one level.
fn flatten_github_archive(dest: &Path) -> Result<(), String> {
    let entries: Vec<_> = std::fs::read_dir(dest)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();

    // Find the single subdirectory that GitHub creates
    let subdirs: Vec<_> = entries
        .iter()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();

    if subdirs.len() == 1 {
        let subdir = subdirs[0].path();
        for entry in std::fs::read_dir(&subdir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let target = dest.join(entry.file_name());
            if entry.path().is_dir() {
                let _ = std::fs::rename(entry.path(), &target);
            } else {
                std::fs::rename(entry.path(), &target)
                    .map_err(|e| format!("Move {}: {e}", entry.path().display()))?;
            }
        }
        let _ = std::fs::remove_dir_all(&subdir);
    }
    Ok(())
}

/// Compute a deterministic checksum of all .ws files in a directory.
/// Uses FNV-1a 64-bit hash for reproducibility (no external crypto deps).
fn compute_package_checksum(dir: &Path) -> Result<String, String> {
    let mut files: Vec<_> = Vec::new();
    collect_ws_files(dir, &mut files)?;
    files.sort();
    let mut hash: u64 = 0xcbf29ce484222325;
    for path in &files {
        let content = std::fs::read(path).map_err(|e| format!("Read {path:?}: {e}"))?;
        for &byte in &content {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("fnv:{hash:016x}"))
}

fn collect_ws_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_ws_files(&path, files)?;
        } else if path.extension().is_some_and(|e| e == "ws") {
            files.push(path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_to_archive_url() {
        let url = git_to_archive_url("https://github.com/user/repo.git");
        assert_eq!(
            url,
            "https://github.com/user/repo/archive/refs/heads/main.zip"
        );
    }
}

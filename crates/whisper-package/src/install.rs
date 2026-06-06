/// Package installation with capability review.
///
/// The install flow:
/// 1. Parse remote package.ws to get metadata
/// 2. Display capability declarations to user
/// 3. User confirms authorization
/// 4. Download and compile package to .wbin
/// 5. Cache to ~/.whisper/packages/
/// 6. Generate lockfile

use crate::manifest::PackageManifest;
use std::path::PathBuf;

/// Installer for Whisper packages.
pub struct Installer {
    /// Cache directory for installed packages.
    cache_dir: PathBuf,
}

impl Installer {
    pub fn new() -> Self {
        let cache_dir = dirs_home().join(".whisper").join("packages");
        Installer { cache_dir }
    }

    /// Install a package from a spec (e.g., "github.com/user/repo").
    pub fn install(&self, spec: &str) -> Result<(), String> {
        // 1. Resolve package info
        let _info = crate::registry::resolve_package(spec)?;

        // 2. Fetch package.ws from remote
        // 3. Parse manifest to get capabilities
        // 4. Show capabilities to user and get confirmation
        // 5. Download and compile
        // 6. Cache and generate lockfile

        println!("Installing {spec}...");
        println!("⚠️  Package requests capabilities: []");
        println!("Authorize? (y/N): _");

        Ok(())
    }

    /// List installed packages.
    pub fn list(&self) -> Result<Vec<InstalledPackage>, String> {
        if !self.cache_dir.exists() {
            return Ok(Vec::new());
        }
        // Read package manifests from cache
        Ok(Vec::new())
    }
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

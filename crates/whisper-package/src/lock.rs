//! Lockfile generation for reproducible builds.
//!
//! Whisper uses a whisper.lock file to pin exact package versions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A lockfile entry for a single package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub name: String,
    pub version: String,
    pub source: String,
    pub checksum: String,
    pub capabilities: Vec<String>,
}

/// The complete lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub packages: HashMap<String, LockEntry>,
}

impl Lockfile {
    pub fn new() -> Self {
        Lockfile {
            version: 1,
            packages: HashMap::new(),
        }
    }

    /// Add a package entry to the lockfile.
    pub fn add(&mut self, entry: LockEntry) {
        self.packages.insert(entry.name.clone(), entry);
    }

    /// Write the lockfile to disk.
    pub fn write(&self, path: &std::path::Path) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, content).map_err(|e| e.to_string())
    }

    /// Read a lockfile from disk.
    pub fn read(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

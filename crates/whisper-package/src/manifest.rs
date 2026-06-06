/// Package manifest parser for package.ws files.
///
/// Format:
///   name: "package-name"
///   version: "1.0.0"
///   capabilities: ["@file_read", "@http_post"]
///   exports: ["func1", "func2"]
///   dependencies: ["std/json", "std/io"]

use serde::{Deserialize, Serialize};

/// A Whisper package manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

impl PackageManifest {
    /// Parse a package.ws file content into a manifest.
    pub fn parse(content: &str) -> Result<Self, String> {
        // Simple line-by-line parser for the package.ws format
        let mut name = String::new();
        let mut version = String::new();
        let mut capabilities = Vec::new();
        let mut exports = Vec::new();
        let mut dependencies = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(value) = parse_key_value(line, "name:") {
                name = value;
            } else if let Some(value) = parse_key_value(line, "version:") {
                version = value;
            } else if let Some(value) = parse_list_value(line, "capabilities:") {
                capabilities = value;
            } else if let Some(value) = parse_list_value(line, "exports:") {
                exports = value;
            } else if let Some(value) = parse_list_value(line, "dependencies:") {
                dependencies = value;
            }
        }

        Ok(PackageManifest {
            name,
            version,
            capabilities,
            exports,
            dependencies,
        })
    }
}

fn parse_key_value(line: &str, key: &str) -> Option<String> {
    line.strip_prefix(key)
        .map(|s| s.trim().trim_matches('"').to_string())
}

fn parse_list_value(line: &str, key: &str) -> Option<Vec<String>> {
    line.strip_prefix(key).map(|s| {
        let s = s.trim().trim_matches('[').trim_matches(']');
        s.split(',')
            .map(|item| item.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest() {
        let content = r#"
name: "my-http-lib"
version: "1.0.0"
capabilities: ["@http_get", "@http_post"]
exports: ["get", "post"]
dependencies: ["std/json", "std/io"]
"#;
        let manifest = PackageManifest::parse(content).unwrap();
        assert_eq!(manifest.name, "my-http-lib");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.capabilities, vec!["@http_get", "@http_post"]);
        assert_eq!(manifest.exports, vec!["get", "post"]);
    }
}

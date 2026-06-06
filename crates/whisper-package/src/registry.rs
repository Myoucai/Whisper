/// Package registry interaction.
///
/// Whisper uses a Git-based package registry initially.
/// Packages are identified by github.com/user/repo paths.

/// Resolve a package spec to a downloadable URL.
pub fn resolve_package(spec: &str) -> Result<PackageInfo, String> {
    // Parse spec format: github.com/user/repo[@version]
    let spec = spec.trim();
    let (repo, version) = if let Some((r, v)) = spec.split_once('@') {
        (r, Some(v.to_string()))
    } else {
        (spec, None)
    };

    Ok(PackageInfo {
        repo: repo.to_string(),
        version,
        download_url: format!("https://{repo}/archive/main.zip"),
    })
}

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub repo: String,
    pub version: Option<String>,
    pub download_url: String,
}

/// Package registry — resolves package specs to download URLs.
///
/// Whisper uses Git repositories as the initial package registry.
/// Package spec format: github.com/user/repo[@version]

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub repo: String,
    pub version: Option<String>,
    pub git_url: String,
    pub package_name: String,
}

/// Resolve a package spec to download info.
pub fn resolve_package(spec: &str) -> Result<PackageInfo, String> {
    let spec = spec.trim().trim_end_matches('/');

    // Parse: [host/]user/repo[@version]
    let (path, version) = if let Some((p, v)) = spec.split_once('@') {
        (p, Some(v.to_string()))
    } else {
        (spec, None)
    };

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 {
        return Err(format!(
            "Invalid package spec: {spec}. Expected: github.com/user/repo"
        ));
    }

    let host = if parts.len() >= 3 {
        parts[0]
    } else {
        "github.com"
    };
    let user = if parts.len() >= 3 { parts[1] } else { parts[0] };
    let repo = if parts.len() >= 3 { parts[2] } else { parts[1] };

    let package_name = repo.to_string();
    let git_url = format!("https://{host}/{user}/{repo}.git");

    Ok(PackageInfo {
        repo: format!("{host}/{user}/{repo}"),
        version,
        git_url,
        package_name,
    })
}

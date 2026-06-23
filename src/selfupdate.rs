/// GitHub repo that publishes jw releases.
pub const REPO: &str = "bfirestone/jj-workspace";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
}

#[derive(Debug, Clone)]
pub struct UpdateOpts {
    pub check: bool,
    pub version: Option<String>,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate(String),
    UpdateAvailable(String),
    Updated(String),
}

impl UpdateOutcome {
    /// The human-readable line for this outcome.
    pub fn message(&self) -> &str {
        match self {
            UpdateOutcome::UpToDate(m)
            | UpdateOutcome::UpdateAvailable(m)
            | UpdateOutcome::Updated(m) => m,
        }
    }
}

/// Release target triple for the running build, or None on a platform jw
/// publishes no asset for.
pub fn current_target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => return None,
    })
}

/// `jw-v{version}`.
pub fn release_tag(version: &str) -> String {
    format!("jw-v{version}")
}

/// (archive, sha256-sidecar) asset file names for a version + target.
pub fn asset_names(version: &str, target: &str) -> (String, String) {
    let archive = format!("jj-workspace_{version}_{target}.tar.gz");
    let sha = format!("{archive}.sha256");
    (archive, sha)
}

/// Full release-asset download URL.
pub fn asset_url(repo: &str, tag: &str, asset: &str) -> String {
    format!("https://github.com/{repo}/releases/download/{tag}/{asset}")
}

/// Is `latest` strictly newer than `current`? `force` => always true.
pub fn should_update(current: &semver::Version, latest: &semver::Version, force: bool) -> bool {
    force || latest > current
}

/// Extract the hex digest from a `sha256sum`-format sidecar ("<hex>  <name>").
pub fn parse_sha256_sidecar(contents: &str) -> Option<String> {
    contents.split_whitespace().next().map(|s| s.to_lowercase())
}

/// Strip a leading `jw-v` / `v` and parse the remainder as semver.
pub fn parse_release_version(tag: &str) -> Option<semver::Version> {
    let v = tag
        .strip_prefix("jw-v")
        .or_else(|| tag.strip_prefix('v'))
        .unwrap_or(tag);
    semver::Version::parse(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tag_format() {
        assert_eq!(release_tag("0.2.0"), "jw-v0.2.0");
    }

    #[test]
    fn asset_names_format() {
        let (a, s) = asset_names("0.2.0", "aarch64-apple-darwin");
        assert_eq!(a, "jj-workspace_0.2.0_aarch64-apple-darwin.tar.gz");
        assert_eq!(s, "jj-workspace_0.2.0_aarch64-apple-darwin.tar.gz.sha256");
    }

    #[test]
    fn asset_url_format() {
        assert_eq!(
            asset_url(
                "bfirestone/jj-workspace",
                "jw-v0.2.0",
                "jj-workspace_0.2.0_x86_64-apple-darwin.tar.gz"
            ),
            "https://github.com/bfirestone/jj-workspace/releases/download/jw-v0.2.0/jj-workspace_0.2.0_x86_64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn should_update_semver() {
        let v = |s| semver::Version::parse(s).unwrap();
        assert!(should_update(&v("0.2.0"), &v("0.3.0"), false));
        assert!(!should_update(&v("0.2.0"), &v("0.2.0"), false));
        assert!(!should_update(&v("0.3.0"), &v("0.2.0"), false));
        assert!(should_update(&v("0.2.0"), &v("0.2.0"), true)); // force
    }

    #[test]
    fn parse_sidecar_takes_first_token() {
        assert_eq!(
            parse_sha256_sidecar("ABC123  jj-workspace_0.2.0_x.tar.gz\n").as_deref(),
            Some("abc123")
        );
        assert_eq!(parse_sha256_sidecar(""), None);
    }

    #[test]
    fn parse_version_strips_prefixes() {
        let v = |s| semver::Version::parse(s).unwrap();
        assert_eq!(parse_release_version("jw-v0.2.0"), Some(v("0.2.0")));
        assert_eq!(parse_release_version("v1.0.0"), Some(v("1.0.0")));
        assert_eq!(parse_release_version("0.2.0"), Some(v("0.2.0")));
        assert!(parse_release_version("garbage").is_none());
    }

    #[test]
    fn current_target_known_on_supported_platforms() {
        if matches!(std::env::consts::OS, "linux" | "macos")
            && matches!(std::env::consts::ARCH, "x86_64" | "aarch64")
        {
            assert!(current_target().is_some());
        }
    }

    #[test]
    fn outcome_message_accessor() {
        assert_eq!(UpdateOutcome::Updated("x".into()).message(), "x");
    }
}

#[cfg(test)]
mod contract {
    use super::*;
    const _: fn() -> Option<&'static str> = current_target;
    const _: fn(&str) -> String = release_tag;
    const _: fn(&str, &str) -> (String, String) = asset_names;
    const _: fn(&str, &str, &str) -> String = asset_url;
    const _: fn(&semver::Version, &semver::Version, bool) -> bool = should_update;
    const _: fn(&str) -> Option<String> = parse_sha256_sidecar;
    const _: fn(&str) -> Option<semver::Version> = parse_release_version;
}

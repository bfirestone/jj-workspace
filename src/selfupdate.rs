use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// User-Agent for GitHub requests (the API requires one).
const USER_AGENT: &str = concat!("jw/", env!("CARGO_PKG_VERSION"));

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

/// GET `url` and return the response body as a String.
fn http_get_string(url: &str) -> Result<String> {
    let body = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("requesting {url}"))?
        .body_mut()
        .read_to_string()
        .with_context(|| format!("reading response from {url}"))?;
    Ok(body)
}

/// Resolve the newest version for `channel`. Stable => GET releases/latest.
/// (Extension point: a future PreRelease variant would list `/releases` and
/// pick the max semver including pre-releases.)
pub fn resolve_latest(channel: Channel, repo: &str) -> Result<semver::Version> {
    match channel {
        Channel::Stable => {
            let url = format!("https://api.github.com/repos/{repo}/releases/latest");
            let body = http_get_string(&url)?;
            let json: serde_json::Value =
                serde_json::from_str(&body).context("parsing releases/latest JSON")?;
            let tag = json
                .get("tag_name")
                .and_then(|t| t.as_str())
                .context("no tag_name in releases/latest response")?;
            parse_release_version(tag).with_context(|| format!("unparseable release tag '{tag}'"))
        }
    }
}

/// Download `url` to `dest` (follows redirects, sends the jw User-Agent).
fn download(url: &str, dest: &Path) -> Result<()> {
    let mut reader = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("downloading {url}"))?
        .into_body()
        .into_reader();
    let mut file =
        std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    std::io::copy(&mut reader, &mut file).with_context(|| format!("writing {}", dest.display()))?;
    Ok(())
}

/// Verify `archive`'s sha256 against `expected_hex`. Errors on mismatch.
fn verify_sha256(archive: &Path, expected_hex: &str) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).context("hashing archive")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let result = hasher.finalize();
    let actual: String = result.iter().map(|b| format!("{b:02x}")).collect();
    if !actual.eq_ignore_ascii_case(expected_hex) {
        anyhow::bail!("checksum mismatch: expected {expected_hex}, got {actual}");
    }
    Ok(())
}

/// Extract the tar.gz at `archive` into `dir`; return the path to the `jw` binary.
fn extract_binary(archive: &Path, dir: &Path) -> Result<PathBuf> {
    let tar_gz =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let dec = flate2::read::GzDecoder::new(tar_gz);
    tar::Archive::new(dec)
        .unpack(dir)
        .context("extracting archive")?;

    fn find(dir: &Path) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()? {
            let path = entry.ok()?.path();
            if path.is_dir() {
                if let Some(found) = find(&path) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|n| n.to_str()) == Some("jw") {
                return Some(path);
            }
        }
        None
    }
    find(dir).with_context(|| format!("'jw' binary not found in archive {}", archive.display()))
}

/// macOS: strip quarantine + ad-hoc codesign so Gatekeeper doesn't kill it.
#[cfg(target_os = "macos")]
fn resign_macos(bin: &Path) -> Result<()> {
    use std::process::Command;
    // Best-effort quarantine removal (fine if the attr is absent).
    let _ = Command::new("xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(bin)
        .output();
    Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(bin)
        .output()
        .context("running codesign")?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn resign_macos(_bin: &Path) -> Result<()> {
    Ok(())
}

/// Top-level: resolve target version, then (unless --check) download, verify,
/// extract, and atomically replace the running binary.
pub fn run_update(opts: UpdateOpts) -> Result<UpdateOutcome> {
    let current =
        semver::Version::parse(env!("CARGO_PKG_VERSION")).context("parsing built-in jw version")?;

    // Target version: explicit pin (no API call) or resolved latest.
    let target_ver = match &opts.version {
        Some(v) => parse_release_version(v).with_context(|| format!("invalid --version '{v}'"))?,
        None => resolve_latest(Channel::Stable, REPO)?,
    };

    if opts.check {
        return Ok(if should_update(&current, &target_ver, false) {
            UpdateOutcome::UpdateAvailable(format!("{target_ver} available (have {current})"))
        } else {
            UpdateOutcome::UpToDate(format!("up to date ({current})"))
        });
    }

    if !should_update(&current, &target_ver, opts.force) {
        return Ok(UpdateOutcome::UpToDate(format!(
            "jw {current} is already up to date"
        )));
    }

    let target = current_target()
        .context("unsupported platform: jw publishes no release asset for this OS/arch")?;
    let ver = target_ver.to_string();
    let tag = release_tag(&ver);
    let (archive_name, sha_name) = asset_names(&ver, target);
    let archive_url = asset_url(REPO, &tag, &archive_name);
    let sha_url = asset_url(REPO, &tag, &sha_name);

    let tmp = tempfile::tempdir().context("creating temp dir")?;
    let archive_path = tmp.path().join(&archive_name);
    download(&archive_url, &archive_path)
        .with_context(|| format!("no release asset for {target} at {tag}"))?;

    // Mandatory checksum.
    let sidecar = http_get_string(&sha_url)
        .with_context(|| format!("no published checksum for {archive_name}"))?;
    let expected = parse_sha256_sidecar(&sidecar).context("malformed .sha256 sidecar")?;
    verify_sha256(&archive_path, &expected)?;

    let new_bin = extract_binary(&archive_path, tmp.path())?;
    self_replace::self_replace(&new_bin).context(
        "replacing the running jw binary (do you have write permission to its directory?)",
    )?;
    let installed = std::env::current_exe().context("locating installed binary")?;
    resign_macos(&installed)?;

    Ok(UpdateOutcome::Updated(format!(
        "Updated jw {current} → {target_ver}"
    )))
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
    const _: fn(Channel, &str) -> anyhow::Result<semver::Version> = resolve_latest;
    const _: fn(UpdateOpts) -> anyhow::Result<UpdateOutcome> = run_update;
}

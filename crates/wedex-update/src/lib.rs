//! GitHub Releases are the update authority. Each binary must have a GitHub SHA-256 digest.
#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const DEFAULT_REPOSITORY: &str = match option_env!("WEDEX_UPDATE_REPOSITORY") {
    Some(repo) => repo,
    None => "AntonDevia/WeDex",
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Windows,
    Android,
    Server,
}
impl Target {
    pub fn asset(self) -> &'static str {
        match self {
            Self::Windows => "WeDex-Windows.exe",
            Self::Android => "WeDex-Android.apk",
            Self::Server => "wedex-server-x86_64-linux-musl",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub target: Target,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: u64,
}
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
pub fn version(value: &str) -> Result<[u64; 3]> {
    let value = value.rsplit_once('v').map(|(_, v)| v).unwrap_or(value);
    let parts = value
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    anyhow::ensure!(parts.len() == 3, "Некорректная версия обновления");
    Ok([parts[0], parts[1], parts[2]])
}
pub fn repository(value: &str) -> Result<String> {
    let value = value
        .trim()
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")
        .unwrap_or(value.trim())
        .trim_end_matches(".git");
    let parts = value.split('/').collect::<Vec<_>>();
    anyhow::ensure!(
        parts.len() == 2
            && parts.iter().all(|p| !p.is_empty()
                && p.len() <= 100
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))),
        "Укажите owner/repository GitHub"
    );
    Ok(parts.join("/"))
}
fn http() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .https_only(true)
        .user_agent("WeDex updater")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()?)
}
pub fn check(repo: &str, target: Target, current: &str) -> Result<Option<Release>> {
    let repo = repository(repo)?;
    let response = http()?
        .get(format!(
            "https://api.github.com/repos/{repo}/releases?per_page=50"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let releases: Vec<GithubRelease> = response.error_for_status()?.json()?;
    select(releases, &repo, target, current)
}
fn select(
    releases: Vec<GithubRelease>,
    repo: &str,
    target: Target,
    current: &str,
) -> Result<Option<Release>> {
    let current = version(current)?;
    let mut available = vec![];
    for release in releases.into_iter().filter(|r| !r.draft && !r.prerelease) {
        let Ok(v) = version(&release.tag_name) else {
            continue;
        };
        if v <= current {
            continue;
        }
        for asset in release
            .assets
            .into_iter()
            .filter(|a| a.name == target.asset())
        {
            let Some(hash) = asset
                .digest
                .and_then(|d| d.strip_prefix("sha256:").map(str::to_owned))
            else {
                continue;
            };
            if hash.len() != 64
                || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                || asset.size == 0
                || asset.size > 300 * 1024 * 1024
            {
                continue;
            }
            let prefix = format!("https://github.com/{repo}/releases/download/");
            if !asset.browser_download_url.starts_with(&prefix) {
                continue;
            }
            available.push((
                v,
                Release {
                    version: format!("{}.{}.{}", v[0], v[1], v[2]),
                    notes: release
                        .body
                        .clone()
                        .unwrap_or_default()
                        .chars()
                        .take(12000)
                        .collect(),
                    page: release.html_url.clone(),
                    url: asset.browser_download_url,
                    sha256: hash.to_ascii_lowercase(),
                    size: asset.size,
                    target,
                },
            ));
        }
    }
    available.sort_by_key(|(v, _)| *v);
    Ok(available.pop().map(|(_, r)| r))
}
pub fn download(release: &Release, folder: &Path, progress: impl Fn(f32)) -> Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let final_path = folder.join(format!("{}-{}", release.version, release.target.asset()));
    let part = folder.join(format!("{}.part", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut response = http()?
            .get(&release.url)
            .timeout(Duration::from_secs(600))
            .send()?
            .error_for_status()?;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&part)?;
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut block = [0u8; 65536];
        loop {
            let n = response.read(&mut block)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            anyhow::ensure!(
                total <= release.size,
                "Обновление больше заявленного размера"
            );
            hasher.update(&block[..n]);
            file.write_all(&block[..n])?;
            progress(total as f32 / release.size as f32);
        }
        anyhow::ensure!(
            total == release.size && format!("{:x}", hasher.finalize()) == release.sha256,
            "Контрольная сумма обновления не совпала"
        );
        file.sync_all()?;
        drop(file);
        if final_path.exists() {
            std::fs::remove_file(&final_path)?;
        }
        std::fs::rename(&part, &final_path)?;
        Ok(final_path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}
#[cfg(windows)]
pub fn install_windows(download: &Path, expected_hash: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe()?;
    let folder = download.parent().context("Нет каталога обновления")?;
    let spec = folder.join(format!("replace-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(
        &spec,
        serde_json::to_vec(
            &serde_json::json!({"pid":std::process::id(),"source":download,"target":exe,"sha256":expected_hash}),
        )?,
    )?;
    let script = folder.join("apply-wedex-update.ps1");
    std::fs::write(&script, include_str!("windows.ps1"))?;
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .arg(spec)
        .creation_flags(0x08000000)
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_platform_versions_and_repository_validation() {
        assert_eq!(version("server-v1.2.3").unwrap(), [1, 2, 3]);
        assert!(version("v1.2.3-beta").is_err());
        assert!(repository("../bad/repo").is_err());
        assert!(repository("x/y?token=z").is_err());
        let input = serde_json::json!([{"tag_name":"server-v1.1.0","body":"server","html_url":"https://github.com/a/b/releases","draft":false,"prerelease":false,"assets":[{"name":Target::Server.asset(),"size":20,"digest":format!("sha256:{}","a".repeat(64)),"browser_download_url":"https://github.com/a/b/releases/download/server-v1.1.0/wedex-server-x86_64-linux-musl"}]}]);
        assert!(select(
            serde_json::from_value(input.clone()).unwrap(),
            "a/b",
            Target::Android,
            "1.0.0"
        )
        .unwrap()
        .is_none());
        assert_eq!(
            select(
                serde_json::from_value(input).unwrap(),
                "a/b",
                Target::Server,
                "1.0.0"
            )
            .unwrap()
            .unwrap()
            .version,
            "1.1.0"
        );
    }
}

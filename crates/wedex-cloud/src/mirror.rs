//! Public catalogue address, separate from the encrypted credentials vault.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitmoMirror {
    pub service: String,
    pub schema: u32,
    pub source: String,
    pub origin: String,
    pub checked_at_ms: u64,
}
impl HitmoMirror {
    pub fn valid(&self, now: u64) -> bool {
        self.service == "wedex-hitmo-mirror"
            && self.schema == 1
            && self.source == "https://hitmos.pro/"
            && valid_origin(&self.origin)
            && self.checked_at_ms > 0
            && self.checked_at_ms <= now.saturating_add(300_000)
        // This is only a candidate. Each client checks it over HTTPS before use,
        // so a failed discovery job need not invalidate a still-working mirror.
    }
}

pub fn valid_origin(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let parts: Vec<_> = host.split('.').collect();
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.as_str() == value
        && parts.len() >= 2
        && parts[parts.len() - 2].starts_with("hitmo")
        && parts.last().is_some_and(|p| {
            (2..=15).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_lowercase())
        })
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 63
                && !p.starts_with('-')
                && !p.ends_with('-')
                && p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

pub async fn fetch(repo: &str) -> Result<HitmoMirror> {
    let repo = wedex_update::repository(repo)?;
    let http = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("WeDex mirror discovery")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut response = http
        .get(format!(
            "https://raw.githubusercontent.com/{repo}/hitmo-mirror/mirror.json"
        ))
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(part) = response.chunk().await? {
        ensure!(bytes.len() + part.len() <= 8192, "Mirror record too large");
        bytes.extend_from_slice(&part);
    }
    let mirror: HitmoMirror = serde_json::from_slice(&bytes)?;
    ensure!(
        mirror.valid(crate::now_ms()),
        "Invalid or expired mirror record"
    );
    Ok(mirror)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Reads the public GitHub mirror record without credentials"]
    async fn live_public_mirror_record() {
        let record = fetch(wedex_update::DEFAULT_REPOSITORY).await.unwrap();
        assert!(record.valid(crate::now_ms()));
        println!("Public mirror record validated: {}", record.origin);
    }
    #[test]
    fn rejects_private_arbitrary_and_future_addresses() {
        for bad in [
            "https://127.0.0.1/",
            "https://hitmos.pro.evil.test/",
            "https://hitmos.pro@localhost/",
            "https://hitmos.pro:444/",
            "http://ru.hitmox.net/",
            "https://ru.hitmox.net/search",
        ] {
            assert!(!valid_origin(bad), "{bad}");
        }
        for good in [
            "https://ru.hitmox.net/",
            "https://rus.hitmos.fm/",
            "https://ru.hitmo-top.cc/",
        ] {
            assert!(valid_origin(good));
        }
        let mut m = HitmoMirror {
            service: "wedex-hitmo-mirror".into(),
            schema: 1,
            source: "https://hitmos.pro/".into(),
            origin: "https://ru.hitmox.net/".into(),
            checked_at_ms: 1_000_000_000,
        };
        assert!(m.valid(1_000_000_000));
        assert!(m.valid(2_000_000_000));
        m.checked_at_ms = 1_001_000_000;
        assert!(!m.valid(1_000_000_000));
    }
}

//! Owner operations. The owner secret is provisioned through SSH, never pairing.
use crate::Invitation;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Owner {
    pub endpoint: String,
    pub certificate: String,
    pub owner: String,
}
impl Owner {
    pub fn blocking_version(&self) -> Result<String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let value: serde_json::Value = self
                    .http()?
                    .get(format!("{}health", self.endpoint))
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                Ok(value["version"].as_str().unwrap_or_default().into())
            })
    }
    pub fn blocking_update(&self) -> Result<serde_json::Value> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                Ok(self
                    .http()?
                    .post(format!("{}api/update", self.endpoint))
                    .bearer_auth(&self.owner)
                    .json(&serde_json::json!({}))
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?)
            })
    }
    pub fn blocking_update_status(&self) -> Result<serde_json::Value> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                Ok(self
                    .http()?
                    .get(format!("{}api/update/status", self.endpoint))
                    .bearer_auth(&self.owner)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?)
            })
    }
    pub fn blocking_revoke(&self, id: &str) -> Result<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(self.revoke(id))
    }
    pub async fn revoke(&self, id: &str) -> Result<()> {
        let id = uuid::Uuid::parse_str(id)?;
        let http = self.http()?;
        http.post(format!("{}api/devices/{id}/revoke", self.endpoint))
            .bearer_auth(&self.owner)
            .json(&serde_json::json!({}))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
    pub fn blocking_invitation(&self) -> Result<Invitation> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(self.invitation())
    }
    pub async fn invitation(&self) -> Result<Invitation> {
        let url = reqwest::Url::parse(&self.endpoint)?;
        anyhow::ensure!(
            url.scheme() == "https" && url.host_str().is_some(),
            "Нужен HTTPS-адрес"
        );
        let http = self.http()?;
        let value: serde_json::Value = http
            .post(url.join("api/invitations")?)
            .bearer_auth(&self.owner)
            .json(&serde_json::json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(Invitation {
            sync_key: None,
            endpoint: self.endpoint.clone(),
            certificate: self.certificate.clone(),
            code: value["code"]
                .as_str()
                .context("Сервер не выдал приглашение")?
                .into(),
        })
    }
    fn http(&self) -> Result<reqwest::Client> {
        let url = reqwest::Url::parse(&self.endpoint)?;
        anyhow::ensure!(
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none(),
            "Некорректный адрес сервера"
        );
        Ok(reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .tls_built_in_root_certs(false)
            .add_root_certificate(reqwest::Certificate::from_pem(self.certificate.as_bytes())?)
            .timeout(std::time::Duration::from_secs(12))
            .build()?)
    }
}

use super::*;
use crate::accounts::{Authenticated, Login};
use base64::Engine;
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use std::{
    net::{TcpStream, ToSocketAddrs},
    sync::Mutex,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerIdentity {
    pub endpoint: String,
    pub certificate: String,
}
pub fn same_certificate(a: &str, b: &str) -> bool {
    let a = rustls_pemfile::certs(&mut a.as_bytes())
        .next()
        .and_then(Result::ok);
    let b = rustls_pemfile::certs(&mut b.as_bytes())
        .next()
        .and_then(Result::ok);
    a.is_some() && a == b
}
impl ServerIdentity {
    pub fn fingerprint(&self) -> Result<String> {
        let cert = rustls_pemfile::certs(&mut self.certificate.as_bytes())
            .next()
            .context("Нет сертификата")??;
        use sha2::Digest;
        Ok(sha2::Sha256::digest(cert.as_ref())
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"))
    }
    pub fn http(&self) -> Result<reqwest::Client> {
        crate::Invitation::decode(
            &crate::Invitation {
                sync_key: None,
                endpoint: self.endpoint.clone(),
                certificate: self.certificate.clone(),
                code: "probe".into(),
            }
            .encode(),
        )?;
        Ok(reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .tls_built_in_root_certs(false)
            .add_root_certificate(reqwest::Certificate::from_pem(self.certificate.as_bytes())?)
            .timeout(Duration::from_secs(30))
            .build()?)
    }
}
#[derive(Debug)]
struct Capture(Mutex<Option<Vec<u8>>>);
impl ServerCertVerifier for Capture {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.0.lock().unwrap() = Some(cert.as_ref().to_vec());
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
/// Fetches only the public TLS certificate. Never sends an HTTP request, login or password.
/// The caller must explicitly approve/pin its fingerprint before authenticating.
pub fn discover(address: &str) -> Result<ServerIdentity> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let address = address.trim();
    let url = url::Url::parse(&if address.contains("://") {
        address.into()
    } else {
        format!("https://{address}")
    })?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "Нужен HTTPS-адрес сервера без пути"
    );
    let host = url
        .host_str()
        .context("Укажите сервер")?
        .trim_matches(['[', ']']);
    let port = if address.contains("://") {
        url.port_or_known_default().unwrap_or(8765)
    } else {
        url.port().unwrap_or(8765)
    };
    let capture = Arc::new(Capture(Mutex::new(None)));
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(capture.clone())
        .with_no_client_auth();
    let mut tls =
        rustls::ClientConnection::new(Arc::new(config), ServerName::try_from(host.to_owned())?)?;
    let addresses = (host, port).to_socket_addrs()?.take(8).collect::<Vec<_>>();
    let mut socket = addresses
        .iter()
        .find_map(|a| TcpStream::connect_timeout(a, Duration::from_secs(4)).ok())
        .context("Сервер недоступен")?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    while tls.is_handshaking() {
        tls.complete_io(&mut socket)?;
    }
    let der = capture
        .0
        .lock()
        .unwrap()
        .take()
        .context("Нет сертификата сервера")?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let certificate = format!(
        "-----BEGIN CERTIFICATE-----\r\n{}\r\n-----END CERTIFICATE-----\r\n",
        b64.as_bytes()
            .chunks(64)
            .map(|v| std::str::from_utf8(v).unwrap())
            .collect::<Vec<_>>()
            .join("\r\n")
    );
    let mut endpoint = url.clone();
    endpoint
        .set_port(Some(port))
        .map_err(|_| anyhow::anyhow!("Некорректный порт"))?;
    Ok(ServerIdentity {
        endpoint: endpoint.to_string(),
        certificate,
    })
}
pub fn blocking_account(
    server: ServerIdentity,
    request: Login,
    register: bool,
    previous: Option<Connection>,
) -> Result<Connection> {
    request.validate()?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let http = server.http()?;
            let mut post = http.post(format!(
                "{}api/accounts/{}",
                server.endpoint,
                if register { "register" } else { "login" }
            ));
            let previous = previous.filter(|c| {
                c.endpoint == server.endpoint
                    && same_certificate(&c.certificate, &server.certificate)
            });
            if let Some(c) = &previous {
                post = post.bearer_auth(&c.credentials.token);
            }
            let response = post.json(&request).send().await?;
            if !response.status().is_success() {
                let value: serde_json::Value = response.json().await.unwrap_or_default();
                anyhow::bail!(
                    "{}",
                    value["error"]
                        .as_str()
                        .unwrap_or("Не удалось войти в аккаунт")
                );
            }
            let response: Authenticated = response.json().await?;
            anyhow::ensure!(
                response.credentials.account.is_some()
                    && crate::vault::valid_key(&response.sync_key),
                "Некорректный ответ сервера"
            );
            Ok(Connection {
                sync_key: Some(response.sync_key),
                endpoint: server.endpoint,
                // Legacy local cursors used the exact PEM bytes as identity.
                // Preserve them while claiming an existing installation.
                certificate: if response
                    .credentials
                    .account
                    .as_ref()
                    .is_some_and(|a| a.id == "legacy")
                {
                    previous
                        .map(|c| c.certificate)
                        .unwrap_or(server.certificate)
                } else {
                    server.certificate
                },
                credentials: response.credentials,
            })
        })
}
pub fn blocking_invite(connection: &Connection) -> Result<Invitation> {
    let connection = connection.clone();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let http = ServerIdentity {
                endpoint: connection.endpoint.clone(),
                certificate: connection.certificate.clone(),
            }
            .http()?;
            let response: serde_json::Value = http
                .post(format!("{}api/invitations", connection.endpoint))
                .bearer_auth(&connection.credentials.token)
                .json(&serde_json::json!({}))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            Ok(Invitation {
                endpoint: connection.endpoint,
                certificate: connection.certificate,
                sync_key: connection.sync_key,
                code: response["code"].as_str().context("Нет приглашения")?.into(),
            })
        })
}
pub fn blocking_revoke(connection: &Connection, id: &str) -> Result<()> {
    let id = uuid::Uuid::parse_str(id)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            ServerIdentity {
                endpoint: connection.endpoint.clone(),
                certificate: connection.certificate.clone(),
            }
            .http()?
            .post(format!("{}api/devices/{id}/revoke", connection.endpoint))
            .bearer_auth(&connection.credentials.token)
            .json(&serde_json::json!({}))
            .send()
            .await?
            .error_for_status()?;
            Ok(())
        })
}
pub fn blocking_search(connection: &Connection, query: &str) -> Result<(String, String)> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let value: serde_json::Value = ServerIdentity {
                endpoint: connection.endpoint.clone(),
                certificate: connection.certificate.clone(),
            }
            .http()?
            .get(format!("{}api/hitmo/search", connection.endpoint))
            .bearer_auth(&connection.credentials.token)
            .query(&[("q", query)])
            .timeout(Duration::from_secs(4))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
            Ok((
                value["base"]
                    .as_str()
                    .context("Нет адреса каталога")?
                    .into(),
                value["html"]
                    .as_str()
                    .filter(|s| s.len() <= 1024 * 1024)
                    .context("Нет страницы каталога")?
                    .into(),
            ))
        })
}

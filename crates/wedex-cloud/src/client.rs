use crate::protocol::*;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc as std_mpsc, Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{tungstenite::Message, Connector};

#[derive(Clone, Serialize, Deserialize)]
pub struct Connection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_key: Option<String>,
    pub endpoint: String,
    pub certificate: String,
    pub credentials: Credentials,
}
impl Connection {
    pub fn server_id(&self) -> String {
        crate::database::digest(&self.certificate)
    }
}
pub async fn pair(invitation: &Invitation, name: &str, can_play: bool) -> Result<Connection> {
    // Validate externally supplied endpoint before sending its one-use invitation.
    Invitation::decode(&invitation.encode())?;
    let http = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(
            invitation.certificate.as_bytes(),
        )?)
        .timeout(Duration::from_secs(8))
        .build()?;
    let response = http
        .post(format!("{}api/pair", invitation.endpoint))
        .json(&PairRequest {
            code: invitation.code.clone(),
            name: name.into(),
            can_play,
        })
        .send()
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "Не удалось привязать устройство: приглашение истекло или уже использовано"
    );
    Ok(Connection {
        sync_key: invitation.sync_key.clone(),
        endpoint: invitation.endpoint.clone(),
        certificate: invitation.certificate.clone(),
        credentials: response.json().await?,
    })
}

pub enum Event {
    Connected,
    Disconnected,
    Message(ServerMessage),
}
pub struct Client {
    pub events: std_mpsc::Receiver<Event>,
    outgoing: mpsc::Sender<ClientMessage>,
    stop: watch::Sender<bool>,
    connected: Arc<AtomicBool>,
}
impl Client {
    pub fn spawn(connection: Connection) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        for certificate in rustls_pemfile::certs(&mut connection.certificate.as_bytes()) {
            roots.add(certificate?)?;
        }
        anyhow::ensure!(!roots.is_empty(), "Отсутствует сертификат сервера");
        let tls = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        let (event_tx, events) = std_mpsc::sync_channel(32);
        let (outgoing, receiver) = mpsc::channel(64);
        let (stop, shutdown) = watch::channel(false);
        let connected = Arc::new(AtomicBool::new(false));
        let flag = connected.clone();
        std::thread::Builder::new()
            .name("wedex-cloud-client".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => return,
                };
                runtime.block_on(worker(connection, tls, receiver, shutdown, flag, event_tx));
            })?;
        Ok(Self {
            events,
            outgoing,
            stop,
            connected,
        })
    }
    pub fn send(&self, message: ClientMessage) -> Result<()> {
        anyhow::ensure!(
            self.connected.load(Ordering::Relaxed),
            "Облако не подключено"
        );
        self.outgoing
            .try_send(message)
            .map_err(|_| anyhow::anyhow!("Очередь облака занята"))
    }
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}
async fn worker(
    connection: Connection,
    tls: Arc<rustls::ClientConfig>,
    mut receiver: mpsc::Receiver<ClientMessage>,
    mut shutdown: watch::Receiver<bool>,
    connected: Arc<AtomicBool>,
    events: std_mpsc::SyncSender<Event>,
) {
    let url = format!(
        "{}ws",
        connection.endpoint.replacen("https://", "wss://", 1)
    );
    let mut delay = 1;
    loop {
        if *shutdown.borrow() {
            break;
        }
        let attempt = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(8), tokio_tungstenite::connect_async_tls_with_config(
                &url, None, false, Some(Connector::Rustls(tls.clone())))) => result,
            _ = shutdown.changed() => break,
        };
        if let Ok(Ok((mut socket, _))) = attempt {
            let auth = serde_json::to_string(&ClientMessage::Authenticate {
                token: connection.credentials.token.clone(),
            })
            .unwrap();
            if socket.send(Message::Text(auth.into())).await.is_ok() {
                let mut authenticated = false;
                let mut ping = tokio::time::interval(Duration::from_secs(10));
                let auth_deadline = tokio::time::sleep(Duration::from_secs(5));
                tokio::pin!(auth_deadline);
                loop {
                    tokio::select! {
                        _ = &mut auth_deadline, if !authenticated => break,
                        _ = shutdown.changed() => { let _ = socket.close(None).await; return; }
                        _ = ping.tick(), if authenticated => {
                            if socket.send(Message::Ping(vec![].into())).await.is_err() { break; }
                        }
                        message = receiver.recv(), if authenticated => {
                            let Some(message) = message else { return; };
                            let Ok(json) = serde_json::to_string(&message) else { continue; };
                            if socket.send(Message::Text(json.into())).await.is_err() { break; }
                        }
                        message = socket.next() => {
                            match message {
                                Some(Ok(Message::Text(text))) => {
                                    if let Ok(event) = serde_json::from_str::<ServerMessage>(&text) {
                                        if let ServerMessage::Welcome { api_version, device_id, .. } = &event {
                                            if *api_version != API_VERSION || device_id != &connection.credentials.device_id { break; }
                                            authenticated = true;
                                            delay = 1;
                                            connected.store(true, Ordering::Relaxed);
                                            if events.send(Event::Connected).is_err() { return; }
                                        }
                                        if events.send(Event::Message(event)).is_err() { return; }
                                    }
                                }
                                Some(Ok(Message::Ping(bytes))) => {
                                    if socket.send(Message::Pong(bytes)).await.is_err() { break; }
                                }
                                Some(Ok(Message::Pong(_))) => {}
                                _ => break,
                            }
                        }
                    }
                }
            }
        }
        connected.store(false, Ordering::Relaxed);
        // A stale button press must not be replayed after reconnection. Sessions remain in the caller's durable outbox.
        while receiver.try_recv().is_ok() {}
        if events.send(Event::Disconnected).is_err() {
            break;
        }
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
        }
        delay = (delay * 2).min(10);
    }
    connected.store(false, Ordering::Relaxed);
}

pub fn blocking_pair(invitation: &Invitation, name: &str, can_play: bool) -> Result<Connection> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Не удалось запустить подключение")?
        .block_on(pair(invitation, name, can_play))
}

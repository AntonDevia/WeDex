use crate::{
    database::{digest, secret, Database},
    now_ms,
    protocol::*,
};
use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Path, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, mpsc};

#[derive(Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub bind: SocketAddr,
}
struct Connected {
    connection: String,
    device: Device,
    outgoing: mpsc::Sender<ServerMessage>,
}
struct Hub {
    data_dir: PathBuf,
    update_repo: String,
    db: Database,
    owner_hash: String,
    connected: Mutex<HashMap<String, Connected>>,
    changes: broadcast::Sender<()>,
    pairing_rate: Mutex<(u64, u32)>,
}

pub struct RunningServer {
    pub endpoint: String,
    pub lan_endpoint: Option<String>,
    pub certificate: String,
    owner: String,
    handle: axum_server::Handle,
    hub: Arc<Hub>,
    finished: Option<std::thread::JoinHandle<()>>,
    _lock: std::fs::File,
}
impl RunningServer {
    pub fn invitation(&self, lan: bool) -> Result<Invitation> {
        Ok(Invitation {
            sync_key: None,
            endpoint: if lan {
                self.lan_endpoint
                    .clone()
                    .context("Домашняя сеть не включена")?
            } else {
                self.endpoint.clone()
            },
            certificate: self.certificate.clone(),
            code: self.hub.db.invitation()?,
        })
    }
    pub fn browser_invitation(&self, lan: bool) -> Result<String> {
        let invitation = self.invitation(lan)?;
        Ok(format!("{}#pair={}", invitation.endpoint, invitation.code))
    }
    pub fn revoke(&self, id: &str) -> Result<()> {
        revoke(&self.hub, id)
    }
    pub fn is_finished(&self) -> bool {
        self.finished.as_ref().is_none_or(|h| h.is_finished())
    }
    pub fn owner_token(&self) -> &str {
        &self.owner
    }
}
impl Drop for RunningServer {
    fn drop(&mut self) {
        self.handle.graceful_shutdown(Some(Duration::from_secs(1)));
        if let Some(thread) = self.finished.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Identity {
    owner: String,
}

pub fn lan_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    // UDP connect selects a route, without transmitting a datagram.
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

pub fn start(config: Config) -> Result<RunningServer> {
    start_with_public_host(config, None)
}

/// The advertised host is supplied by the SSH installer, never by a public API.
pub fn start_with_public_host(config: Config, public_host: Option<&str>) -> Result<RunningServer> {
    start_configured(config, public_host, wedex_update::DEFAULT_REPOSITORY)
}
pub fn start_configured(
    config: Config,
    public_host: Option<&str>,
    repo: &str,
) -> Result<RunningServer> {
    if let Some(host) = public_host {
        anyhow::ensure!(
            valid_public_host(host),
            "Некорректный публичный адрес сервера"
        );
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    std::fs::create_dir_all(&config.data_dir)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(config.data_dir.join("server.lock"))?;
    lock.try_lock()
        .map_err(|_| anyhow::anyhow!("Эта база уже используется другим сервером WeDex"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.data_dir, std::fs::Permissions::from_mode(0o700))?;
    }
    // Acquire the listening port before touching identity/data for a second launch.
    let listener =
        std::net::TcpListener::bind(config.bind).context("Порт локального сервера занят")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let id_file = config.data_dir.join("identity.json");
    let identity: Identity = if id_file.exists() {
        serde_json::from_slice(&std::fs::read(&id_file)?)?
    } else {
        let value = Identity { owner: secret() };
        private_write(&id_file, &serde_json::to_vec(&value)?)?;
        value
    };
    let cert_file = config.data_dir.join("certificate.pem");
    let key_file = config.data_dir.join("private-key.pem");
    let lan = lan_ip();
    if !cert_file.exists() || !key_file.exists() {
        let mut names = vec!["localhost".into(), "127.0.0.1".into(), "::1".into()];
        if let Some(host) = public_host {
            names.push(host.into());
        }
        if let Some(ip) = lan {
            names.push(ip.to_string());
        }
        if !config.bind.ip().is_unspecified() {
            names.push(config.bind.ip().to_string());
        }
        let certified = rcgen::generate_simple_self_signed(names)?;
        private_write(&key_file, certified.key_pair.serialize_pem().as_bytes())?;
        private_write(&cert_file, certified.cert.pem().as_bytes())?;
    }
    let certificate = std::fs::read_to_string(&cert_file)?;
    let key = std::fs::read(&key_file)?;
    let db = Database::open(&config.data_dir.join("cloud.sqlite3"))?;
    let (changes, _) = broadcast::channel(16);
    let hub = Arc::new(Hub {
        data_dir: config.data_dir.clone(),
        update_repo: wedex_update::repository(repo)?,
        db,
        owner_hash: digest(&identity.owner),
        connected: Mutex::new(HashMap::new()),
        changes,
        pairing_rate: Mutex::new((0, 0)),
    });
    let routes = Router::new()
        .route("/", get(|| async { Html(include_str!("remote.html")) }))
        .route("/medeo.ttf", get(|| async { ([(header::CONTENT_TYPE, "font/ttf")],
            include_bytes!("../../../assets/fonts/medeo/MedeoSans-Medium.ttf").as_slice()) }))
        .route("/health", get(|| async { Json(serde_json::json!({"service":"wedex", "api_version":API_VERSION, "version":env!("CARGO_PKG_VERSION")})) }))
        .route("/api/pair", post(pair))
        .route("/api/invitations", post(invitation))
        .route("/api/update",post(request_update))
        .route("/api/update/status",get(update_status))
        .route("/api/devices/{id}/revoke", post(revoke_device))
        .route("/ws", get(upgrade))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(axum::middleware::map_response(|mut response: Response| async move {
            let headers = response.headers_mut();
            headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
            headers.insert("x-content-type-options", "nosniff".parse().unwrap());
            headers.insert("content-security-policy", "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; font-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().unwrap());
            response
        }))
        .with_state(hub.clone());
    let handle = axum_server::Handle::new();
    let worker_handle = handle.clone();
    let cert = certificate.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let finished = std::thread::Builder::new()
        .name("wedex-server".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            runtime.block_on(async move {
                let tls =
                    match axum_server::tls_rustls::RustlsConfig::from_pem(cert.into_bytes(), key)
                        .await
                    {
                        Ok(tls) => tls,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e.to_string()));
                            return;
                        }
                    };
                let _ = ready_tx.send(Ok(()));
                if let Err(e) = axum_server::from_tcp_rustls(listener, tls)
                    .handle(worker_handle)
                    .serve(routes.into_make_service())
                    .await
                {
                    eprintln!("WeDex server transport: {e}");
                }
            });
        })?;
    ready_rx
        .recv_timeout(Duration::from_secs(10))?
        .map_err(anyhow::Error::msg)?;
    let advertised_ip = if address.ip().is_unspecified() {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        address.ip()
    };
    Ok(RunningServer {
        endpoint: format!(
            "https://{}/",
            SocketAddr::new(advertised_ip, address.port())
        ),
        lan_endpoint: if address.ip().is_unspecified() {
            lan.map(|ip| format!("https://{}/", SocketAddr::new(ip, address.port())))
        } else {
            None
        },
        certificate,
        owner: identity.owner,
        handle,
        hub,
        finished: Some(finished),
        _lock: lock,
    })
}

pub fn valid_public_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return !ip.is_unspecified() && !ip.is_multicast();
    }
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

fn private_write(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

type ApiResult<T> = std::result::Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;
fn failure(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({"error":message})))
}
fn internal<T>(result: Result<T>) -> ApiResult<T> {
    result
        .map(Json)
        .map_err(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "Ошибка базы сервера"))
}
fn owner(hub: &Hub, headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|token| digest(token) == hub.owner_hash)
}
async fn pair(
    State(hub): State<Arc<Hub>>,
    Json(request): Json<PairRequest>,
) -> ApiResult<Credentials> {
    {
        let mut rate = hub.pairing_rate.lock().unwrap();
        let minute = now_ms() / 60000;
        if rate.0 != minute {
            *rate = (minute, 0);
        }
        rate.1 += 1;
        if rate.1 > 30 {
            return Err(failure(
                StatusCode::TOO_MANY_REQUESTS,
                "Повторите подключение через минуту",
            ));
        }
    }
    match hub.db.pair(&request) {
        Ok(credentials) => {
            let _ = hub.changes.send(());
            Ok(Json(credentials))
        }
        Err(_) => Err(failure(
            StatusCode::BAD_REQUEST,
            "Приглашение недействительно или имя устройства некорректно",
        )),
    }
}
async fn invitation(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    if !owner(&hub, &headers) {
        return Err(failure(
            StatusCode::UNAUTHORIZED,
            "Требуется доступ владельца",
        ));
    }
    internal(
        hub.db
            .invitation()
            .map(|code| serde_json::json!({"code":code,"expires_ms":now_ms()+600_000})),
    )
}
fn revoke(hub: &Hub, id: &str) -> Result<()> {
    hub.db.revoke(id)?;
    if let Some(connection) = hub.connected.lock().unwrap().remove(id) {
        let _ = connection.outgoing.try_send(ServerMessage::Error {
            message: "Доступ устройства отозван".into(),
        });
    }
    let _ = hub.changes.send(());
    Ok(())
}
async fn revoke_device(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<bool> {
    if !owner(&hub, &headers) {
        return Err(failure(
            StatusCode::UNAUTHORIZED,
            "Требуется доступ владельца",
        ));
    }
    internal(revoke(&hub, &id).map(|_| true))
}
async fn upgrade(State(hub): State<Arc<Hub>>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(256 * 1024)
        .max_frame_size(256 * 1024)
        .on_upgrade(move |socket| connection(socket, hub))
}
async fn send(socket: &mut WebSocket, event: &ServerMessage) -> bool {
    match serde_json::to_string(event) {
        Ok(json) => matches!(
            tokio::time::timeout(
                Duration::from_secs(5),
                socket.send(Message::Text(json.into()))
            )
            .await,
            Ok(Ok(()))
        ),
        Err(_) => false,
    }
}
fn snapshot(hub: &Hub) -> Result<ServerMessage> {
    let mut devices = hub.db.devices()?;
    let connected = hub.connected.lock().unwrap();
    for device in &mut devices {
        if let Some(active) = connected.get(&device.id) {
            *device = active.device.clone();
        }
    }
    drop(connected);
    Ok(ServerMessage::Snapshot {
        devices,
        statistics: hub.db.statistics()?,
    })
}
async fn connection(mut socket: WebSocket, hub: Arc<Hub>) {
    let first = tokio::time::timeout(Duration::from_secs(5), socket.recv()).await;
    let token = match first {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Authenticate { token }) if token.len() <= 128 => token,
            _ => return,
        },
        _ => return,
    };
    let mut device = match hub.db.authenticate(&token) {
        Ok(Some(device)) => device,
        _ => return,
    };
    device.online = true;
    device.updated_ms = now_ms();
    let id = device.id.clone();
    let connection_id = uuid::Uuid::new_v4().to_string();
    let (outgoing, mut incoming) = mpsc::channel(64);
    // One live connection per device key. A superseded connection exits on its next tick.
    hub.connected.lock().unwrap().insert(
        id.clone(),
        Connected {
            connection: connection_id.clone(),
            device: device.clone(),
            outgoing,
        },
    );
    let _ = hub.changes.send(());
    if !send(
        &mut socket,
        &ServerMessage::Welcome {
            version: env!("CARGO_PKG_VERSION").into(),
            capabilities: if crate::update::enabled() {
                vec!["connections-vault".into(), "server-update".into()]
            } else {
                vec!["connections-vault".into()]
            },
            device_id: id.clone(),
            api_version: API_VERSION,
        },
    )
    .await
    {
        hub.connected.lock().unwrap().remove(&id);
        let _ = hub.changes.send(());
        return;
    }
    let mut changes = hub.changes.subscribe();
    let mut timer = tokio::time::interval(Duration::from_secs(2));
    loop {
        let event = tokio::select! {
            _ = timer.tick() => {
                let valid = hub.connected.lock().unwrap().get(&id).is_some_and(|c| c.connection == connection_id);
                if !valid || !matches!(hub.db.authenticate(&token), Ok(Some(_))) { break; }
                snapshot(&hub).ok()
            }
            _ = changes.recv() => snapshot(&hub).ok(),
            message = incoming.recv() => message,
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let current = hub.connected.lock().unwrap().get(&id).is_some_and(|c| c.connection == connection_id);
                        if !current || !matches!(hub.db.authenticate(&token), Ok(Some(_))) { break; }
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(message) => match receive(&hub, &device, message) {
                                Ok(response) => response,
                                Err(_) => Some(ServerMessage::Error { message: "Команда отклонена сервером".into() }),
                            },
                            Err(_) => Some(ServerMessage::Error { message: "Некорректный формат сообщения".into() }),
                        }
                    }
                    Some(Ok(Message::Ping(bytes))) => {
                        if socket.send(Message::Pong(bytes)).await.is_err() { break; }
                        None
                    }
                    Some(Ok(Message::Pong(_))) => None,
                    _ => break,
                }
            }
        };
        if let Some(event) = event {
            if !send(&mut socket, &event).await {
                break;
            }
        }
    }
    let mut connected = hub.connected.lock().unwrap();
    if connected
        .get(&id)
        .is_some_and(|c| c.connection == connection_id)
    {
        connected.remove(&id);
    }
    drop(connected);
    let _ = hub.changes.send(());
}

async fn request_update(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    if !owner(&hub, &headers) {
        return Err(failure(StatusCode::UNAUTHORIZED, "Требуется владелец"));
    }
    if !crate::update::enabled() {
        return Err(failure(
            StatusCode::CONFLICT,
            "Сначала обновите установку через SSH-мастер",
        ));
    }
    let repo = hub.update_repo.clone();
    let release = tokio::task::spawn_blocking(move || {
        wedex_update::check(
            &repo,
            wedex_update::Target::Server,
            env!("CARGO_PKG_VERSION"),
        )
    })
    .await;
    let release = match release {
        Ok(Ok(value)) => value,
        _ => {
            return Err(failure(
                StatusCode::BAD_GATEWAY,
                "Не удалось проверить GitHub Releases",
            ))
        }
    };
    let Some(release) = release else {
        return Ok(Json(serde_json::json!({"state":"up_to_date"})));
    };
    let request = crate::update::Request {
        version: release.version,
    };
    let temporary = hub
        .data_dir
        .join(format!("update-{}.tmp", uuid::Uuid::new_v4()));
    internal((|| -> Result<serde_json::Value> {
        let status = hub
            .data_dir
            .join(format!("status-{}.tmp", uuid::Uuid::new_v4()));
        private_write(
            &status,
            &serde_json::to_vec(
                &serde_json::json!({"state":"requested","version":request.version,"message":"Обновление запрошено"}),
            )?,
        )?;
        std::fs::rename(status, hub.data_dir.join("update-status.json"))?;
        private_write(&temporary, &serde_json::to_vec(&request)?)?;
        std::fs::rename(temporary, hub.data_dir.join("update-request.json"))?;
        Ok(serde_json::json!({"state":"requested"}))
    })())
}
async fn update_status(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    if !owner(&hub, &headers) {
        return Err(failure(StatusCode::UNAUTHORIZED, "Требуется владелец"));
    }
    let value = std::fs::read(hub.data_dir.join("update-status.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .unwrap_or(serde_json::json!({"state":"idle"}));
    Ok(Json(value))
}
fn receive(hub: &Hub, device: &Device, message: ClientMessage) -> Result<Option<ServerMessage>> {
    match message {
        ClientMessage::VaultGet => {
            anyhow::ensure!(device.can_play, "Только нативные устройства");
            Ok(Some(ServerMessage::VaultSnapshot {
                entries: hub.db.vault()?,
            }))
        }
        ClientMessage::VaultSet { entries } => {
            anyhow::ensure!(device.can_play, "Только нативные устройства");
            hub.db.vault_set(&entries)?;
            let message = ServerMessage::VaultSnapshot {
                entries: hub.db.vault()?,
            };
            for peer in hub
                .connected
                .lock()
                .unwrap()
                .values()
                .filter(|c| c.device.can_play)
            {
                let _ = peer.outgoing.try_send(message.clone());
            }
            Ok(None)
        }
        ClientMessage::Publish { playback } => {
            anyhow::ensure!(
                device.can_play && playback.valid(),
                "Недопустимое состояние плеера"
            );
            if let Some(connected) = hub.connected.lock().unwrap().get_mut(&device.id) {
                connected.device.playback = playback;
                connected.device.updated_ms = now_ms();
            }
            let _ = hub.changes.send(());
            Ok(None)
        }
        ClientMessage::Command { command } => {
            let (fresh, mut status) = hub.db.command(&device.id, &command)?;
            if fresh {
                let connected = hub.connected.lock().unwrap();
                let delivered = connected.get(&command.target).is_some_and(|target| {
                    target
                        .outgoing
                        .try_send(ServerMessage::Execute {
                            delivery: Delivery {
                                command: command.clone(),
                                expires_ms: now_ms() + 10_000,
                            },
                        })
                        .is_ok()
                });
                if !delivered {
                    status = "offline".into();
                    hub.db.acknowledge(&command.target, &command.id, &status)?;
                }
            }
            Ok(Some(ServerMessage::CommandStatus {
                id: command.id,
                status,
            }))
        }
        ClientMessage::Acknowledge { id, error } => {
            let status = if error.is_some() { "rejected" } else { "done" };
            if let Some(sender) = hub.db.acknowledge(&device.id, &id, status)? {
                if let Some(connection) = hub.connected.lock().unwrap().get(&sender) {
                    let _ = connection.outgoing.try_send(ServerMessage::CommandStatus {
                        id,
                        status: status.into(),
                    });
                }
            }
            Ok(None)
        }
        ClientMessage::Sessions { sessions } => {
            anyhow::ensure!(device.can_play, "Пульт не записывает прослушивания");
            hub.db.sessions(&device.id, &sessions)?;
            let _ = hub.changes.send(());
            Ok(Some(ServerMessage::SessionsStored {
                sessions: sessions.into_iter().map(|s| (s.id, s.revision)).collect(),
            }))
        }
        ClientMessage::Ping => Ok(None),
        ClientMessage::Authenticate { .. } => anyhow::bail!("Повторная авторизация"),
    }
}

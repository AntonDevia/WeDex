//! Desktop-only SSH provisioning. Credentials never cross the application UI/log boundary.
mod ssh;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use wedex_cloud::{admin::Owner, client::Connection, Invitation};
use zeroize::Zeroizing;

pub const VERSION: &str = wedex_cloud::SERVER_VERSION;
#[derive(Clone)]
pub struct Request {
    pub host: String,
    pub public_host: String,
    pub ssh_port: u16,
    pub port: u16,
    pub user: String,
    pub password: Zeroizing<String>,
    pub key_path: String,
    pub key_passphrase: Zeroizing<String>,
    pub sudo_password: Zeroizing<String>,
    pub fingerprint: Option<String>,
    pub device_name: String,
    pub connect_only: bool,
    pub previous: Option<Connection>,
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            wedex_cloud::server::valid_public_host(&self.host),
            "Укажите IP или домен без https:// и порта"
        );
        anyhow::ensure!(
            wedex_cloud::server::valid_public_host(&self.public_host),
            "Некорректный публичный IP или домен"
        );
        anyhow::ensure!(
            self.ssh_port != 0 && self.port != 0 && self.ssh_port != self.port,
            "Порты SSH и WeDex должны быть разными и ненулевыми"
        );
        anyhow::ensure!(
            !self.user.is_empty()
                && self.user.len() <= 64
                && self
                    .user
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
            "Некорректный SSH-логин"
        );
        anyhow::ensure!(
            self.device_name.len() <= 120 && !self.device_name.trim().is_empty(),
            "Укажите имя устройства"
        );
        anyhow::ensure!(
            !self.password.contains(['\n', '\r', '\0'])
                && !self.sudo_password.contains(['\n', '\r', '\0']),
            "Пароль не должен содержать перевод строки или NUL"
        );
        Ok(())
    }
    pub fn pin_name(&self) -> String {
        format!("vps-host:{}:{}", self.host, self.ssh_port)
    }
}
pub enum Event {
    Step(String),
    HostKey {
        fingerprint: String,
        previous: Option<String>,
    },
    Trusted {
        pin_name: String,
        fingerprint: String,
    },
    Bootstrap(Owner),
    Complete(Connection),
    Failed(String),
}
pub struct Worker {
    pub events: mpsc::UnboundedReceiver<Event>,
    decision: Option<oneshot::Sender<bool>>,
    cancelled: Arc<AtomicBool>,
}
impl Worker {
    pub fn start(request: Request, binary: &'static [u8]) -> Result<Self> {
        request.validate()?;
        if !request.connect_only {
            validate_binary(binary)?;
        }
        let (sender, events) = mpsc::unbounded_channel();
        let (decision, trust) = oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        std::thread::Builder::new()
            .name("wedex-vps-install".into())
            .spawn(move || {
                let result = (|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(async {
                        tokio::select! {
                            result=run(&request, binary, sender.clone(), trust, stop.clone())=>result,
                            _=async {
                                while !stop.load(Ordering::Relaxed) {tokio::time::sleep(Duration::from_millis(50)).await;}
                            }=>Err(anyhow::anyhow!("Установка отменена; выполненные шаги и данные сервера сохранены")),
                        }
                    })
                })();
                if let Err(error) = result {
                    let mut message = format!("{error:#}");
                    for secret in [
                        &request.password,
                        &request.sudo_password,
                        &request.key_passphrase,
                    ] {
                        if !secret.is_empty() {
                            message = message.replace(secret.as_str(), "[secret]");
                        }
                    }
                    let _ = sender.send(Event::Failed(message));
                }
            })?;
        Ok(Self {
            events,
            decision: Some(decision),
            cancelled,
        })
    }
    pub fn trust(&mut self, accept: bool) {
        if let Some(tx) = self.decision.take() {
            let _ = tx.send(accept);
        }
    }
    pub fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.trust(false);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
pub fn validate_binary(binary: &[u8]) -> Result<()> {
    anyhow::ensure!(
        binary.len() > 4096
            && binary.get(..6) == Some(b"\x7fELF\x02\x01")
            && binary.get(18..20) == Some(&[62, 0]),
        "В сборке нет Linux x86_64 сервера; используйте полный дистрибутив WeDex"
    );
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct Bootstrap {
    pub owner: Owner,
    pub invitation: Invitation,
}

async fn run(
    request: &Request,
    binary: &[u8],
    events: mpsc::UnboundedSender<Event>,
    trust: oneshot::Receiver<bool>,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    let step = |text: &str| {
        let _ = events.send(Event::Step(text.into()));
    };
    step("Подключаемся по SSH");
    let handler = ssh::Handler {
        pin_name: request.pin_name(),
        expected: request.fingerprint.clone(),
        events: events.clone(),
        decision: Some(trust),
    };
    let mut remote = ssh::Remote::connect(request, handler, cancelled).await?;
    // The handler has already compared the pinned key, or received explicit UI trust.
    step("Проверяем Linux, права и свободное место");
    let info = remote.exec(include_str!("probe.sh"), &[], true).await?;
    let get = |name: &str| {
        info.lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")))
            .unwrap_or("")
    };
    anyhow::ensure!(
        get("ARCH") == "x86_64",
        "Поддерживается Linux x86_64; архитектура сервера: {}",
        get("ARCH")
    );
    let os = get("OS");
    let version = get("VERSION");
    anyhow::ensure!(
        matches!(
            (os, version),
            ("debian", "12" | "13") | ("ubuntu", "22.04" | "24.04")
        ),
        "Поддерживаются Debian 12/13 и Ubuntu 22.04/24.04; найдено {os} {version}"
    );
    anyhow::ensure!(get("SYSTEMD") == "yes", "Нужен Linux с работающим systemd");
    anyhow::ensure!(
        get("DISK_KB").parse::<u64>().unwrap_or(0) >= 256 * 1024,
        "Нужно минимум 256 МБ свободного места в /var"
    );
    anyhow::ensure!(
        get("MANAGED") != "foreign",
        "Каталоги /opt/wedex или /etc/wedex заняты другой установкой"
    );
    let old = get("VERSION_INSTALLED");
    anyhow::ensure!(
        old.is_empty() || version_parts(old)? <= version_parts(VERSION)?,
        "На VPS установлена более новая версия; обновите WeDex на ПК"
    );
    if request.connect_only {
        anyhow::ensure!(
            get("MANAGED") == "yes",
            "WeDex ещё не установлен; выберите установку"
        );
    } else {
        let occupied = remote
            .exec(
                &format!("ss -H -lnt 'sport = :{}'", request.port),
                &[],
                true,
            )
            .await?;
        anyhow::ensure!(
            occupied.trim().is_empty() || get("MANAGED") == "yes",
            "Порт {} уже занят. Выберите другой порт WeDex",
            request.port
        );
        step("Передаём Linux-сервер по SSH и проверяем SHA-256");
        let stage = format!("/tmp/wedex-{}", uuid::Uuid::new_v4());
        remote
            .exec(&format!("umask 077; mkdir {}", quote(&stage)), &[], false)
            .await?;
        let hash = format!("{:x}", Sha256::digest(binary));
        let bind_host = if request.public_host.parse::<std::net::Ipv6Addr>().is_ok() {
            "[::]"
        } else {
            "0.0.0.0"
        };
        let config = serde_json::to_vec(
            &serde_json::json!({"data_dir":"/var/lib/wedex","bind":format!("{bind_host}:{}",request.port),"public_host":request.public_host}),
        )?;
        let manifest = serde_json::to_vec(
            &serde_json::json!({"product":"wedex","schema":1,"version":VERSION,"sha256":hash,"host":request.public_host,"port":request.port}),
        )?;
        let unit = service_unit(request.port);
        let update_service = wedex_cloud::update::SERVICE_UNIT.as_bytes();
        let update_path = wedex_cloud::update::PATH_UNIT.as_bytes();
        for (name, bytes) in [
            ("wedex-server", binary),
            ("install.sh", include_bytes!("install.sh").as_slice()),
            ("server.json", &config),
            ("install.json", &manifest),
            ("wedex.service", unit.as_bytes()),
            ("wedex-update.service", update_service),
            ("wedex-update.path", update_path),
        ] {
            remote.upload(&format!("{stage}/{name}"), bytes).await?;
        }
        remote
            .exec(
                &format!("chmod 700 {}/wedex-server", quote(&stage)),
                &[],
                false,
            )
            .await?;
        step("Создаём службу, сохраняем данные и запускаем WeDex");
        remote
            .exec(
                &format!(
                    "/bin/sh {}/install.sh {} {} {} {}",
                    quote(&stage),
                    quote(&stage),
                    quote(&hash),
                    quote(VERSION),
                    request.port
                ),
                &[],
                true,
            )
            .await?;
        // Only our exact generated files are removed; no recursive deletion.
        remote.exec(&format!("rm -f {0}/wedex-server {0}/install.sh {0}/server.json {0}/install.json {0}/wedex.service {0}/wedex-update.service {0}/wedex-update.path {0}/bootstrap.json; rmdir {0}", quote(&stage)), &[], true).await?;
    }
    step("Получаем приглашение через доверенный SSH-канал");
    let output = remote
        .exec(
            "/opt/wedex/current/wedex-server --config /etc/wedex/server.json --invite-json",
            &[],
            true,
        )
        .await?;
    let bootstrap: Bootstrap =
        serde_json::from_str(&output).context("Не удалось прочитать регистрацию сервера")?;
    let expected = endpoint(&request.public_host, request.port);
    anyhow::ensure!(
        bootstrap.owner.endpoint == expected && bootstrap.invitation.endpoint == expected,
        "Публичный адрес или порт отличаются от установленной конфигурации"
    );
    let _ = events.send(Event::Bootstrap(bootstrap.owner));
    step("Проверяем HTTPS и WebSocket с этого ПК");
    if let Some(connection) = request.previous.as_ref().filter(|c| {
        c.endpoint == bootstrap.invitation.endpoint
            && c.certificate == bootstrap.invitation.certificate
    }) {
        let previous = wedex_cloud::client::Client::spawn(connection.clone())?;
        for _ in 0..50 {
            if previous.connected() {
                let _ = events.send(Event::Complete(connection.clone()));
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    let connection = wedex_cloud::client::pair(&bootstrap.invitation, &request.device_name, true).await
        .with_context(|| format!("Сервер установлен, но недоступен с ПК. Разрешите TCP-порт {} в firewall VPS и панели хостинга", request.port))?;
    let client = wedex_cloud::client::Client::spawn(connection.clone())?;
    for _ in 0..100 {
        if client.connected() {
            let _ = events.send(Event::Complete(connection));
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("HTTPS работает, но WebSocket не подключился; проверьте сетевые ограничения VPS")
}
pub fn endpoint(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("https://[{host}]:{port}/")
    } else {
        format!("https://{host}:{port}/")
    }
}
fn version_parts(value: &str) -> Result<Vec<u64>> {
    let parts = value
        .trim()
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    anyhow::ensure!(
        parts.len() == 3,
        "Неизвестная версия установленного сервера"
    );
    Ok(parts)
}
pub fn service_unit(port: u16) -> String {
    let capabilities = if port < 1024 {
        "CAP_NET_BIND_SERVICE"
    } else {
        ""
    };
    format!("[Unit]\nDescription=WeDex personal music cloud\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser=wedex\nGroup=wedex\nExecStart=/opt/wedex/current/wedex-server --config /etc/wedex/server.json --service\nWorkingDirectory=/var/lib/wedex\nRestart=on-failure\nRestartSec=3\nTimeoutStopSec=15\nUMask=0077\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=strict\nProtectHome=true\nReadWritePaths=/var/lib/wedex\nAmbientCapabilities={capabilities}\nCapabilityBoundingSet={capabilities}\nRestrictSUIDSGID=true\nLockPersonality=true\nMemoryMax=256M\nTasksMax=64\nLimitNOFILE=4096\nStandardOutput=journal\nStandardError=journal\nLogRateLimitIntervalSec=30s\nLogRateLimitBurst=100\n\n[Install]\nWantedBy=multi-user.target\n")
}

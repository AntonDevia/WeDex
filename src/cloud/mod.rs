mod connections_sync;
mod listening;
#[cfg(windows)]
pub mod vps;
use crate::{
    audio,
    model::Track,
    store::{self, Settings, Store},
};
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
use wedex_cloud::{
    client::{self, Client, Connection},
    server::{self, RunningServer},
    *,
};

#[derive(Clone)]
pub enum Action {
    #[cfg(windows)]
    UpdateServer,
    #[cfg(windows)]
    Provision(Box<wedex_provision::Request>),
    #[cfg(windows)]
    ProvisionTrust(bool),
    #[cfg(windows)]
    ProvisionCancel,
    #[cfg(windows)]
    ProvisionReset,
    Start {
        lan: bool,
    },
    Stop,
    Connect {
        invitation: String,
        name: String,
    },
    Disconnect,
    Invite {
        browser: bool,
    },
    Revoke(String),
    Target(String),
    Remote(wedex_cloud::Action),
}
#[derive(Clone, Default)]
pub struct View {
    pub server_updating: bool,
    pub sync_status: String,
    pub server_version: String,
    pub capabilities: Vec<String>,
    #[cfg(windows)]
    pub vps: vps::View,
    pub owner_access: bool,
    pub server_running: bool,
    pub busy: bool,
    pub connected: bool,
    pub status: String,
    pub endpoint: String,
    pub lan_endpoint: Option<String>,
    pub invitation: String,
    pub browser_url: String,
    pub devices: Vec<Device>,
    pub statistics: Statistics,
    pub local_ms: u64,
    pub selected: String,
    pub own_device: String,
    pub observed: Option<(String, u64, Instant)>,
}
enum Job {
    Version(String, anyhow::Result<String>),
    ServerUpdateStatus(u64, anyhow::Result<serde_json::Value>),
    #[cfg(windows)]
    ServerUpdate(u64, anyhow::Result<serde_json::Value>),
    Revoked(u64, anyhow::Result<()>),
    Invited(u64, bool, anyhow::Result<Invitation>),
    Started(u64, anyhow::Result<(RunningServer, Connection)>),
    Paired(u64, anyhow::Result<Connection>),
}
pub struct Cloud {
    last_update: Instant,
    update_started: Instant,
    sync: connections_sync::Sync,
    last_vault: Instant,
    #[cfg(windows)]
    provision: Option<wedex_provision::Worker>,
    pub view: View,
    server: Option<RunningServer>,
    client: Option<Client>,
    connection: Option<Connection>,
    sender: Sender<Job>,
    receiver: Receiver<Job>,
    generation: u64,
    listening: listening::Listening,
    last_publish: Instant,
    last_sync: Instant,
    handled: VecDeque<String>,
    pub errors: Vec<String>,
    explicit_target: bool,
    pending_volume: Option<(String, f32, Instant)>,
}
pub struct LocalPlayback<'a> {
    pub status: &'a audio::Status,
    pub queue: &'a [Track],
    pub cursor: Option<usize>,
    pub settings: &'a Settings,
    pub shuffle: bool,
    pub favorites: &'a [Track],
}
impl View {
    pub fn remote_device(&self) -> Option<&Device> {
        self.devices
            .iter()
            .find(|d| d.id == self.selected && d.id != self.own_device && d.can_play)
    }
    pub fn remote_playback(&self) -> Option<Playback> {
        let device = self.remote_device()?;
        let mut p = device.playback.clone();
        if !self.connected || !device.online {
            p.playing = false;
            p.loading = false;
        } else if p.playing && !p.loading {
            if let Some((id, _, at)) = &self.observed {
                if id == &device.id {
                    p.position = (p.position + at.elapsed().as_secs_f64().min(2.)).min(p.duration);
                }
            }
        }
        Some(p)
    }
}
pub fn display_track(t: &wedex_cloud::Track) -> anyhow::Result<Track> {
    use crate::model::{Edition, Provider};
    let (provider, id) = t
        .key
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("Неизвестный источник трека"))?;
    let provider = match provider {
        "Yandex" => Provider::Yandex,
        "Hitmotop" => Provider::Hitmotop,
        "SoundCloud" => Provider::SoundCloud,
        "YouTube" => Provider::YouTube,
        "LastFm" => Provider::LastFm,
        _ => anyhow::bail!("Неизвестный источник трека"),
    };
    Ok(Track {
        id: id.into(),
        provider,
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        duration: t.duration.max(0.) as u64,
        cover: t.cover.clone(),
        page_url: t.page_url.clone(),
        edition: Edition::Unknown,
        reason: String::new(),
        score: 0.,
    })
}
pub fn track(track: &Track) -> wedex_cloud::Track {
    wedex_cloud::Track {
        key: track.key(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        duration: track.duration as f64,
        cover: track
            .cover
            .as_ref()
            .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
            .cloned(),
        page_url: track.page_url.clone(),
        album: track.album.clone(),
        liked: false,
    }
}
impl Cloud {
    #[cfg(test)]
    pub(crate) fn connected_for_test(connection: Connection) -> Self {
        let mut cloud = Self::new(true);
        cloud.attach(connection, false).unwrap();
        cloud
    }
    pub fn new(preview: bool) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut cloud = Self {
            last_update: Instant::now(),
            update_started: Instant::now(),
            sync: Default::default(),
            last_vault: Instant::now(),
            #[cfg(windows)]
            provision: None,
            view: View {
                status: "Облако не подключено".into(),
                ..Default::default()
            },
            server: None,
            client: None,
            connection: None,
            sender,
            receiver,
            generation: 0,
            listening: Default::default(),
            last_publish: Instant::now(),
            last_sync: Instant::now(),
            handled: VecDeque::new(),
            errors: vec![],
            explicit_target: false,
            pending_volume: None,
        };
        if !preview {
            if let Ok(connection) = serde_json::from_str::<Connection>(&store::secret("cloud")) {
                if let Err(e) = cloud.attach(connection, false) {
                    cloud.error(e);
                }
            }
        }
        cloud
    }
    fn error(&mut self, error: anyhow::Error) {
        self.errors.push(format!("{error:#}"));
        self.view.status = "Что-то пошло не так".into();
    }
    fn attach(&mut self, mut connection: Connection, save: bool) -> anyhow::Result<()> {
        let owner =
            serde_json::from_str::<wedex_cloud::admin::Owner>(&store::secret("cloud-owner"))
                .ok()
                .is_some_and(|o| {
                    o.endpoint == connection.endpoint && o.certificate == connection.certificate
                });
        let mut save = save;
        if connection.sync_key.is_none() && (self.server.is_some() || owner) {
            connection.sync_key = Some(wedex_cloud::vault::new_key()?);
            save = true;
        }
        self.sync = std::fs::read_to_string(vault_path(&connection))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        self.view.server_version.clear();
        self.view.capabilities.clear();
        self.view.server_updating = false;
        self.view.sync_status.clear();
        self.client = Some(Client::spawn(connection.clone())?);
        self.view.own_device = connection.credentials.device_id.clone();
        self.view.endpoint = connection.endpoint.clone();
        self.view.owner_access =
            serde_json::from_str::<wedex_cloud::admin::Owner>(&store::secret("cloud-owner"))
                .is_ok_and(|owner| {
                    owner.endpoint == connection.endpoint
                        && owner.certificate == connection.certificate
                });
        self.view.devices.clear();
        self.view.statistics = Statistics::default();
        self.view.selected.clear();
        self.explicit_target = false;
        self.view.observed = None;
        self.view.connected = false;
        self.view.status = "Подключаемся к облаку…".into();
        self.connection = Some(connection.clone());
        if save {
            store::save_secret("cloud", &serde_json::to_string(&connection)?)?;
        }
        Ok(())
    }
    pub fn action(&mut self, action: Action) {
        let result = match action {
            #[cfg(windows)]
            Action::UpdateServer => {
                if let Some(owner) = self.remote_owner() {
                    if self.view.server_updating {
                        return;
                    }
                    self.view.server_updating = true;
                    self.last_update = Instant::now();
                    self.update_started = Instant::now();
                    self.view.status = "Обновляем VPS…".into();
                    let sender = self.sender.clone();
                    let generation = self.generation;
                    std::thread::spawn(move || {
                        let _ = sender.send(Job::ServerUpdate(generation, owner.blocking_update()));
                    });
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Для обновления VPS нужен доступ владельца"))
                }
            }
            #[cfg(windows)]
            Action::ProvisionReset if self.provision.is_none() => {
                self.view.vps = Default::default();
                Ok(())
            }
            #[cfg(windows)]
            Action::Provision(mut request) if self.provision.is_none() && !self.view.busy => {
                request.previous = self.connection.clone();
                let fingerprint = store::secret(&request.pin_name());
                request.fingerprint = (!fingerprint.is_empty()).then_some(fingerprint);
                match wedex_provision::Worker::start(*request, vps::BINARY) {
                    Ok(worker) => {
                        self.provision = Some(worker);
                        self.view.busy = true;
                        self.view.vps = vps::View {
                            busy: true,
                            ..Default::default()
                        };
                        Ok(())
                    }
                    Err(error) => {
                        self.view.vps.error = Some(error.to_string());
                        Err(error)
                    }
                }
            }
            #[cfg(windows)]
            Action::ProvisionTrust(accept) => {
                if let Some(worker) = &mut self.provision {
                    worker.trust(accept);
                }
                self.view.vps.fingerprint = None;
                Ok(())
            }
            #[cfg(windows)]
            Action::ProvisionCancel => {
                if let Some(worker) = &mut self.provision {
                    worker.cancel();
                }
                self.view.vps.fingerprint = None;
                Ok(())
            }
            Action::Start { lan } if !self.view.busy && self.server.is_none() => {
                self.generation += 1;
                let generation = self.generation;
                self.view.busy = true;
                self.view.status = "Запускаем локальный сервер…".into();
                let sender = self.sender.clone();
                let previous = self.connection.clone();
                std::thread::spawn(move || {
                    let result = (|| {
                        let bind = if lan {
                            "0.0.0.0:8765"
                        } else {
                            "127.0.0.1:8765"
                        }
                        .parse()?;
                        let server = server::start(server::Config {
                            data_dir: store::data_dir().join("cloud-server"),
                            bind,
                        })?;
                        let connection = match previous.filter(|old| {
                            old.endpoint == server.endpoint && old.certificate == server.certificate
                        }) {
                            Some(connection) => connection,
                            None => client::blocking_pair(
                                &server.invitation(false)?,
                                &computer_name(),
                                true,
                            )?,
                        };
                        Ok((server, connection))
                    })();
                    let _ = sender.send(Job::Started(generation, result));
                });
                Ok(())
            }
            Action::Stop => {
                self.generation += 1;
                self.view.busy = false;
                self.client = None;
                self.view.connected = false;
                self.view.server_running = false;
                self.view.invitation.clear();
                self.view.browser_url.clear();
                self.view.lan_endpoint = None;
                for device in &mut self.view.devices {
                    device.online = false;
                }
                self.view.status = "Локальный сервер остановлен".into();
                if let Some(server) = self.server.take() {
                    std::thread::spawn(move || drop(server));
                }
                Ok(())
            }
            Action::Connect { invitation, name } if !self.view.busy => {
                match Invitation::decode(&invitation) {
                    Ok(invitation) => {
                        if self.view.connected && invitation.sync_key.is_some() {
                            if let Some(mut existing) = self.connection.clone().filter(|c| {
                                c.endpoint == invitation.endpoint
                                    && c.certificate == invitation.certificate
                            }) {
                                existing.sync_key = invitation.sync_key;
                                if let Err(e) = self.attach(existing, true) {
                                    self.error(e);
                                }
                                return;
                            }
                        }
                        self.generation += 1;
                        let generation = self.generation;
                        self.view.busy = true;
                        self.view.status = "Привязываем устройство…".into();
                        let sender = self.sender.clone();
                        std::thread::spawn(move || {
                            let result = client::blocking_pair(&invitation, &name, true);
                            let _ = sender.send(Job::Paired(generation, result));
                        });
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            Action::Disconnect => {
                #[cfg(windows)]
                if let Some(mut worker) = self.provision.take() {
                    worker.cancel();
                    self.view.vps.busy = false;
                    self.view.vps.fingerprint = None;
                }
                self.view.owner_access = false;
                self.view.server_updating = false;
                self.generation += 1;
                self.view.busy = false;
                self.client = None;
                self.connection = None;
                self.view.connected = false;
                self.view.devices.clear();
                self.view.status = "Облако отключено".into();
                store::save_secret("cloud", "")
            }
            Action::Invite { browser } => {
                if let Some(server) = &self.server {
                    if browser {
                        server
                            .browser_invitation(server.lan_endpoint.is_some())
                            .map(|url| {
                                self.view.browser_url = url;
                                self.view.invitation.clear();
                            })
                    } else {
                        server
                            .invitation(server.lan_endpoint.is_some())
                            .map(|invite| {
                                let mut invite = invite;
                                invite.sync_key =
                                    self.connection.as_ref().and_then(|c| c.sync_key.clone());
                                self.view.invitation = invite.encode();
                                self.view.browser_url.clear();
                            })
                    }
                } else {
                    let owner = serde_json::from_str::<wedex_cloud::admin::Owner>(&store::secret(
                        "cloud-owner",
                    ));
                    if let Some(owner) = owner.ok().filter(|o| {
                        o.endpoint == self.view.endpoint
                            && self
                                .connection
                                .as_ref()
                                .is_some_and(|c| c.certificate == o.certificate)
                    }) {
                        let sender = self.sender.clone();
                        let generation = self.generation;
                        std::thread::spawn(move || {
                            let result = tokio_invite(owner);
                            let _ = sender.send(Job::Invited(generation, browser, result));
                        });
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!(
                            "Создать приглашение может владелец сервера"
                        ))
                    }
                }
            }
            Action::Revoke(id) => {
                if let Some(server) = &self.server {
                    server.revoke(&id)
                } else if let Some(owner) = self.remote_owner() {
                    let sender = self.sender.clone();
                    let generation = self.generation;
                    std::thread::spawn(move || {
                        let _ = sender.send(Job::Revoked(generation, owner.blocking_revoke(&id)));
                    });
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Нужен доступ владельца сервера"))
                }
            }
            Action::Target(id) => {
                self.view.selected = id;
                self.explicit_target = true;
                self.view.observed = None;
                Ok(())
            }
            Action::Remote(action) => {
                self.explicit_target = true;
                let target = self.view.selected.clone();
                if self
                    .view
                    .devices
                    .iter()
                    .any(|d| d.id == target && d.online && d.can_play)
                {
                    if let wedex_cloud::Action::Volume { value } = &action {
                        self.pending_volume = Some((target.clone(), *value, Instant::now()));
                        if let Some(d) = self.view.devices.iter_mut().find(|d| d.id == target) {
                            d.playback.volume = *value;
                        }
                    }
                    self.send(ClientMessage::Command {
                        command: RemoteCommand {
                            id: uuid::Uuid::new_v4().to_string(),
                            target,
                            action,
                        },
                    })
                } else {
                    Err(anyhow::anyhow!("Выбранное устройство недоступно"))
                }
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.error(error);
        }
    }
    pub fn connection_values(&mut self, values: std::collections::BTreeMap<String, String>) {
        self.sync.values = values;
    }
    pub fn connection_changes(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.sync.changes)
    }
    pub fn connection_edited(&mut self, name: &str, value: &str) {
        if let Some(connection) = &self.connection {
            if let Some(key) = &connection.sync_key {
                if let Err(e) =
                    self.sync
                        .edited(key, &connection.credentials.device_id, name, value)
                {
                    self.error(e);
                } else {
                    self.persist_vault();
                }
            }
        }
    }
    fn persist_vault(&mut self) {
        if let Some(connection) = &self.connection {
            let result = (|| -> anyhow::Result<()> {
                let path = vault_path(connection);
                std::fs::create_dir_all(store::data_dir())?;
                let temp = path.with_extension("tmp");
                std::fs::write(&temp, serde_json::to_vec(&self.sync)?)?;
                std::fs::rename(temp, path)?;
                Ok(())
            })();
            if let Err(e) = result {
                self.error(e);
            }
        }
    }
    fn remote_owner(&self) -> Option<wedex_cloud::admin::Owner> {
        serde_json::from_str::<wedex_cloud::admin::Owner>(&store::secret("cloud-owner"))
            .ok()
            .filter(|owner| {
                owner.endpoint == self.view.endpoint
                    && self
                        .connection
                        .as_ref()
                        .is_some_and(|c| c.certificate == owner.certificate)
            })
    }
    #[cfg(windows)]
    fn poll_provision(&mut self) {
        use wedex_provision::Event;
        let mut events = vec![];
        if let Some(worker) = &mut self.provision {
            while let Ok(event) = worker.events.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                Event::Step(step) => self.view.vps.steps.push(step),
                Event::HostKey {
                    fingerprint,
                    previous,
                } => {
                    self.view.vps.fingerprint = Some(fingerprint);
                    self.view.vps.previous = previous;
                }
                Event::Trusted {
                    pin_name,
                    fingerprint,
                } => {
                    if let Err(error) = store::save_secret(&pin_name, &fingerprint) {
                        self.error(error);
                    }
                }
                Event::Bootstrap(owner) => {
                    if let Err(error) = serde_json::to_string(&owner)
                        .map_err(anyhow::Error::from)
                        .and_then(|value| store::save_secret("cloud-owner", &value))
                    {
                        self.error(error);
                    }
                }
                Event::Complete(connection) => {
                    self.view.busy = false;
                    // Switching to a VPS never destroys the previous local server's data.
                    if let Some(server) = self.server.take() {
                        std::thread::spawn(move || drop(server));
                    }
                    self.view.server_running = false;
                    self.view.lan_endpoint = None;
                    self.view.invitation.clear();
                    self.view.browser_url.clear();
                    self.view.vps.busy = false;
                    self.view.vps.complete = true;
                    self.view
                        .vps
                        .steps
                        .push("Готово · ПК подключён к VPS".into());
                    self.provision = None;
                    if let Err(error) = self.attach(connection, true) {
                        self.error(error);
                    }
                }
                Event::Failed(error) => {
                    self.view.busy = false;
                    self.view.vps.busy = false;
                    self.view.vps.fingerprint = None;
                    self.view.vps.error = Some(error.clone());
                    self.provision = None;
                    self.error(anyhow::anyhow!(error));
                }
            }
        }
    }
    fn send(&self, message: ClientMessage) -> anyhow::Result<()> {
        self.client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Облако не подключено"))?
            .send(message)
    }
    pub fn acknowledge(&mut self, delivery: &Delivery, error: Option<String>) {
        let _ = self.send(ClientMessage::Acknowledge {
            id: delivery.command.id.clone(),
            error,
        });
    }
    pub fn discontinuity(&mut self) {
        self.listening.discontinuity();
    }
    pub fn flush(&mut self, store: Option<&Store>) {
        if let Err(error) = self.listening.flush(store) {
            self.error(error);
        }
    }
    pub fn tick(&mut self, local: LocalPlayback<'_>, store: Option<&Store>) -> Vec<Delivery> {
        if self.view.server_updating && self.update_started.elapsed() > Duration::from_secs(1200) {
            self.view.server_updating = false;
            self.error(anyhow::anyhow!(
                "Истекло время ожидания обновления VPS; проверьте подключение к серверу"
            ));
        }
        if self.view.server_updating && self.last_update.elapsed() > Duration::from_secs(3) {
            if let Some(owner) = self.remote_owner() {
                let sender = self.sender.clone();
                let generation = self.generation;
                std::thread::spawn(move || {
                    let _ = sender.send(Job::ServerUpdateStatus(
                        generation,
                        owner.blocking_update_status(),
                    ));
                });
            }
            self.last_update = Instant::now();
        }
        #[cfg(windows)]
        self.poll_provision();
        let LocalPlayback {
            status,
            queue,
            cursor,
            settings,
            shuffle,
            favorites,
        } = local;
        let now = Instant::now();
        if let Err(error) = self.listening.tick(status, store, now) {
            self.error(error);
        }
        for event in self.receiver.try_iter().collect::<Vec<_>>() {
            match event {
                Job::ServerUpdateStatus(generation, Ok(value)) if generation == self.generation => {
                    let state = value["state"].as_str().unwrap_or("");
                    if state == "done" {
                        self.view.server_updating = false;
                        self.view.status = "VPS обновлена".into();
                        self.view.server_version =
                            value["version"].as_str().unwrap_or_default().into();
                    } else if state == "error" {
                        self.view.server_updating = false;
                        self.error(anyhow::anyhow!(
                            "{}",
                            value["message"].as_str().unwrap_or("Ошибка обновления VPS")
                        ));
                    }
                }
                Job::Version(endpoint, Ok(version))
                    if endpoint == self.view.endpoint && self.view.server_version.is_empty() =>
                {
                    self.view.server_version = version
                }
                #[cfg(windows)]
                Job::ServerUpdate(generation, result) if generation == self.generation => {
                    match result {
                        Ok(value) if value["state"] == "up_to_date" => {
                            self.view.server_updating = false;
                            self.view.status = "На VPS установлена последняя версия".into();
                        }
                        Ok(_) => self.view.status = "VPS обновляется · ожидаем перезапуск".into(),
                        Err(e) => {
                            self.view.server_updating = false;
                            self.error(e)
                        }
                    }
                }
                Job::Revoked(generation, Err(error)) if generation == self.generation => {
                    self.error(error)
                }
                Job::Invited(generation, browser, result) if generation == self.generation => {
                    match result {
                        Ok(invite) => {
                            let mut invite = invite;
                            invite.sync_key =
                                self.connection.as_ref().and_then(|c| c.sync_key.clone());
                            if browser {
                                self.view.browser_url =
                                    format!("{}#pair={}", invite.endpoint, invite.code);
                                self.view.invitation.clear();
                            } else {
                                self.view.invitation = invite.encode();
                                self.view.browser_url.clear();
                            }
                        }
                        Err(error) => self.error(error),
                    }
                }
                Job::Started(generation, result) if generation == self.generation => {
                    self.view.busy = false;
                    match result {
                        Ok((server, connection)) => {
                            self.view.server_running = true;
                            self.view.lan_endpoint = server.lan_endpoint.clone();
                            self.server = Some(server);
                            if let Err(error) = self.attach(connection, true) {
                                self.error(error);
                            }
                        }
                        Err(error) => self.error(error),
                    }
                }
                Job::Paired(generation, result) if generation == self.generation => {
                    self.view.busy = false;
                    match result {
                        Ok(connection) => {
                            if let Err(error) = self.attach(connection, true) {
                                self.error(error);
                            }
                        }
                        Err(error) => self.error(error),
                    }
                }
                _ => {}
            }
        }
        let mut deliveries = vec![];
        let events = self
            .client
            .as_ref()
            .map(|c| c.events.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in events {
            match event {
                client::Event::Connected => {
                    if let Some(owner) = self.remote_owner() {
                        let sender = self.sender.clone();
                        let endpoint = owner.endpoint.clone();
                        std::thread::spawn(move || {
                            let _ = sender.send(Job::Version(endpoint, owner.blocking_version()));
                        });
                    }
                    self.sync.ready = false;
                    self.last_vault = now - Duration::from_secs(31);
                    self.view.connected = true;
                    self.view.status = "Облако подключено".into();
                    self.last_sync = now - Duration::from_secs(6);
                    self.last_publish = now - Duration::from_secs(2);
                }
                client::Event::Disconnected => {
                    self.view.connected = false;
                    self.view.status = "Сервер недоступен · переподключение…".into();
                    for device in &mut self.view.devices {
                        device.online = false;
                    }
                }
                client::Event::Message(ServerMessage::Welcome {
                    version,
                    capabilities,
                    ..
                }) => {
                    self.view.sync_status =
                        if !capabilities.iter().any(|s| s == "connections-vault") {
                            "Для синхронизации подключений нужно обновить сервер WeDex"
                        } else if self
                            .connection
                            .as_ref()
                            .is_some_and(|c| c.sync_key.is_some())
                        {
                            "Подключения синхронизируются между устройствами"
                        } else {
                            "Для синхронизации подключений нужно новое приглашение WeDex"
                        }
                        .into();
                    self.view.server_version = version;
                    self.view.capabilities = capabilities;
                }
                client::Event::Message(ServerMessage::VaultSnapshot { entries }) => {
                    if let Some(connection) = &self.connection {
                        if let Some(key) = &connection.sync_key {
                            if let Err(e) =
                                self.sync
                                    .merge(key, &connection.credentials.device_id, entries)
                            {
                                self.error(e);
                            } else {
                                self.persist_vault();
                            }
                        }
                    }
                }
                client::Event::Message(ServerMessage::Snapshot {
                    mut devices,
                    statistics,
                }) => {
                    if let Some((id, value, at)) = &self.pending_volume {
                        if let Some(d) = devices.iter_mut().find(|d| &d.id == id) {
                            if at.elapsed() < Duration::from_secs(2)
                                && (d.playback.volume - value).abs() > 0.005
                            {
                                d.playback.volume = *value;
                            } else {
                                self.pending_volume = None;
                            }
                        }
                    }
                    if crate::platform::mobile() && !self.explicit_target && !status.playing {
                        self.view.selected = devices
                            .iter()
                            .filter(|d| d.can_play && d.online && d.id != self.view.own_device)
                            .max_by_key(|d| (d.playback.playing, d.playback.track.is_some()))
                            .map(|d| d.id.clone())
                            .unwrap_or_else(|| self.view.own_device.clone());
                    } else if self.view.selected.is_empty() {
                        self.view.selected = devices
                            .iter()
                            .find(|d| d.can_play && d.online)
                            .map(|d| d.id.clone())
                            .unwrap_or_default();
                    }
                    self.view.devices = devices;
                    if let Some(d) = self
                        .view
                        .devices
                        .iter()
                        .find(|d| d.id == self.view.selected)
                    {
                        if self
                            .view
                            .observed
                            .as_ref()
                            .is_none_or(|(id, stamp, _)| id != &d.id || *stamp != d.updated_ms)
                        {
                            self.view.observed = Some((d.id.clone(), d.updated_ms, now));
                        }
                    }
                    self.view.statistics = statistics;
                }
                client::Event::Message(ServerMessage::Execute { delivery }) => {
                    if delivery.expires_ms < wedex_cloud::now_ms() {
                        self.acknowledge(&delivery, Some("expired".into()));
                    } else if self.handled.contains(&delivery.command.id) {
                        self.acknowledge(&delivery, None);
                    } else if delivery.command.target == self.view.own_device {
                        self.handled.push_back(delivery.command.id.clone());
                        while self.handled.len() > 256 {
                            self.handled.pop_front();
                        }
                        deliveries.push(delivery);
                    }
                }
                client::Event::Message(ServerMessage::CommandStatus { status, .. })
                    if status == "rejected" || status == "offline" =>
                {
                    self.pending_volume = None;
                    self.error(anyhow::anyhow!("Удалённая команда: {status}"));
                }
                client::Event::Message(ServerMessage::SessionsStored { sessions }) => {
                    if let (Some(store), Some(connection)) = (store, &self.connection) {
                        if let Err(error) =
                            store.acknowledge_sessions(&connection.server_id(), &sessions)
                        {
                            self.error(error);
                        }
                    }
                }
                client::Event::Message(ServerMessage::Error { message }) => {
                    self.error(anyhow::anyhow!(message))
                }
                _ => {}
            }
        }
        if self.client.as_ref().is_some_and(Client::connected) {
            if self
                .view
                .capabilities
                .iter()
                .any(|s| s == "connections-vault")
                && self
                    .connection
                    .as_ref()
                    .is_some_and(|c| c.sync_key.is_some())
                && self.last_vault.elapsed()
                    > Duration::from_secs(if self.sync.pending().is_empty() && self.sync.ready {
                        30
                    } else {
                        1
                    })
            {
                let pending = self.sync.pending();
                let message = if self.sync.ready && !pending.is_empty() {
                    ClientMessage::VaultSet { entries: pending }
                } else {
                    ClientMessage::VaultGet
                };
                if let Err(e) = self.send(message) {
                    self.error(e);
                }
                self.last_vault = now;
            }
            if now.duration_since(self.last_publish) >= Duration::from_millis(750) {
                let published_queue = queue.iter().take(500).map(track).collect::<Vec<_>>();
                let catalog = |t: &Track| {
                    let mut published = track(t);
                    published.liked = favorites.iter().any(|f| f.key() == t.key());
                    published
                };
                let published_queue = published_queue
                    .into_iter()
                    .map(|mut t: wedex_cloud::Track| {
                        t.liked = favorites.iter().any(|f| f.key() == t.key);
                        t
                    })
                    .collect();
                let playback = Playback {
                    track: status.track.as_ref().map(catalog),
                    queue: published_queue,
                    cursor: cursor.filter(|i| *i < queue.len().min(500)),
                    playing: status.playing,
                    loading: status.loading,
                    position: status.position.max(0.) as f64,
                    duration: status.duration.max(0.) as f64,
                    volume: settings.volume,
                    repeat: settings.repeat_current,
                    shuffle,
                    eq: settings.eq,
                    preamp: settings.eq_preamp,
                };
                let _ = self.send(ClientMessage::Publish { playback });
                self.last_publish = now;
            }
            if now.duration_since(self.last_sync) >= Duration::from_secs(5) {
                if let (Some(store), Some(connection)) = (store, &self.connection) {
                    match store.pending_sessions(&connection.server_id()) {
                        Ok(sessions) if !sessions.is_empty() => {
                            let _ = self.send(ClientMessage::Sessions { sessions });
                        }
                        Err(error) => self.error(error),
                        _ => {}
                    }
                }
                self.last_sync = now;
                if let Some(store) = store {
                    self.view.local_ms = store.listened_ms().unwrap_or(0);
                }
            }
        } else if now.duration_since(self.last_sync) >= Duration::from_secs(5) {
            if let Some(store) = store {
                self.view.local_ms = store.listened_ms().unwrap_or(0);
            }
            self.last_sync = now;
        }
        deliveries
    }
}
fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .unwrap_or_else(|_| "WeDex на ПК".into())
        .chars()
        .take(60)
        .collect()
}
fn vault_path(connection: &Connection) -> std::path::PathBuf {
    store::data_dir().join(format!(
        "connections-{}-{:x}.json",
        connection.server_id(),
        md5::compute(connection.sync_key.as_deref().unwrap_or_default())
    ))
}
fn tokio_invite(owner: wedex_cloud::admin::Owner) -> anyhow::Result<Invitation> {
    owner.blocking_invitation()
}

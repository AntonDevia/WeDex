mod connections_sync;
mod hitmo;
mod library_sync;
mod listening;
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
    *,
};

#[derive(Clone)]
pub enum Action {
    Discover(String),
    TrustServer,
    Account {
        username: String,
        password: String,
        name: String,
        register: bool,
    },
    Connect {
        invitation: String,
        name: String,
    },
    Disconnect,
    Invite,
    Revoke(String),
    Target(String),
    Remote(wedex_cloud::Action),
}
#[derive(Clone, Default)]
pub struct View {
    pub library_enabled: bool,
    pub library_pending: bool,
    pub library_syncing: bool,
    pub transport_pending: bool,
    pub sync_status: String,
    pub server_version: String,
    pub capabilities: Vec<String>,
    pub server_identity: Option<client::accounts::ServerIdentity>,
    pub server_trusted: bool,
    pub account_name: String,
    pub busy: bool,
    pub connected: bool,
    pub status: String,
    pub endpoint: String,
    pub invitation: String,
    pub devices: Vec<Device>,
    pub statistics: Statistics,
    pub local_ms: u64,
    pub selected: String,
    pub own_device: String,
    pub observed: Option<(String, u64, Instant)>,
}
enum Job {
    Discovered(u64, anyhow::Result<client::accounts::ServerIdentity>),

    Revoked(u64, anyhow::Result<()>),
    Invited(u64, anyhow::Result<Invitation>),
    Paired(u64, anyhow::Result<Connection>),
}
pub struct Cloud {
    profile_reload: bool,
    library: library_sync::Sync,
    transport: Option<Transport>,
    hitmo: hitmo::Discovery,
    sync: connections_sync::Sync,
    last_vault: Instant,
    pub view: View,
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
struct Transport {
    id: String,
    target: String,
    at: Instant,
    updated: u64,
    done: bool,
    playing: Option<bool>,
    seek: Option<f64>,
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
    pub fn position(&self, p: &Playback) -> f64 {
        let Some(device) = self.remote_device() else {
            return p.position;
        };
        if self.connected && device.online && p.playing && !p.loading {
            if let Some((id, _, at)) = &self.observed {
                if id == &device.id {
                    return (p.position + at.elapsed().as_secs_f64().min(2.)).min(p.duration);
                }
            }
        }
        p.position
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
    pub fn catalog_connection(&self) -> Option<&Connection> {
        self.connection.as_ref().filter(|_| {
            self.view.connected && self.view.capabilities.iter().any(|c| c == "accounts-v1")
        })
    }
    pub fn take_profile_reload(&mut self) -> bool {
        std::mem::take(&mut self.profile_reload)
    }
    #[cfg(test)]
    pub(crate) fn connected_for_test(connection: Connection) -> Self {
        let mut cloud = Self::new(true);
        cloud.attach(connection, false).unwrap();
        cloud
    }
    pub fn new(preview: bool) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut cloud = Self {
            profile_reload: false,
            library: Default::default(),
            transport: None,
            sync: Default::default(),
            hitmo: Default::default(),
            last_vault: Instant::now(),
            view: View {
                status: "Облако не подключено".into(),
                ..Default::default()
            },
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
    fn attach(&mut self, connection: Connection, save: bool) -> anyhow::Result<()> {
        #[cfg(not(test))]
        if save {
            self.profile_reload = store::activate_account(&connection)?;
        }
        self.library = Default::default();
        self.transport = None;
        self.view.transport_pending = false;
        self.hitmo = Default::default();
        self.view.account_name = connection
            .credentials
            .account
            .as_ref()
            .map(|a| a.username.clone())
            .unwrap_or_default();
        self.view.server_identity = Some(client::accounts::ServerIdentity {
            endpoint: connection.endpoint.clone(),
            certificate: connection.certificate.clone(),
        });
        self.view.server_trusted = true;
        self.sync = std::fs::read_to_string(vault_path(&connection))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        self.view.server_version.clear();
        self.view.capabilities.clear();
        self.view.sync_status.clear();
        self.client = Some(Client::spawn(connection.clone())?);
        self.view.own_device = connection.credentials.device_id.clone();
        self.view.endpoint = connection.endpoint.clone();
        self.view.devices.clear();
        self.view.statistics = Statistics::default();
        self.view.selected.clear();
        self.explicit_target = false;
        self.view.observed = None;
        self.view.connected = false;
        self.view.status = "Подключаемся к облаку…".into();
        self.connection = Some(connection.clone());
        if save {
            let value = serde_json::to_string(&connection)?;
            if let Some(account) = &connection.credentials.account {
                store::save_secret(
                    &device_profile(&connection.endpoint, &account.username),
                    &value,
                )?;
            }
            store::save_secret("cloud", &value)?;
        }
        Ok(())
    }
    pub fn action(&mut self, action: Action) {
        let result = match action {
            Action::Discover(address) if !self.view.busy => {
                self.generation += 1;
                self.view.busy = true;
                self.view.server_trusted = false;
                self.view.server_identity = None;
                self.view.status = "Проверяем сервер…".into();
                let sender = self.sender.clone();
                let generation = self.generation;
                std::thread::spawn(move || {
                    let _ = sender.send(Job::Discovered(
                        generation,
                        client::accounts::discover(&address),
                    ));
                });
                Ok(())
            }
            Action::TrustServer => {
                if let Some(identity) = &self.view.server_identity {
                    match store::save_secret(
                        &format!("cloud-pin:{}", identity.endpoint),
                        &identity.certificate,
                    ) {
                        Ok(()) => {
                            self.view.server_trusted = true;
                            self.view.status = "Сервер подтверждён".into();
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(())
                }
            }
            Action::Account {
                username,
                password,
                name,
                register,
            } if !self.view.busy => {
                if !self.view.server_trusted {
                    return;
                }
                if let Some(server) = self.view.server_identity.clone() {
                    self.view.busy = true;
                    self.generation += 1;
                    let generation = self.generation;
                    let sender = self.sender.clone();
                    let previous = self
                        .connection
                        .clone()
                        .filter(|c| {
                            c.credentials
                                .account
                                .as_ref()
                                .is_none_or(|a| a.username.eq_ignore_ascii_case(&username))
                        })
                        .or_else(|| {
                            serde_json::from_str::<Connection>(&store::secret(&device_profile(
                                &server.endpoint,
                                &username,
                            )))
                            .ok()
                        });
                    let request = wedex_cloud::accounts::Login {
                        username,
                        password: password.into(),
                        name,
                        sync_key: previous
                            .as_ref()
                            .and_then(|c| c.sync_key.clone())
                            .map(Into::into),
                    };
                    std::thread::spawn(move || {
                        let _ = sender.send(Job::Paired(
                            generation,
                            client::accounts::blocking_account(server, request, register, previous),
                        ));
                    });
                }
                Ok(())
            }
            Action::Connect { invitation, name } if !self.view.busy => {
                match Invitation::decode(&invitation) {
                    Ok(invitation) => {
                        if self.view.connected && invitation.sync_key.is_some() {
                            if let Some(mut existing) = self.connection.clone().filter(|c| {
                                c.endpoint == invitation.endpoint
                                    && client::accounts::same_certificate(
                                        &c.certificate,
                                        &invitation.certificate,
                                    )
                                    && c.sync_key == invitation.sync_key
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
                self.hitmo = Default::default();
                self.generation += 1;
                self.view.busy = false;
                self.client = None;
                self.connection = None;
                self.view.connected = false;
                self.view.devices.clear();
                self.view.status = "Облако отключено".into();
                store::remember_profile().and_then(|_| store::save_secret("cloud", ""))
            }
            Action::Invite => {
                if let Some(connection) = self.connection.clone() {
                    let sender = self.sender.clone();
                    let generation = self.generation;
                    std::thread::spawn(move || {
                        let _ = sender.send(Job::Invited(
                            generation,
                            client::accounts::blocking_invite(&connection),
                        ));
                    });
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Сначала войдите в аккаунт"))
                }
            }
            Action::Revoke(id) => {
                if let Some(connection) = self.connection.clone() {
                    let sender = self.sender.clone();
                    let generation = self.generation;
                    std::thread::spawn(move || {
                        let _ = sender.send(Job::Revoked(
                            generation,
                            client::accounts::blocking_revoke(&connection, &id),
                        ));
                    });
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Сначала войдите в аккаунт"))
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
                    let id = uuid::Uuid::new_v4().to_string();
                    let transport = matches!(
                        &action,
                        wedex_cloud::Action::Next
                            | wedex_cloud::Action::Previous
                            | wedex_cloud::Action::Seek { .. }
                            | wedex_cloud::Action::SetPlaying { .. }
                            | wedex_cloud::Action::QueueIndex { .. }
                            | wedex_cloud::Action::PlayTrack { .. }
                            | wedex_cloud::Action::PlayCollection { .. }
                    );
                    let playing = if let wedex_cloud::Action::SetPlaying { playing } = &action {
                        Some(*playing)
                    } else {
                        None
                    };
                    let seek = if let wedex_cloud::Action::Seek { seconds } = &action {
                        Some(*seconds)
                    } else {
                        None
                    };
                    let pending = Transport {
                        id: id.clone(),
                        target: target.clone(),
                        at: Instant::now(),
                        updated: self
                            .view
                            .devices
                            .iter()
                            .find(|d| d.id == target)
                            .map(|d| d.updated_ms)
                            .unwrap_or(0),
                        done: false,
                        playing,
                        seek,
                    };
                    let result = self.send(ClientMessage::Command {
                        command: RemoteCommand { id, target, action },
                    });
                    if result.is_ok() && transport {
                        if let Some(d) = self
                            .view
                            .devices
                            .iter_mut()
                            .find(|d| d.id == pending.target)
                        {
                            if let Some(playing) = pending.playing {
                                d.playback.playing = playing;
                            }
                            if let Some(position) = pending.seek {
                                d.playback.position = position;
                            }
                        }
                        self.transport = Some(pending);
                        self.view.transport_pending = true;
                    }
                    result
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
        if self.sync.changes.iter().any(|(name, _)| name == "hitmo") {
            // A synchronized user edit takes priority over an in-flight probe.
            self.hitmo = Default::default();
            return std::mem::take(&mut self.sync.changes);
        }
        if let Some(origin) = self.hitmo.take(
            self.sync
                .values
                .get("hitmo")
                .map(String::as_str)
                .unwrap_or(""),
        ) {
            self.connection_edited("hitmo", &origin);
            self.sync.changes.push(("hitmo".into(), origin));
        }
        std::mem::take(&mut self.sync.changes)
    }
    pub fn connection_edited(&mut self, name: &str, value: &str) {
        if name == "hitmo" {
            self.hitmo = Default::default();
        }
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
        if self
            .transport
            .as_ref()
            .is_some_and(|p| p.at.elapsed() > Duration::from_secs(8))
        {
            self.transport = None;
            self.view.transport_pending = false;
        }
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
                Job::Discovered(generation, result) if generation == self.generation => {
                    self.view.busy = false;
                    match result {
                        Ok(identity) => {
                            self.view.server_trusted = client::accounts::same_certificate(
                                &store::secret(&format!("cloud-pin:{}", identity.endpoint)),
                                &identity.certificate,
                            );
                            self.view.server_identity = Some(identity);
                            self.view.status = if self.view.server_trusted {
                                "Сервер подтверждён"
                            } else {
                                "Сверьте отпечаток с владельцем сервера"
                            }
                            .into();
                        }
                        Err(e) => self.error(e),
                    }
                }
                Job::Revoked(generation, Err(error)) if generation == self.generation => {
                    self.error(error)
                }
                Job::Invited(generation, result) if generation == self.generation => match result {
                    Ok(invite) => {
                        let mut invite = invite;
                        invite.sync_key = self.connection.as_ref().and_then(|c| c.sync_key.clone());
                        self.view.invitation = invite.encode();
                    }
                    Err(error) => self.error(error),
                },
                Job::Paired(generation, result) if generation == self.generation => {
                    self.view.busy = false;
                    match result {
                        Ok(connection) => {
                            if let Err(error) = self.listening.flush(store) {
                                self.error(error);
                                continue;
                            }
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
        if self.profile_reload {
            return vec![];
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
                    self.library.inflight = None;
                    self.library.dirty = true;
                    self.sync.ready = false;
                    self.last_vault = now - Duration::from_secs(31);
                    self.view.connected = true;
                    self.view.status = "Облако подключено".into();
                    self.last_sync = now - Duration::from_secs(6);
                    self.last_publish = now - Duration::from_secs(2);
                }
                client::Event::Disconnected => {
                    self.library.inflight = None;
                    self.view.library_syncing = false;
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
                client::Event::Message(ServerMessage::LibraryChanged { .. }) => {
                    self.library.dirty = true;
                }
                client::Event::Message(ServerMessage::LibraryDelta { delta }) => {
                    if let Some(store) = store {
                        if let Err(e) = self.library_delta(store, &delta) {
                            self.error(e);
                        }
                    }
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
                    hitmo_mirror,
                }) => {
                    if let Some(mirror) = hitmo_mirror {
                        self.hitmo.offer(
                            mirror,
                            self.sync
                                .values
                                .get("hitmo")
                                .map(String::as_str)
                                .unwrap_or(""),
                        );
                    }
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
                    if let Some(pending) = &self.transport {
                        if let Some(d) = self
                            .view
                            .devices
                            .iter_mut()
                            .find(|d| d.id == pending.target)
                        {
                            if pending.done && d.updated_ms > pending.updated {
                                self.transport = None;
                                self.view.transport_pending = false;
                            } else {
                                if let Some(playing) = pending.playing {
                                    d.playback.playing = playing;
                                }
                                if let Some(position) = pending.seek {
                                    d.playback.position = position;
                                }
                            }
                        }
                    }
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
                client::Event::Message(ServerMessage::CommandStatus { id, status })
                    if status == "done" =>
                {
                    if let Some(p) = self.transport.as_mut().filter(|p| p.id == id) {
                        p.done = true;
                    }
                }
                client::Event::Message(ServerMessage::CommandStatus { status, .. })
                    if status == "rejected" || status == "offline" =>
                {
                    self.pending_volume = None;
                    self.transport = None;
                    self.view.transport_pending = false;
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
        if let Some(store) = store {
            if let Err(e) = self.library_tick(store, now) {
                self.error(e);
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
                    loading: status.loading || status.buffering,
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
fn vault_path(connection: &Connection) -> std::path::PathBuf {
    store::data_dir().join(format!(
        "connections-{}-{:x}.json",
        connection.server_id(),
        md5::compute(connection.sync_key.as_deref().unwrap_or_default())
    ))
}

fn device_profile(endpoint: &str, username: &str) -> String {
    format!(
        "cloud-device:{:x}",
        md5::compute(format!("{endpoint}:{}", username.to_ascii_lowercase()))
    )
}

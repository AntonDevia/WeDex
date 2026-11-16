use super::*;
use crate::accounts::{Account, Authenticated, Login, Registry};
use axum::extract::ConnectInfo;

impl Hub {
    pub(super) fn registry(&self) -> Result<&Registry> {
        self.accounts.as_ref().context("Реестр недоступен")
    }
    fn tenant(self: &Arc<Self>, id: &str) -> Result<Arc<Self>> {
        if id == "legacy" {
            return Ok(self.clone());
        }
        anyhow::ensure!(
            uuid::Uuid::parse_str(id).is_ok() && self.registry()?.account(id)?.is_some(),
            "Аккаунт не найден"
        );
        let mut tenants = self.tenants.lock().unwrap();
        if let Some(hub) = tenants.get(id) {
            return Ok(hub.clone());
        }
        let directory = self.data_dir.join("accounts").join(id);
        std::fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        }
        let (changes, _) = broadcast::channel(16);
        let tenant = Arc::new(Hub {
            catalog: self.catalog.clone(),
            data_dir: directory.clone(),
            update_repo: self.update_repo.clone(),
            db: Database::open(&directory.join("personal.sqlite3"))?,
            // A tenant has no infrastructure-admin credential.
            owner_hash: String::new(),
            connected: Mutex::new(HashMap::new()),
            changes,
            hitmo_mirror: self.hitmo_mirror.clone(),
            accounts: None,
            tenants: Mutex::new(HashMap::new()),
            auth_slots: self.auth_slots.clone(),
            auth_rate: Mutex::new(HashMap::new()),
        });
        tenants.insert(id.into(), tenant.clone());
        Ok(tenant)
    }
    pub(super) fn for_token(self: &Arc<Self>, token: &str) -> Result<Arc<Self>> {
        anyhow::ensure!(token.len() <= 128, "Недействительный токен");
        let tenant = match self.registry()?.route_token(token)? {
            Some(id) => self.tenant(&id)?,
            None => self.clone(),
        };
        anyhow::ensure!(tenant.db.authenticate(token)?.is_some(), "Требуется вход");
        Ok(tenant)
    }
    pub(super) fn for_invite(self: &Arc<Self>, code: &str) -> Result<Arc<Self>> {
        anyhow::ensure!(code.len() <= 128, "Недействительное приглашение");
        match self.registry()?.route_invite(code)? {
            Some(id) => self.tenant(&id),
            None => Ok(self.clone()),
        }
    }
    pub(super) fn authorize(
        self: &Arc<Self>,
        headers: &HeaderMap,
    ) -> Result<(Arc<Self>, Device, Option<Account>)> {
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .context("Требуется вход")?;
        let tenant = self.for_token(token)?;
        let device = tenant.db.authenticate(token)?.context("Требуется вход")?;
        let id = self
            .registry()?
            .route_token(token)?
            .unwrap_or_else(|| "legacy".into());
        Ok((tenant, device, self.registry()?.account(&id)?))
    }
}
pub(super) fn rate(hub: &Hub, ip: IpAddr) -> bool {
    let minute = now_ms() / 60_000;
    let mut rates = hub.auth_rate.lock().unwrap();
    rates.retain(|_, v| v.0 == minute);
    if rates.len() >= 4096 && !rates.contains_key(&ip) {
        return false;
    }
    let attempts = rates.entry(ip).or_insert((minute, 0));
    attempts.1 += 1;
    attempts.1 <= 10
}
pub(super) async fn register(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<Login>,
) -> ApiResult<Authenticated> {
    authenticate(hub, peer.ip(), headers, request, true).await
}
pub(super) async fn login(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<Login>,
) -> ApiResult<Authenticated> {
    authenticate(hub, peer.ip(), headers, request, false).await
}
async fn authenticate(
    hub: Arc<Hub>,
    ip: IpAddr,
    headers: HeaderMap,
    request: Login,
    register: bool,
) -> ApiResult<Authenticated> {
    if !rate(&hub, ip) {
        return Err(failure(
            StatusCode::TOO_MANY_REQUESTS,
            "Слишком много попыток. Повторите через минуту",
        ));
    }
    if request.validate().is_err() {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "Проверьте логин, пароль (минимум 10 символов) и имя устройства",
        ));
    }
    let permit = hub.auth_slots.clone().try_acquire_owned().map_err(|_| {
        failure(
            StatusCode::TOO_MANY_REQUESTS,
            "Сервер занят. Повторите вход",
        )
    })?;
    // Preserve the old owner's data only on proof of an existing legacy-native device token.
    let legacy = register
        && hub
            .authorize(&headers)
            .is_ok_and(|(h, d, account)| Arc::ptr_eq(&h, &hub) && d.can_play && account.is_none());
    let result = tokio::task::spawn_blocking(move || -> Result<Authenticated> {
        let _permit = permit;
        let (account, sync_key) = if register {
            hub.registry()?.register(&request, legacy)?
        } else {
            hub.registry()?.login(&request)?
        };
        let tenant = hub.tenant(&account.id)?;
        let previous = headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));
        let mut credentials = tenant.db.login_device(previous, &request.name)?;
        hub.registry()?.device(&credentials.token, &account.id)?;
        credentials.account = Some(account);
        let _ = tenant.changes.send(());
        Ok(Authenticated {
            credentials,
            sync_key,
        })
    })
    .await;
    match result {
        Ok(Ok(value)) => Ok(Json(value)),
        _ => Err(failure(
            StatusCode::UNAUTHORIZED,
            if register {
                "Не удалось создать аккаунт: логин занят или перенос уже выполнен"
            } else {
                "Неверный логин или пароль"
            },
        )),
    }
}

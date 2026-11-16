//! Shared public Hitmo catalogue; no account keys, playback history or audio files enter this cache.
use super::*;
use axum::extract::Query;
#[derive(Clone, Serialize, Deserialize)]
pub struct Page {
    pub base: String,
    pub html: String,
}
#[derive(Deserialize)]
pub(super) struct Search {
    q: String,
}
pub(super) struct Catalog {
    http: reqwest::Client,
    cache: Mutex<HashMap<String, (std::time::Instant, Page)>>,
    slots: tokio::sync::Semaphore,
}
impl Catalog {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .connect_timeout(Duration::from_secs(5))
                .user_agent("WeDex public catalogue/0.12")
                .build()?,
            cache: Mutex::new(HashMap::new()),
            slots: tokio::sync::Semaphore::new(8),
        })
    }
}
pub(super) async fn search(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
    Query(query): Query<Search>,
) -> ApiResult<Page> {
    hub.authorize(&headers)
        .map_err(|_| failure(StatusCode::UNAUTHORIZED, "Требуется вход"))?;
    let q = query.q.trim();
    if q.is_empty() || q.chars().count() > 160 {
        return Err(failure(StatusCode::BAD_REQUEST, "Некорректный запрос"));
    }
    let origin = hub
        .hitmo_mirror
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.origin.clone())
        .unwrap_or_else(|| "https://ru.hitmox.net/".into());
    let key = format!("{origin}:{q}");
    {
        let cache = hub.catalog.cache.lock().unwrap();
        if let Some((at, page)) = cache
            .get(&key)
            .filter(|(at, _)| at.elapsed() < Duration::from_secs(90))
        {
            let _ = at;
            return Ok(Json(page.clone()));
        }
    }
    let _permit = hub
        .catalog
        .slots
        .try_acquire()
        .map_err(|_| failure(StatusCode::TOO_MANY_REQUESTS, "Каталог занят"))?;
    let fetch = async {
        anyhow::ensure!(crate::mirror::valid_origin(&origin), "Некорректное зеркало");
        let mut response = hub
            .catalog
            .http
            .get(format!("{origin}search"))
            .query(&[("q", q)])
            .send()
            .await?
            .error_for_status()?;
        let mut bytes = Vec::new();
        while let Some(part) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len() + part.len() <= 1024 * 1024,
                "Страница слишком большая"
            );
            bytes.extend_from_slice(&part);
        }
        Ok::<_, anyhow::Error>(Page {
            base: response.url().to_string(),
            html: String::from_utf8(bytes)?,
        })
    }
    .await;
    let page =
        fetch.map_err(|_| failure(StatusCode::BAD_GATEWAY, "Каталог временно недоступен"))?;
    let mut cache = hub.catalog.cache.lock().unwrap();
    cache.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(90));
    if cache.len() >= 32 {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, v)| v.0)
            .map(|(k, _)| k.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(key, (std::time::Instant::now(), page.clone()));
    Ok(Json(page))
}

use std::time::Duration;
use wedex_cloud::{
    client::{self, Client, Event},
    library::{Delta, Operation, Track},
    server::{self, Config},
    *,
};
async fn wait(c: &Client, accept: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < until {
        while let Ok(event) = c.events.try_recv() {
            if let Event::Message(m) = event {
                if accept(&m) {
                    return m;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Expected library message did not arrive")
}
fn operation(id: &str, liked: bool, base: u64) -> Operation {
    Operation {
        id: uuid::Uuid::new_v4().to_string(),
        track: Track {
            id: id.into(),
            provider: "Hitmotop".into(),
            title: "Synthetic song".into(),
            artist: "Synthetic artist".into(),
            duration: 180,
            album: String::new(),
            cover: None,
            page_url: format!("https://example.test/song/{id}"),
            edition: "Unknown".into(),
        },
        liked,
        base,
        parent: None,
        seed: false,
    }
}
async fn sync(c: &Client, cursor: u64, operations: Vec<Operation>) -> Delta {
    c.send(ClientMessage::LibrarySync { cursor, operations })
        .unwrap();
    match wait(c, |m| matches!(m, ServerMessage::LibraryDelta { .. })).await {
        ServerMessage::LibraryDelta { delta } => delta,
        _ => unreachable!(),
    }
}
#[tokio::test]
async fn native_collection_sync_over_real_tls_is_durable_bounded_and_not_exposed_to_browser() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    };
    let server = server::start(config.clone()).unwrap();
    let pc = client::pair(&server.invitation(false).unwrap(), "PC", true)
        .await
        .unwrap();
    let phone = client::pair(&server.invitation(false).unwrap(), "Phone", true)
        .await
        .unwrap();
    let browser = client::pair(&server.invitation(false).unwrap(), "Browser", false)
        .await
        .unwrap();
    let p = Client::spawn(pc.clone()).unwrap();
    let q = Client::spawn(phone.clone()).unwrap();
    let b = Client::spawn(browser).unwrap();
    for c in [&p, &q, &b] {
        wait(c, |m| matches!(m, ServerMessage::Welcome { .. })).await;
    }
    let added = sync(&p, 0, vec![operation("123", true, 0)]).await;
    assert_eq!(added.entries.len(), 1);
    wait(&q, |m| matches!(m, ServerMessage::LibraryChanged { .. })).await;
    let phone_state = sync(&q, 0, vec![]).await;
    assert!(phone_state.entries[0].liked);
    assert_eq!(phone_state.entries[0].track.id, "123");
    let remove = operation("123", false, phone_state.cursor);
    let removed = sync(&q, phone_state.cursor, vec![remove.clone()]).await;
    assert!(!removed.entries[0].liked);
    assert!(
        !sync(&q, phone_state.cursor, vec![remove])
            .await
            .acknowledged[0]
            .entry
            .liked
    );
    b.send(ClientMessage::LibrarySync {
        cursor: 0,
        operations: vec![],
    })
    .unwrap();
    wait(&b, |m| matches!(m, ServerMessage::Error { .. })).await;
    assert!(!b.events.try_iter().any(|e| matches!(
        e,
        Event::Message(ServerMessage::LibraryChanged { .. } | ServerMessage::LibraryDelta { .. })
    )));
    let owner = admin::Owner {
        endpoint: server.endpoint.clone(),
        certificate: server.certificate.clone(),
        owner: server.owner_token().into(),
    };
    owner.revoke(&phone.credentials.device_id).await.unwrap();
    q.send(ClientMessage::LibrarySync {
        cursor: 0,
        operations: vec![],
    })
    .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(3);
    while q.connected() && std::time::Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!q.connected());
    drop(p);
    drop(q);
    drop(b);
    drop(server);
    tokio::time::sleep(Duration::from_millis(150)).await;
    let server = server::start(config).unwrap();
    let mut pc = pc;
    pc.endpoint = server.endpoint.clone();
    let p = Client::spawn(pc).unwrap();
    wait(&p, |m| matches!(m, ServerMessage::Welcome { .. })).await;
    let restored = sync(&p, 0, vec![]).await;
    assert_eq!(restored.entries.len(), 1);
    assert!(!restored.entries[0].liked);
    assert_eq!(restored.cursor, removed.cursor);
}

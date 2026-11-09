use std::time::Duration;
use wedex_cloud::{
    client::{self, Client, Event},
    server::{self, Config},
    *,
};
async fn wait(client: &Client, accept: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
    for _ in 0..200 {
        while let Ok(event) = client.events.try_recv() {
            if let Event::Message(message) = event {
                if accept(&message) {
                    return message;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("No expected server message")
}
#[tokio::test]
async fn native_peers_share_only_ciphertext_and_browser_cannot_read_vault() {
    let dir = tempfile::tempdir().unwrap();
    let server = server::start(Config {
        data_dir: dir.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let key = vault::new_key().unwrap();
    let mut invite = server.invitation(false).unwrap();
    invite.sync_key = Some(key.clone());
    let pc = client::pair(&invite, "PC fixture", true).await.unwrap();
    assert_eq!(pc.sync_key.as_deref(), Some(key.as_str()));
    let phone = client::pair(&server.invitation(false).unwrap(), "Phone fixture", true)
        .await
        .unwrap();
    let browser = client::pair(&server.invitation(false).unwrap(), "Browser fixture", false)
        .await
        .unwrap();
    let p = Client::spawn(pc.clone()).unwrap();
    let q = Client::spawn(phone).unwrap();
    let b = Client::spawn(browser).unwrap();
    for c in [&p, &q, &b] {
        wait(c, |m| matches!(m, ServerMessage::Welcome { .. })).await;
    }
    let entry = vault::Entry {
        name: "yandex".into(),
        clock: 1,
        writer: pc.credentials.device_id.clone(),
        encrypted: vault::seal(&key, "yandex", "fixture-token-not-for-server").unwrap(),
    };
    p.send(ClientMessage::VaultSet {
        entries: vec![entry.clone()],
    })
    .unwrap();
    let received = wait(&q, |m| matches!(m, ServerMessage::VaultSnapshot { .. })).await;
    if let ServerMessage::VaultSnapshot { entries } = received {
        assert_eq!(entries, vec![entry]);
        assert_eq!(
            vault::open(&key, "yandex", &entries[0].encrypted).unwrap(),
            "fixture-token-not-for-server"
        );
    }
    b.send(ClientMessage::VaultGet).unwrap();
    wait(&b, |m| matches!(m, ServerMessage::Error { .. })).await;
    let owner = admin::Owner {
        endpoint: server.endpoint.clone(),
        certificate: server.certificate.clone(),
        owner: server.owner_token().into(),
    };
    assert_eq!(owner.invitation().await.unwrap().sync_key, None);
    for name in ["cloud.sqlite3", "cloud.sqlite3-wal"] {
        if let Ok(bytes) = std::fs::read(dir.path().join(name)) {
            assert!(!bytes
                .windows(28)
                .any(|s| s == b"fixture-token-not-for-server"));
        }
    }
}

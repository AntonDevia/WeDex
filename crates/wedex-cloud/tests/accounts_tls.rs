use std::time::Duration;
use wedex_cloud::{
    accounts::{Authenticated, Login},
    client::{self, accounts::ServerIdentity, Client, Event},
    library::{Operation, Track as Metadata},
    server::{self, Config},
    *,
};
fn identity(server: &server::RunningServer) -> ServerIdentity {
    ServerIdentity {
        endpoint: server.endpoint.clone(),
        certificate: server.certificate.clone(),
    }
}
async fn account(server: &server::RunningServer, user: &str, register: bool) -> client::Connection {
    let response = identity(server)
        .http()
        .unwrap()
        .post(format!(
            "{}api/accounts/{}",
            server.endpoint,
            if register { "register" } else { "login" }
        ))
        .json(&Login {
            username: user.into(),
            password: "synthetic-password-123".to_string().into(),
            name: format!("{user} device"),
            sync_key: None,
        })
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, 200, "{body}");
    let value: Authenticated = serde_json::from_str(&body).unwrap();
    client::Connection {
        endpoint: server.endpoint.clone(),
        certificate: server.certificate.clone(),
        sync_key: Some(value.sync_key),
        credentials: value.credentials,
    }
}
async fn wait(client: &Client, accept: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            while let Ok(event) = client.events.try_recv() {
                if let Event::Message(m) = event {
                    if accept(&m) {
                        return m;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("server event")
}
async fn online(connection: &client::Connection) -> Client {
    let client = Client::spawn(connection.clone()).unwrap();
    wait(&client, |m| matches!(m, ServerMessage::Welcome { .. })).await;
    client
}
fn operation() -> Operation {
    Operation {
        id: uuid::Uuid::new_v4().to_string(),
        track: Metadata {
            id: "same-id".into(),
            provider: "Hitmotop".into(),
            title: "Private fixture".into(),
            artist: "A".into(),
            duration: 180,
            album: String::new(),
            cover: None,
            page_url: "https://example.test/song/1".into(),
            edition: "Unknown".into(),
        },
        liked: true,
        base: 0,
        parent: None,
        seed: false,
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn independent_accounts_isolate_library_vault_statistics_devices_commands_and_survive_restart(
) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    };
    let server = server::start(config.clone()).unwrap();
    let mut a = account(&server, "alice", true).await;
    let mut b = account(&server, "bob", true).await;
    assert_ne!(a.server_id(), b.server_id());
    assert_ne!(a.sync_key, b.sync_key);
    let ca = online(&a).await;
    let cb = online(&b).await;
    let op = operation();
    ca.send(ClientMessage::LibrarySync {
        cursor: 0,
        operations: vec![op.clone()],
    })
    .unwrap();
    let delta = wait(&ca, |m| matches!(m, ServerMessage::LibraryDelta { .. })).await;
    assert!(matches!(delta,ServerMessage::LibraryDelta{delta} if delta.acknowledged.len()==1));
    let entry = vault::Entry {
        name: "yandex".into(),
        clock: 1,
        writer: a.credentials.device_id.clone(),
        encrypted: vault::seal(
            a.sync_key.as_deref().unwrap(),
            "yandex",
            "private-fixture-token",
        )
        .unwrap(),
    };
    ca.send(ClientMessage::VaultSet {
        entries: vec![entry],
    })
    .unwrap();
    wait(
        &ca,
        |m| matches!(m,ServerMessage::VaultSnapshot{entries} if entries.len()==1),
    )
    .await;
    ca.send(ClientMessage::Sessions {
        sessions: vec![Session {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            track: Track {
                key: "Hitmotop:same-id".into(),
                title: "Private fixture".into(),
                artist: "A".into(),
                duration: 180.,
                ..Default::default()
            },
            started_ms: now_ms(),
            listened_ms: 5000,
        }],
    })
    .unwrap();
    wait(&ca, |m| matches!(m, ServerMessage::SessionsStored { .. })).await;
    ca.send(ClientMessage::Publish {
        playback: Playback {
            playing: true,
            track: Some(Track {
                key: "Hitmotop:same-id".into(),
                title: "Private fixture".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    })
    .unwrap();
    cb.send(ClientMessage::LibrarySync {
        cursor: 0,
        operations: vec![],
    })
    .unwrap();
    assert!(
        matches!(wait(&cb,|m|matches!(m,ServerMessage::LibraryDelta{..})).await,ServerMessage::LibraryDelta{delta} if delta.entries.is_empty()&&delta.cursor==0)
    );
    cb.send(ClientMessage::VaultGet).unwrap();
    assert!(
        matches!(wait(&cb,|m|matches!(m,ServerMessage::VaultSnapshot{..})).await,ServerMessage::VaultSnapshot{entries} if entries.is_empty())
    );
    assert!(
        matches!(wait(&cb,|m|matches!(m,ServerMessage::Snapshot{..})).await,ServerMessage::Snapshot{devices,statistics,..} if devices.len()==1&&devices[0].id==b.credentials.device_id&&statistics.listened_ms==0)
    );
    cb.send(ClientMessage::Command {
        command: RemoteCommand {
            id: uuid::Uuid::new_v4().to_string(),
            target: a.credentials.device_id.clone(),
            action: Action::Next,
        },
    })
    .unwrap();
    wait(&cb, |m| matches!(m, ServerMessage::Error { .. })).await;
    let http = identity(&server).http().unwrap();
    assert_eq!(
        http.post(format!(
            "{}api/devices/{}/revoke",
            server.endpoint, a.credentials.device_id
        ))
        .bearer_auth(&b.credentials.token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .status(),
        200
    );
    ca.send(ClientMessage::VaultGet).unwrap();
    wait(
        &ca,
        |m| matches!(m,ServerMessage::VaultSnapshot{entries} if entries.len()==1),
    )
    .await;
    // Each account may use the same catalogue ID and operation UUID independently.
    cb.send(ClientMessage::LibrarySync {
        cursor: 0,
        operations: vec![op],
    })
    .unwrap();
    wait(
        &cb,
        |m| matches!(m,ServerMessage::LibraryDelta{delta} if delta.acknowledged.len()==1),
    )
    .await;
    let invite: serde_json::Value = http
        .post(format!("{}api/invitations", server.endpoint))
        .bearer_auth(&a.credentials.token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let invitation = Invitation {
        endpoint: server.endpoint.clone(),
        certificate: server.certificate.clone(),
        sync_key: a.sync_key.clone(),
        code: invite["code"].as_str().unwrap().into(),
    };
    let phone = client::pair(&invitation, "Alice phone", true)
        .await
        .unwrap();
    assert_eq!(phone.server_id(), a.server_id());
    assert_eq!(phone.credentials.account, a.credentials.account);
    assert!(client::pair(&invitation, "Replay", true).await.is_err());
    let pc = online(&phone).await;
    assert!(
        matches!(wait(&pc,|m|matches!(m,ServerMessage::Snapshot{statistics,..} if statistics.listened_ms==5000)).await,ServerMessage::Snapshot{devices,..} if devices.len()==2)
    );
    for token in [&a.credentials.token, &b.credentials.token] {
        assert_eq!(
            http.get(format!("{}api/update/status", server.endpoint))
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
    drop(pc);
    drop(ca);
    drop(cb);
    drop(server);
    let server = server::start(config).unwrap();
    a.endpoint = server.endpoint.clone();
    b.endpoint = server.endpoint.clone();
    let ca = online(&a).await;
    let cb = online(&b).await;
    assert!(
        matches!(wait(&ca,|m|matches!(m,ServerMessage::Snapshot{statistics,..} if statistics.listened_ms==5000)).await,ServerMessage::Snapshot{devices,..} if devices.len()==2)
    );
    assert!(
        matches!(wait(&cb,|m|matches!(m,ServerMessage::Snapshot{..})).await,ServerMessage::Snapshot{devices,statistics,..} if devices.len()==1&&statistics.listened_ms==0)
    );
    let again = account(&server, "ALICE", false).await;
    assert_eq!(again.server_id(), a.server_id());
    assert_eq!(again.sync_key, a.sync_key);
    drop(ca);
    drop(cb);
    drop(server);
    let bytes = std::fs::read(dir.path().join("accounts.sqlite3")).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        !text.contains("synthetic-password-123") && !text.contains(a.sync_key.as_deref().unwrap())
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn legacy_data_requires_authenticated_claim_and_login_checks_password_and_case_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let server = server::start(Config {
        data_dir: dir.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let legacy = client::pair(&server.invitation(false).unwrap(), "Old PC", true)
        .await
        .unwrap();
    let old = online(&legacy).await;
    old.send(ClientMessage::LibrarySync {
        cursor: 0,
        operations: vec![operation()],
    })
    .unwrap();
    wait(&old, |m| matches!(m, ServerMessage::LibraryDelta { .. })).await;
    let outsider = account(&server, "outsider", true).await;
    assert_ne!(outsider.server_id(), legacy.server_id());
    let key = vault::new_key().unwrap();
    let request = Login {
        username: "original-owner".into(),
        password: "old-owner-passphrase".to_string().into(),
        name: "Migrated PC".into(),
        sync_key: Some(key.clone().into()),
    };
    let http = identity(&server).http().unwrap();
    let url = format!("{}api/accounts/register", server.endpoint);
    let result: Authenticated = http
        .post(&url)
        .bearer_auth(&legacy.credentials.token)
        .json(&request)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(result.credentials.account.unwrap().id, "legacy");
    assert_eq!(result.sync_key, key);
    let separate: Authenticated = http
        .post(&url)
        .bearer_auth(&legacy.credentials.token)
        .json(&Login {
            username: "takeover".into(),
            ..request
        })
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(separate.credentials.account.unwrap().id, "legacy");
    for (username, password) in [
        ("OUTSIDER", "synthetic-password-123"),
        ("missing", "synthetic-password-123"),
        ("outsider", "incorrect-password"),
    ] {
        let response = http
            .post(format!("{}api/accounts/login", server.endpoint))
            .json(&Login {
                username: username.into(),
                password: password.to_string().into(),
                name: "Login test".into(),
                sync_key: None,
            })
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if username == "OUTSIDER" { 200 } else { 401 }
        );
    }
    assert_eq!(
        http.post(&url)
            .json(&Login {
                username: "OUTSIDER".into(),
                password: "another-good-password".to_string().into(),
                name: "Duplicate".into(),
                sync_key: None
            })
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    drop(old);
    drop(server);
}
#[tokio::test(flavor = "multi_thread")]
async fn public_certificate_discovery_and_pinned_login_work_without_transmitting_admin_credentials()
{
    let dir = tempfile::tempdir().unwrap();
    let server = server::start(Config {
        data_dir: dir.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let endpoint = server.endpoint.clone();
    let found = tokio::task::spawn_blocking(move || client::accounts::discover(&endpoint))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.certificate, server.certificate);
    assert_eq!(found.fingerprint().unwrap().len(), 95);
    let request = Login {
        username: "probe-user".into(),
        password: "probe-password-123".to_string().into(),
        name: "Probe".into(),
        sync_key: None,
    };
    let joined = tokio::task::spawn_blocking(move || {
        client::accounts::blocking_account(found, request, true, None)
    })
    .await
    .unwrap()
    .unwrap();
    assert!(joined.credentials.account.is_some());
    let client = online(&joined).await;
    drop(client);
    drop(server);
}
#[tokio::test(flavor = "multi_thread")]
async fn password_login_rotates_a_remembered_device_without_losing_its_statistics_identity() {
    let dir = tempfile::tempdir().unwrap();
    let server = server::start(Config {
        data_dir: dir.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let first = account(&server, "returning-user", true).await;
    let http = identity(&server).http().unwrap();
    let login = Login {
        username: "returning-user".into(),
        password: "synthetic-password-123".to_string().into(),
        name: "Same installation".into(),
        sync_key: None,
    };
    let second: Authenticated = http
        .post(format!("{}api/accounts/login", server.endpoint))
        .bearer_auth(&first.credentials.token)
        .json(&login)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second.credentials.device_id, first.credentials.device_id);
    assert_ne!(second.credentials.token, first.credentials.token);
    assert_eq!(
        http.post(format!("{}api/invitations", server.endpoint))
            .bearer_auth(&first.credentials.token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    http.post(format!(
        "{}api/devices/{}/revoke",
        server.endpoint, second.credentials.device_id
    ))
    .bearer_auth(server.owner_token())
    .json(&serde_json::json!({}))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
    let third: Authenticated = http
        .post(format!("{}api/accounts/login", server.endpoint))
        .bearer_auth(&second.credentials.token)
        .json(&login)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(third.credentials.device_id, first.credentials.device_id);
    assert_ne!(third.credentials.token, second.credentials.token);
    let wrong = http
        .post(format!("{}api/accounts/login", server.endpoint))
        .bearer_auth(&third.credentials.token)
        .json(&Login {
            password: "wrong-password-123".to_string().into(),
            ..login
        })
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);
    drop(server);
}

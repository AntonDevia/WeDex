use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{tungstenite::Message, Connector};
use wedex_cloud::{client, server, *};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[tokio::test(flavor = "multi_thread")]
async fn cached_mirror_is_delivered_over_authenticated_tls_and_legacy_snapshots_still_decode() {
    let folder = tempfile::tempdir().unwrap();
    let mirror = mirror::HitmoMirror {
        service: "wedex-hitmo-mirror".into(),
        schema: 1,
        source: "https://hitmos.pro/".into(),
        origin: "https://ru.hitmox.net/".into(),
        checked_at_ms: now_ms(),
        alternatives: vec!["https://ru.hitmox.org/".into()],
    };
    std::fs::write(
        folder.path().join("hitmo-mirror.json"),
        serde_json::to_vec(&mirror).unwrap(),
    )
    .unwrap();
    let server = server::start(server::Config {
        data_dir: folder.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let peer = client::pair(&server.invitation(false).unwrap(), "Mirror test", false)
        .await
        .unwrap();
    let mut socket = connect(&peer).await;
    let event = wait(&mut socket, |m| {
        matches!(
            m,
            ServerMessage::Snapshot {
                hitmo_mirror: Some(_),
                ..
            }
        )
    })
    .await;
    if let ServerMessage::Snapshot { hitmo_mirror, .. } = event {
        assert_eq!(hitmo_mirror, Some(mirror));
    }
    // Old servers omit the field and remain readable by the new client.
    let legacy = serde_json::to_value(ServerMessage::Snapshot {
        devices: vec![],
        statistics: Statistics::default(),
        hitmo_mirror: None,
    })
    .unwrap();
    assert!(legacy.get("hitmo_mirror").is_none());
    let decoded: ServerMessage = serde_json::from_value(legacy).unwrap();
    assert!(matches!(
        decoded,
        ServerMessage::Snapshot {
            hitmo_mirror: None,
            ..
        }
    ));
    drop(socket);
    drop(server);
}
async fn connect(connection: &client::Connection) -> Socket {
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut connection.certificate.as_bytes()) {
        roots.add(cert.unwrap()).unwrap();
    }
    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let (mut socket, _) = tokio_tungstenite::connect_async_tls_with_config(
        format!(
            "{}ws",
            connection.endpoint.replacen("https://", "wss://", 1)
        ),
        None,
        false,
        Some(Connector::Rustls(Arc::new(tls))),
    )
    .await
    .unwrap();
    send(
        &mut socket,
        ClientMessage::Authenticate {
            token: connection.credentials.token.clone(),
        },
    )
    .await;
    wait(&mut socket, |m| matches!(m, ServerMessage::Welcome { .. })).await;
    socket
}
async fn send(socket: &mut Socket, message: ClientMessage) {
    socket
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}
async fn wait(socket: &mut Socket, condition: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            let message = socket.next().await.expect("connected").unwrap();
            if let Message::Text(text) = message {
                let event: ServerMessage = serde_json::from_str(&text).unwrap();
                if condition(&event) {
                    return event;
                }
            }
        }
    })
    .await
    .expect("server event before timeout")
}

#[tokio::test]
async fn real_tls_pairing_remote_control_statistics_revocation_and_restart() {
    let folder = tempfile::tempdir().unwrap();
    let running = server::start(server::Config {
        data_dir: folder.path().to_owned(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let invitation = running.invitation(false).unwrap();
    let pc = client::pair(&invitation, "Компьютер", true).await.unwrap();
    assert!(client::pair(&invitation, "Повтор", true).await.is_err());
    let phone = client::pair(&running.invitation(false).unwrap(), "Телефон", false)
        .await
        .unwrap();
    let mut player = connect(&pc).await;
    let mut remote = connect(&phone).await;
    let track = Track {
        key: "fixture".into(),
        title: "Музыка".into(),
        artist: "Исполнитель".into(),
        duration: 180.,
        ..Default::default()
    };
    send(
        &mut player,
        ClientMessage::Publish {
            playback: Playback {
                track: Some(track.clone()),
                playing: true,
                duration: 180.,
                queue: vec![track.clone()],
                cursor: Some(0),
                ..Default::default()
            },
        },
    )
    .await;
    wait(&mut remote, |m| {
        matches!(m, ServerMessage::Snapshot { devices, .. }
        if devices.iter().any(|d| d.id == pc.credentials.device_id && d.playback.playing))
    })
    .await;
    let command = RemoteCommand {
        id: uuid::Uuid::new_v4().to_string(),
        target: pc.credentials.device_id.clone(),
        action: Action::Next,
    };
    send(
        &mut remote,
        ClientMessage::Command {
            command: command.clone(),
        },
    )
    .await;
    let delivery = wait(&mut player, |m| matches!(m, ServerMessage::Execute { .. })).await;
    assert!(
        matches!(delivery, ServerMessage::Execute { delivery } if delivery.command.id == command.id && delivery.expires_ms >= now_ms())
    );
    send(
        &mut player,
        ClientMessage::Acknowledge {
            id: command.id.clone(),
            error: None,
        },
    )
    .await;
    wait(&mut remote, |m| matches!(m, ServerMessage::CommandStatus { id, status } if id == &command.id && status == "done")).await;
    send(
        &mut remote,
        ClientMessage::Command {
            command: command.clone(),
        },
    )
    .await;
    wait(
        &mut remote,
        |m| matches!(m, ServerMessage::CommandStatus { status, .. } if status == "done"),
    )
    .await;
    let duplicate = tokio::time::timeout(Duration::from_millis(200), async {
        loop {
            if let Some(Ok(Message::Text(text))) = player.next().await {
                if matches!(
                    serde_json::from_str::<ServerMessage>(&text),
                    Ok(ServerMessage::Execute { .. })
                ) {
                    return true;
                }
            }
        }
    })
    .await;
    assert!(duplicate.is_err(), "Next must not be delivered twice");
    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        track,
        started_ms: now_ms(),
        listened_ms: 65000,
    };
    send(
        &mut player,
        ClientMessage::Sessions {
            sessions: vec![session.clone()],
        },
    )
    .await;
    wait(&mut player, |m| {
        matches!(m, ServerMessage::SessionsStored { .. })
    })
    .await;
    send(
        &mut player,
        ClientMessage::Sessions {
            sessions: vec![session.clone()],
        },
    )
    .await;
    wait(&mut remote, |m| matches!(m, ServerMessage::Snapshot { statistics, .. } if statistics.listened_ms == 65000 && statistics.sessions == 1)).await;
    send(
        &mut remote,
        ClientMessage::Sessions {
            sessions: vec![session],
        },
    )
    .await;
    wait(&mut remote, |m| matches!(m, ServerMessage::Error { .. })).await;
    let http = reqwest::Client::builder()
        .add_root_certificate(
            reqwest::Certificate::from_pem(running.certificate.as_bytes()).unwrap(),
        )
        .build()
        .unwrap();
    assert_eq!(
        http.post(format!("{}api/invitations", running.endpoint))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    // A different trust anchor cannot authenticate this server: verification is never disabled.
    let bad = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let strict = reqwest::Client::builder()
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(bad.cert.pem().as_bytes()).unwrap())
        .build()
        .unwrap();
    assert!(strict
        .get(format!("{}health", running.endpoint))
        .send()
        .await
        .is_err());
    running.revoke(&phone.credentials.device_id).unwrap();
    // Reconnect with a revoked token gets no welcome message.
    drop(remote);
    drop(player);
    let old_cert = running.certificate.clone();
    drop(running);
    let restarted = server::start(server::Config {
        data_dir: folder.path().to_owned(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    assert_eq!(old_cert, restarted.certificate);
    let mut reconnect = pc.clone();
    reconnect.endpoint = restarted.endpoint.clone();
    let mut socket = connect(&reconnect).await;
    wait(&mut socket, |m| {
        matches!(m, ServerMessage::Snapshot { statistics, devices, .. }
        if statistics.listened_ms == 65000 && devices.len() == 1)
    })
    .await;
    drop(socket);
    drop(restarted);
}

#[test]
fn second_server_cannot_open_same_data_directory() {
    let folder = tempfile::tempdir().unwrap();
    let config = server::Config {
        data_dir: folder.path().to_owned(),
        bind: "127.0.0.1:0".parse().unwrap(),
    };
    let first = server::start(config.clone()).unwrap();
    assert!(server::start(config).is_err());
    drop(first);
}

use rustls::client::danger::ServerCertVerifier;
use std::sync::Arc;
use wedex_cloud::server::{self, Config};

#[test]
fn public_host_is_in_the_certificate_and_unrelated_hosts_fail_verification() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let server = server::start_with_public_host(
        Config {
            data_dir: dir.path().into(),
            bind: "127.0.0.1:0".parse().unwrap(),
        },
        Some("music.example.test"),
    )
    .unwrap();
    let certificate = rustls_pemfile::certs(&mut server.certificate.as_bytes())
        .next()
        .unwrap()
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate.clone()).unwrap();
    let verifier = rustls::client::WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .unwrap();
    for (name, valid) in [
        ("music.example.test", true),
        ("127.0.0.1", true),
        ("unrelated.example.test", false),
    ] {
        assert_eq!(
            verifier
                .verify_server_cert(
                    &certificate,
                    &[],
                    &rustls::pki_types::ServerName::try_from(name).unwrap(),
                    &[],
                    rustls::pki_types::UnixTime::now()
                )
                .is_ok(),
            valid,
            "{name}"
        );
    }
}

#[test]
fn administrative_cli_uses_running_server_and_service_log_has_no_invitation() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
    };
    let dir = tempfile::tempdir().unwrap();
    let server = server::start_with_public_host(
        Config {
            data_dir: dir.path().into(),
            bind: "127.0.0.1:0".parse().unwrap(),
        },
        Some("127.0.0.1"),
    )
    .unwrap();
    let config = dir.path().join("config.json");
    let port = reqwest::Url::parse(&server.endpoint)
        .unwrap()
        .port()
        .unwrap();
    std::fs::write(&config,serde_json::to_vec(&serde_json::json!({"data_dir":dir.path(),"bind":format!("127.0.0.1:{port}"),"public_host":"127.0.0.1"})).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wedex-server"))
        .args(["--config", config.to_str().unwrap(), "--invite-json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["owner"]["endpoint"], server.endpoint);
    let invite: wedex_cloud::Invitation =
        serde_json::from_value(value["invitation"].clone()).unwrap();
    assert!(wedex_cloud::client::blocking_pair(&invite, "CLI test", true).is_ok());
    let other = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_wedex-server"))
        .args([
            "--service",
            "--data-dir",
            other.path().to_str().unwrap(),
            "--bind",
            "127.0.0.1:0",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(line.starts_with(&format!(
        "WeDex Cloud {} listening",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(!line.contains("wedex-cloud:") && !line.contains("#pair="));
}

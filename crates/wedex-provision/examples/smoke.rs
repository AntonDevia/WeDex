//! Owned-server smoke harness. Secrets are read from environment/file, never printed.
use anyhow::{Context, Result};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use wedex_provision::{Event, Request, Worker};
fn main() -> Result<()> {
    let folder = PathBuf::from(
        std::env::var("WEDEX_TEST_DIR")
            .context("Set WEDEX_TEST_DIR to the owned fixture directory")?,
    );
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("test-credentials.json"))?)?;
    let key = std::env::args().any(|x| x == "--key");
    let connect_only = std::env::args().any(|x| x == "--connect-only");
    let reject = std::env::args().any(|x| x == "--reject");
    let pin = std::fs::read_to_string(folder.join("fingerprint.txt"))?
        .trim()
        .to_owned();
    let request = Request {
        host: "127.0.0.1".into(),
        public_host: "127.0.0.1".into(),
        ssh_port: 22222,
        port: 28765,
        user: if key { "wedexqa".into() } else { "root".into() },
        password: value["password"].as_str().unwrap().to_owned().into(),
        key_path: if key {
            folder.join("id_ed25519").display().to_string()
        } else {
            String::new()
        },
        key_passphrase: String::new().into(),
        sudo_password: if std::env::args().any(|x| x == "--sudo-password") {
            value["password"].as_str().unwrap().to_owned().into()
        } else {
            String::new().into()
        },
        fingerprint: None,
        device_name: if key {
            "Smoke key".into()
        } else {
            "Smoke password".into()
        },
        connect_only,
        previous: std::fs::read(folder.join("connection.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok()),
    };
    let binary = std::fs::read(std::env::var("WEDEX_TEST_BINARY")?)?;
    let mut worker = Worker::start(request, Box::leak(binary.into_boxed_slice()))?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(240) {
        while let Ok(event) = worker.events.try_recv() {
            match event {
                Event::HostKey { fingerprint, .. } => {
                    anyhow::ensure!(fingerprint == pin, "Fixture SSH fingerprint mismatch");
                    worker.trust(!reject);
                }
                Event::Trusted { .. } => println!("Trusted known fixture key"),
                Event::Step(step) => println!("{step}"),
                Event::Bootstrap(owner) => {
                    std::fs::write(folder.join("owner.json"), serde_json::to_vec(&owner)?)?
                }
                Event::Complete(connection) => {
                    std::fs::write(
                        folder.join("connection.json"),
                        serde_json::to_vec(&connection)?,
                    )?;
                    println!("Provisioning and external TLS/WSS passed");
                    return Ok(());
                }
                Event::Failed(error) => {
                    if reject {
                        println!("Untrusted SSH key correctly rejected");
                        return Ok(());
                    }
                    anyhow::bail!("{error}");
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("Fixture timed out")
}

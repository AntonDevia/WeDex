use anyhow::{Context, Result};
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Deserialize)]
struct Config {
    #[serde(default = "default_repository")]
    update_repo: String,
    data_dir: PathBuf,
    bind: SocketAddr,
    public_host: Option<String>,
}

fn default_repository() -> String {
    wedex_update::DEFAULT_REPOSITORY.into()
}
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut config = Config {
        update_repo: default_repository(),
        data_dir: "wedex-cloud-data".into(),
        bind: "127.0.0.1:8765".parse()?,
        public_host: None,
    };
    let mut service = false;
    let mut invite = false;
    let mut apply_update = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                config.data_dir = args.next().context("Нужен путь после --data-dir")?.into()
            }
            "--bind" => config.bind = args.next().context("Нужен адрес после --bind")?.parse()?,
            "--public-host" => {
                config.public_host = Some(args.next().context("Нужен IP или домен")?)
            }
            "--config" => {
                config = serde_json::from_slice(&std::fs::read(
                    args.next().context("Нужен путь конфигурации")?,
                )?)?
            }
            "--service" => service = true,
            "--invite-json" => invite = true,
            "--apply-pending-update" => apply_update = true,
            "--version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("wedex-server [--config PATH] [--data-dir PATH] [--bind 127.0.0.1:8765] [--public-host IP_OR_DOMAIN] [--service]\n--invite-json: owner bootstrap over local TLS; keep output private");
                return Ok(());
            }
            _ => anyhow::bail!("Неизвестный аргумент: {arg}"),
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    if apply_update {
        return wedex_cloud::update::apply(&config.data_dir, &config.update_repo);
    }
    if invite {
        let host = config
            .public_host
            .context("Для регистрации нужен public_host")?;
        anyhow::ensure!(
            wedex_cloud::server::valid_public_host(&host),
            "Некорректный публичный адрес"
        );
        let certificate = std::fs::read_to_string(config.data_dir.join("certificate.pem"))?;
        let identity: serde_json::Value =
            serde_json::from_slice(&std::fs::read(config.data_dir.join("identity.json"))?)?;
        let token = identity["owner"]
            .as_str()
            .context("Не найден владелец")?
            .to_owned();
        let mut owner = wedex_cloud::admin::Owner {
            endpoint: format!(
                "https://{}:{}/",
                if config.bind.is_ipv6() {
                    "[::1]"
                } else {
                    "127.0.0.1"
                },
                config.bind.port()
            ),
            certificate,
            owner: token,
        };
        let mut invitation = runtime.block_on(owner.invitation())?;
        let authority = if host.contains(':') {
            format!("[{host}]")
        } else {
            host
        };
        owner.endpoint = format!("https://{authority}:{}/", config.bind.port());
        invitation.endpoint = owner.endpoint.clone();
        println!(
            "{}",
            serde_json::json!({"owner":owner,"invitation":invitation})
        );
        return Ok(());
    }
    let server = wedex_cloud::server::start_configured(
        wedex_cloud::server::Config {
            data_dir: config.data_dir,
            bind: config.bind,
        },
        config.public_host.as_deref(),
        &config.update_repo,
    )?;
    if service {
        println!(
            "WeDex Cloud {} listening on {}",
            env!("CARGO_PKG_VERSION"),
            config.bind
        );
    } else {
        println!("WeDex Cloud: {}", server.endpoint);
        println!(
            "Приглашение (10 минут, одно использование):\n{}",
            server.invitation(false)?.encode()
        );
        println!(
            "Браузерный пульт:\n{}",
            server.browser_invitation(server.lan_endpoint.is_some())?
        );
    }
    runtime.block_on(async {
        #[cfg(unix)] {
            let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            tokio::select! { result = tokio::signal::ctrl_c() => result, _ = terminate.recv() => Ok(()) }
        }
        #[cfg(not(unix))] { tokio::signal::ctrl_c().await }
    })?;
    drop(server);
    Ok(())
}

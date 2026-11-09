//! The installer-owned systemd worker applies server updates as root.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
};
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub version: String,
}
pub const PATH_UNIT:&str="[Unit]\nDescription=Watch WeDex update requests\n\n[Path]\nPathExists=/var/lib/wedex/update-request.json\nUnit=wedex-update.service\n\n[Install]\nWantedBy=multi-user.target\n";
pub const SERVICE_UNIT:&str="[Unit]\nDescription=Apply verified WeDex server update\nAfter=network-online.target\n\n[Service]\nType=oneshot\nExecStart=/opt/wedex/current/wedex-server --config /etc/wedex/server.json --apply-pending-update\nTimeoutStartSec=900\nUMask=0077\nNoNewPrivileges=true\nProtectHome=true\nPrivateTmp=true\n";
pub fn enabled() -> bool {
    cfg!(target_os = "linux") && Path::new("/etc/systemd/system/wedex-update.path").is_file()
}
pub fn apply(data: &Path, repo: &str) -> Result<()> {
    let requested = data.join("update-request.json");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open("/run/lock/wedex-install.lock")?;
    lock.try_lock()
        .map_err(|_| anyhow::anyhow!("Другая установка WeDex ещё выполняется"))?;
    let mut bytes = vec![];
    regular(&requested)?.take(4097).read_to_end(&mut bytes)?;
    std::fs::remove_file(requested)?;
    anyhow::ensure!(bytes.len() <= 4096, "Слишком большой запрос обновления");
    let request: Request = serde_json::from_slice(&bytes)?;
    let status = |state: &str, message: &str| -> Result<()> {
        write_status(
            data,
            &serde_json::json!({"state":state,"message":message,"version":request.version}),
        )?;
        Ok(())
    };
    let result = (|| -> Result<()> {
        anyhow::ensure!(
            wedex_update::version(&request.version)?
                > wedex_update::version(env!("CARGO_PKG_VERSION"))?,
            "Версия уже установлена"
        );
        let release = wedex_update::check(
            repo,
            wedex_update::Target::Server,
            env!("CARGO_PKG_VERSION"),
        )?
        .context("Нет опубликованного обновления сервера")?;
        anyhow::ensure!(
            release.version == request.version,
            "Релиз изменился; повторите обновление"
        );
        status("downloading", "Скачиваем и проверяем сервер")?;
        let folder = Path::new("/opt/wedex/.updates");
        std::fs::create_dir_all(folder)?;
        let binary = wedex_update::download(&release, folder, |_| {})?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))?;
        }
        let version = std::process::Command::new(&binary)
            .arg("--version")
            .output()?;
        anyhow::ensure!(
            version.status.success()
                && String::from_utf8_lossy(&version.stdout).trim() == request.version,
            "Версия файла обновления не совпала"
        );
        let release_path = Path::new("/opt/wedex/releases")
            .join(format!("{}-{}", release.version, release.sha256));
        std::fs::create_dir_all(&release_path)?;
        std::fs::copy(&binary, release_path.join("wedex-server"))?;
        let previous = std::fs::read_link("/opt/wedex/current")?;
        status("installing", "Сохраняем базу и перезапускаем службу")?;
        systemctl("stop")?;
        let change = (|| -> Result<()> {
            let backup =
                Path::new("/var/backups/wedex").join(format!("update-{}", crate::now_ms()));
            std::fs::create_dir_all(&backup)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o700))?;
            }
            for name in [
                "cloud.sqlite3",
                "cloud.sqlite3-wal",
                "cloud.sqlite3-shm",
                "identity.json",
                "private-key.pem",
                "certificate.pem",
            ] {
                let from = data.join(name);
                if from.exists() {
                    anyhow::ensure!(
                        !std::fs::symlink_metadata(&from)?.file_type().is_symlink(),
                        "Нельзя копировать ссылку в резервную копию"
                    );
                    let mut source = regular(&from)?;
                    let mut options = std::fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    let mut target = options.open(backup.join(name))?;
                    std::io::copy(&mut source, &mut target)?;
                    target.sync_all()?;
                }
            }
            switch(&release_path)?;
            systemctl("start")?;
            let mut ready = false;
            for _ in 0..15 {
                let health = std::process::Command::new(release_path.join("wedex-server"))
                    .args(["--config", "/etc/wedex/server.json", "--invite-json"])
                    .output()?;
                if health.status.success() {
                    ready = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            anyhow::ensure!(ready, "Новая служба не прошла TLS-проверку");
            Ok(())
        })();
        if change.is_err() {
            let _ = systemctl("stop");
            let _ = switch(&previous);
            let _ = systemctl("start");
        }
        change?;
        let manifest = Path::new("/etc/wedex/install.json");
        if let Ok(mut value) = std::fs::read(manifest)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .ok_or(())
        {
            value["version"] = request.version.clone().into();
            value["sha256"] = release.sha256.into();
            let temporary = manifest.with_extension("next");
            std::fs::write(&temporary, serde_json::to_vec(&value)?)?;
            std::fs::rename(temporary, manifest)?;
        }
        status("done", "Сервер обновлён")?;
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = status("error", &format!("{error:#}"));
    }
    result
}
fn systemctl(action: &str) -> Result<()> {
    anyhow::ensure!(
        std::process::Command::new("systemctl")
            .args([action, "wedex.service"])
            .status()?
            .success(),
        "Не удалось выполнить systemctl {action}"
    );
    Ok(())
}
fn regular(path: &Path) -> Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    anyhow::ensure!(file.metadata()?.is_file(), "Нужен обычный файл");
    Ok(file)
}
fn write_status(data: &Path, value: &serde_json::Value) -> Result<()> {
    let temporary = data.join(format!("status-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    }
    drop(file);
    std::fs::rename(temporary, data.join("update-status.json"))?;
    Ok(())
}
#[cfg(unix)]
fn switch(path: &Path) -> Result<()> {
    let next = Path::new("/opt/wedex/current.update");
    if next.exists() {
        std::fs::remove_file(next)?;
    }
    std::os::unix::fs::symlink(path, next)?;
    std::fs::rename(next, "/opt/wedex/current")?;
    Ok(())
}
#[cfg(not(unix))]
fn switch(_: &Path) -> Result<()> {
    anyhow::bail!("Обновление VPS доступно только в Linux")
}

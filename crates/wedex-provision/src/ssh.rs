use crate::{Event, Request};
use anyhow::{Context, Result};
use russh::{
    client,
    keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate},
    ChannelMsg,
};
use std::{
    borrow::Cow,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

pub(crate) fn posix_script(source: &str) -> Cow<'_, str> {
    if source.contains("\r\n") {
        Cow::Owned(source.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(source)
    }
}

pub struct Handler {
    pub pin_name: String,
    pub expected: Option<String>,
    pub events: mpsc::UnboundedSender<Event>,
    pub decision: Option<oneshot::Receiver<bool>>,
}

impl client::Handler for Handler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        if self.expected.as_deref() == Some(&fingerprint) {
            return Ok(true);
        }
        let _ = self.events.send(Event::HostKey {
            fingerprint: fingerprint.clone(),
            previous: self.expected.clone(),
        });
        let Some(decision) = self.decision.take() else {
            return Ok(false);
        };
        let accepted = tokio::time::timeout(Duration::from_secs(600), decision)
            .await
            .is_ok_and(|r| r.unwrap_or(false));
        if accepted {
            let _ = self.events.send(Event::Trusted {
                pin_name: self.pin_name.clone(),
                fingerprint,
            });
        }
        Ok(accepted)
    }
}
pub struct Remote {
    pub session: client::Handle<Handler>,
    pub sudo: Option<Zeroizing<String>>,
    pub cancelled: Arc<AtomicBool>,
}
impl Remote {
    pub async fn connect(
        request: &Request,
        handler: Handler,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self> {
        let config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(660)),
            ..Default::default()
        };
        let mut session = tokio::time::timeout(
            Duration::from_secs(620),
            client::connect(
                Arc::new(config),
                (request.host.as_str(), request.ssh_port),
                handler,
            ),
        )
        .await
        .context("Истекло время ожидания SSH")?
        .context("Не удалось подключиться по SSH или ключ сервера не подтверждён")?;
        let auth = if request.key_path.is_empty() {
            session
                .authenticate_password(&request.user, request.password.as_str())
                .await?
        } else {
            let size = std::fs::metadata(&request.key_path)
                .context("Не найден файл SSH-ключа")?
                .len();
            anyhow::ensure!(size <= 1024 * 1024, "Слишком большой файл SSH-ключа");
            let key = russh::keys::load_secret_key(
                &request.key_path,
                (!request.key_passphrase.is_empty()).then_some(request.key_passphrase.as_str()),
            )
            .context("Не удалось открыть SSH-ключ: проверьте формат и пароль ключа")?;
            let algorithm = session.best_supported_rsa_hash().await?.flatten();
            session
                .authenticate_publickey(
                    &request.user,
                    PrivateKeyWithHashAlg::new(Arc::new(key), algorithm),
                )
                .await?
        };
        anyhow::ensure!(auth.success(), "SSH не принял логин, пароль или ключ");
        let mut remote = Self {
            session,
            sudo: None,
            cancelled,
        };
        let root = remote.exec("id -u", &[], false).await?.trim() == "0";
        if !root {
            remote.sudo = Some(Zeroizing::new(
                if request.sudo_password.is_empty() && request.key_path.is_empty() {
                    request.password.to_string()
                } else {
                    request.sudo_password.to_string()
                },
            ));
            remote
                .exec("test \"$(id -u)\" = 0", &[], true)
                .await
                .context("Нужен root или sudo; укажите пароль sudo")?;
        }
        Ok(remote)
    }
    pub async fn exec(&mut self, command: &str, data: &[u8], privileged: bool) -> Result<String> {
        anyhow::ensure!(
            !self.cancelled.load(Ordering::Relaxed),
            "Установка отменена"
        );
        let command = posix_script(command);
        let mut channel = self.session.channel_open_session().await?;
        let sudo = if privileged { self.sudo.as_ref() } else { None };
        let wrapped = if let Some(password) = sudo {
            if password.is_empty() {
                format!("sudo -n -- /bin/sh -c {}", crate::quote(&command))
            } else {
                format!("sudo -S -p '' -- /bin/sh -c {}", crate::quote(&command))
            }
        } else {
            format!("/bin/sh -c {}", crate::quote(&command))
        };
        channel.exec(true, wrapped).await?;
        if let Some(password) = sudo.filter(|s| !s.is_empty()) {
            let line = Zeroizing::new(format!("{}\n", password.as_str()));
            channel.data(line.as_bytes()).await?;
        }
        for chunk in data.chunks(64 * 1024) {
            anyhow::ensure!(
                !self.cancelled.load(Ordering::Relaxed),
                "Установка отменена"
            );
            channel.data(chunk).await?;
        }
        channel.eof().await?;
        let started = Instant::now();
        let mut output = Vec::new();
        let mut error = Vec::new();
        let mut status = None;
        loop {
            if self.cancelled.load(Ordering::Relaxed)
                || started.elapsed() > Duration::from_secs(120)
            {
                let _ = channel.close().await;
                anyhow::bail!(
                    "Операция прервана; повторите подключение для проверки состояния сервера"
                );
            }
            let message = match tokio::time::timeout(Duration::from_secs(1), channel.wait()).await {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(_) => continue,
            };
            match message {
                ChannelMsg::Data { data } => output.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => error.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
            anyhow::ensure!(
                output.len() + error.len() <= 128 * 1024,
                "Слишком большой ответ SSH"
            );
        }
        if status != Some(0) {
            let detail = String::from_utf8_lossy(&error);
            anyhow::bail!(
                "Команда сервера завершилась с ошибкой: {}",
                detail.chars().take(2048).collect::<String>()
            );
        }
        String::from_utf8(output).context("Некорректный ответ SSH")
    }
    pub async fn upload(&mut self, path: &str, bytes: &[u8]) -> Result<()> {
        self.exec(
            &format!("umask 077; cat > {}", crate::quote(path)),
            bytes,
            false,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::client::Handler as _;
    #[test]
    fn windows_line_endings_are_removed_from_inline_and_uploaded_scripts() {
        for source in [include_str!("probe.sh"), include_str!("install.sh")] {
            let lf = source.replace("\r\n", "\n");
            let crlf = lf.replace('\n', "\r\n");
            assert_eq!(posix_script(&crlf), lf);
            assert_eq!(posix_script(&lf), lf);
        }
    }
    #[cfg(unix)]
    #[test]
    fn linux_shell_executes_normalized_windows_script_with_heredoc() {
        let source = "set -eu\r\nvalue='two words'\r\ncat <<'END'\r\n$value\r\nEND\r\nprintf '%s\\n' \"$value\"\r\n";
        let run = |script: &str| {
            std::process::Command::new("/bin/sh")
                .args(["-c", script])
                .output()
                .unwrap()
        };
        assert!(!run(source).status.success());
        let fixed = run(&posix_script(source));
        assert!(
            fixed.status.success(),
            "{}",
            String::from_utf8_lossy(&fixed.stderr)
        );
        assert_eq!(fixed.stdout, b"$value\ntwo words\n");
    }
    fn key() -> PublicKeyOrCertificate {
        russh::keys::PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJeLWu7O2vvsXdUQ5bO6BcYTG9dD4x1rvv2kgzSm4sKp",
        )
        .unwrap()
        .into()
    }
    #[tokio::test]
    async fn known_key_needs_no_prompt_and_changed_key_cannot_silently_replace_it() {
        let key = key();
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (_decision, trust) = oneshot::channel();
        let mut h = Handler {
            pin_name: "test".into(),
            expected: Some(fingerprint),
            events: tx,
            decision: Some(trust),
        };
        assert!(h.check_server_key(&key).await.unwrap());
        assert!(rx.try_recv().is_err());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (decision, trust) = oneshot::channel();
        decision.send(false).unwrap();
        let mut h = Handler {
            pin_name: "test".into(),
            expected: Some("SHA256:old".into()),
            events: tx,
            decision: Some(trust),
        };
        assert!(!h.check_server_key(&key).await.unwrap());
        assert!(matches!(
            rx.try_recv().unwrap(),
            Event::HostKey {
                previous: Some(_),
                ..
            }
        ));
        assert!(rx.try_recv().is_err());
    }
}

use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use wedex_update::{Release, Target};
#[derive(Clone, Default)]
pub struct View {
    pub app: Option<Release>,
    pub server: Option<Release>,
    pub busy: bool,
    pub progress: Option<f32>,
    pub ready: Option<PathBuf>,
    pub message: String,
}
#[derive(Clone)]
pub enum Action {
    Check,
    Download,
    Apply,
    #[cfg(windows)]
    Server,
}
enum Event {
    Checked(Target, Result<Option<Release>, String>),
    Progress(f32),
    Downloaded(Result<PathBuf, String>),
}
pub struct Updates {
    pub view: View,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    last: Option<Instant>,
    server_version: String,
    checking: usize,
    pub prompt: bool,
    pub exit: bool,
}
impl Default for Updates {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            view: Default::default(),
            tx,
            rx,
            last: None,
            server_version: String::new(),
            checking: 0,
            prompt: false,
            exit: false,
        }
    }
}
impl Updates {
    pub fn check(&mut self, server: &str) {
        if self.view.busy {
            return;
        }
        self.last = Some(Instant::now());
        self.view.busy = true;
        self.view.message = "Проверяем GitHub Releases…".into();
        let target = if cfg!(target_os = "android") {
            Target::Android
        } else {
            Target::Windows
        };
        for (target, current) in [
            (target, env!("CARGO_PKG_VERSION").to_owned()),
            (
                Target::Server,
                if cfg!(windows) {
                    server.to_owned()
                } else {
                    String::new()
                },
            ),
        ] {
            if current.is_empty() {
                continue;
            }
            self.checking += 1;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result =
                    wedex_update::check(wedex_update::DEFAULT_REPOSITORY, target, &current)
                        .map_err(|e| e.to_string());
                let _ = tx.send(Event::Checked(target, result));
            });
        }
    }
    pub fn tick(&mut self, server: &str) {
        if server.is_empty() {
            self.view.server = None;
        }
        if self
            .last
            .is_none_or(|t| t.elapsed() > Duration::from_secs(6 * 3600))
        {
            self.check(server);
        }
        if !server.is_empty() && server != self.server_version {
            self.server_version = server.into();
            self.last = None;
        }
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Checked(target, result) => {
                    self.checking = self.checking.saturating_sub(1);
                    self.view.busy = self.checking > 0 || self.view.progress.is_some();
                    match result {
                        Ok(release) => {
                            if release.is_some() {
                                self.prompt = true;
                            }
                            if target == Target::Server {
                                self.view.server = release;
                            } else {
                                self.view.app = release;
                            }
                            self.view.message =
                                if self.view.app.is_some() || self.view.server.is_some() {
                                    "Доступно обновление"
                                } else {
                                    "Установлена актуальная версия"
                                }
                                .into();
                        }
                        Err(error) => {
                            self.view.message = "Не удалось проверить обновления".into();
                            let _ = crate::diagnostics::append(
                                &crate::diagnostics::log_path(),
                                "update-check",
                                &error,
                                &[],
                            );
                        }
                    }
                }
                Event::Progress(value) => self.view.progress = Some(value),
                Event::Downloaded(result) => {
                    self.view.busy = false;
                    self.view.progress = None;
                    match result {
                        Ok(path) => {
                            self.view.ready = Some(path);
                            self.view.message = "Обновление проверено и готово к установке".into();
                        }
                        Err(error) => {
                            self.view.message = "Что-то пошло не так".into();
                            let _ = crate::diagnostics::append(
                                &crate::diagnostics::log_path(),
                                "update-download",
                                &error,
                                &[],
                            );
                        }
                    }
                }
            }
        }
    }
    pub fn action(&mut self, action: Action, server: &str) -> anyhow::Result<()> {
        match action {
            Action::Check => self.check(server),
            Action::Download if !self.view.busy => {
                if let Some(release) = self.view.app.clone() {
                    self.view.busy = true;
                    self.view.progress = Some(0.);
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let folder = crate::store::data_dir().join("updates");
                        let result = wedex_update::download(&release, &folder, |p| {
                            let _ = tx.send(Event::Progress(p));
                        })
                        .map_err(|e| e.to_string());
                        let _ = tx.send(Event::Downloaded(result));
                    });
                }
            }
            Action::Apply => {
                if let Some(path) = &self.view.ready {
                    #[cfg(windows)]
                    {
                        wedex_update::install_windows(
                            path,
                            &self
                                .view
                                .app
                                .as_ref()
                                .ok_or_else(|| anyhow::anyhow!("Нет релиза обновления"))?
                                .sha256,
                        )?;
                        self.exit = true;
                    }
                    #[cfg(target_os = "android")]
                    {
                        crate::android::install_update(path)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

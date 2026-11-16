use eframe::egui::{self, Color32, RichText};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use wedex_cloud::admin::Owner;
use wedex_provision::{Event, Request, Worker};
use zeroize::Zeroize;

#[cfg(wedex_server_bundle)]
const BINARY: &[u8] = include_bytes!("../../../assets/server/wedex-server-x86_64-linux-musl");
#[cfg(not(wedex_server_bundle))]
const BINARY: &[u8] = &[];
fn read(name: &str) -> String {
    keyring::Entry::new("WeDex.Master", name)
        .and_then(|e| e.get_password())
        .unwrap_or_default()
}
fn save(name: &str, value: &str) -> anyhow::Result<()> {
    keyring::Entry::new("WeDex.Master", name)?.set_password(value)?;
    Ok(())
}
pub fn run() -> eframe::Result<()> {
    eframe::run_native(
        "WeDex · Мастер",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([780., 820.])
                .with_min_inner_size([520., 540.]),
            ..Default::default()
        },
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "medeo".into(),
                egui::FontData::from_static(include_bytes!(
                    "../../../assets/fonts/medeo/MedeoSans-Medium.ttf"
                ))
                .into(),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "medeo".into());
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.style_mut(|s| {
                s.animation_time = 0.2;
                s.spacing.item_spacing = egui::vec2(12., 12.);
                s.spacing.button_padding = egui::vec2(16., 10.);
                s.visuals.selection.bg_fill = Color32::from_rgb(232, 255, 163);
                s.visuals.selection.stroke.color = Color32::from_rgb(28, 26, 30);
                s.visuals.extreme_bg_color = Color32::from_rgb(38, 35, 44);
                s.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(12);
                s.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(12);
                s.visuals.widgets.active.corner_radius = egui::CornerRadius::same(12);
                s.visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(237, 235, 242);
            });
            Ok(Box::new(Master::new()))
        }),
    )
}
struct Master {
    fields: [String; 9],
    key: bool,
    worker: Option<Worker>,
    owner: Option<Owner>,
    trust: Option<(String, Option<String>)>,
    steps: Vec<String>,
    status: String,
    job: Option<mpsc::Receiver<anyhow::Result<serde_json::Value>>>,
    updating: bool,
    poll: Instant,
    started: Instant,
}
impl Master {
    fn new() -> Self {
        let mut fields = std::array::from_fn(|_| String::new());
        fields[1] = "22".into();
        fields[2] = "root".into();
        fields[7] = "8765".into();
        if let Ok(saved) = serde_json::from_str::<Vec<String>>(&read("profile")) {
            for i in [0, 1, 2, 4, 5, 7] {
                if let Some(v) = saved.get(i) {
                    fields[i] = v.clone();
                }
            }
        }
        // Existing installations keep their administrator access, now exclusively in Master.
        let owner = serde_json::from_str(&read("owner")).ok().or_else(|| {
            keyring::Entry::new("WeDex.Sonora", "cloud-owner")
                .and_then(|e| e.get_password())
                .ok()
                .and_then(|v| serde_json::from_str::<Owner>(&v).ok())
        });
        Self {
            fields,
            key: false,
            worker: None,
            owner,
            trust: None,
            steps: vec![],
            status: String::new(),
            job: None,
            updating: false,
            poll: Instant::now(),
            started: Instant::now(),
        }
    }
    fn clear_secrets(&mut self) {
        for i in [3, 6, 8] {
            self.fields[i].zeroize();
            self.fields[i].clear();
        }
    }
    fn start(&mut self, connect_only: bool) -> anyhow::Result<()> {
        let request = Request {
            host: self.fields[0].trim().into(),
            public_host: if self.fields[5].trim().is_empty() {
                self.fields[0].trim().into()
            } else {
                self.fields[5].trim().into()
            },
            ssh_port: self.fields[1].parse()?,
            port: self.fields[7].parse()?,
            user: self.fields[2].trim().into(),
            password: std::mem::take(&mut self.fields[3]).into(),
            key_path: if self.key {
                self.fields[4].trim().trim_matches('"').into()
            } else {
                String::new()
            },
            key_passphrase: std::mem::take(&mut self.fields[6]).into(),
            sudo_password: std::mem::take(&mut self.fields[8]).into(),
            fingerprint: None,
            device_name: "WeDex Мастер".into(),
            connect_only,
            previous: None,
        };
        let mut request = request;
        let pin = read(&request.pin_name());
        request.fingerprint = (!pin.is_empty()).then_some(pin);
        self.worker = Some(Worker::start_master(request, BINARY)?);
        self.steps.clear();
        self.status = "Подключаемся…".into();
        let mut public = self.fields.clone();
        for i in [3, 6, 8] {
            public[i].clear();
        }
        save("profile", &serde_json::to_string(&public)?)?;
        Ok(())
    }
    fn request_update(&mut self, status: bool) {
        if self.job.is_some() {
            return;
        }
        let Some(owner) = self.owner.clone() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(if status {
                owner.blocking_update_status()
            } else {
                owner.blocking_update()
            });
        });
        self.poll = Instant::now();
    }
    fn poll(&mut self) {
        let mut events = vec![];
        if let Some(w) = &mut self.worker {
            while let Ok(e) = w.events.try_recv() {
                events.push(e);
            }
        }
        for event in events {
            match event {
                Event::Step(s) => self.steps.push(s),
                Event::HostKey {
                    fingerprint,
                    previous,
                } => self.trust = Some((fingerprint, previous)),
                Event::Trusted {
                    pin_name,
                    fingerprint,
                } => {
                    if let Err(e) = save(&pin_name, &fingerprint) {
                        self.status = e.to_string();
                    }
                }
                Event::Bootstrap(owner) => {
                    if let Err(e) = save("owner", &serde_json::to_string(&owner).unwrap()) {
                        self.status = e.to_string();
                    }
                    self.owner = Some(owner);
                }
                Event::Managed(owner) => {
                    self.owner = Some(owner);
                    self.worker = None;
                    self.status =
                        "Сервер готов. Пользователи создают аккаунты в WeDex по адресу VPS.".into();
                }
                Event::Failed(e) => {
                    self.status = e;
                    self.worker = None;
                    self.trust = None;
                }
                Event::Complete(_) => {}
            }
        }
        if let Some(result) = self.job.as_ref().and_then(|r| r.try_recv().ok()) {
            self.job = None;
            match result {
                Ok(v) => match v["state"].as_str().unwrap_or("") {
                    "up_to_date" => {
                        self.updating = false;
                        self.status = "Установлена последняя версия сервера".into();
                    }
                    "done" => {
                        self.updating = false;
                        self.status =
                            format!("Сервер обновлён: {}", v["version"].as_str().unwrap_or(""));
                    }
                    "error" => {
                        self.updating = false;
                        self.status = v["message"].as_str().unwrap_or("Ошибка обновления").into();
                    }
                    _ => self.status = "Сервер обновляется…".into(),
                },
                Err(e) => self.status = e.to_string(),
            }
        }
        if self.updating && self.started.elapsed() > Duration::from_secs(1200) {
            self.updating = false;
            self.status = "Время ожидания истекло. Проверьте сервер.".into();
        }
        if self.updating && self.poll.elapsed() > Duration::from_secs(3) {
            self.request_update(true);
        }
    }
}
impl Drop for Master {
    fn drop(&mut self) {
        self.clear_secrets();
    }
}
impl eframe::App for Master {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        ctx.request_repaint_after(Duration::from_millis(150));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(24,22,29)).inner_margin(28.))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading(RichText::new("wedex").size(40.));
                    ui.label("Мастер · управление VPS");
                    ui.add_space(16.);
                    if !self.status.is_empty() {ui.label(&self.status);ui.add_space(12.);}
                    if let Some(owner)=&self.owner {
                        ui.label(RichText::new(&owner.endpoint).strong());
                        let identity=wedex_cloud::client::accounts::ServerIdentity{endpoint:owner.endpoint.clone(),certificate:owner.certificate.clone()};
                        ui.add(egui::Label::new(RichText::new(format!("TLS · SHA-256\n{}",identity.fingerprint().unwrap_or_default())).monospace().size(11.)).wrap());
                        ui.horizontal_wrapped(|ui| {
                            if ui.add_enabled(self.worker.is_none()&&!self.updating,egui::Button::new("Обновить сервис на VPS")).clicked() {
                                self.updating=true;self.started=Instant::now();self.request_update(false);
                            }
                            if ui.button("Копировать адрес для WeDex").clicked() {
                                if let Some(owner)=&self.owner {ctx.copy_text(owner.endpoint.clone());}
                            }
                        });
                        ui.separator();
                    }
                    if self.worker.is_none()&&!self.updating {
                        ui.label("Debian 12/13 · Ubuntu 22.04/24.04 · x86_64 · systemd");
                        for (i,label) in [(0,"IP или домен SSH"),(1,"SSH-порт"),(2,"SSH-логин"),(7,"Порт WeDex"),(5,"Публичный адрес · если отличается от SSH")] {
                            ui.label(label);
                            ui.add(egui::TextEdit::singleline(&mut self.fields[i]).margin(egui::vec2(12.,10.)).desired_width(f32::INFINITY));
                        }
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.key,false,"Пароль");
                            ui.selectable_value(&mut self.key,true,"SSH-ключ");
                        });
                        let fields=if self.key {
                            vec![(4,"Путь к SSH-ключу"),(6,"Пароль ключа"),(8,"Пароль sudo · для root не нужен")]
                        } else {vec![(3,"Пароль SSH"),(8,"Пароль sudo · для root не нужен")]};
                        for (i,label) in fields {
                            ui.label(label);
                            ui.add(egui::TextEdit::singleline(&mut self.fields[i]).password(i!=4).margin(egui::vec2(12.,10.)).desired_width(f32::INFINITY));
                        }
                        ui.label("Пароли SSH не сохраняются. Административный ключ хранится в учётных данных Windows.");
                        ui.horizontal_wrapped(|ui| {
                            for (label,connect_only) in [("Установить / обновить по SSH",false),("Подключить установленную VPS",true)] {
                                if ui.button(label).clicked() {
                                    if let Err(error)=self.start(connect_only) {self.status=error.to_string();}
                                    self.clear_secrets();
                                }
                            }
                        });
                    }
                    if let Some((fingerprint,previous))=self.trust.clone() {
                        ui.separator();
                        ui.label(if previous.is_some() {"SSH-ключ изменился. Сверьте его с панелью VPS."} else {"Первое подключение. Сверьте отпечаток с панелью VPS."});
                        ui.monospace(fingerprint);
                        if let Some(previous)=previous {ui.label("Предыдущий:");ui.monospace(previous);}
                        ui.horizontal(|ui| {
                            for (label,accept) in [("Доверять и продолжить",true),("Отмена",false)] {
                                if ui.button(label).clicked() {
                                    if let Some(worker)=&mut self.worker {worker.trust(accept);}
                                    self.trust=None;
                                }
                            }
                        });
                    }
                    if self.worker.is_some()||self.updating {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            if self.worker.is_some()&&ui.button("Отменить").clicked() {
                                if let Some(worker)=&mut self.worker {worker.cancel();}
                            }
                        });
                    }
                    for step in &self.steps {ui.label(step);}
                });
            });
    }
}

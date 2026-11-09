use super::{panels::*, *};

#[derive(Clone, Default)]
pub(super) enum State {
    #[default]
    Idle,
    Waiting,
    Checking,
    Ready(String),
    Failed,
    SaveFailed,
}
pub(super) struct Editor {
    pub provider: Provider,
    pub draft: String,
    revision: u64,
    changed_at: f64,
    checked: Option<String>,
    state: State,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            provider: Provider::Yandex,
            draft: String::new(),
            revision: 0,
            changed_at: 0.,
            checked: None,
            state: State::Idle,
        }
    }
}
impl Editor {
    pub fn open(&mut self, provider: Provider, value: String, now: f64) {
        self.provider = provider;
        self.draft = value;
        self.edited(now);
    }
    fn edited(&mut self, now: f64) {
        self.revision = self.revision.wrapping_add(1);
        self.changed_at = now;
        self.checked = None;
        self.state = if self.draft.trim().is_empty() && self.provider != Provider::YouTube {
            State::Idle
        } else {
            State::Waiting
        };
    }
    pub fn retry(&mut self) {
        self.edited(-1.);
    }
    fn check(&mut self, now: f64) -> Option<Intent> {
        if (self.draft.trim().is_empty() && self.provider != Provider::YouTube)
            || now - self.changed_at < 0.6
            || self.checked.as_ref() == Some(&self.draft)
        {
            return None;
        }
        self.checked = Some(self.draft.clone());
        self.state = State::Checking;
        Some(Intent::CheckConnection(
            self.provider,
            self.draft.trim().into(),
            self.revision,
        ))
    }
}
pub(super) fn provider(label: &str) -> Provider {
    match label {
        "Last.fm" => Provider::LastFm,
        "SoundCloud" => Provider::SoundCloud,
        "Hitmo" | "Hitmotop" => Provider::Hitmotop,
        "YouTube Music" => Provider::YouTube,
        _ => Provider::Yandex,
    }
}
pub(super) fn label_for_asset(asset: &str) -> &'static str {
    match asset {
        "provider-lastfm" => "Last.fm",
        "provider-soundcloud" => "SoundCloud",
        "provider-hitmo" => "Hitmo",
        "provider-youtube" => "YouTube Music",
        _ => "Яндекс Музыка",
    }
}
impl GlassUi {
    pub fn connection_save_failed(&mut self) {
        self.connection.state = State::SaveFailed;
    }
    pub fn connection_result(
        &mut self,
        provider: Provider,
        revision: u64,
        result: Result<String, String>,
    ) -> bool {
        if provider != self.connection.provider || revision != self.connection.revision {
            return false;
        }
        self.connection.state = match result {
            Ok(message) => State::Ready(message),
            Err(_) => State::Failed,
        };
        true
    }
    pub fn connection_saved(&mut self, provider: Provider, value: &str) {
        self.connection_updated(provider, value);
        self.overlay = Some(Overlay::Sources);
    }
    pub fn connection_updated(&mut self, provider: Provider, value: &str) {
        match provider {
            Provider::Yandex => self.yandex = value.into(),
            Provider::LastFm => self.lastfm = value.into(),
            Provider::SoundCloud => self.soundcloud = value.into(),
            _ => {}
        }
    }
}

pub(super) fn panel(ui: &mut GlassUi) -> Node {
    let editor = &ui.connection;
    let provider = editor.provider;
    let youtube = provider == Provider::YouTube;
    let (mut root, mut p) = super::panels::panel(ui, provider.label(), 620., 456.);
    let mut viewport = body(ui, &p, 260.);
    let b = &mut viewport.children[0];
    let w = b.width;
    let label = match provider {
        Provider::Yandex => "OAuth-токен",
        Provider::LastFm => "API-ключ",
        Provider::SoundCloud => "OAuth access token",
        Provider::Hitmotop => "Домен Hitmo",
        Provider::YouTube => "Поиск без API-ключа",
    };
    b.children.push(text(label, 0., 0., w, 15., true));
    if !youtube {
        let mut field = frame("Credential input/0", 0., 34., w, 48.);
        field.command = Some(Control::Noop);
        b.children.push(field);
    }
    let status_y = if youtube { 44. } else { 100. };
    let (heading, detail, asset) = match &ui.connection.state {
        State::Idle => (
            "Ожидаем данные",
            "Проверка запустится после вставки".to_owned(),
            "ellipsis",
        ),
        State::Waiting => (
            "Готовимся к проверке",
            "Подождите немного…".to_owned(),
            "ellipsis",
        ),
        State::Checking => (
            "Проверяем подключение…",
            "Сохранение пока не требуется".to_owned(),
            "audio-lines",
        ),
        State::Ready(detail) => ("Подключение доступно", detail.clone(), "check"),
        State::Failed => (
            "Что-то пошло не так",
            if provider == Provider::Hitmotop {
                "Проверьте домен и доступность сайта".into()
            } else if youtube {
                "Сервис сейчас не отвечает. Можно проверить ещё раз".into()
            } else {
                "Проверьте ключ или токен и доступность сервиса".into()
            },
            "x",
        ),
        State::SaveFailed => (
            "Что-то пошло не так",
            "Не удалось сохранить. Попробуйте ещё раз".into(),
            "x",
        ),
    };
    let mut status = frame("Connection status", 0., status_y, w, 80.);
    status.corner_radius = 18.;
    status.fills = vec![fill([0.93, 0.925, 0.915], 1.)];
    status.children.push(icon(asset, 14., 18., 20.));
    status
        .children
        .push(text(heading, 46., 10., w - 60., 14., true));
    let mut detail = text(
        &detail,
        46.,
        38.,
        w - 60.,
        if w < 400. { 10. } else { 12. },
        false,
    );
    detail.opacity = 0.65;
    status.children.push(detail);
    b.children.push(status);
    b.children.push(button(
        "Проверить ещё раз",
        0.,
        status_y + 92.,
        w.min(210.),
        34.,
        Control::CheckConnection,
        false,
    ));
    let note = match provider {
        Provider::Hitmotop if w < 400. => "Только домен — пути запросов прежние.",
        Provider::Hitmotop => "Меняется только домен. Пути запросов остаются прежними.",
        Provider::YouTube => "Воспроизведение зависит от доступности записи.",
        Provider::SoundCloud => "Нужен токен доступа приложения SoundCloud.",
        _ if crate::platform::mobile() => "Ключ защищён Android Keystore.",
        _ if w < 400. => "Ключ сохранится в хранилище Windows.",
        _ => "Ключ сохраняется в учётных данных Windows.",
    };
    let mut note = text(
        note,
        0.,
        status_y + 138.,
        w,
        if w < 400. { 10. } else { 12. },
        false,
    );
    note.opacity = 0.6;
    b.children.push(note);
    p.children.push(viewport);
    let button_w = (p.width - 64.) / 2.;
    p.children.push(button(
        "К источникам",
        24.,
        p.height - 58.,
        button_w,
        36.,
        Control::Overlay(Overlay::Sources),
        false,
    ));
    p.children.push(button(
        if youtube {
            "Готово"
        } else {
            "Сохранить"
        },
        40. + button_w,
        p.height - 58.,
        button_w,
        36.,
        if youtube {
            Control::Overlay(Overlay::Sources)
        } else {
            Control::SaveConnection
        },
        true,
    ));
    root.children.push(p);
    root
}

pub(super) fn input(glass: &mut GlassUi, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
    let now = ui.input(|i| i.time);
    if let Some((rect, clip)) = glass.renderer.regions.get("Credential input/0").copied() {
        ui.set_clip_rect(clip);
        ui.painter().rect(
            rect,
            16.,
            if glass.dark {
                Color32::from_rgb(39, 36, 44)
            } else {
                Color32::from_rgb(241, 240, 238)
            },
            egui::Stroke::new(1_f32, Color32::BLACK.gamma_multiply(0.12)),
            egui::StrokeKind::Inside,
        );
        let domain = glass.connection.provider == Provider::Hitmotop;
        let id = egui::Id::new(("credential", glass.connection.provider));
        let ime_changed = crate::platform::text_before(ui.ctx(), id, &mut glass.connection.draft);
        let (rect, pasted) =
            crate::platform::paste_button(ui, rect, &mut glass.connection.draft, glass.dark);
        let response = ui.put(
            rect,
            egui::TextEdit::singleline(&mut glass.connection.draft)
                .id(egui::Id::new(("credential", glass.connection.provider)))
                .password(!domain)
                .frame(false)
                .hint_text(
                    egui::RichText::new(if domain {
                        "ru.hitmo-top.cc"
                    } else {
                        "Вставьте ключ или токен"
                    })
                    .color(Color32::from_gray(if glass.dark {
                        150
                    } else {
                        125
                    })),
                )
                .font(FontId::new(14., FontFamily::Name("medeo-medium".into())))
                .text_color(if glass.dark {
                    Color32::from_rgb(242, 240, 246)
                } else {
                    Color32::from_rgb(28, 26, 30)
                })
                .margin(vec2(12., 12.)),
        );
        crate::platform::text_after(ui.ctx(), id, &glass.connection.draft, !domain, false);
        if response.changed() || ime_changed || pasted {
            glass.connection.edited(now);
        }
    }
    if let Some(check) = glass.connection.check(now) {
        out.push(check);
    }
    if matches!(glass.connection.state, State::Waiting) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}

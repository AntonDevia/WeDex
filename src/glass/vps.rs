//! The SSH installer is available only in the Windows application.
use super::*;
use zeroize::Zeroize;

pub(super) struct Editor {
    pub(super) fields: [String; 9],
    pub(super) key: bool,
    pub(super) error: Option<String>,
}
impl Default for Editor {
    fn default() -> Self {
        let saved: serde_json::Value =
            serde_json::from_str(&crate::store::secret("vps-profile")).unwrap_or_default();
        let mut fields: [String; 9] = Default::default();
        for (i, name, fallback) in [
            (0, "host", ""),
            (1, "ssh_port", "22"),
            (2, "user", "root"),
            (5, "public_host", ""),
            (7, "port", "8765"),
            (4, "key_path", ""),
        ] {
            fields[i] = saved[name].as_str().unwrap_or(fallback).into();
        }
        Self {
            fields,
            key: false,
            error: None,
        }
    }
}
impl Editor {
    pub fn clear_secrets(&mut self) {
        for i in [3, 6, 8] {
            self.fields[i].zeroize();
            self.fields[i].clear();
        }
    }
    pub fn request(
        &mut self,
        connect_only: bool,
        name: &str,
    ) -> anyhow::Result<wedex_provision::Request> {
        let ssh_port = self.fields[1]
            .parse()
            .map_err(|_| anyhow::anyhow!("Некорректный SSH-порт"))?;
        let port = self.fields[7]
            .parse()
            .map_err(|_| anyhow::anyhow!("Некорректный порт WeDex"))?;
        let host = self.fields[0].trim().trim_matches(['[', ']']).to_owned();
        let public_host = if self.fields[5].trim().is_empty() {
            host.clone()
        } else {
            self.fields[5].trim().trim_matches(['[', ']']).into()
        };
        let request = wedex_provision::Request {
            host,
            public_host,
            ssh_port,
            port,
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
            device_name: name.into(),
            connect_only,
            previous: None,
        };
        request.validate()?;
        let profile = serde_json::json!({"host":request.host,"ssh_port":self.fields[1],"port":self.fields[7],"user":request.user,"public_host":self.fields[5],"key_path":self.fields[4]});
        #[cfg(not(test))]
        crate::store::save_secret("vps-profile", &profile.to_string())?;
        #[cfg(test)]
        let _ = profile;
        Ok(request)
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.clear_secrets();
    }
}

pub(super) fn panel(ui: &mut GlassUi) -> Node {
    let (mut root, mut p) = panels::panel(ui, "Личная VPS", 760., 840.);
    let view = ui.cloud_view.vps.clone();
    let mut viewport = panels::body(
        ui,
        &p,
        if view.busy || view.complete {
            660. + view.steps.len() as f32 * 30.
        } else {
            if ui.vps_editor.key {
                980.
            } else {
                904.
            }
        },
    );
    let b = &mut viewport.children[0];
    let w = b.width;
    if let Some(fingerprint) = &view.fingerprint {
        b.children.push(panels::text(
            if view.previous.is_some() {
                "SSH-ключ сервера изменился"
            } else {
                "Первое подключение к серверу"
            },
            0.,
            0.,
            w,
            20.,
            true,
        ));
        b.children.push(panels::text(
            "Сверьте отпечаток с консолью или панелью вашей VPS.",
            0.,
            44.,
            w,
            13.,
            false,
        ));
        b.children
            .push(panels::text(fingerprint, 0., 88., w, 14., true));
        if let Some(previous) = &view.previous {
            b.children.push(panels::text(
                "Ранее сохранённый ключ:",
                0.,
                132.,
                w,
                12.,
                false,
            ));
            b.children
                .push(panels::text(previous, 0., 158., w, 12., false));
        }
        b.children.push(panels::button(
            if view.previous.is_some() {
                "Подтвердить новый ключ и продолжить"
            } else {
                "Доверять серверу и продолжить"
            },
            0.,
            220.,
            w,
            44.,
            Control::Cloud(crate::cloud::Action::ProvisionTrust(true)),
            true,
        ));
        b.children.push(panels::button(
            "Отмена",
            0.,
            280.,
            w,
            42.,
            Control::Cloud(crate::cloud::Action::ProvisionCancel),
            false,
        ));
    } else if view.busy || view.complete {
        b.children.push(panels::text(
            if view.complete {
                "VPS готова"
            } else {
                "Настраиваем сервер"
            },
            0.,
            0.,
            w,
            22.,
            true,
        ));
        for (i, step) in view.steps.iter().enumerate() {
            b.children.push(panels::text(
                step,
                0.,
                50. + i as f32 * 32.,
                w,
                14.,
                i + 1 == view.steps.len(),
            ));
        }
        let y = 80. + view.steps.len() as f32 * 32.;
        if view.complete {
            b.children.push(panels::text(
                "Телефон подключается по приглашению WeDex.",
                0.,
                y,
                w,
                14.,
                false,
            ));
            b.children.push(panels::button(
                "К облаку и приглашению телефона",
                0.,
                y + 50.,
                w,
                44.,
                Control::Overlay(Overlay::Cloud),
                true,
            ));
            b.children.push(panels::button(
                "Повторить настройку",
                0.,
                y + 108.,
                w,
                42.,
                Control::Cloud(crate::cloud::Action::ProvisionReset),
                false,
            ));
        } else {
            b.children.push(panels::button(
                "Отменить",
                0.,
                y,
                w,
                42.,
                Control::Cloud(crate::cloud::Action::ProvisionCancel),
                false,
            ));
            b.children.push(panels::text(
                "Отмена сохраняет данные и уже выполненные шаги.",
                0.,
                y + 58.,
                w,
                12.,
                false,
            ));
        }
    } else {
        b.children.push(panels::text(
            "Debian 12/13 · Ubuntu 22.04/24.04 · x86_64 · systemd",
            0.,
            0.,
            w,
            12.,
            false,
        ));
        let fields = [
            (0, "IP или домен SSH", 36.),
            (1, "SSH-порт", 112.),
            (7, "Порт WeDex", 112.),
            (2, "SSH-логин", 188.),
        ];
        for (i, label, y) in fields {
            field(
                b,
                i,
                label,
                if i == 7 { (w + 12.) / 2. } else { 0. },
                y,
                if i == 1 || i == 7 { (w - 12.) / 2. } else { w },
            );
        }
        let half = (w - 12.) / 2.;
        b.children.push(panels::button(
            "Пароль",
            0.,
            270.,
            half,
            40.,
            Control::VpsAuth(false),
            !ui.vps_editor.key,
        ));
        b.children.push(panels::button(
            "SSH-ключ",
            half + 12.,
            270.,
            half,
            40.,
            Control::VpsAuth(true),
            ui.vps_editor.key,
        ));
        if ui.vps_editor.key {
            field(b, 4, "Путь к закрытому SSH-ключу", 0., 330., w);
            field(b, 6, "Пароль ключа · если ключ зашифрован", 0., 406., w);
        } else {
            field(b, 3, "Пароль SSH", 0., 330., w);
        }
        let compact = if ui.vps_editor.key { 0. } else { -76. };
        field(
            b,
            8,
            "Пароль sudo · для root не нужен",
            0.,
            482. + compact,
            w,
        );
        field(
            b,
            5,
            "Публичный IP или домен · пусто = адрес SSH",
            0.,
            558. + compact,
            w,
        );
        b.children.push(panels::text(
            "Пароли используются только для этой операции и не сохраняются.",
            0.,
            640. + compact,
            w,
            12.,
            false,
        ));
        b.children.push(panels::button(
            "Установить или восстановить",
            0.,
            680. + compact,
            w,
            44.,
            Control::VpsStart(false),
            true,
        ));
        b.children.push(panels::button(
            "Подключить уже установленную VPS",
            0.,
            736. + compact,
            w,
            42.,
            Control::VpsStart(true),
            false,
        ));
        if let Some(error) = ui.vps_editor.error.as_ref().or(view.error.as_ref()) {
            let mut text = panels::text(error, 0., 798. + compact, w, 12., false);
            text.height = 150.;
            b.children.push(text);
        }
    }
    p.children.push(viewport);
    root.children.push(p);
    root
}
fn field(body: &mut Node, index: usize, label: &str, x: f32, y: f32, w: f32) {
    body.children
        .push(panels::text(label, x + 4., y, w, 12., false));
    let mut input = panels::frame(&format!("VPS input/{index}"), x, y + 24., w, 42.);
    input.corner_radius = 14.;
    input.command = Some(Control::Noop);
    body.children.push(input);
}
pub(super) fn input(glass: &mut GlassUi, ui: &mut egui::Ui) {
    for i in 0..9 {
        let Some((rect, clip)) = glass
            .renderer
            .regions
            .get(&format!("VPS input/{i}"))
            .copied()
        else {
            continue;
        };
        ui.set_clip_rect(clip);
        ui.painter().rect_filled(
            rect,
            14.,
            if glass.dark {
                Color32::from_rgb(39, 36, 44)
            } else {
                Color32::from_rgb(241, 240, 238)
            },
        );
        let secret = matches!(i, 3 | 6 | 8);
        ui.put(
            rect,
            egui::TextEdit::singleline(&mut glass.vps_editor.fields[i])
                .id(egui::Id::new(("vps-field", i)))
                .password(secret)
                .frame(false)
                .char_limit(4096)
                .font(FontId::new(14., FontFamily::Name("medeo-medium".into())))
                .text_color(if glass.dark {
                    Color32::from_rgb(242, 240, 246)
                } else {
                    Color32::from_rgb(28, 26, 30)
                })
                .margin(vec2(12., 12.)),
        );
    }
}

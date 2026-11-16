use super::*;
use crate::cloud::Action;
#[derive(Default)]
pub(super) struct Editor {
    pub name: String,
    pub invitation: String,
    pub server: String,
    pub username: String,
    pub password: String,
    pub copy: bool,
}
impl Editor {
    pub fn clear_password(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
        self.password.clear();
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.clear_password();
    }
}
fn action(action: Action) -> Control {
    Control::Cloud(action)
}
fn field(body: &mut Node, index: usize, label: &str, y: f32, w: f32) {
    body.children
        .push(panels::text(label, 4., y, w, 12., false));
    let mut n = panels::frame(&format!("Cloud input/{index}"), 0., y + 24., w, 42.);
    n.corner_radius = 14.;
    n.command = Some(Control::Noop);
    body.children.push(n);
}
pub(super) fn panel(ui: &mut GlassUi) -> Node {
    let view = ui.cloud_view.clone();
    if ui.cloud_editor.name.is_empty() {
        ui.cloud_editor.name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| {
            if crate::platform::mobile() {
                "Мой телефон"
            } else {
                "Мой компьютер"
            }
            .into()
        });
    }
    if ui.cloud_editor.server.is_empty() {
        ui.cloud_editor.server = view.endpoint.clone();
    }
    let account_form = !view.connected || view.account_name.is_empty();
    let (mut root, mut p) = panels::panel(ui, "Моё облако", 760., 820.);
    let mut viewport = panels::body(
        ui,
        &p,
        if account_form {
            1030.
        } else {
            430. + view.devices.len() as f32 * 62.
        },
    );
    let b = &mut viewport.children[0];
    let w = b.width;
    b.children
        .push(panels::text(&view.status, 4., 0., w, 15., true));
    field(b, 0, "Имя устройства", 42., w);
    let mut y = 126.;
    if account_form {
        field(b, 2, "Адрес VPS · IP или домен и порт", y, w);
        y += 78.;
        b.children.push(panels::button(
            "Проверить сервер",
            0.,
            y,
            w,
            42.,
            if view.busy {
                Control::Noop
            } else {
                Control::CloudDiscover
            },
            false,
        ));
        y += 58.;
        if let Some(identity) = &view.server_identity {
            if !view.server_trusted {
                let mut fingerprint = panels::text(
                    &format!(
                        "Отпечаток SHA-256\n{}",
                        identity.fingerprint().unwrap_or_default()
                    ),
                    4.,
                    y,
                    w,
                    12.,
                    false,
                );
                fingerprint.height = 94.;
                b.children.push(fingerprint);
                y += 102.;
                b.children.push(panels::button(
                    "Доверять этому серверу",
                    0.,
                    y,
                    w,
                    42.,
                    action(Action::TrustServer),
                    true,
                ));
                y += 58.;
            }
        }
        if view.server_trusted {
            field(b, 3, "Логин аккаунта", y, w);
            y += 78.;
            field(b, 4, "Пароль · не менее 10 символов", y, w);
            y += 78.;
            let half = (w - 12.) / 2.;
            b.children.push(panels::button(
                "Войти",
                0.,
                y,
                half,
                44.,
                if view.busy {
                    Control::Noop
                } else {
                    Control::CloudAccount(false)
                },
                true,
            ));
            b.children.push(panels::button(
                "Создать аккаунт",
                half + 12.,
                y,
                half,
                44.,
                if view.busy {
                    Control::Noop
                } else {
                    Control::CloudAccount(true)
                },
                false,
            ));
            y += 68.;
        }
        b.children.push(panels::text(
            "Или привяжите устройство приглашением WeDex",
            4.,
            y,
            w,
            13.,
            true,
        ));
        y += 34.;
        field(b, 1, "Приглашение WeDex", y, w);
        y += 78.;
        b.children.push(panels::button(
            "Подключить по приглашению",
            0.,
            y,
            w,
            44.,
            if view.busy {
                Control::Noop
            } else {
                Control::CloudConnect
            },
            false,
        ));
        y += 64.;
    } else {
        b.children.push(panels::text(
            &format!("Аккаунт · {}", view.account_name),
            4.,
            y,
            w,
            18.,
            true,
        ));
        y += 38.;
        b.children.push(panels::text(
            &format!("{} · сервер {}", view.endpoint, view.server_version),
            4.,
            y,
            w,
            12.,
            false,
        ));
        y += 42.;
    }
    if view.connected {
        let half = (w - 12.) / 2.;
        b.children.push(panels::button(
            "Пригласить WeDex",
            0.,
            y,
            half,
            42.,
            action(Action::Invite),
            true,
        ));
        b.children.push(panels::button(
            "Отключиться",
            half + 12.,
            y,
            half,
            42.,
            action(Action::Disconnect),
            false,
        ));
        y += 56.;
        if !view.invitation.is_empty() {
            b.children.push(panels::button(
                "Копировать приглашение · 10 минут",
                0.,
                y,
                w,
                42.,
                Control::CloudCopy,
                false,
            ));
            y += 58.;
        }
        b.children.push(panels::text(
            &format!(
                "Прослушано {} мин · {} сессий",
                view.statistics.listened_ms / 60000,
                view.statistics.sessions
            ),
            4.,
            y,
            w,
            13.,
            false,
        ));
        y += 48.;
        b.children
            .push(panels::text("Мои устройства", 4., y, w, 16., true));
        y += 36.;
        for device in &view.devices {
            let own = device.id == view.own_device;
            b.children.push(panels::text(
                &format!(
                    "{} · {}",
                    device.name,
                    if own {
                        "это устройство"
                    } else if device.online {
                        "на связи"
                    } else {
                        "не в сети"
                    }
                ),
                4.,
                y + 12.,
                if own { w } else { w - 100. },
                13.,
                false,
            ));
            if !own {
                b.children.push(panels::button(
                    "Отозвать",
                    w - 96.,
                    y,
                    96.,
                    42.,
                    action(Action::Revoke(device.id.clone())),
                    false,
                ));
            }
            y += 58.;
        }
    }
    viewport.children[0].height = y + 24.;
    p.children.push(viewport);
    root.children.push(p);
    root
}
pub(super) fn devices_panel(ui: &mut GlassUi) -> Node {
    let (mut root, mut p) = panels::panel(ui, "Где играет музыка", 620., 580.);
    let view = ui.cloud_view.clone();
    let mut viewport = panels::body(ui, &p, 110. + view.devices.len() as f32 * 86.);
    let b = &mut viewport.children[0];
    let w = b.width;
    b.children.push(panels::text(
        "Плеер управляет выбранным устройством.",
        0.,
        0.,
        w,
        13.,
        false,
    ));
    let mut y = 42.;
    let mut own = panels::button(
        "Этот телефон",
        0.,
        y,
        w,
        60.,
        action(Action::Target(view.own_device.clone())),
        view.selected.is_empty() || view.selected == view.own_device,
    );
    own.name = "Player device/local".into();
    b.children.push(own);
    y += 76.;
    for device in view
        .devices
        .iter()
        .filter(|d| d.can_play && d.id != view.own_device)
    {
        let label = format!(
            "{} · {}",
            device.name,
            if device.online && view.connected {
                "на связи"
            } else {
                "не в сети"
            }
        );
        let mut row = panels::button(
            &label,
            0.,
            y,
            w,
            60.,
            action(Action::Target(device.id.clone())),
            view.selected == device.id,
        );
        row.name = format!("Player device/{}", device.id);
        if !device.online || !view.connected {
            row.opacity = 0.55;
        }
        b.children.push(row);
        y += 76.;
    }
    if !view.connected {
        b.children.push(panels::button(
            "Подключить устройство",
            0.,
            y,
            w,
            44.,
            Control::Overlay(Overlay::Cloud),
            false,
        ));
    }
    p.children.push(viewport);
    root.children.push(p);
    root
}
pub(super) fn input(glass: &mut GlassUi, ui: &mut egui::Ui) {
    if std::mem::take(&mut glass.cloud_editor.copy) {
        ui.ctx().copy_text(glass.cloud_view.invitation.clone());
    }
    for index in 0..5 {
        let Some((rect, clip)) = glass
            .renderer
            .regions
            .get(&format!("Cloud input/{index}"))
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
        let draft = match index {
            0 => &mut glass.cloud_editor.name,
            1 => &mut glass.cloud_editor.invitation,
            2 => &mut glass.cloud_editor.server,
            3 => &mut glass.cloud_editor.username,
            _ => &mut glass.cloud_editor.password,
        };
        let id = egui::Id::new(("cloud-field", index));
        crate::platform::text_before(ui.ctx(), id, draft);
        let rect = if matches!(index, 1 | 4) {
            crate::platform::paste_button(ui, rect, draft, glass.dark).0
        } else {
            rect
        };
        ui.put(
            rect,
            egui::TextEdit::singleline(draft)
                .id(id)
                .password(matches!(index, 1 | 4))
                .frame(false)
                .char_limit(match index {
                    0 => 60,
                    1 => 32768,
                    2 => 512,
                    3 => 64,
                    _ => 256,
                })
                .font(FontId::new(14., FontFamily::Name("medeo-medium".into())))
                .text_color(if glass.dark {
                    Color32::from_rgb(242, 240, 246)
                } else {
                    Color32::from_rgb(28, 26, 30)
                })
                .margin(vec2(12., 12.)),
        );
        crate::platform::text_after(ui.ctx(), id, draft, matches!(index, 1 | 4), false);
    }
}

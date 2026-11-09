use super::*;
use crate::cloud::Action;
use wedex_cloud::Action as Remote;

pub(super) struct Editor {
    pub name: String,
    pub invitation: String,
    pub lan: bool,
    pub copy: Option<bool>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| {
                if crate::platform::mobile() {
                    "Мой телефон".into()
                } else {
                    "Мой компьютер".into()
                }
            }),
            invitation: String::new(),
            lan: false,
            copy: None,
        }
    }
}
fn action(action: Action) -> Control {
    Control::Cloud(action)
}
pub(super) fn panel(ui: &mut GlassUi) -> Node {
    let (mut root, mut p) = panels::panel(ui, "Моё облако", 760., 820.);
    let view = ui.cloud_view.clone();
    let shift = if crate::platform::mobile() {
        -358.
    } else if !view.server_running && !view.owner_access {
        -144.
    } else if view.invitation.is_empty() && view.browser_url.is_empty() {
        -76.
    } else {
        0.
    };
    let controls = view
        .devices
        .iter()
        .any(|d| d.id == view.selected && d.online);
    let mut viewport = panels::body(
        ui,
        &p,
        if crate::platform::mobile() {
            420.
        } else {
            (if controls { 1090. } else { 830. })
                + shift
                + view.devices.len() as f32 * 56.
                + if cfg!(windows) { 64. } else { 0. }
        },
    );
    let body = &mut viewport.children[0];
    let w = body.width;
    body.children
        .push(panels::text(&view.status, 4., 0., w, 16., true));
    body.children
        .push(panels::text("Локальный сервер", 4., 42., w, 12., true));
    body.children.push(panels::text(
        "Пульт и статистика на вашем компьютере.",
        4.,
        66.,
        w,
        13.,
        false,
    ));
    body.children.push(panels::button(
        if view.busy {
            "Подождите…"
        } else if view.server_running {
            "Остановить сервер"
        } else {
            "Запустить сервер"
        },
        0.,
        100.,
        w,
        42.,
        if view.busy {
            Control::Noop
        } else {
            action(if view.server_running {
                Action::Stop
            } else {
                Action::Start {
                    lan: ui.cloud_editor.lan,
                }
            })
        },
        !view.server_running,
    ));
    body.children.push(panels::button(
        "Доступ из домашней сети",
        0.,
        154.,
        w,
        38.,
        if view.server_running || view.busy {
            Control::Noop
        } else {
            Control::CloudLan
        },
        ui.cloud_editor.lan,
    ));
    body.children.push(panels::text(
        if w < 460. && ui.cloud_editor.lan {
            "Телефон: домашний Wi-Fi · порт 8765."
        } else if w < 460. {
            "Сервер работает, пока открыт WeDex."
        } else if ui.cloud_editor.lan {
            "Для телефона: та же Wi-Fi сеть и порт 8765 в частной сети Windows."
        } else {
            "Доступ только с этого ПК. Сервер работает, пока открыт WeDex."
        },
        4.,
        202.,
        w,
        11.,
        false,
    ));
    if !view.endpoint.is_empty() {
        body.children.push(panels::text(
            view.lan_endpoint.as_deref().unwrap_or(&view.endpoint),
            4.,
            230.,
            w,
            13.,
            true,
        ));
    }
    let half = (w - 8.) / 2.;
    if view.server_running || view.owner_access {
        for (index, (label, browser)) in [("Пригласить WeDex", false), ("Пригласить браузер", true)]
            .into_iter()
            .enumerate()
        {
            body.children.push(panels::button(
                label,
                index as f32 * (half + 8.),
                262.,
                half,
                38.,
                if view.server_running || view.owner_access {
                    action(Action::Invite { browser })
                } else {
                    Control::Noop
                },
                false,
            ));
        }
    }
    if !view.invitation.is_empty() || !view.browser_url.is_empty() {
        body.children.push(panels::text(
            "Одноразовое приглашение · действует 10 минут",
            4.,
            310.,
            w,
            12.,
            false,
        ));
        let browser = !view.browser_url.is_empty();
        body.children.push(panels::button(
            "Копировать приглашение",
            0.,
            340.,
            half,
            38.,
            Control::CloudCopy(browser),
            false,
        ));
        body.children.push(panels::button(
            "Открыть пульт",
            half + 8.,
            340.,
            half,
            38.,
            if browser {
                Control::Open(view.browser_url.clone())
            } else {
                Control::Noop
            },
            false,
        ));
    }
    if crate::platform::mobile() {
        body.children.truncate(1);
    }
    body.children.push(panels::text(
        if crate::platform::mobile() {
            "Подключить моё облако"
        } else if w < 460. {
            "Подключить другой WeDex"
        } else {
            "Подключить этот WeDex к другому серверу"
        },
        4.,
        406. + shift,
        w,
        15.,
        true,
    ));
    for (index, (label, y)) in [("Имя устройства", 442.), ("Приглашение WeDex", 520.)]
        .into_iter()
        .enumerate()
    {
        let y = y + shift;
        body.children
            .push(panels::text(label, 4., y, w, 12., false));
        let mut input = panels::frame(&format!("Cloud input/{index}"), 0., y + 22., w, 42.);
        input.corner_radius = 14.;
        input.command = Some(Control::Noop);
        body.children.push(input);
    }
    body.children.push(panels::button(
        "Подключиться",
        0.,
        600. + shift,
        half,
        40.,
        if view.busy {
            Control::Noop
        } else {
            Control::CloudConnect
        },
        true,
    ));
    body.children.push(panels::button(
        "Отключить",
        half + 8.,
        600. + shift,
        half,
        40.,
        action(Action::Disconnect),
        false,
    ));
    if crate::platform::mobile() {
        body.children.push(panels::text(
            "Выбор устройства — в плеере, кнопка наушников.",
            4.,
            310.,
            w,
            12.,
            false,
        ));
        let mut status = panels::text(&view.sync_status, 4., 354., w, 12., false);
        status.height = 54.;
        body.children.push(status);
        p.children.push(viewport);
        root.children.push(p);
        return root;
    }
    body.children.push(panels::text(
        "Статистика прослушивания",
        4.,
        674. + shift,
        w,
        15.,
        true,
    ));
    body.children.push(panels::text(
        &format!(
            "{}: {} мин · Облако: {} мин · Сессий: {}",
            if crate::platform::mobile() {
                "Телефон"
            } else {
                "ПК"
            },
            view.local_ms / 60000,
            view.statistics.listened_ms / 60000,
            view.statistics.sessions
        ),
        4.,
        702. + shift,
        w,
        12.,
        false,
    ));
    body.children.push(panels::text(
        if w < 460. {
            "Устройства"
        } else {
            "Устройства · выберите, где управлять музыкой"
        },
        4.,
        752. + shift,
        w,
        14.,
        true,
    ));
    let mut y = 788. + shift;
    if view.devices.is_empty() {
        body.children.push(panels::text(
            "Подключённые устройства появятся здесь.",
            4.,
            y,
            w,
            12.,
            false,
        ));
    }
    for device in &view.devices {
        let label = format!(
            "{} · {}{}",
            device.name,
            if device.online {
                "на связи"
            } else {
                "не в сети"
            },
            if device.id == view.own_device {
                if crate::platform::mobile() {
                    " · этот телефон"
                } else {
                    " · этот ПК"
                }
            } else if !device.can_play {
                " · пульт"
            } else {
                ""
            }
        );
        let can_revoke = (view.server_running || view.owner_access) && device.id != view.own_device;
        body.children.push(panels::button(
            &label,
            0.,
            y,
            if can_revoke { w - 100. } else { w },
            42.,
            if device.can_play {
                action(Action::Target(device.id.clone()))
            } else {
                Control::Noop
            },
            device.id == view.selected,
        ));
        if can_revoke {
            body.children.push(panels::button(
                "Отозвать",
                w - 92.,
                y,
                92.,
                42.,
                action(Action::Revoke(device.id.clone())),
                false,
            ));
        }
        y += 56.;
    }
    if let Some(device) = view
        .devices
        .iter()
        .find(|d| d.id == view.selected && d.online)
    {
        let playing = &device.playback;
        if let Some(track) = &playing.track {
            body.children
                .push(panels::text(&track.title, 4., y, w, 18., true));
            body.children
                .push(panels::text(&track.artist, 4., y + 30., w, 13., false));
        }
        y += 64.;
        let start = (w - 176.) / 2.;
        for (index, (icon, command, selected)) in [
            ("skip-back", Remote::Previous, false),
            (
                if playing.playing { "pause" } else { "play" },
                Remote::SetPlaying {
                    playing: !playing.playing,
                },
                true,
            ),
            ("skip-forward", Remote::Next, false),
        ]
        .into_iter()
        .enumerate()
        {
            body.children.push(panels::icon_button(
                icon,
                start + index as f32 * 64.,
                y,
                48.,
                action(Action::Remote(command)),
                selected,
            ));
        }
        y += 62.;
        body.children.push(panels::text(
            &format!(
                "{}:{:02} / {}:{:02}",
                playing.position as u64 / 60,
                playing.position as u64 % 60,
                playing.duration as u64 / 60,
                playing.duration as u64 % 60
            ),
            4.,
            y,
            w,
            12.,
            false,
        ));
        body.children.push(panels::button(
            "Перемешать",
            0.,
            y + 30.,
            half,
            38.,
            action(Action::Remote(Remote::Shuffle {
                enabled: !playing.shuffle,
            })),
            playing.shuffle,
        ));
        body.children.push(panels::button(
            "Повтор",
            half + 8.,
            y + 30.,
            half,
            38.,
            action(Action::Remote(Remote::Repeat {
                enabled: !playing.repeat,
            })),
            playing.repeat,
        ));
    }
    #[cfg(windows)]
    {
        for node in body.children.iter_mut().skip(1) {
            node.move_to(node.x, node.y + 64.);
        }
        body.children.push(panels::button(
            "Настроить VPS по SSH",
            0.,
            38.,
            w,
            44.,
            Control::Overlay(Overlay::Vps),
            true,
        ));
    }
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
    if let Some(browser) = glass.cloud_editor.copy.take() {
        ui.ctx().copy_text(if browser {
            glass.cloud_view.browser_url.clone()
        } else {
            glass.cloud_view.invitation.clone()
        });
    }
    for index in 0..2 {
        if let Some((rect, clip)) = glass
            .renderer
            .regions
            .get(&format!("Cloud input/{index}"))
            .copied()
        {
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
            let draft = if index == 0 {
                &mut glass.cloud_editor.name
            } else {
                &mut glass.cloud_editor.invitation
            };
            let id = egui::Id::new(("cloud-field", index));
            crate::platform::text_before(ui.ctx(), id, draft);
            let rect = if index == 1 {
                crate::platform::paste_button(ui, rect, draft, glass.dark).0
            } else {
                rect
            };
            ui.put(
                rect,
                egui::TextEdit::singleline(draft)
                    .id(egui::Id::new(("cloud-field", index)))
                    .password(index == 1)
                    .frame(false)
                    .char_limit(if index == 0 { 60 } else { 32768 })
                    .hint_text(if index == 0 {
                        "Мой компьютер"
                    } else {
                        "wedex-cloud:…"
                    })
                    .font(FontId::new(14., FontFamily::Name("medeo-medium".into())))
                    .text_color(if glass.dark {
                        Color32::from_rgb(242, 240, 246)
                    } else {
                        Color32::from_rgb(28, 26, 30)
                    })
                    .margin(vec2(12., 12.)),
            );
            crate::platform::text_after(ui.ctx(), id, draft, index == 1, false);
        }
    }
}

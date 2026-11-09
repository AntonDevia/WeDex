use super::*;
pub(super) fn panel(ui: &mut GlassUi, m: &Model) -> Node {
    let (mut root, mut p) = panels::panel(ui, "Обновления", 650., 620.);
    let mut view = panels::body(ui, &p, 530.);
    let b = &mut view.children[0];
    let w = b.width;
    let s = &m.updates;
    b.children
        .push(panels::text(&s.message, 0., 0., w, 16., true));
    let mut y = 52.;
    if let Some(release) = &s.app {
        b.children.push(panels::text(
            &format!("WeDex {}", release.version),
            0.,
            y,
            w,
            22.,
            true,
        ));
        y += 40.;
        let mut notes = panels::text(&release.notes, 0., y, w, 13., false);
        notes.height = 120.;
        b.children.push(notes);
        y += 140.;
        if let Some(progress) = s.progress {
            b.children.push(panels::text(
                &format!("Скачиваем · {:.0}%", progress * 100.),
                0.,
                y,
                w,
                16.,
                true,
            ));
        } else {
            b.children.push(panels::button(
                if s.ready.is_some() {
                    "Установить обновление"
                } else {
                    "Скачать обновление"
                },
                0.,
                y,
                w,
                44.,
                Control::Update(if s.ready.is_some() {
                    crate::updates::Action::Apply
                } else {
                    crate::updates::Action::Download
                }),
                true,
            ));
        }
        y += 68.;
    }
    #[cfg(windows)]
    if let Some(release) = &s.server {
        b.children.push(panels::text(
            &format!("Сервер VPS · {}", release.version),
            0.,
            y,
            w,
            20.,
            true,
        ));
        y += 40.;
        b.children.push(panels::button(
            if ui.cloud_view.server_updating {
                "Обновляем VPS…"
            } else {
                "Обновить VPS"
            },
            0.,
            y,
            w,
            44.,
            if ui.cloud_view.server_updating {
                Control::Noop
            } else {
                Control::Update(crate::updates::Action::Server)
            },
            true,
        ));
        y += 62.;
    }
    b.children.push(panels::button(
        "Проверить обновления",
        0.,
        y,
        w,
        42.,
        if s.busy {
            Control::Noop
        } else {
            Control::Update(crate::updates::Action::Check)
        },
        false,
    ));
    p.children.push(view);
    root.children.push(p);
    root
}

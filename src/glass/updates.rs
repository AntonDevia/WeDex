use super::*;
use panels::*;
fn notes_text(raw: &str) -> String {
    raw.lines()
        .enumerate()
        .filter(|(i, line)| *i != 0 || !line.starts_with("# "))
        .map(|(_, line)| {
            let line = line.trim_end();
            if let Some(t) = line.trim_start().strip_prefix("- ") {
                format!("• {t}")
            } else {
                line.trim_start_matches('#').trim_start().replace("**", "")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub(super) fn panel(ui: &mut GlassUi, ctx: &egui::Context, m: &Model) -> Node {
    let (mut root, mut p) = panels::panel(ui, "Обновления", 650., 760.);
    let s = &m.updates;
    let w = p.width - 48.;
    let notes = s
        .app
        .as_ref()
        .map(|r| notes_text(&r.notes))
        .unwrap_or_else(|| {
            notes_text(include_str!(concat!(
                "../../docs/RELEASE_",
                env!("CARGO_PKG_VERSION"),
                ".md"
            )))
        });
    let mut job = egui::text::LayoutJob::simple(
        notes.clone(),
        FontId::new(26., FontFamily::Name("medeo-medium".into())),
        Color32::BLACK,
        w * 2. + 8.,
    );
    job.sections[0].format.line_height = Some(39.);
    let notes_height = ctx.fonts(|f| f.layout_job(job).size().y * 0.5).max(24.);
    let server = s
        .server
        .as_ref()
        .filter(|_| ui.cloud_view.can_update_server || ui.cloud_view.owner_access);
    let footer_rows = usize::from(s.app.is_some()) + usize::from(server.is_some());
    let reserve = footer_rows as f32 * 56.;
    let mut viewport_panel = p.clone();
    viewport_panel.height -= reserve;
    let total = notes_height + 166.;
    let mut view = body(ui, &viewport_panel, total);
    let b = &mut view.children[0];
    if !s.busy {
        b.children.push(text(&s.message, 0., 0., w, 14., true));
    }
    {
        b.children.push(text(
            &format!(
                "WeDex {}",
                s.app
                    .as_ref()
                    .map(|r| r.version.as_str())
                    .unwrap_or(env!("CARGO_PKG_VERSION"))
            ),
            0.,
            44.,
            w,
            22.,
            true,
        ));
        let mut n = text(&notes, 0., 86., w, 13., false);
        n.name = "Release notes".into();
        n.height = notes_height;
        n.line_height = serde_json::json!({"unit":"PIXELS","value":19.5});
        b.children.push(n);
    }
    let check_y = notes_height + 110.;
    b.children.push(button(
        "Проверить обновления",
        0.,
        check_y,
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
    let mut y = p.height - reserve - 12.;
    if s.app.is_some() {
        let ready = s.ready.is_some();
        let mut button = button(
            if ready {
                "Установить обновление"
            } else {
                "Скачать обновление"
            },
            24.,
            y,
            w,
            44.,
            if s.progress.is_some() {
                Control::Noop
            } else {
                Control::Update(if ready {
                    crate::updates::Action::Apply
                } else {
                    crate::updates::Action::Download
                })
            },
            true,
        );
        button.name = "Install application update".into();
        if let Some(progress) = s.progress {
            button.children.clear();
            let mut rail = frame(
                "Download progress",
                12.,
                20.,
                (w - 24.) * progress.clamp(0., 1.),
                4.,
            );
            rail.corner_radius = 2.;
            rail.fills = vec![fill([0.2, 0.25, 0.1], 0.7)];
            button.children.push(rail);
        }
        p.children.push(button);
        y += 56.;
    }
    if let Some(release) = server {
        p.children.push(button(
            &format!("Обновить VPS · {}", release.version),
            24.,
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
    }
    root.children.push(p);
    root
}

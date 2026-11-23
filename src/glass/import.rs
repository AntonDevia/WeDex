use super::{panels, Control, GlassUi, Model, Node};
use crate::track_import::{Input, Report, Source, View};
use eframe::egui::{self, Color32, RichText};
use std::sync::atomic::Ordering::Relaxed;

#[derive(Default)]
pub(super) struct Editor {
    pub(super) source: Source,
    pub(super) text: String,
    pub(super) help: bool,
    pub(super) copy_bookmarklet: bool,
    browser_payload: Option<String>,
}

pub(super) fn compose(glass: &mut GlassUi, model: &Model) -> Node {
    use panels::{button, frame, text};

    let (mut root, mut panel) = panels::panel(glass, "Импорт треков", 550., 466.);
    let w = panel.width - 48.;
    glass.dialog_scroll_max = 0.;
    if glass.import_editor.help {
        browser_help(glass, &mut panel);
        root.children.push(panel);
        return root;
    }
    match &model.track_import {
        View::Finished(report) => {
            report_panel(glass, &mut panel, report);
            panel.children.push(button(
                "Новый импорт",
                24.,
                panel.height - 66.,
                w,
                44.,
                Control::CancelImport,
                true,
            ));
        }
        View::Working(progress) => {
            let done = progress.done.load(Relaxed);
            let total = progress.total.load(Relaxed);
            let message = if total == 0 {
                "Читаем список треков…".into()
            } else {
                format!("Ищем в WeDex · {} из {total}", done.min(total))
            };
            panel.children.push(text(&message, 24., 110., w, 16., true));
            if total != 0 {
                let mut bar = frame("Ход поиска", 24., 160., w, 8.);
                bar.corner_radius = 4.;
                bar.fills = vec![panels::fill([0.52, 0.50, 0.55], 0.28)];
                let mut fill = frame(
                    "Проверено",
                    0.,
                    0.,
                    w * (done.min(total) as f32 / total as f32),
                    8.,
                );
                fill.corner_radius = 4.;
                fill.fills = vec![panels::fill([0.91, 1., 0.64], 1.)];
                bar.children.push(fill);
                panel.children.push(bar);
            }
            panel.children.push(button(
                "Отменить",
                24.,
                panel.height - 66.,
                w,
                44.,
                Control::CancelImport,
                false,
            ));
        }
        View::Picking => {
            panel
                .children
                .push(text("Открываем файл…", 24., 110., w, 16., true));
        }
        View::Idle | View::Error(_) => {
            let source = glass.import_editor.source;
            let tab_width = (w - 8.) / 3.;
            for (index, tab) in Source::ALL.into_iter().enumerate() {
                panel.children.push(button(
                    tab.label(),
                    24. + index as f32 * (tab_width + 4.),
                    73.,
                    tab_width,
                    38.,
                    Control::ImportSource(tab),
                    source == tab,
                ));
            }
            let (hint, extra) = match source {
                Source::Spotify => (
                    "Вставьте ссылку на плейлист Spotify. Если список закрыт, скопируйте треки из браузера.",
                    Some(("Скопировать из браузера", Control::ImportBrowserHelp)),
                ),
                Source::Yandex => (
                    "Откройте «Мне нравится» или другой плейлист Яндекс Музыки и вставьте ссылку из меню «Поделиться».",
                    model.connected[0].then_some(("Из понравившихся Яндекса", Control::ImportLikes)),
                ),
                Source::Vk => (
                    "Вставьте ссылку на профиль VK с открытыми аудио или на плейлист. Можно скопировать треки из браузера.",
                    Some(("Скопировать из браузера", Control::ImportBrowserHelp)),
                ),
            };
            let (message, size) = match &model.track_import {
                View::Error(error) => (error.as_str(), 13.),
                _ => (hint, 14.),
            };
            let mut description = text(message, 24., 124., w, size, false);
            description.name = "Import hint".into();
            description.height = 59.;
            panel.children.push(description);
            if let Some((label, control)) = extra {
                let width = w.min(244.);
                panel
                    .children
                    .push(button(label, 24., 185., width, 34., control, false));
            }
            let mut field = frame("Import text", 24., 231., w, panel.height - 306.);
            field.fills = vec![panels::fill([0.45, 0.43, 0.48], 0.13)];
            field.corner_radius = 16.;
            panel.children.push(field);
            let left = (w - 8.) / 2.;
            panel.children.push(button(
                "Выбрать файл",
                24.,
                panel.height - 66.,
                left,
                44.,
                Control::PickImportFile,
                false,
            ));
            let has_text = !glass.import_editor.text.trim().is_empty();
            panel.children.push(button(
                "Найти треки",
                32. + left,
                panel.height - 66.,
                left,
                44.,
                if has_text {
                    Control::StartImport
                } else {
                    Control::Noop
                },
                has_text,
            ));
        }
    }
    root.children.push(panel);
    root
}

fn browser_help(glass: &mut GlassUi, panel: &mut Node) {
    let mut body = panels::body(glass, panel, 226.);
    let content = &mut body.children[0];
    let w = content.width;
    for (index, line) in [
        "1. Скопируйте код. Создайте закладку WeDex в браузере и вставьте код в поле адреса закладки.",
        "2. Откройте аудиозаписи или плейлист в браузере и нажмите закладку WeDex.",
        "3. Нажмите «Скопировать список», вернитесь сюда и вставьте его в поле импорта.",
    ].into_iter().enumerate() {
        let mut label = panels::text(line, 0., 8. + index as f32 * 67., w, 14., false);
        label.name = "Import hint".into();
        label.height = 63.;
        content.children.push(label);
    }
    panel.children.push(body);
    let w = (panel.width - 56.) / 2.;
    for (index, (label, control, primary)) in [
        ("К импорту", Control::ImportBrowserHelp, false),
        ("Копировать код", Control::CopyImportBookmarklet, true),
    ]
    .into_iter()
    .enumerate()
    {
        panel.children.push(panels::button(
            label,
            24. + index as f32 * (w + 8.),
            panel.height - 66.,
            w,
            44.,
            control,
            primary,
        ));
    }
}

fn report_panel(glass: &mut GlassUi, panel: &mut Node, report: &Report) {
    use panels::{frame, text};

    let groups = [
        ("Не найдено", &report.missing),
        ("Не удалось проверить", &report.unchecked),
        ("Уточните исполнителя", &report.ambiguous),
    ];
    let mut height = 50.;
    for (count, _) in [
        (report.existing, "Уже в понравившихся"),
        (report.hidden, "Ранее удалены"),
    ] {
        if count > 0 {
            height += 28.;
        }
    }
    let row_height = 44.;
    let total = height
        + groups
            .iter()
            .filter(|(_, songs)| !songs.is_empty())
            .map(|(_, songs)| 40. + songs.len() as f32 * row_height)
            .sum::<f32>();
    let mut viewport = panel.clone();
    viewport.children.clear();
    viewport.height -= 58.;
    let mut body = panels::body(glass, &viewport, total);
    let visible_height = body.height;
    let content = &mut body.children[0];
    let offset = -content.y;
    let width = content.width - 10.;
    content.children.push(text(
        &format!("Добавлено в понравившиеся · {}", report.added),
        0.,
        0.,
        width,
        16.,
        true,
    ));
    let mut y = 44.;
    for (count, label) in [
        (report.existing, "Уже в понравившихся"),
        (report.hidden, "Ранее удалены"),
    ] {
        if count > 0 {
            content.children.push(text(
                &format!("{label} · {count}"),
                0.,
                y,
                width,
                13.,
                false,
            ));
            y += 28.;
        }
    }
    for (label, songs) in groups {
        if songs.is_empty() {
            continue;
        }
        content.children.push(text(
            &format!("{label} · {}", songs.len()),
            0.,
            y,
            width,
            15.,
            true,
        ));
        y += 34.;
        let start = ((offset - y) / row_height).floor().max(0.) as usize;
        let end = ((offset + visible_height - y) / row_height).ceil().max(0.) as usize;
        for (index, song) in songs.iter().enumerate().take(end).skip(start) {
            let row_y = y + index as f32 * row_height;
            let mut row = frame("Строка отчёта", 0., row_y, width, row_height);
            row.clips_content = true;
            let mut label = text(&song.label(), 0., 0., width, 13., false);
            label.name = "Import track".into();
            label.height = row_height - 4.;
            row.children.push(label);
            content.children.push(row);
        }
        y += songs.len() as f32 * row_height + 6.;
    }
    panel.children.push(body);
}

pub(super) fn input(glass: &mut GlassUi, ui: &mut egui::Ui) {
    if std::mem::take(&mut glass.import_editor.copy_bookmarklet) {
        ui.ctx()
            .copy_text(crate::track_import::browser_bookmarklet());
    }
    let Some((bounds, clip)) = glass.renderer.regions.get("Import text").copied() else {
        return;
    };
    let rect = bounds.intersect(clip).shrink(8.);
    let heading = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 36.));
    let (heading, pasted) =
        crate::platform::paste_button(ui, heading, &mut glass.import_editor.text, glass.dark);
    let rect = egui::Rect::from_min_max(rect.min, egui::pos2(heading.right(), rect.bottom()));
    let id = egui::Id::new("track-import-text");
    let color = if glass.dark {
        Color32::from_rgb(242, 240, 246)
    } else {
        Color32::from_rgb(28, 26, 30)
    };
    let context = ui.ctx().clone();
    if let Some(payload) = context.input_mut(|input| {
        let position = input.events.iter().position(
            |event| matches!(event, egui::Event::Paste(value) if Editor::is_browser_export(value)),
        )?;
        if let egui::Event::Paste(payload) = input.events.remove(position) {
            Some(payload)
        } else {
            None
        }
    }) {
        glass.import_editor.receive_browser_payload(payload);
    } else if pasted {
        glass.import_editor.browser_payload = None;
        if Editor::is_browser_export(&glass.import_editor.text) {
            let payload = std::mem::take(&mut glass.import_editor.text);
            glass.import_editor.receive_browser_payload(payload);
        }
    }
    if crate::platform::text_before(&context, id, &mut glass.import_editor.text) {
        glass.import_editor.browser_payload = None;
    }
    let response = ui.scope(|ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(rect));
        ui.put(
            rect,
            egui::TextEdit::multiline(&mut glass.import_editor.text)
                .id(id)
                .frame(false)
                .char_limit(131_072)
                .hint_text(
                    RichText::new("Вставьте ссылку или список\nИсполнитель — название")
                        .color(color.gamma_multiply(0.65)),
                )
                .text_color(color)
                .desired_width(rect.width())
                .desired_rows(3),
        )
    });
    if response.inner.changed() {
        glass.import_editor.browser_payload = None;
    }
    crate::platform::text_after(&context, id, &glass.import_editor.text, false, false);
}

impl Editor {
    pub(super) fn input(&self) -> Input {
        Input::Text(
            self.source,
            self.browser_payload.as_ref().unwrap_or(&self.text).clone(),
        )
    }
    pub(super) fn clear(&mut self) {
        self.text.clear();
        self.browser_payload = None;
    }
    fn is_browser_export(value: &str) -> bool {
        value.starts_with("{\"wedexTracks\":")
    }
    fn receive_browser_payload(&mut self, payload: String) {
        self.text = "Список треков из браузера".into();
        self.browser_payload = Some(payload);
    }
}

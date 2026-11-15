use super::{panels::*, *};

pub(super) fn track_actions(ui: &mut GlassUi, m: &Model) -> Node {
    let (mut root, mut p) = super::panels::panel(ui, "Трек", 480., 560.);
    if let Some(t) = &ui.action_track {
        let w = p.width - 48.;
        let mut cover = frame("Track action artwork", 24., 86., 64., 64.);
        cover.corner_radius = 16.;
        cover.clips_content = true;
        cover.fills = vec![serde_json::json!({"type":"IMAGE","source":GlassUi::source(t)})];
        p.children.push(cover);
        p.children
            .push(text(&t.title, 104., 90., w - 80., 18., true));
        let mut artist = text(&t.artist, 104., 120., w - 80., 13., false);
        artist.opacity = 0.6;
        p.children.push(artist);
        let saved = m
            .downloads
            .entries
            .iter()
            .any(|e| e.track.key() == t.key() && e.pinned);
        let pending = m.downloads.downloading.contains(&t.key());
        for (i, (label, control, selected)) in [
            ("Слушать", Control::Play(t.clone()), false),
            ("Sonora по треку", Control::Radio(t.clone()), false),
            (
                if m.favorites.iter().any(|f| f.key() == t.key()) {
                    "Убрать из понравившихся"
                } else {
                    "Мне нравится"
                },
                Control::Like(t.clone()),
                false,
            ),
            (
                if pending {
                    "В очереди загрузки"
                } else if saved {
                    "Скачано на устройство"
                } else {
                    "Скачать на устройство"
                },
                if pending || saved {
                    Control::Noop
                } else {
                    Control::Download(vec![t.clone()])
                },
                saved,
            ),
            ("Другие версии записи", Control::Versions(t.clone()), false),
        ]
        .into_iter()
        .enumerate()
        {
            p.children.push(button(
                label,
                24.,
                174. + i as f32 * 62.,
                w,
                48.,
                control,
                selected,
            ));
        }
    }
    root.children.push(p);
    root
}

pub(super) fn panel(ui: &mut GlassUi, m: &Model) -> Node {
    let (mut root, mut p) = super::panels::panel(ui, "Скачанное", 720., 820.);
    let count = m.downloads.entries.len();
    let mut viewport = body(ui, &p, 600. + count as f32 * 72.);
    let b = &mut viewport.children[0];
    let w = b.width;
    b.children.push(text(
        &format!(
            "{} треков · {:.0} МБ из {} МБ",
            count,
            m.downloads.bytes as f64 / 1048576.,
            m.settings.cache_limit_mb
        ),
        0.,
        0.,
        w,
        14.,
        true,
    ));
    let mut note = text(
        "Музыка хранится на этом устройстве.",
        0.,
        28.,
        w,
        12.,
        false,
    );
    note.opacity = 0.6;
    b.children.push(note);
    b.children.push(button(
        "Автоматически сохранять прослушанное",
        0.,
        70.,
        w,
        44.,
        Control::CacheAuto,
        m.settings.cache_auto,
    ));
    b.children.push(button(
        "Офлайн-режим",
        0.,
        122.,
        w,
        44.,
        Control::OfflineOnly,
        m.settings.offline_only,
    ));
    b.children
        .push(text("Лимит хранилища", 0., 188., w, 12., true));
    let bw = (w - 16.) / 3.;
    for (i, (mb, label)) in [(1024, "1 ГБ"), (2048, "2 ГБ"), (8192, "8 ГБ")]
        .into_iter()
        .enumerate()
    {
        b.children.push(button(
            label,
            i as f32 * (bw + 8.),
            216.,
            bw,
            40.,
            Control::CacheLimit(mb),
            m.settings.cache_limit_mb == mb,
        ));
    }
    b.children.push(button(
        "Скачать понравившиеся",
        0.,
        278.,
        w,
        44.,
        Control::Download(m.favorites.clone()),
        false,
    ));
    if let Some(t) = &m.status.track {
        b.children.push(button(
            "Скачать текущий трек",
            0.,
            330.,
            w,
            44.,
            Control::Download(vec![t.clone()]),
            false,
        ));
    }
    b.children.push(button(
        "Очистить временный кэш",
        0.,
        382.,
        w,
        40.,
        Control::RemoveDownload(None),
        false,
    ));
    let status = if m.downloads.busy {
        format!("В очереди: {}", m.downloads.downloading.len())
    } else {
        "Ручные загрузки сохраняются при очистке кэша.".into()
    };
    let mut status = text(&status, 0., 434., w, 11., false);
    status.opacity = 0.6;
    b.children.push(status);
    if m.downloads.busy {
        b.children.push(button(
            "Отменить загрузки",
            0.,
            466.,
            w,
            40.,
            Control::CancelDownloads,
            false,
        ));
    }
    if count == 0 {
        b.children
            .push(text("Пока нет скачанных треков", 0., 530., w, 14., false));
    }
    for (i, entry) in m.downloads.entries.iter().enumerate() {
        let y = 530. + i as f32 * 72.;
        let mut row = frame(
            &format!("offline/{}", entry.track.key()),
            0.,
            y,
            w - 48.,
            64.,
        );
        row.command = Some(Control::OfflinePlay(i));
        let mut cover = frame("Offline artwork", 0., 8., 48., 48.);
        cover.corner_radius = 12.;
        cover.clips_content = true;
        cover.fills =
            vec![serde_json::json!({"type":"IMAGE","source":GlassUi::source(&entry.track)})];
        row.children.push(cover);
        row.children.push(text(
            &entry.track.title,
            60.,
            8.,
            (w - 118.).max(80.),
            14.,
            true,
        ));
        let mut detail = text(
            &format!(
                "{} · {:.1} МБ",
                entry.track.artist,
                entry.size as f64 / 1048576.
            ),
            60.,
            32.,
            (w - 118.).max(80.),
            11.,
            false,
        );
        detail.opacity = 0.6;
        row.children.push(detail);
        b.children.push(row);
        let mut remove = icon_button(
            "x",
            w - 44.,
            y + 10.,
            44.,
            Control::RemoveDownload(Some(entry.track.clone())),
            false,
        );
        remove.id = format!("remove-cache/{}", entry.track.key());
        b.children.push(remove);
    }
    p.children.push(viewport);
    root.children.push(p);
    root
}

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
            .any(|e| e.track.id == t.id && e.track.provider == t.provider);
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
                    "Удалить с устройства"
                } else {
                    "Скачать на устройство"
                },
                if pending {
                    Control::Noop
                } else if saved {
                    Control::RemoveDownload(Some(t.clone()))
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

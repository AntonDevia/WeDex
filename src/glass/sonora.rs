use super::{panels::*, *};

pub(super) fn start_card(mut card: Node, m: &Model) -> Node {
    card.name = "Album/sonora-start".into();
    card.id = "Sonora start".into();
    card.visible = true;
    card.command = Some(if m.sonora_loading {
        Control::Noop
    } else {
        Control::ResetRadio
    });
    if let Some(cover) = card.find_mut("Cover") {
        cover.fills = vec![fill([1., 1., 1.], 0.08)];
        cover.children.clear();
        let mut waves = icon("audio-lines", 44., 44., 200.);
        waves.opacity = 0.07;
        cover.children.push(waves);
        let mut play = frame("Start Sonora disc", 88., 88., 112., 112.);
        play.corner_radius = 56.;
        play.fills = vec![fill([0.91, 1., 0.64], 1.)];
        let mut glyph = icon(
            if m.sonora_loading {
                "audio-lines"
            } else {
                "play"
            },
            34.,
            30.,
            52.,
        );
        glyph.tint = serde_json::json!({"r":0.10,"g":0.09,"b":0.12});
        play.children.push(glyph);
        cover.children.push(play);
    }
    if let Some(caption) = card.find_mut("Caption") {
        caption.children.clear();
        let mut title = text(
            if m.sonora_loading {
                "Подбираем музыку…"
            } else {
                "Запустить Sonora"
            },
            0.,
            0.,
            caption.width,
            if m.sonora_loading { 22. } else { 24. },
            true,
        );
        title.text_align_horizontal = "CENTER".into();
        let mut subtitle = text("По понравившимся", 0., 36., caption.width, 14., false);
        subtitle.text_align_horizontal = "CENTER".into();
        subtitle.opacity = 0.65;
        caption.children.extend([title, subtitle]);
    }
    card
}

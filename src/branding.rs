use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

pub fn configure(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut fallback = fonts.families[&FontFamily::Proportional].clone();
    for &(name, bytes) in crate::font_fallback::FACES {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
        fallback.push(name.into());
    }
    let faces: &[(&str, &[u8])] = &[
        (
            "medeo-regular",
            include_bytes!("../assets/fonts/medeo/MedeoSans-Regular.ttf"),
        ),
        (
            "medeo-medium",
            include_bytes!("../assets/fonts/medeo/MedeoSans-Medium.ttf"),
        ),
        (
            "medeo-semibold",
            include_bytes!("../assets/fonts/medeo/MedeoSans-SemiBold.ttf"),
        ),
        (
            "medeo-bold",
            include_bytes!("../assets/fonts/medeo/MedeoSans-Bold.ttf"),
        ),
        (
            "medeo-display",
            include_bytes!("../assets/fonts/medeo/MedeoSansDisplay-Medium.ttf"),
        ),
        (
            "medeo-display-semibold",
            include_bytes!("../assets/fonts/medeo/MedeoSansDisplay-SemiBold.ttf"),
        ),
    ];
    for &(name, bytes) in faces {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
        let mut family = vec![name.to_owned()];
        family.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "medeo-regular".into());
    for &(name, _) in crate::font_fallback::FACES {
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .push(name.into());
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .push(name.into());
    }
    ctx.set_fonts(fonts);
    ctx.style_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(15.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Name("medeo-medium".into())),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(28.0, FontFamily::Name("medeo-display-semibold".into())),
        );
    });
}

#[cfg(not(target_os = "android"))]
pub fn icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/brand/app-icon.png"))
        .expect("The bundled wedex application icon must be a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_ui_weights_resolve_cjk_music_math_and_multilingual_fallback_glyphs() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            for text in [
                "Hello",
                "你好，世界 · 音乐",
                "東京の夜 · 音楽",
                "서울의 밤 · 음악",
            ] {
                let galley = ctx.fonts(|fonts| {
                    fonts.layout_no_wrap(
                        text.into(),
                        FontId::new(28., FontFamily::Name("medeo-medium".into())),
                        egui::Color32::WHITE,
                    )
                });
                assert!(galley.size().x < 400., "Invalid fallback metrics");
                let visible = text.chars().filter(|c| !c.is_whitespace()).count();
                assert_eq!(
                    galley
                        .rows
                        .iter()
                        .map(|row| row.visuals.mesh.vertices.len())
                        .sum::<usize>(),
                    visible * 4,
                    "Fallback must rasterize actual outlines for {text:?}"
                );
            }
            ctx.fonts(|fonts| {
                for family in [
                    FontFamily::Proportional,
                    FontFamily::Monospace,
                    FontFamily::Name("medeo-medium".into()),
                    FontFamily::Name("medeo-bold".into()),
                    FontFamily::Name("medeo-display-semibold".into()),
                ] {
                    let id = FontId::new(18., family);
                    for c in
                        "音乐你好世界東京한국어♫♯𝄞𝕏∑∞☆⚡אבגمرحباहिन्दीবাংলাไทยதமிழ்ქართულიՀայերենᗪ꧁꧂𓂀😀"
                            .chars()
                    {
                        assert!(
                            fonts.has_glyph(&id, c),
                            "Missing {c:?} U+{:04X} in {id:?}",
                            c as u32
                        );
                    }
                }
            });
        });
    }
}

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

pub fn configure(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families[&FontFamily::Proportional].clone();
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

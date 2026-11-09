pub fn mobile() -> bool {
    #[cfg(test)]
    if TEST_MOBILE.with(std::cell::Cell::get) {
        return true;
    }
    cfg!(target_os = "android") || std::env::var_os("WEDEX_MOBILE_PREVIEW").is_some()
}
#[cfg(test)]
thread_local! { static TEST_MOBILE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub fn mobile_for_test() -> impl Drop {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_MOBILE.with(|value| value.set(self.0));
        }
    }
    Reset(TEST_MOBILE.with(|value| value.replace(true)))
}
pub fn paste_button(
    ui: &mut eframe::egui::Ui,
    rect: eframe::egui::Rect,
    value: &mut String,
    dark: bool,
) -> (eframe::egui::Rect, bool) {
    #[cfg(target_os = "android")]
    {
        let button = eframe::egui::Rect::from_min_max(
            eframe::egui::pos2(rect.max.x - 84., rect.min.y + 4.),
            rect.max - eframe::egui::vec2(4., 4.),
        );
        let clicked = ui
            .put(
                button,
                eframe::egui::Button::new(eframe::egui::RichText::new("Вставить").size(11.).color(
                    if dark {
                        eframe::egui::Color32::from_rgb(232, 255, 163)
                    } else {
                        eframe::egui::Color32::from_rgb(28, 26, 30)
                    },
                ))
                .fill(if dark {
                    eframe::egui::Color32::from_rgb(48, 44, 54)
                } else {
                    eframe::egui::Color32::from_rgb(230, 229, 226)
                })
                .wrap_mode(eframe::egui::TextWrapMode::Extend)
                .corner_radius(12.),
            )
            .clicked();
        let mut changed = false;
        if clicked {
            if let Ok(text) = crate::android::paste() {
                if !text.is_empty() {
                    *value = text;
                    changed = true;
                }
            }
        }
        (
            eframe::egui::Rect::from_min_max(rect.min, rect.max - eframe::egui::vec2(88., 0.)),
            changed,
        )
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (ui, value, dark);
        (rect, false)
    }
}
pub fn text_before(ctx: &eframe::egui::Context, id: eframe::egui::Id, text: &mut String) -> bool {
    #[cfg(target_os = "android")]
    {
        crate::android::text_before(ctx, id, text)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (ctx, id, text);
        false
    }
}
pub fn text_after(
    ctx: &eframe::egui::Context,
    id: eframe::egui::Id,
    text: &str,
    secret: bool,
    search: bool,
) {
    #[cfg(target_os = "android")]
    crate::android::text_after(ctx, id, text, secret, search);
    #[cfg(not(target_os = "android"))]
    let _ = (ctx, id, text, secret, search);
}
pub fn system_dark(ctx: &eframe::egui::Context) -> bool {
    #[cfg(target_os = "android")]
    {
        let _ = ctx;
        crate::android::system_dark()
    }
    #[cfg(not(target_os = "android"))]
    {
        ctx.system_theme() == Some(eframe::egui::Theme::Dark)
    }
}
pub fn open_url(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "android")]
    {
        crate::android::open_url(url)
    }
    #[cfg(not(target_os = "android"))]
    {
        webbrowser::open(url).map_err(Into::into)
    }
}
pub fn shader_source(source: &str) -> std::borrow::Cow<'_, str> {
    #[cfg(target_os = "android")]
    {
        std::borrow::Cow::Owned(source.replace(
            "#version 330 core",
            "#version 300 es\nprecision highp float;\nprecision highp int;",
        ))
    }
    #[cfg(not(target_os = "android"))]
    {
        std::borrow::Cow::Borrowed(source)
    }
}

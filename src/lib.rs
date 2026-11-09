mod app;
mod audio;
mod branding;
mod cloud;
mod diagnostics;
mod discovery;
mod equalizer;
mod fingerprinting;
mod glass;
mod lyrics;
mod model;
mod platform;
mod providers;
mod recognition;
mod store;
mod tray;
mod updates;

#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> eframe::Result<()> {
    let preview_size = std::env::var("WEDEX_PREVIEW_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .filter(|s| s[0] >= 360. && s[1] >= 480.)
        .unwrap_or([1440., 960.]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(branding::icon())
            .with_inner_size(preview_size)
            .with_min_inner_size([360.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "WeDex · Sonora",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

#[cfg(target_os = "android")]
mod android;

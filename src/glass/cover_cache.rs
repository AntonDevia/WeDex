//! File reads and JPEG decoding never run on the gesture/render thread.
use eframe::egui::{self, ColorImage};
use std::sync::mpsc::{self, Receiver, SyncSender};
pub struct Cache {
    pub tx: SyncSender<(String, egui::Context)>,
    pub rx: Receiver<(String, ColorImage)>,
}
impl Default for Cache {
    fn default() -> Self {
        let (tx, requests) = mpsc::sync_channel::<(String, egui::Context)>(32);
        let (results, rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((source, ctx)) = requests.recv() {
                if let Some(bytes) = crate::cache::global().cover(&source) {
                    let image = image::ImageReader::new(std::io::Cursor::new(&bytes))
                        .with_guessed_format()
                        .ok()
                        .and_then(|reader| reader.into_dimensions().ok())
                        .filter(|(w, h)| *w <= 1024 && *h <= 1024)
                        .and_then(|_| image::load_from_memory(&bytes).ok());
                    if let Some(image) = image {
                        let image = image.to_rgba8();
                        let image = ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        );
                        if results.send((source, image)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                }
            }
        });
        Self { tx, rx }
    }
}

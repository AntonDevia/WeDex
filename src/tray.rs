#[derive(Clone, Copy, Debug)]
#[cfg_attr(target_os = "android", allow(dead_code))]
pub enum Action {
    Show,
    Quit,
}
#[cfg(windows)]
mod platform {
    use super::Action;
    use eframe::egui;
    use std::sync::{Arc, Mutex};
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };
    pub struct Tray {
        pub _icon: TrayIcon,
        events: Arc<Mutex<Vec<Action>>>,
    }
    impl Tray {
        pub fn new(ctx: &egui::Context) -> anyhow::Result<Self> {
            let events = Arc::new(Mutex::new(vec![]));
            let menu = Menu::new();
            let show = MenuItem::new("Открыть WeDex", true, None);
            let quit = MenuItem::new("Выйти", true, None);
            menu.append_items(&[&show, &quit])?;
            let image = image::load_from_memory(include_bytes!("../assets/brand/app-icon.png"))?
                .into_rgba8();
            let icon =
                tray_icon::Icon::from_rgba(image.as_raw().clone(), image.width(), image.height())?;
            let tray = TrayIconBuilder::new()
                .with_tooltip("WeDex · Sonora")
                .with_icon(icon)
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()?;
            let wake = ctx.clone();
            let receiver = events.clone();
            let show_id = show.id().clone();
            let quit_id = quit.id().clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let action = if event.id == show_id {
                    Some(Action::Show)
                } else if event.id == quit_id {
                    Some(Action::Quit)
                } else {
                    None
                };
                if let Some(action) = action {
                    receiver.lock().unwrap().push(action);
                    wake.request_repaint();
                }
            }));
            let wake = ctx.clone();
            let receiver = events.clone();
            TrayIconEvent::set_event_handler(Some(move |event| {
                if matches!(
                    event,
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } | TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                ) {
                    receiver.lock().unwrap().push(Action::Show);
                    wake.request_repaint();
                }
            }));
            Ok(Self {
                _icon: tray,
                events,
            })
        }
        pub fn poll(&self) -> Vec<Action> {
            std::mem::take(&mut *self.events.lock().unwrap())
        }
        #[cfg(test)]
        pub fn schedule_test_action(
            &self,
            action: Action,
            ctx: &egui::Context,
            delay: std::time::Duration,
        ) {
            let events = self.events.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                events.lock().unwrap().push(action);
                ctx.request_repaint();
            });
        }
    }
}
#[cfg(not(windows))]
mod platform {
    pub struct Tray;
    impl Tray {
        pub fn new(_: &eframe::egui::Context) -> anyhow::Result<Self> {
            anyhow::bail!("Трей доступен в сборке Windows")
        }
        pub fn poll(&self) -> Vec<super::Action> {
            vec![]
        }
    }
}
pub use platform::Tray;

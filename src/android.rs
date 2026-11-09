use crate::{app::App, audio};
use anyhow::{Context, Result};
use eframe::egui;
use jni::{
    objects::{JClass, JObject, JString, JValue},
    sys::{jfloat, jint},
    JNIEnv, JavaVM,
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use winit::platform::android::activity::AndroidApp;

static ACTIVITY: OnceLock<Mutex<Option<AndroidApp>>> = OnceLock::new();
static COMMANDS: Mutex<VecDeque<(i32, f32)>> = Mutex::new(VecDeque::new());
static INVITATION: Mutex<Option<String>> = Mutex::new(None);
static FOREGROUND: AtomicBool = AtomicBool::new(true);
static DARK: AtomicBool = AtomicBool::new(false);
static WAKE: OnceLock<Mutex<Option<egui::Context>>> = OnceLock::new();
mod text;
pub use text::{text_after, text_before};

fn with_activity<T>(work: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Result<T>) -> Result<T> {
    let app = ACTIVITY
        .get()
        .context("Android ещё не запущен")?
        .lock()
        .unwrap()
        .clone()
        .context("Activity закрыта")?;
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let result = work(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result
}
pub fn data_dir() -> PathBuf {
    ACTIVITY
        .get()
        .and_then(|a| {
            a.lock()
                .unwrap()
                .as_ref()
                .and_then(AndroidApp::internal_data_path)
        })
        .unwrap_or_else(|| PathBuf::from("/data/user/0/app.wedex.music/files"))
}
pub fn read_secret(name: &str) -> Result<String> {
    with_activity(|env, activity| {
        let name = env.new_string(name)?;
        let result = env
            .call_method(
                activity,
                "wedexReadSecret",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        if result.is_null() {
            return Ok(String::new());
        }
        Ok(env.get_string(&JString::from(result))?.into())
    })
}
pub fn paste() -> Result<String> {
    with_activity(|env, activity| {
        let value = env
            .call_method(activity, "wedexPaste", "()Ljava/lang/String;", &[])?
            .l()?;
        Ok(env.get_string(&JString::from(value))?.into())
    })
}
pub fn write_secret(name: &str, value: &str) -> Result<()> {
    with_activity(|env, activity| {
        let name = env.new_string(name)?;
        let value = env.new_string(value)?;
        env.call_method(
            activity,
            "wedexWriteSecret",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&name), JValue::Object(&value)],
        )?;
        Ok(())
    })
}
pub fn open_url(url: &str) -> Result<()> {
    with_activity(|env, activity| {
        let url = env.new_string(url)?;
        env.call_method(
            activity,
            "wedexOpenUrl",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&url)],
        )?;
        Ok(())
    })
}
pub fn install_update(path: &std::path::Path) -> Result<()> {
    with_activity(|env, activity| {
        let path = env.new_string(path.to_string_lossy())?;
        env.call_method(
            activity,
            "wedexInstallUpdate",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&path)],
        )?;
        Ok(())
    })
}
pub fn request_microphone() -> bool {
    with_activity(|env, activity| {
        Ok(env
            .call_method(activity, "wedexRequestMicrophone", "()Z", &[])?
            .z()?)
    })
    .unwrap_or(false)
}
pub fn take_commands() -> Vec<(i32, f32)> {
    COMMANDS.lock().unwrap().drain(..).collect()
}
pub fn take_invitation() -> Option<String> {
    INVITATION.lock().unwrap().take()
}
#[no_mangle]
pub extern "system" fn Java_app_wedex_music_WeDexActivity_nativeInvitation(
    mut env: JNIEnv,
    _class: JClass,
    value: JString,
) {
    if let Ok(value) = env.get_string(&value) {
        let value: String = value.into();
        if value.len() <= 32768 && value.starts_with("wedex-cloud:") {
            *INVITATION.lock().unwrap() = Some(value);
            if let Some(wake) = WAKE.get().and_then(|w| w.lock().unwrap().clone()) {
                wake.request_repaint();
            }
        }
    }
}
pub fn system_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}
pub fn foreground() -> bool {
    FOREGROUND.load(Ordering::Relaxed)
}
pub fn minimize() {
    let _ = with_activity(|env, activity| {
        env.call_method(activity, "wedexMinimize", "()V", &[])?;
        Ok(())
    });
}
pub fn fullscreen(enabled: bool) {
    let _ = with_activity(|env, activity| {
        env.call_method(
            activity,
            "wedexFullscreen",
            "(Z)V",
            &[JValue::Bool(enabled.into())],
        )?;
        Ok(())
    });
}
pub fn publish(status: &audio::Status, remote: bool, volume: f32) {
    struct Published {
        at: Instant,
        playing: bool,
        loading: bool,
        track: Option<String>,
        remote: bool,
        volume: f32,
    }
    static LAST: OnceLock<Mutex<Published>> = OnceLock::new();
    let mut last = LAST
        .get_or_init(|| {
            Mutex::new(Published {
                at: Instant::now() - Duration::from_secs(2),
                playing: false,
                loading: false,
                track: None,
                remote: false,
                volume: 0.,
            })
        })
        .lock()
        .unwrap();
    let track = status.track.as_ref().map(crate::model::Track::key);
    // Starting playback must start the Android service before a quick Home
    // gesture. Throttle progress updates, never transport-state transitions.
    if last.at.elapsed() < Duration::from_millis(500)
        && last.playing == status.playing
        && last.loading == status.loading
        && last.track == track
        && last.remote == remote
        && (last.volume - volume).abs() < 0.001
    {
        return;
    }
    *last = Published {
        at: Instant::now(),
        playing: status.playing,
        loading: status.loading,
        track,
        remote,
        volume,
    };
    let json = serde_json::json!({"remote":remote,"volume":volume,"playing":status.playing,"loading":status.loading,"position":status.position,
        "duration":status.duration,"title":status.track.as_ref().map(|t|t.title.as_str()).unwrap_or(""),
        "artist":status.track.as_ref().map(|t|t.artist.as_str()).unwrap_or("")}).to_string();
    let _ = with_activity(|env, activity| {
        let json = env.new_string(json)?;
        env.call_method(
            activity,
            "wedexPublish",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&json)],
        )?;
        Ok(())
    });
}

#[no_mangle]
pub extern "system" fn Java_app_wedex_music_WeDexActivity_nativeCommand(
    _env: JNIEnv,
    _class: JClass,
    command: jint,
    value: jfloat,
) {
    let mut commands = COMMANDS.lock().unwrap();
    if commands.len() < 64 && value.is_finite() {
        commands.push_back((command, value));
    }
    if let Some(wake) = WAKE.get().and_then(|w| w.lock().unwrap().clone()) {
        wake.request_repaint();
    }
}
#[no_mangle]
pub extern "system" fn Java_app_wedex_music_WeDexActivity_nativeForeground(
    _env: JNIEnv,
    _class: JClass,
    value: jint,
) {
    FOREGROUND.store(value != 0, Ordering::Relaxed);
}
#[no_mangle]
pub extern "system" fn Java_app_wedex_music_WeDexActivity_nativeTheme(
    _env: JNIEnv,
    _class: JClass,
    value: jint,
) {
    DARK.store(value != 0, Ordering::Relaxed);
}

struct AndroidUi {
    app: Arc<Mutex<App>>,
    stop: Arc<AtomicBool>,
}
impl eframe::App for AndroidUi {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        text::input_hook(input);
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.lock().unwrap().update(ctx, frame);
        let copied = ctx.output(|output| {
            output
                .commands
                .iter()
                .filter_map(|command| {
                    if let egui::OutputCommand::CopyText(text) = command {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        for text in copied {
            let _ = with_activity(|env, activity| {
                let text = env.new_string(text)?;
                env.call_method(
                    activity,
                    "wedexCopy",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&text)],
                )?;
                Ok(())
            });
        }
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.stop.store(true, Ordering::Relaxed);
        self.app.lock().unwrap().on_exit(gl);
    }
}
#[no_mangle]
fn android_main(android_app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("WeDex")
            .with_max_level(log::LevelFilter::Info),
    );
    *ACTIVITY.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(android_app.clone());
    let options = eframe::NativeOptions {
        android_app: Some(android_app),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        ..Default::default()
    };
    let result = eframe::run_native(
        "WeDex",
        options,
        Box::new(|cc| {
            *WAKE.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(cc.egui_ctx.clone());
            let app = Arc::new(Mutex::new(App::new(cc)));
            let stop = Arc::new(AtomicBool::new(false));
            let weak = Arc::downgrade(&app);
            let stopped = stop.clone();
            std::thread::spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    if !FOREGROUND.load(Ordering::Relaxed) {
                        let Some(app) = weak.upgrade() else {
                            break;
                        };
                        if let Ok(mut app) = app.lock() {
                            app.background_tick();
                        };
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            });
            Ok(Box::new(AndroidUi { app, stop }))
        }),
    );
    if let Err(error) = result {
        log::error!("Android renderer: {error}");
    }
    *WAKE.get().unwrap().lock().unwrap() = None;
    *ACTIVITY.get().unwrap().lock().unwrap() = None;
}

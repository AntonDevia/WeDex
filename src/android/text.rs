//! GameActivity reports complete UTF-16 edit buffers; winit 0.30 only forwards
//! hardware keys. Keep the native IME buffer and egui cursor in sync without
//! converting composition into simulated key presses or logging credentials.
use super::*;
use egui::text::{CCursor, CCursorRange};
use jni::sys::jlong;

struct Buffer {
    generation: i64,
    text: String,
    start: usize,
    end: usize,
}
#[derive(Default)]
struct Bridge {
    field: Option<egui::Id>,
    generation: i64,
    sent: String,
    selection: (usize, usize),
    pending: Option<Buffer>,
}
static BRIDGE: OnceLock<Mutex<Bridge>> = OnceLock::new();
static SUBMIT: AtomicBool = AtomicBool::new(false);
fn bridge() -> &'static Mutex<Bridge> {
    BRIDGE.get_or_init(|| Mutex::new(Bridge::default()))
}
fn wake() {
    if let Some(ctx) = WAKE.get().and_then(|w| w.lock().unwrap().clone()) {
        ctx.request_repaint();
    }
}
#[no_mangle]
pub extern "system" fn Java_app_wedex_music_WeDexActivity_nativeText(
    mut env: JNIEnv,
    _class: JClass,
    generation: jlong,
    text: JString,
    start: jint,
    end: jint,
) {
    let Ok(text) = env.get_string(&text) else {
        return;
    };
    let text: String = text.into();
    if text.len() > 131_072 {
        return;
    }
    let mut b = bridge().lock().unwrap();
    if generation == b.generation {
        b.pending = Some(Buffer {
            generation,
            text,
            start: start.max(0) as usize,
            end: end.max(0) as usize,
        });
    }
    drop(b);
    wake();
}
#[no_mangle]
pub extern "system" fn Java_app_wedex_music_WeDexActivity_nativeSubmit(
    _env: JNIEnv,
    _class: JClass,
) {
    SUBMIT.store(true, Ordering::Relaxed);
    wake();
}
pub(super) fn input_hook(input: &mut egui::RawInput) {
    if SUBMIT.swap(false, Ordering::Relaxed) {
        for pressed in [true, false] {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            });
        }
    }
}
pub fn text_before(ctx: &egui::Context, id: egui::Id, value: &mut String) -> bool {
    if !ctx.memory(|m| m.has_focus(id)) {
        return false;
    }
    let mut b = bridge().lock().unwrap();
    if b.field != Some(id) {
        return false;
    }
    let Some(input) = b.pending.take() else {
        return false;
    };
    if input.generation != b.generation {
        return false;
    }
    let changed = *value != input.text;
    *value = input.text;
    // Android indexes Java strings in UTF-16 code units; egui uses characters.
    let from_utf16 = |offset| {
        value
            .chars()
            .scan(0, |units, c| {
                *units += c.len_utf16();
                Some(*units)
            })
            .take_while(|&n| n <= offset)
            .count()
    };
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(from_utf16(input.start)),
        CCursor::new(from_utf16(input.end)),
    )));
    state.store(ctx, id);
    b.sent.clone_from(value);
    b.selection = (input.start, input.end);
    changed
}
pub fn text_after(ctx: &egui::Context, id: egui::Id, value: &str, secret: bool, search: bool) {
    if !ctx.memory(|m| m.has_focus(id)) {
        return;
    }
    let state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let cursor = state
        .cursor
        .char_range()
        .unwrap_or(CCursorRange::one(CCursor::new(value.chars().count())));
    let to_utf16 = |offset| {
        value
            .chars()
            .take(offset)
            .map(char::len_utf16)
            .sum::<usize>()
    };
    let selection = (
        to_utf16(cursor.secondary.index),
        to_utf16(cursor.primary.index),
    );
    let mut b = bridge().lock().unwrap();
    let new_field = b.field != Some(id);
    if !new_field && b.sent == value && b.selection == selection {
        return;
    }
    if new_field {
        b.generation += 1;
        b.field = Some(id);
        b.pending = None;
    }
    b.sent = value.into();
    b.selection = selection;
    let generation = b.generation;
    drop(b);
    let _ = with_activity(|env, activity| {
        let text = env.new_string(value)?;
        env.call_method(
            activity,
            "wedexSetText",
            "(JLjava/lang/String;IIZZ)V",
            &[
                JValue::Long(generation),
                JValue::Object(&text),
                JValue::Int(selection.0 as i32),
                JValue::Int(selection.1 as i32),
                JValue::Bool(secret.into()),
                JValue::Bool(search.into()),
            ],
        )?;
        Ok(())
    });
}

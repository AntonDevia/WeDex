use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const USER_ERROR: &str = "Что-то пошло не так";
const MAX_LOG: u64 = 1024 * 1024;
static LOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn log_path() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        crate::store::data_dir().join("WeDex-errors.log")
    }
    #[cfg(not(target_os = "android"))]
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("WeDex-Sonora.exe"))
        .with_file_name("WeDex-errors.log")
}
pub fn append(path: &Path, context: &str, error: &str, secrets: &[&str]) -> io::Result<()> {
    let _lock = LOG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut message = error.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        message = message.replace(secret, "[REDACTED]");
    }
    if message.len() > 64 * 1024 {
        let mut end = 64 * 1024;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str("… [truncated]");
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() >= MAX_LOG) {
        let previous = path.with_extension("previous.log");
        if previous.exists() {
            std::fs::remove_file(&previous)?;
        }
        std::fs::rename(path, previous)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "{} v{} [{}] {}",
        timestamp(),
        env!("CARGO_PKG_VERSION"),
        context,
        message.replace('\r', "").replace('\n', "\n    ")
    )
}
#[cfg(target_os = "android")]
pub fn export(secrets: &[&str]) -> io::Result<PathBuf> {
    export_at(
        &log_path(),
        &crate::store::data_dir().join("diagnostics"),
        secrets,
    )
}
#[cfg(any(target_os = "android", test))]
fn export_at(path: &Path, folder: &Path, secrets: &[&str]) -> io::Result<PathBuf> {
    let _lock = LOG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut text = format!("WeDex {} · diagnostic log\n", env!("CARGO_PKG_VERSION"));
    for file in [path.with_extension("previous.log"), path.to_owned()] {
        if file.exists() {
            text.push_str(&std::fs::read_to_string(file)?);
        }
    }
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        text = text.replace(secret, "[REDACTED]");
    }
    std::fs::create_dir_all(folder)?;
    let output = folder.join("WeDex-errors.log");
    std::fs::write(&output, text)?;
    Ok(output)
}
fn timestamp() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct UtcTime {
            year: u16,
            month: u16,
            weekday: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            millis: u16,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetSystemTime(value: *mut UtcTime);
        }
        let mut time = UtcTime::default();
        unsafe {
            GetSystemTime(&mut time);
        }
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            time.year, time.month, time.day, time.hour, time.minute, time.second, time.millis
        )
    }
    #[cfg(not(windows))]
    {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown-time".into())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn logs_rotate_and_an_export_is_a_redacted_snapshot() {
        let folder =
            std::env::temp_dir().join(format!("wedex-log-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("WeDex-errors.log");
        std::fs::write(&path, vec![b'x'; super::MAX_LOG as usize]).unwrap();
        super::append(&path, "test", "cached-secret 你好", &[]).unwrap();
        assert!(path.with_extension("previous.log").exists());
        let snapshot =
            super::export_at(&path, &folder.join("diagnostics"), &["cached-secret"]).unwrap();
        let text = std::fs::read_to_string(&snapshot).unwrap();
        assert!(
            text.contains("你好") && text.contains("[REDACTED]") && !text.contains("cached-secret")
        );
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("cached-secret"));
        std::fs::remove_file(snapshot).unwrap();
        std::fs::remove_dir(folder.join("diagnostics")).unwrap();
        std::fs::remove_file(path.with_extension("previous.log")).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
    #[test]
    fn errors_are_appended_and_credentials_are_redacted() {
        let folder = std::env::temp_dir().join(format!("wedex-log-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("WeDex-errors.log");
        super::append(&path, "search", "failed secret-token", &["secret-token"]).unwrap();
        super::append(&path, "audio", "unavailable", &[]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("secret-token"));
        assert!(text.contains("[REDACTED]"));
        assert!(text.contains("[search]") && text.contains("[audio]"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
}

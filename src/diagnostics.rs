use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const USER_ERROR: &str = "Что-то пошло не так";
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
    let mut message = error.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        message = message.replace(secret, "[REDACTED]");
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
        format!(
            "unix:{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )
    }
}
#[cfg(test)]
mod tests {
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

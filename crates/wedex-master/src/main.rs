#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod windows;
#[cfg(windows)]
fn main() -> eframe::Result<()> {
    windows::run()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("WeDex Мастер запускается на Windows");
}

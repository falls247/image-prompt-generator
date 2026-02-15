#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "windows")]
mod windows_app;

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    windows_app::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This application supports Windows 10/11. Build the release binary on Windows.");
}

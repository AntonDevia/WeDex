//! Publisher smoke check against the actual public release metadata.
fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let target = match args.get(1).map(String::as_str) {
        Some("android") => wedex_update::Target::Android,
        Some("server") => wedex_update::Target::Server,
        _ => wedex_update::Target::Windows,
    };
    let current = args.get(2).map(String::as_str).unwrap_or("0.0.0");
    if let Some(release) = wedex_update::check(wedex_update::DEFAULT_REPOSITORY, target, current)? {
        println!(
            "{:?}: {} · {} bytes · SHA-256 {}",
            target, release.version, release.size, release.sha256
        );
        if let Some(folder) = args.get(3) {
            let path = wedex_update::download(&release, std::path::Path::new(folder), |_| {})?;
            println!("Verified download: {}", path.display());
        }
    } else {
        println!("{:?}: no newer release", target);
    }
    Ok(())
}

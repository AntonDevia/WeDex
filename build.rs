fn main() {
    println!("cargo:rustc-check-cfg=cfg(wedex_server_bundle)");
    let path = "assets/server/wedex-server-x86_64-linux-musl";
    println!("cargo:rerun-if-changed={path}");
    if std::path::Path::new(path).is_file() {
        println!("cargo:rustc-cfg=wedex_server_bundle");
    }
}

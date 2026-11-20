fn main() {
    println!("cargo:rustc-check-cfg=cfg(wedex_server_bundle)");
    let path = "assets/server/wedex-server-x86_64-linux-musl";
    println!("cargo:rerun-if-changed={path}");
    if std::path::Path::new(path).is_file() {
        println!("cargo:rustc-cfg=wedex_server_bundle");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/brand/app-icon.rc");
        println!("cargo:rerun-if-changed=assets/brand/app-icon.ico");
        embed_resource::compile_for("assets/brand/app-icon.rc", ["sonora"], embed_resource::NONE)
            .manifest_required()
            .expect("embed the Windows application icon");
    }
}

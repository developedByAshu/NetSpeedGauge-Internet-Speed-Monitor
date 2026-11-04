//! Builds the app icon, name and version into the program.

fn main() {
    println!("cargo:rerun-if-changed=assets/netspeedgauge.rc");
    println!("cargo:rerun-if-changed=assets/netspeedgauge.ico");
    println!("cargo:rerun-if-changed=assets/netspeedgauge.manifest");
    let version_macros = ["MAJOR", "MINOR", "PATCH"].map(|part| {
        let value = std::env::var(format!("CARGO_PKG_VERSION_{part}")).expect("Cargo always sets the version");
        format!("VERSION_{part}={value}")
    });
    embed_resource::compile("assets/netspeedgauge.rc", version_macros)
        .manifest_required()
        .expect("the app icon and details could not be built into the program");
}

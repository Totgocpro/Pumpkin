fn main() {
    // Structure templates and worldgen data are now loaded at runtime
    // from the extracted Mojang asset cache.
    println!("cargo:rerun-if-changed=build.rs");
}

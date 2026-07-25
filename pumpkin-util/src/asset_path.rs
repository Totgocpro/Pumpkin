use std::path::PathBuf;
use std::sync::OnceLock;

static CACHE_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set the root of the extracted Minecraft asset cache.
/// This should point to the directory containing `assets/minecraft/` and
/// `data/minecraft/` subtrees.
pub fn set_cache_root(path: PathBuf) {
    CACHE_ROOT
        .set(path)
        .expect("asset cache_root already set");
}

/// Returns the cache root path, or `None` if not set.
#[must_use]
pub fn get_cache_root() -> Option<&'static PathBuf> {
    CACHE_ROOT.get()
}

/// Returns the path to `assets/minecraft/lang/` (language files).
#[must_use]
pub fn lang_dir() -> Option<PathBuf> {
    Some(
        CACHE_ROOT
            .get()?
            .join("assets")
            .join("minecraft")
            .join("lang"),
    )
}

/// Returns the path to `data/minecraft/` (loot tables, structures, worldgen, etc.).
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    Some(CACHE_ROOT.get()?.join("data").join("minecraft"))
}

/// Returns the path to `data/minecraft/loot_tables/chests/`.
#[must_use]
pub fn chest_loot_dir() -> Option<PathBuf> {
    Some(data_dir()?.join("loot_tables").join("chests"))
}

/// Returns the path to `data/minecraft/structures/`.
#[must_use]
pub fn structures_dir() -> Option<PathBuf> {
    Some(data_dir()?.join("structures"))
}

/// Returns the path to `data/minecraft/worldgen/template_pool/`.
#[must_use]
pub fn template_pool_dir() -> Option<PathBuf> {
    Some(data_dir()?.join("worldgen").join("template_pool"))
}

/// Returns the path to `data/minecraft/worldgen/processor_list/`.
#[must_use]
pub fn processor_list_dir() -> Option<PathBuf> {
    Some(data_dir()?.join("worldgen").join("processor_list"))
}

/// Read a structure NBT file by name (without extension).
pub fn read_structure_bytes(name: &str) -> Option<Vec<u8>> {
    let path = structures_dir()?.join(format!("{name}.nbt"));
    std::fs::read(&path).ok()
}

/// Read a template pool JSON file by name (without extension).
pub fn read_template_pool_json(name: &str) -> Option<String> {
    let path = template_pool_dir()?.join(format!("{name}.json"));
    std::fs::read_to_string(&path).ok()
}

/// Read a processor list JSON file by name (without extension).
pub fn read_processor_list_json(name: &str) -> Option<String> {
    let path = processor_list_dir()?.join(format!("{name}.json"));
    std::fs::read_to_string(&path).ok()
}

/// List all `.nbt` structure files under a given prefix directory.
pub fn list_pool_templates(pool_prefix: &str) -> Vec<String> {
    let base = match structures_dir() {
        Some(p) => p.join(pool_prefix),
        None => return Vec::new(),
    };
    let dir = match std::fs::read_dir(&base) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut results = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("nbt") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                results.push(format!("{pool_prefix}/{stem}"));
            }
        }
    }
    results
}

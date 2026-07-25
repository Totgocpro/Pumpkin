use sha1::Digest;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Version to download (e.g. "26.1.2").
/// This corresponds to the Minecraft version the server intends to support.
pub const MC_VERSION: &str = "26.1.2";

/// Mojang version manifest URL.
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Subdirectory inside the cache root where extracted client assets go.
const ASSET_CACHE_SUBDIR: &str = "minecraft_assets";

/// Manages downloading and extracting Mojang/Minecraft assets.
pub struct AssetManager {
    cache_root: PathBuf,
}

impl AssetManager {
    /// Create a new `AssetManager` rooted at `server_root` (the directory
    /// containing `pumpkin.toml`).  All cached/extracted data lives under
    /// `<server_root>/assets/`.
    #[must_use]
    pub fn new(server_root: &Path) -> Self {
        let cache_root = server_root.join("assets").join(ASSET_CACHE_SUBDIR);
        Self { cache_root }
    }

    // ── Public helpers ──────────────────────────────────────────────

    /// Returns the cache root directory.
    /// This is where `assets/minecraft/` and `data/minecraft/` subtrees live.
    #[must_use]
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// Returns the path to the directory where the client jar is cached.
    #[must_use]
    pub fn jar_cache_path(&self) -> PathBuf {
        self.cache_root.join("client.jar")
    }

    /// Returns `true` if assets have already been extracted and cached.
    #[must_use]
    pub fn is_cached(&self) -> bool {
        self.cache_root
            .join("assets")
            .join("minecraft")
            .join("lang")
            .join("en_us.json")
            .exists()
    }

    /// Ensure assets are downloaded and extracted.
    /// If already cached, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error message string on failure.
    pub fn ensure(&self, eula_accepted: bool) -> Result<(), String> {
        if self.is_cached() {
            tracing::info!("Minecraft assets already cached, skipping download.");
            return Ok(());
        }

        if !eula_accepted {
            return Err(
                "You must accept the Minecraft EULA to download Mojang assets. \
                 See https://www.minecraft.net/en-us/eula"
                    .to_string(),
            );
        }

        tracing::info!("Downloading Minecraft assets (version {MC_VERSION})...");

        let manifest = self.fetch_version_manifest()?;
        let version_meta = self.resolve_version(&manifest)?;

        let jar_sha1 = version_meta
            .get("downloads")
            .and_then(|d| d.get("client"))
            .and_then(|c| c.get("sha1"))
            .and_then(|s| s.as_str())
            .ok_or_else(|| "Client jar SHA1 not found in version metadata".to_string())?;

        let jar_url = version_meta
            .get("downloads")
            .and_then(|d| d.get("client"))
            .and_then(|c| c.get("url"))
            .and_then(|s| s.as_str())
            .ok_or_else(|| "Client jar URL not found in version metadata".to_string())?;

        let jar_path = self.jar_cache_path();
        self.download_file(jar_url, &jar_path)?;
        self.verify_sha1(&jar_path, jar_sha1)?;

        tracing::info!("Extracting assets from client.jar...");
        self.extract_jar(&jar_path)?;

        tracing::info!("Minecraft assets cached successfully.");
        Ok(())
    }

    // ── Private helpers ─────────────────────────────────────────────

    fn fetch_version_manifest(&self) -> Result<serde_json::Value, String> {
        let response = ureq::get(VERSION_MANIFEST_URL)
            .header("User-Agent", "Pumpkin-MC")
            .call()
            .map_err(|e| format!("Failed to fetch version manifest: {e}"))?;

        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read version manifest: {e}"))?;

        serde_json::from_str(&body).map_err(|e| format!("Failed to parse version manifest: {e}"))
    }

    fn resolve_version(&self, manifest: &serde_json::Value) -> Result<serde_json::Value, String> {
        let versions = manifest
            .get("versions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "Invalid version manifest: missing 'versions' array".to_string())?;

        for entry in versions {
            if let Some(id) = entry.get("id").and_then(|i| i.as_str()) {
                if id == MC_VERSION {
                    if let Some(url) = entry.get("url").and_then(|u| u.as_str()) {
                        return self.fetch_json(url);
                    }
                }
            }
        }

        Err(format!(
            "Version {MC_VERSION} not found in Mojang manifest. \
             Check https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        ))
    }

    fn fetch_json(&self, url: &str) -> Result<serde_json::Value, String> {
        let response = ureq::get(url)
            .header("User-Agent", "Pumpkin-MC")
            .call()
            .map_err(|e| format!("Failed to fetch {url}: {e}"))?;

        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read response from {url}: {e}"))?;

        serde_json::from_str(&body).map_err(|e| format!("Failed to parse JSON from {url}: {e}"))
    }

    fn download_file(&self, url: &str, dest: &Path) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {:?}: {e}", parent))?;
        }

        let response = ureq::get(url)
            .header("User-Agent", "Pumpkin-MC")
            .call()
            .map_err(|e| format!("Failed to download {url}: {e}"))?;

        // Default read_to_vec() has a 10 MB limit – client.jar can be ~30 MB
        let body = response
            .into_body()
            .into_with_config()
            .limit(200 * 1024 * 1024) // 200 MB
            .read_to_vec()
            .map_err(|e| format!("Failed to read download stream: {e}"))?;

        std::fs::write(dest, &body)
            .map_err(|e| format!("Failed to write {:?}: {e}", dest))?;

        Ok(())
    }

    fn verify_sha1(&self, path: &Path, expected_hex: &str) -> Result<(), String> {
        let data = std::fs::read(path).map_err(|e| format!("Failed to read {:?}: {e}", path))?;
        let digest = sha1::Sha1::digest(&data);
        let hex = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();

        if hex != expected_hex {
            return Err(format!(
                "SHA1 mismatch for {:?}: expected {expected_hex}, got {hex}",
                path
            ));
        }
        Ok(())
    }

    fn extract_jar(&self, jar_path: &Path) -> Result<(), String> {
        let file = std::fs::File::open(jar_path)
            .map_err(|e| format!("Failed to open {:?}: {e}", jar_path))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {e}"))?;

        // Paths to extract from the jar
        let extract_patterns = [
            "assets/minecraft/lang/",
            "data/minecraft/loot_tables/",
            "data/minecraft/structures/",
            "data/minecraft/worldgen/",
        ];

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| format!("Zip entry error: {e}"))?;
            let raw_name = entry.name().split('?').next().unwrap_or("").to_string();
            let name = raw_name.trim_end_matches('/').to_string();

            let should_extract = extract_patterns.iter().any(|pat| name.starts_with(pat));
            if !should_extract {
                continue;
            }

            let dest = self.cache_root.join(&name);

            if entry.is_dir() {
                std::fs::create_dir_all(&dest)
                    .map_err(|e| format!("Failed to create dir {:?}: {e}", dest))?;
                continue;
            }

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create dir {:?}: {e}", parent))?;
            }

            let mut data = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut data)
                .map_err(|e| format!("Failed to read zip entry {name}: {e}"))?;

            std::fs::write(&dest, &data)
                .map_err(|e| format!("Failed to write {:?}: {e}", dest))?;
        }

        Ok(())
    }
}

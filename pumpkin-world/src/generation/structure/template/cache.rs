//! Template caching for structure templates loaded from extracted Mojang assets.

use std::sync::Arc;

use dashmap::DashMap;

use super::{StructureTemplate, structure_template::TemplateError};
use pumpkin_util::asset_path;

/// A cache for loaded structure templates.
///
/// Templates are loaded lazily on first access and stored for reuse.
pub struct TemplateCache {
    cache: DashMap<String, Arc<StructureTemplate>>,
}

impl Default for TemplateCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateCache {
    /// Creates a new empty template cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Gets a template by `name`, loading it from disk if not cached.
    pub fn get(&self, name: &str) -> Option<Arc<StructureTemplate>> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);

        if let Some(template) = self.cache.get(name) {
            return Some(Arc::clone(&template));
        }

        let bytes = Self::load_template_bytes(name)?;

        match StructureTemplate::from_nbt_bytes(&bytes) {
            Ok(template) => {
                let arc = Arc::new(template);
                self.cache.insert(name.to_owned(), Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                tracing::error!("Failed to load template '{}': {}", name, e);
                None
            }
        }
    }

    /// Gets a template by name, returning an error if loading fails.
    pub fn get_or_error(&self, name: &str) -> Result<Arc<StructureTemplate>, TemplateError> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);

        if let Some(template) = self.cache.get(name) {
            return Ok(Arc::clone(&template));
        }

        let bytes = Self::load_template_bytes(name)
            .ok_or(TemplateError::MissingField("template file not found"))?;

        let template = StructureTemplate::from_nbt_bytes(&bytes)?;
        let arc = Arc::new(template);
        self.cache.insert(name.to_owned(), Arc::clone(&arc));
        Ok(arc)
    }

    /// Preloads a list of templates into the cache.
    pub fn preload(&self, names: &[&'static str]) {
        for name in names {
            if let Err(e) = self.get_or_error(name) {
                tracing::warn!("Failed to preload template '{}': {}", name, e);
            }
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Loads raw template bytes from the extracted Mojang asset cache.
    fn load_template_bytes(path: &str) -> Option<Vec<u8>> {
        asset_path::read_structure_bytes(path)
    }
}

/// Global template cache instance.
static GLOBAL_CACHE: std::sync::LazyLock<TemplateCache> =
    std::sync::LazyLock::new(TemplateCache::new);

/// Gets the global template cache.
#[must_use]
pub fn global_cache() -> &'static TemplateCache {
    &GLOBAL_CACHE
}

/// Gets a template by `name` from the global cache.
#[must_use]
pub fn get_template(name: &str) -> Option<Arc<StructureTemplate>> {
    global_cache().get(name)
}

/// Gets template pool element names from the on-disk directory listing.
#[must_use]
pub fn get_pool_elements(pool_id: &str) -> Option<Vec<String>> {
    let id = pool_id.strip_prefix("minecraft:").unwrap_or(pool_id);
    let templates = asset_path::list_pool_templates(id);
    if templates.is_empty() {
        None
    } else {
        Some(templates)
    }
}

/// Gets the JSON content of a template pool file.
#[must_use]
pub fn get_template_pool_json(path: &str) -> Option<String> {
    let id = path.strip_prefix("minecraft:").unwrap_or(path);
    asset_path::read_template_pool_json(id)
}

/// Gets the JSON content of a processor list file.
#[must_use]
pub fn get_processor_list_json(path: &str) -> Option<String> {
    let id = path.strip_prefix("minecraft:").unwrap_or(path);
    asset_path::read_processor_list_json(id)
}

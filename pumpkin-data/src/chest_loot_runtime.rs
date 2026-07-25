use std::sync::LazyLock;

use dashmap::DashMap;
use serde::Deserialize;

use pumpkin_util::asset_path;
use pumpkin_util::chest_loot_table::*;

// ── Deserialization types (mirror of vanilla JSON structure) ───────

#[derive(Deserialize)]
struct RawChestLootTable {
    pools: Vec<RawPool>,
}

#[derive(Deserialize)]
struct RawPool {
    rolls: RollsValue,
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RollsValue {
    Constant(f64),
    Object {
        #[serde(rename = "type")]
        _type: String,
        min: Option<f64>,
        max: Option<f64>,
    },
}

impl RollsValue {
    fn min(&self) -> i32 {
        match self {
            Self::Constant(v) => v.round() as i32,
            Self::Object { min, .. } => min.unwrap_or(1.0).round() as i32,
        }
    }
    fn max(&self) -> i32 {
        match self {
            Self::Constant(v) => v.round() as i32,
            Self::Object { max, .. } => max.unwrap_or(1.0).round() as i32,
        }
    }
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "type")]
    entry_type: String,
    name: Option<String>,
    #[serde(default = "one")]
    weight: i32,
    #[serde(default)]
    functions: Vec<RawFunction>,
}

fn one() -> i32 {
    1
}

#[derive(Deserialize)]
struct RawFunction {
    function: String,
    count: Option<CountValue>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CountValue {
    Constant(f64),
    Object {
        #[serde(rename = "type")]
        _type: String,
        min: Option<f64>,
        max: Option<f64>,
    },
}

impl CountValue {
    fn min(&self) -> i32 {
        match self {
            Self::Constant(v) => v.round() as i32,
            Self::Object { min, .. } => min.unwrap_or(1.0).round() as i32,
        }
    }
    fn max(&self) -> i32 {
        match self {
            Self::Constant(v) => v.round() as i32,
            Self::Object { max, .. } => max.unwrap_or(1.0).round() as i32,
        }
    }
}

// ── Runtime cache ─────────────────────────────────────────────────

static CHEST_TABLE_CACHE: LazyLock<DashMap<String, &'static ChestLootTable>> =
    LazyLock::new(DashMap::new);

/// Look up a chest loot table by key (e.g. `"minecraft:chests/abandoned_mineshaft"`).
/// Loads and caches the table from the extracted Mojang assets on first access.
#[must_use]
pub fn get_chest_loot_table(key: &str) -> Option<&'static ChestLootTable> {
    let id = key.strip_prefix("minecraft:").unwrap_or(key);

    if let Some(entry) = CHEST_TABLE_CACHE.get(id) {
        return Some(entry.value());
    }

    let dir = asset_path::chest_loot_dir()?;
    let path = dir.join(format!("{id}.json"));
    let raw = std::fs::read_to_string(&path).ok()?;
    let parsed: RawChestLootTable = serde_json::from_str(&raw).ok()?;

    let table = convert_and_leak(parsed);
    CHEST_TABLE_CACHE.insert(id.to_owned(), table);
    Some(table)
}

fn convert_and_leak(raw: RawChestLootTable) -> &'static ChestLootTable {
    let mut leaked_pools = Vec::with_capacity(raw.pools.len());

    for pool in raw.pools {
        let mut empty_weight: i32 = 0;
        let mut leaked_entries = Vec::new();

        for entry in pool.entries {
            match entry.entry_type.as_str() {
                "minecraft:empty" => {
                    empty_weight += entry.weight;
                }
                "minecraft:item" => {
                    if let Some(name) = entry.name {
                        let (min_count, max_count) = entry
                            .functions
                            .iter()
                            .find(|f| f.function == "minecraft:set_count")
                            .and_then(|f| f.count.as_ref())
                            .map(|c| (c.min(), c.max()))
                            .unwrap_or((1, 1));

                        leaked_entries.push(ChestLootEntry {
                            item: Box::leak(name.into_boxed_str()),
                            weight: entry.weight,
                            min_count,
                            max_count,
                        });
                    }
                }
                _ => {}
            }
        }

        leaked_pools.push(ChestLootPool {
            entries: Box::leak(leaked_entries.into_boxed_slice()),
            min_rolls: pool.rolls.min(),
            max_rolls: pool.rolls.max(),
            empty_weight,
        });
    }

    let table = ChestLootTable {
        pools: Box::leak(leaked_pools.into_boxed_slice()),
    };
    Box::leak(Box::new(table))
}

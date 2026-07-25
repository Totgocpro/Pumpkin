/// Re-export from pumpkin-util's asset_path module.
/// All Minecraft asset paths are managed centrally in pumpkin_util::asset_path.
pub use pumpkin_util::asset_path::{
    read_structure_bytes, read_template_pool_json, read_processor_list_json, list_pool_templates,
};

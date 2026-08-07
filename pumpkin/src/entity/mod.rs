use crate::{
    entity::item::ItemEntity,
    net::{ClientPlatform, bedrock::BedrockClient},
    server::Server,
    world::{
        World,
        chunker::is_within_view_distance,
        portal::{NetherPortal, PortalProcessor, PortalType, SourcePortalInfo},
    },
};
use arc_swap::ArcSwap;
use bytes::BufMut;
use crossbeam::atomic::AtomicCell;
use living::LivingEntity;
use player::Player;
use pumpkin_data::BlockState;
use pumpkin_data::biome::Biome;
use pumpkin_data::block_properties::blocks_movement;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::dimension::Dimension;
use pumpkin_data::entity::EntityStatus;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::meta_data_type::MetaDataType;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data::TrackedData;
use pumpkin_data::{Block, BlockDirection};
use pumpkin_data::{
    block_properties::{Facing, HorizontalFacing},
    damage::DamageType,
    entity::{EntityPose, EntityType},
    sound::{Sound, SoundCategory},
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_protocol::bedrock::client::{CAddActor, CSetActorMotion};
use pumpkin_protocol::codec::var_long::VarLong;
use pumpkin_protocol::java::client::play::{CUpdateEntityPos, CUpdateEntityPosRot};
use pumpkin_protocol::{
    PositionFlag,
    bedrock::client::{
        move_actor_delta::{
            CMoveActorDelta, MOVE_ACTOR_DELTA_FLAG_HAS_HEAD_YAW, MOVE_ACTOR_DELTA_FLAG_HAS_PITCH,
            MOVE_ACTOR_DELTA_FLAG_HAS_X, MOVE_ACTOR_DELTA_FLAG_HAS_Y,
            MOVE_ACTOR_DELTA_FLAG_HAS_YAW, MOVE_ACTOR_DELTA_FLAG_HAS_Z,
            MOVE_ACTOR_DELTA_FLAG_ON_GROUND,
        },
        move_player::CMovePlayer,
        set_actor_data::{
            CSetActorData, EntityMetadata, MetadataValue, PropertySyncData, entity_data_flag,
            entity_data_key,
        },
    },
    codec::var_int::VarInt,
    codec::var_ulong::VarULong,
    java::client::play::{
        CEntityPositionSync, CEntityVelocity, CHeadRot, CPlayerPosition, CSetEntityMetadata,
        CSetPassengers, CSpawnEntity, CUpdateEntityRot, Metadata, MetadataSerializer,
    },
    ser::NetworkWriteExt,
};
use pumpkin_util::math::vector3::Axis;
use pumpkin_util::math::{
    boundingbox::{BoundingBox, EntityDimensions},
    get_section_cord,
    position::BlockPos,
    vector2::Vector2,
    vector3::Vector3,
    wrap_degrees,
};
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::hover::HoverEvent;
use pumpkin_util::version::JavaMinecraftVersion;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{
        AtomicBool, AtomicI32, AtomicU8, AtomicU32,
        Ordering::{self, Relaxed},
    },
};
use tokio::sync::Mutex;
use uuid::Uuid;

pub mod ai;
pub mod area_effect_cloud;
pub mod attributes;
pub mod boss;
pub mod breath;
pub mod decoration;
pub mod effect;
pub mod experience_orb;
pub mod falling;
pub mod hunger;
pub mod item;
pub mod living;
pub mod mob;
pub mod passive;
pub mod player;
pub mod projectile;
pub mod projectile_deflection;
pub mod tnt;
pub mod r#type;
pub mod vehicle;

mod combat;
pub mod predicate;

/// The maximum number of scoreboard tags an entity can carry, matching Vanilla.
pub const MAX_SCOREBOARD_TAGS: usize = 1024;

/// Returns the [`EntityStatus`] that should be broadcast when the given
/// equipment slot breaks.
#[must_use]
pub const fn equipment_break_status(slot: &EquipmentSlot) -> EntityStatus {
    match slot {
        EquipmentSlot::MainHand(_) => EntityStatus::MainhandBreak,
        EquipmentSlot::OffHand(_) => EntityStatus::OffhandBreak,
        EquipmentSlot::Head(_) => EntityStatus::HeadBreak,
        EquipmentSlot::Chest(_) => EntityStatus::ChestBreak,
        EquipmentSlot::Legs(_) => EntityStatus::LegsBreak,
        EquipmentSlot::Feet(_) => EntityStatus::FeetBreak,
        EquipmentSlot::Body(_) => EntityStatus::BodyBreak,
        EquipmentSlot::Saddle(_) => EntityStatus::SaddleBreak,
    }
}

pub type EntityBaseFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub type TeleportFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub trait EntityBase: Send + Sync + NBTStorage + std::any::Any {
    /// Called every tick for this entity.
    ///
    /// The `caller` parameter is a reference to the entity that initiated the tick.
    /// This can be the same entity the method is being called on (`self`),
    /// but in some scenarios (e.g., interactions or events), it might be a different entity.
    ///
    /// The `server` parameter provides access to the game server instance.
    fn tick<'a>(
        &'a self,
        caller: &'a Arc<dyn EntityBase>,
        server: &'a Server,
    ) -> EntityBaseFuture<'a, ()> {
        Box::pin(async move {
            if let Some(living) = self.get_living_entity() {
                living.tick(caller, server).await;
            } else {
                self.get_entity().tick(caller, server).await;
            }
        })
    }

    fn get_job_site_pos(&self) -> Option<pumpkin_util::math::position::BlockPos> {
        None
    }

    fn get_home_pos(&self) -> Option<pumpkin_util::math::position::BlockPos> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any
    where
        Self: Sized,
    {
        self
    }

    fn get_eye_pos(&self) -> Vector3<f64> {
        self.get_entity().get_eye_pos()
    }

    fn get_looking_vector(&self) -> Vector3<f64> {
        let entity = self.get_entity();
        Vector3::from_yaw_pitch(entity.yaw.load(), entity.pitch.load())
    }

    fn init_data_tracker(&self) -> EntityBaseFuture<'_, ()> {
        Box::pin(async move {
            let entity = self.get_entity();

            // If the internal age is negative, it's a baby
            let is_baby = entity.age.load(Ordering::Relaxed) < 0;

            if is_baby {
                let mut bedrock_meta = EntityMetadata::new();
                bedrock_meta.set_flag(entity_data_key::FLAGS, entity_data_flag::BABY as u8, true);
                entity.send_meta_data(
                    &[Metadata::new(
                        TrackedData::BABY_ID,
                        MetaDataType::BOOLEAN,
                        true,
                    )],
                    Some(&bedrock_meta),
                );
            }
        })
    }
    fn set_variant_name(&self, _name: &str) {}

    // This method takes ownership of Arc<Self>, so the lifetime bounds are different.
    fn teleport(
        self: Arc<Self>,
        position: Vector3<f64>,
        yaw: Option<f32>,
        pitch: Option<f32>,
        world: Arc<World>,
    ) -> TeleportFuture
    where
        Self: 'static,
    {
        Box::pin(async move {
            self.get_entity().teleport(position, yaw, pitch, world);
        })
    }

    fn is_pushed_by_fluids(&self) -> bool {
        true
    }

    /// Whether the entity is immune from explosion knockback and damage
    fn is_immune_to_explosion(&self) -> bool {
        false
    }

    fn get_gravity(&self) -> f64 {
        0.0
    }

    fn tick_in_void<'a>(&'a self, _dyn_self: &'a dyn EntityBase) -> EntityBaseFuture<'a, ()> {
        Box::pin(async move { self.get_entity().remove().await })
    }

    /// Returns if damage was successful or not
    fn damage<'a>(
        &'a self,
        caller: &'a dyn EntityBase,
        amount: f32,
        damage_type: DamageType,
    ) -> EntityBaseFuture<'a, bool> {
        Box::pin(async move {
            caller
                .damage_with_context(caller, amount, damage_type, None, None, None)
                .await
        })
    }

    fn is_spectator(&self) -> bool {
        false
    }

    fn is_collidable(&self, _entity: Option<Box<dyn EntityBase>>) -> bool {
        false
    }

    fn can_hit(&self) -> bool {
        false
    }

    fn is_flutterer(&self) -> bool {
        false
    }

    /// Custom Y-axis velocity drag multiplier applied during `travel_in_air`.
    /// Bats return `Some(0.6)` to match vanilla's `travel()` override.
    fn get_y_velocity_drag(&self) -> Option<f64> {
        None
    }

    fn send_bedrock_spawn_packet<'a>(
        &'a self,
        client: &'a BedrockClient,
    ) -> EntityBaseFuture<'a, ()> {
        Box::pin(async move {
            let entity = self.get_entity();
            let runtime_id = entity.entity_id as u64;
            let packet = CAddActor::new(
                VarLong(runtime_id as i64),
                VarULong(runtime_id),
                self.get_entity().entity_type.resource_name.to_string(),
                entity.pos.load().to_f32_lossy(),
                entity.velocity.load().to_f32_lossy(),
                entity.pitch.load(),
                entity.yaw.load(),
                entity.head_yaw.load(),
                entity.body_yaw.load(),
                Vec::new(),
                entity.bedrock_metadata(),
                PropertySyncData {
                    int_properties: std::collections::HashMap::new(),
                    float_properties: std::collections::HashMap::new(),
                },
                Vec::new(),
            );
            client.send_game_packet(&packet).await;
        })
    }

    fn damage_with_context<'a>(
        &'a self,
        caller: &'a dyn EntityBase,
        amount: f32,
        damage_type: DamageType,
        position: Option<Vector3<f64>>,
        source: Option<&'a dyn EntityBase>,
        cause: Option<&'a dyn EntityBase>,
    ) -> EntityBaseFuture<'a, bool> {
        Box::pin(async move {
            if caller.get_living_entity().is_some() {
                return caller
                    .damage_with_context(caller, amount, damage_type, position, source, cause)
                    .await;
            }
            false
        })
    }

    /// Called when a player right-clicks this entity with an item.
    /// Returns true if the interaction was handled.
    fn interact<'a>(
        &'a self,
        _player: &'a Arc<Player>,
        _item_stack: &'a mut ItemStack,
    ) -> EntityBaseFuture<'a, bool> {
        Box::pin(async { false })
    }

    fn set_on_fire_for(&self, seconds: f32) {
        let entity = self.get_entity();
        // Exclude fire-immune entities (ex. certain items) from burn damage
        if !entity.fire_immune.load(Ordering::Relaxed) {
            self.set_on_fire_for_ticks((seconds * 20.0).floor() as u32);
        }
    }

    fn set_on_fire_for_ticks(&self, ticks: u32) {
        let entity = self.get_entity();
        if entity.fire_ticks.load(Ordering::Relaxed) < ticks as i32 {
            entity.fire_ticks.store(ticks as i32, Ordering::Relaxed);
        }
        // TODO: defrost
    }

    /// Called when a player collides with a entity
    fn on_player_collision<'a>(&'a self, _player: &'a Arc<Player>) -> EntityBaseFuture<'a, ()> {
        Box::pin(async {})
    }

    fn is_passenger(&self) -> EntityBaseFuture<'_, bool> {
        Box::pin(async move { self.get_entity().has_vehicle().await })
    }

    fn is_vehicle(&self) -> EntityBaseFuture<'_, bool> {
        Box::pin(async move { self.get_entity().has_passengers().await })
    }

    fn has_passenger<'a>(&'a self, other: &'a Arc<dyn EntityBase>) -> EntityBaseFuture<'a, bool> {
        Box::pin(async move {
            self.get_entity()
                .passengers
                .lock()
                .await
                .iter()
                .any(|p| p.get_entity().entity_id == other.get_entity().entity_id)
        })
    }

    fn move_entity<'a>(
        &'a self,
        caller: &'a Arc<dyn EntityBase>,
        motion: Vector3<f64>,
    ) -> EntityBaseFuture<'a, ()> {
        Box::pin(async move {
            self.get_entity().move_entity(caller, motion).await;
        })
    }

    fn is_pushable(&self) -> bool {
        false
    }

    fn push<'a>(&'a self, entity: &'a Arc<dyn EntityBase>) -> EntityBaseFuture<'a, ()> {
        Box::pin(async move {
            let self_entity = self.get_entity();
            let other_entity = entity.get_entity();

            if self_entity.no_clip.load(Ordering::Relaxed)
                || other_entity.no_clip.load(Ordering::Relaxed)
            {
                return;
            }

            {
                let passengers = self_entity.passengers.lock().await;
                if passengers
                    .iter()
                    .any(|p| p.get_entity().entity_id == other_entity.entity_id)
                {
                    return;
                }
            }
            {
                let passengers = other_entity.passengers.lock().await;
                if passengers
                    .iter()
                    .any(|p| p.get_entity().entity_id == self_entity.entity_id)
                {
                    return;
                }
            }

            let mut dx = other_entity.pos.load().x - self_entity.pos.load().x;
            let mut dz = other_entity.pos.load().z - self_entity.pos.load().z;
            let mut d = dx.abs().max(dz.abs());
            if d >= 0.01 {
                d = d.sqrt();
                dx /= d;
                dz /= d;
                let mut d2 = 1.0 / d;
                if d2 > 1.0 {
                    d2 = 1.0;
                }
                dx *= d2;
                dz *= d2;
                dx *= 0.05;
                dz *= 0.05;

                if !self_entity.has_passengers().await && self.is_pushable() {
                    let mut vel = self_entity.velocity.load();
                    vel.x -= dx;
                    vel.z -= dz;
                    self_entity.velocity.store(vel);
                    self_entity.send_velocity();
                }

                if !other_entity.has_passengers().await && entity.is_pushable() {
                    let mut vel = other_entity.velocity.load();
                    vel.x += dx;
                    vel.z += dz;
                    other_entity.velocity.store(vel);
                    other_entity.send_velocity();
                }
            }
        })
    }

    #[allow(clippy::too_many_lines)]
    fn push_entities<'a>(
        &'a self,
        dyn_self: &'a Arc<dyn EntityBase>,
    ) -> EntityBaseFuture<'a, bool> {
        Box::pin(async move {
            let mut picked_up = false;
            let mut pushed = false;
            let self_entity = self.get_entity();
            let entity_bb = self_entity.bounding_box.load();

            if !self.is_pushable() {
                return false;
            }

            let world = self_entity.world.load();

            let is_rideable_minecart = self_entity.entity_type.id == EntityType::MINECART.id;
            let is_abstract_minecart = is_rideable_minecart
                || self_entity.entity_type.id == EntityType::CHEST_MINECART.id
                || self_entity.entity_type.id == EntityType::COMMAND_BLOCK_MINECART.id
                || self_entity.entity_type.id == EntityType::FURNACE_MINECART.id
                || self_entity.entity_type.id == EntityType::HOPPER_MINECART.id
                || self_entity.entity_type.id == EntityType::SPAWNER_MINECART.id
                || self_entity.entity_type.id == EntityType::TNT_MINECART.id;

    // ... file continues with the complete merged content from the fork ...

use std::mem;

use glam::{IVec2, Vec2};
use rustc_hash::FxHashMap;

use crate::util::{
    lang::{
        entity::{Entity, StrongEntity},
        obj::{Obj, StrongObj},
        vec::ensure_index,
    },
    math::aabb::{Aabb, AabbI},
};

// === TileMap === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct LayerIndex(pub usize);

#[derive(Debug, Default)]
pub struct TileMap {
    pub layers: Vec<TileLayer>,
    pub layer_names: FxHashMap<String, usize>,
    pub materials: StrongObj<MaterialRegistry>,
}

impl TileMap {
    pub fn push_layer(&mut self, name: impl Into<String>, config: TileLayerConfig) -> LayerIndex {
        let index = self.layers.len();
        self.layers.push(TileLayer {
            data: TileLayerData::default(),
            config,
        });
        self.layer_names.insert(name.into(), index);
        LayerIndex(index)
    }

    pub fn layer(&self, name: &str) -> LayerIndex {
        LayerIndex(*self.layer_names.get(name).unwrap_or_else(|| {
            panic!(
                "failed to find layer with name {name:?}; layers: {:?}",
                self.layer_names
            )
        }))
    }

    pub fn layers(&self) -> impl Iterator<Item = LayerIndex> {
        (0..self.layers.len()).map(LayerIndex)
    }

    pub fn get(&mut self, layer: LayerIndex, pos: IVec2) -> MaterialInfo {
        self.materials.get().get(self.layers[layer.0].data.get(pos))
    }

    pub fn set(&mut self, layer: LayerIndex, pos: IVec2, info: MaterialInfo) {
        self.layers[layer.0].data.set(pos, info.id);
    }

    pub fn layer_config(&self, layer: LayerIndex) -> TileLayerConfig {
        self.layers[layer.0].config
    }

    pub fn actor_to_tile(&self, layer: LayerIndex, pos: Vec2) -> IVec2 {
        self.layer_config(layer).actor_to_tile(pos)
    }

    pub fn actor_aabb_to_tile(&self, layer: LayerIndex, aabb: Aabb) -> AabbI {
        self.layer_config(layer).actor_aabb_to_tile(aabb)
    }

    pub fn tile_to_actor_rect(&self, layer: LayerIndex, pos: IVec2) -> Aabb {
        self.layer_config(layer).tile_to_actor_rect(pos)
    }
}

#[derive(Debug, Clone)]
pub struct TileLayer {
    pub data: TileLayerData,
    pub config: TileLayerConfig,
}

#[derive(Debug, Copy, Clone)]
pub struct TileLayerConfig {
    pub size: f32,
    pub offset: Vec2,
}

impl TileLayerConfig {
    pub fn from_size(size: f32) -> Self {
        Self {
            size,
            offset: Vec2::ZERO,
        }
    }

    pub fn actor_to_tile(&self, Vec2 { x, y }: Vec2) -> IVec2 {
        IVec2::new(
            x.div_euclid(self.size).floor() as i32,
            y.div_euclid(self.size).floor() as i32,
        )
    }

    pub fn actor_aabb_to_tile(&self, aabb: Aabb) -> AabbI {
        AabbI {
            min: self.actor_to_tile(aabb.min),
            max: self.actor_to_tile(aabb.max),
        }
    }

    pub fn tile_to_actor_rect(&self, IVec2 { x, y }: IVec2) -> Aabb {
        Aabb::new_sized(
            Vec2::new(x as f32, y as f32) * self.size,
            Vec2::splat(self.size),
        )
    }
}

// === TileLayerData === //

const CHUNK_EDGE: i32 = 16;
const CHUNK_AREA: i32 = CHUNK_EDGE * CHUNK_EDGE;

fn decompose_world_pos(v: IVec2) -> (IVec2, IVec2) {
    let IVec2 { x, y } = v;

    (
        IVec2::new(x.div_euclid(CHUNK_EDGE), y.div_euclid(CHUNK_EDGE)),
        IVec2::new(x.rem_euclid(CHUNK_EDGE), y.rem_euclid(CHUNK_EDGE)),
    )
}

fn to_tile_index(v: IVec2) -> i32 {
    v.y * CHUNK_EDGE + v.x
}

#[derive(Debug, Clone, Default)]
pub struct TileLayerData {
    chunks: FxHashMap<IVec2, TileChunk>,
    cache_pos: IVec2,
    cache: Option<TileChunk>,
}

#[derive(Debug, Clone)]
struct TileChunk {
    non_air_count: i32,
    data: Box<[u16; CHUNK_AREA as usize]>,
}

impl TileLayerData {
    fn update_cache(&mut self, chunk: IVec2) {
        if chunk != self.cache_pos {
            // Unload the old cache if applicable.
            if let Some(cached_data) = self.cache.take() {
                if cached_data.non_air_count > 0 {
                    self.chunks.insert(self.cache_pos, cached_data);
                }
            }

            // Load the chunk into the cache if possible.
            self.cache_pos = chunk;
            if let Some(cached_data) = self.chunks.remove(&self.cache_pos) {
                self.cache = Some(cached_data);
            }
            self.cache_pos = chunk;
        }
    }

    pub fn get(&mut self, pos: IVec2) -> u16 {
        let (chunk, tile) = decompose_world_pos(pos);
        self.update_cache(chunk);

        self.cache
            .as_ref()
            .map_or(0, |cache| cache.data[to_tile_index(tile) as usize])
    }

    pub fn set(&mut self, pos: IVec2, data: u16) {
        let (chunk, tile) = decompose_world_pos(pos);
        self.update_cache(chunk);

        let cache = self.cache.get_or_insert_with(|| TileChunk {
            non_air_count: 0,
            data: Box::new([0; CHUNK_AREA as usize]),
        });

        let old_data = mem::replace(&mut cache.data[to_tile_index(tile) as usize], data);
        let was_not_air = (old_data != 0) as i32;
        let is_not_air = (data != 0) as i32;
        let delta = is_not_air - was_not_air;
        cache.non_air_count += delta;
    }
}

// === MaterialRegistry === //

#[derive(Debug, Default)]
pub struct MaterialRegistry {
    by_id: Vec<StrongEntity>,
    by_name: FxHashMap<String, u16>,
}

impl MaterialRegistry {
    pub fn register(&mut self, name: impl Into<String>, descriptor: StrongEntity) -> MaterialInfo {
        let (descriptor_guard, descriptor) = descriptor.split_guard();
        let id = u16::try_from(self.by_id.len()).expect("too many materials registered");
        self.by_id.push(descriptor_guard);
        self.by_name.insert(name.into(), id);

        MaterialInfo { id, descriptor }
    }

    pub fn get(&self, id: u16) -> MaterialInfo {
        MaterialInfo {
            id,
            descriptor: self.by_id[id as usize].entity(),
        }
    }

    pub fn get_by_name(&self, name: &str) -> MaterialInfo {
        self.get(self.by_name[name])
    }
}

#[derive(Debug, Copy, Clone)]
pub struct MaterialInfo {
    pub id: u16,
    pub descriptor: Entity,
}

#[derive(Debug)]
pub struct MaterialCache<T> {
    registry: Obj<MaterialRegistry>,
    cached: Vec<Option<Obj<T>>>,
}

impl<T: 'static> MaterialCache<T> {
    pub fn new(registry: Obj<MaterialRegistry>) -> Self {
        Self {
            registry,
            cached: Vec::new(),
        }
    }

    pub fn lookup_id(&mut self, id: u16) -> &Obj<T> {
        self.lookup(self.registry.get().get(id))
    }

    pub fn lookup(&mut self, info: MaterialInfo) -> &Obj<T> {
        let slot = ensure_index(&mut self.cached, info.id as usize);
        if let Some(slot) = slot {
            slot
        } else {
            slot.insert(info.descriptor.obj())
        }
    }
}

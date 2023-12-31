use std::{cell::RefCell, ops::ControlFlow};

use glam::{IVec2, Vec2};
use smallvec::SmallVec;

use crate::util::{
    lang::{
        entity::{CyclicCtor, Entity},
        obj::Obj,
    },
    math::{
        aabb::Aabb,
        glam::{add_magnitude, Axis2, Sign, Vec2Ext},
    },
};

use super::{
    tile::{MaterialCache, MaterialInfo, TileMap},
    transform::{Collider, ColliderManager, EntityExt, ObjTransformExt, Transform},
};

#[derive(Debug)]
pub struct KinematicManager {
    tile_map: Obj<TileMap>,
    collider_mgr: Obj<ColliderManager>,
    tile_cache: RefCell<MaterialCache<TileColliderDescriptor>>,
    tolerance: f32,
}

#[derive(Debug, Clone)]
pub struct TileColliderDescriptor {
    pub aabbs: SmallVec<[Aabb; 1]>,
}

impl TileColliderDescriptor {
    pub fn new(aabbs: impl IntoIterator<Item = Aabb>) -> Self {
        Self {
            aabbs: aabbs.into_iter().collect(),
        }
    }
}

impl KinematicManager {
    pub fn new() -> impl CyclicCtor<Self> {
        |me, _| {
            let tile_map = me.deep_obj::<TileMap>();
            let collider_mgr = me.deep_obj::<ColliderManager>();
            let tile_cache = MaterialCache::new(tile_map.get().materials.downgrade());

            Self {
                tile_map,
                collider_mgr,
                tile_cache: RefCell::new(tile_cache),
                tolerance: 0.01,
            }
        }
    }

    pub fn iter_colliders_in<B>(
        &self,
        check_aabb: Aabb,
        mut f: impl FnMut(AnyCollision<'_>) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        let layers = self.tile_map.get().layers();

        // Iterate through tiles
        {
            let mut tile_map = self.tile_map.get_mut();
            let mut tile_cache = self.tile_cache.borrow_mut();

            // For each layer...
            for layer in layers {
                let tile_check_aabb = tile_map.layers[layer.0].actor_aabb_to_tile(check_aabb);

                // For each visible tile...
                for pos in tile_check_aabb.inclusive().iter() {
                    let offset = tile_map.layers[layer.0].tile_to_actor_rect(pos).min;

                    let material = tile_map.get(layer, pos);
                    if material.id == 0 {
                        continue;
                    }

                    let info = tile_cache.lookup(material).get();

                    // For each aabb in that tile...
                    for tile_aabb in &info.aabbs {
                        let tile_aabb = tile_aabb.translated(offset);

                        // Check if the collision is real and yield it
                        if check_aabb.intersects(tile_aabb) {
                            drop(tile_map);
                            f(AnyCollision::Tile(material, pos, tile_aabb))?;
                            tile_map = self.tile_map.get_mut();
                        }
                    }
                }
            }
        }

        // Iterate through colliders
        {
            let collider_mgr = self.collider_mgr.get();

            for (collider, obj, aabb) in collider_mgr.iter_in(check_aabb) {
                f(AnyCollision::Collider(collider, obj, aabb))?;
            }
        }

        ControlFlow::Continue(())
    }

    pub fn move_by_raw(
        &self,
        aabb: Aabb,
        by: Vec2,
        mut filter: impl FnMut(AnyCollision) -> bool,
    ) -> Vec2 {
        let mut aabb = aabb;
        let mut total_by = Vec2::ZERO;

        for axis in Axis2::iter() {
            let signed_delta = by.get_axis(axis);
            let check_aabb =
                aabb.translate_extend(axis.unit_mag(add_magnitude(signed_delta, self.tolerance)));

            let mut delta = signed_delta.abs();

            cbit::cbit!(for collider in self.iter_colliders_in(check_aabb) {
                let collider_aabb = collider.aabb();
                if !filter(collider) {
                    continue;
                }

                let acceptable_delta = if signed_delta < 0. {
                    // We're moving to the left/top so we're presumably right/below the target.
                    aabb.min.get_axis(axis) - collider_aabb.max.get_axis(axis)
                } else {
                    // We're moving to the right/bottom so we're presumably left/above the target.
                    collider_aabb.min.get_axis(axis) - aabb.max.get_axis(axis)
                }
                .abs();

                let acceptable_delta = acceptable_delta - self.tolerance;
                delta = delta.min(acceptable_delta.max(0.));
            });

            let delta = axis.unit_mag(Sign::of_biased(signed_delta).unit_mag(delta));

            total_by += delta;
            aabb = aabb.translated(delta);
        }

        total_by
    }

    pub fn move_by(
        &self,
        aabb: Aabb,
        by: Vec2,
        ignore_descendants: Option<&Obj<Transform>>,
    ) -> Vec2 {
        self.move_by_raw(aabb, by, |collider| match (collider, ignore_descendants) {
            (AnyCollision::Collider(_, ob, _), Some(ignore_descendants)) => {
                !ob.get().transform().is_descendant_of(ignore_descendants)
            }
            _ => true,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub enum AnyCollision<'a> {
    Tile(MaterialInfo, IVec2, Aabb),
    Collider(Entity, &'a Obj<Collider>, Aabb),
}

impl AnyCollision<'_> {
    pub fn aabb(self) -> Aabb {
        match self {
            AnyCollision::Tile(_, _, aabb) => aabb,
            AnyCollision::Collider(_, _, aabb) => aabb,
        }
    }
}

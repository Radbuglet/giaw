use aunty::{CyclicCtor, Obj};
use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{
    game::services::{
        kinematic::{filter_descendants, KinematicManager},
        transform::{Collider, EntityExt, Transform},
    },
    rpc_path,
    util::math::aabb::Aabb,
};

rpc_path! {
    pub enum PlayerRpcs {
        Packet1,
        Packet2,
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerPacket1 {
    pub hello: u32,
    pub world: String,
}

#[derive(Debug)]
pub struct PlayerState {
    transform: Obj<Transform>,
    collider: Obj<Collider>,
    kinematic: Obj<KinematicManager>,
    pub velocity: Vec2,
}

impl PlayerState {
    pub fn new() -> impl CyclicCtor<Self> {
        |me, _ob| Self {
            transform: me.obj(),
            collider: me.obj(),
            kinematic: me.deep_obj(),
            velocity: Vec2::ZERO,
        }
    }

    pub fn is_on_ground(&self) -> bool {
        let kinematic = self.kinematic.get();
        let aabb = self.collider.get().global_aabb();
        let aabb = Aabb {
            min: Vec2::new(aabb.min.x, aabb.max.y),
            max: Vec2::new(aabb.max.x, aabb.max.y + 0.01),
        };

        kinematic.has_colliders_in(aabb, filter_descendants(Some(&self.transform)))
    }

    pub fn is_on_ceiling(&self) -> bool {
        let kinematic = self.kinematic.get();
        let aabb = self.collider.get().global_aabb();
        let aabb = Aabb {
            min: Vec2::new(aabb.min.x, aabb.min.y - 0.02),
            max: Vec2::new(aabb.max.x, aabb.min.y),
        };

        kinematic.has_colliders_in(aabb, filter_descendants(Some(&self.transform)))
    }

    pub fn update(&mut self, dt: f32) {
        let xform = self.transform.get();
        let aabb = self.collider.get().global_aabb();
        let kinematic = self.kinematic.get();

        self.velocity += Vec2::new(0., 18.) * dt;

        if self.is_on_ground() && self.velocity.y > 0. {
            self.velocity.y = 0.;
        }

        if self.is_on_ceiling() && self.velocity.y < 0. {
            self.velocity.y = 0.;
        }

        xform.translate_local(kinematic.move_by(
            aabb,
            self.velocity * dt,
            filter_descendants(Some(&self.transform)),
        ));
    }
}

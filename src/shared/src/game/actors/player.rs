use glam::Vec2;

use crate::{
    game::services::{
        kinematic::{filter_descendants, KinematicManager},
        transform::{Collider, EntityExt, Transform},
    },
    util::{
        lang::{entity::CyclicCtor, obj::Obj},
        math::aabb::Aabb,
    },
};

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

        cbit::cbit!(for collider in kinematic.iter_colliders_in(aabb) {
            if filter_descendants(Some(&self.transform))(collider) {
                return true;
            }
        });

        false
    }

    pub fn update(&mut self, dt: f32) {
        let xform = self.transform.get();
        let aabb = self.collider.get().global_aabb();
        let kinematic = self.kinematic.get();

        self.velocity += Vec2::new(0., 18.) * dt;

        xform.translate_local_pos(kinematic.move_by(
            aabb,
            self.velocity * dt,
            filter_descendants(Some(&self.transform)),
        ));
    }
}

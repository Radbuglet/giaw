use giaw_shared::{
    game::{
        actors::player::PlayerState,
        services::{
            actors::{ActorManager, DespawnHandler, UpdateHandler},
            collider::Collider,
            transform::Transform,
        },
    },
    util::{
        lang::{entity::Entity, obj::Obj},
        math::aabb::Aabb,
    },
};
use macroquad::{color::RED, math::Vec2, shapes::draw_circle};

use crate::{
    engine::scene::RenderHandler,
    game::services::camera::{VirtualCamera, VirtualCameraConstraints},
};

pub fn create_player(actors: &mut ActorManager, parent: Option<Obj<Transform>>) -> Entity {
    actors
        .spawn()
        .with_debug_label("player")
        .with_cyclic(Transform::new(parent))
        .with_cyclic(Collider::new_centered(Vec2::ZERO, Vec2::splat(2.)))
        .with_cyclic(PlayerState::new())
        .with_cyclic(VirtualCamera::new_attached(
            Aabb::ZERO,
            VirtualCameraConstraints::default().keep_visible_area(Vec2::splat(100.)),
        ))
        // Handlers
        .with_cyclic(|me, _| {
            let player = me.obj::<PlayerState>();
            UpdateHandler::new(move || {
                player.get_mut().update();
            })
        })
        .with_cyclic(|me, _| {
            let xform = me.obj::<Transform>();
            RenderHandler::new(move || {
                let xform = xform.get();
                let pos = xform.local_pos();
                draw_circle(pos.x, pos.y, 5., RED);
            })
        })
        .with_cyclic(|me, _| {
            DespawnHandler::new(move || {
                me.get::<Collider>().despawn();
            })
        })
}

use std::cell::RefCell;

use giaw_shared::{
    game::services::{
        actors::ActorManager,
        tile::{MaterialCache, TileMap},
        transform::EntityExt,
    },
    util::lang::{entity::CyclicCtor, obj::Obj},
};
use macroquad::{
    camera::{pop_camera_state, push_camera_state, set_camera},
    color::Color,
    miniquad::window::screen_size,
    shapes::draw_rectangle,
};

use crate::engine::scene::RenderHandler;

use super::camera::CameraManager;

#[derive(Debug)]
pub struct WorldRenderer {
    actors: Obj<ActorManager>,
    tile_map: Obj<TileMap>,
    camera_mgr: Obj<CameraManager>,
    tile_infos: RefCell<MaterialCache<TileVisualDescriptor>>,
}

impl WorldRenderer {
    pub fn new() -> impl CyclicCtor<Self> {
        |me, _| {
            let actors = me.deep_obj::<ActorManager>();
            let tile_map = me.deep_obj::<TileMap>();
            let camera_mgr = me.deep_obj::<CameraManager>();
            let tile_infos = MaterialCache::new(tile_map.get().materials.downgrade());

            Self {
                actors,
                tile_map,
                camera_mgr,
                tile_infos: RefCell::new(tile_infos),
            }
        }
    }

    pub fn render(&self) {
        let visible_aabb;
        {
            let Some(active_camera) = self.camera_mgr.get_mut().camera().cloned() else {
                return;
            };

            let mut active_camera = active_camera.get_mut();
            push_camera_state();
            active_camera.constrain(screen_size().into());
            visible_aabb = active_camera.visible_aabb();
            set_camera(&active_camera.snapshot());
        }

        // Draw tiles
        let layers = self.tile_map.get().layers();
        for layer in layers {
            {
                let mut tile_map = self.tile_map.get_mut();
                let mut tile_infos = self.tile_infos.borrow_mut();
                let visible_aabb = tile_map.layers[layer.0].actor_aabb_to_tile(visible_aabb);

                for pos in visible_aabb.inclusive().iter() {
                    let tile = tile_map.get(layer, pos);
                    if tile.id == 0 {
                        continue;
                    }

                    let tile_aabb = tile_map.layers[layer.0].tile_to_actor_rect(pos);
                    let color = tile_infos.lookup(tile).get().color;

                    draw_rectangle(
                        tile_aabb.x(),
                        tile_aabb.y(),
                        tile_aabb.w(),
                        tile_aabb.h(),
                        color,
                    );
                }
            }
        }

        // Draw actors
        cbit::cbit!(for actor in self.actors.get().iter_actors() {
            actor.get::<RenderHandler>().call();
        });

        pop_camera_state();
    }
}

#[derive(Debug, Clone)]
pub struct TileVisualDescriptor {
    pub color: Color,
}
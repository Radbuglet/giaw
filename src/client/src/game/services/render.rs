use std::cell::RefCell;

use aunty::{autoken::ImmutableBorrow, CyclicCtor, Obj};
use giaw_shared::util::game::{
    actors::ActorManager,
    tile::{MaterialCache, TileMap},
    transform::EntityExt,
};
use macroquad::{
    camera::{pop_camera_state, push_camera_state, set_camera},
    color::{Color, SKYBLUE},
    miniquad::window::screen_size,
    shapes::draw_rectangle,
    window::clear_background,
};

use crate::engine::scene::RenderHandler;

use super::camera::CameraManager;

#[derive(Debug)]
pub struct WorldRenderer {
    actors: Obj<ActorManager>,
    tile_map: Obj<TileMap>,
    camera_mgr: Obj<CameraManager>,
    mat_cache: RefCell<MaterialCache<TileVisualDescriptor>>,
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
                mat_cache: RefCell::new(tile_infos),
            }
        }
    }

    pub fn render(&self) {
        // Render background
        clear_background(SKYBLUE);

        // Bind camera
        let visible_aabb;
        {
            let Some(active_camera) = self.camera_mgr.get_mut().camera().cloned() else {
                return;
            };

            let mut active_camera = active_camera.get_mut();
            push_camera_state();
            active_camera.update(screen_size().into());
            visible_aabb = active_camera.visible_aabb();
            set_camera(&active_camera.snapshot());
        }

        // Draw tiles
        {
            let mut tile_map = self.tile_map.get_mut();
            let mut tile_infos = self.mat_cache.borrow_mut();

            for layer in tile_map.layers() {
                let layer_config = tile_map.layer_config(layer);
                let visible_aabb = layer_config.actor_aabb_to_tile(visible_aabb);

                for pos in visible_aabb.inclusive().iter() {
                    let tile = tile_map.get(layer, pos);
                    if tile.id == 0 {
                        continue;
                    }

                    let tile_aabb = layer_config.tile_to_actor_rect(pos);
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
            let loaner = ImmutableBorrow::new();
            if let Some(handler) = actor.try_get::<RenderHandler>(&loaner) {
                handler.call();
            };
        });

        pop_camera_state();
    }
}

#[derive(Debug, Clone)]
pub struct TileVisualDescriptor {
    pub color: Color,
}

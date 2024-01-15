use aunty::{delegate, Entity, Obj};
use giaw_shared::game::actors::inventory::ItemStackBase;
use macroquad::{color::Color, math::Vec2};

#[derive(Debug, Clone)]
pub struct ClientItemDescriptor {
    pub color: Color,
}

delegate! {
    pub fn ClientItemUseHandler(
        player: Entity,
        stack: Obj<ItemStackBase>,
        mode: InteractMode,
        world_from: Vec2,
        world_to: Vec2,
    )
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum InteractMode {
    Build,
    Break,
}

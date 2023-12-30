use autoken::ImmutableBorrow;
use rustc_hash::FxHashSet;

use crate::{
    delegate,
    util::lang::entity::{Entity, OwnedEntity},
};

use super::transform::Transform;

#[derive(Debug, Default)]
pub struct ActorManager {
    actors: FxHashSet<OwnedEntity>,
    queued_despawns: FxHashSet<Entity>,
}

impl ActorManager {
    pub fn spawn(&mut self) -> Entity {
        let (actor, actor_ref) = OwnedEntity::new().split_guard();
        self.actors.insert(actor);
        actor_ref
    }

    pub fn queue_despawn(&mut self, actor: &Transform) {
        self.queued_despawns.insert(actor.entity());

        for descendant in actor.children() {
            self.queue_despawn(&descendant.get());
        }
    }

    pub fn despawn_all(&mut self) {
        for actor in self.actors.drain() {
            let loaner = ImmutableBorrow::new();
            if let Some(dtor) = actor.try_get::<DespawnHandler>(&loaner) {
                dtor.call();
            };
        }
    }

    pub fn process_despawns(&mut self) {
        for actor in self.queued_despawns.drain() {
            if let Some(actor) = self.actors.take(&actor) {
                let loaner = ImmutableBorrow::new();
                if let Some(dtor) = actor.try_get::<DespawnHandler>(&loaner) {
                    dtor.call();
                };
            }
        }
    }
}

delegate! {
    pub fn DespawnHandler()
}

delegate! {
    pub fn UpdateHandler()
}

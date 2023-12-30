use autoken::ImmutableBorrow;
use rustc_hash::FxHashSet;

use crate::{
    delegate,
    util::lang::entity::{Entity, OwnedEntity},
};

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

    pub fn queue_despawn(&mut self, actor: Entity) {
        self.queued_despawns.insert(actor);
    }

    pub fn process_despawns(&mut self) {
        for actor in self.queued_despawns.drain() {
            if let Some(actor) = self.actors.take(&actor) {
                let loaner = ImmutableBorrow::new();
                if let Some(dtor) = actor.try_get::<ActorDespawnHandler>(&loaner) {
                    dtor.call();
                };
            }
        }
    }
}

delegate! {
    pub fn ActorDespawnHandler()
}

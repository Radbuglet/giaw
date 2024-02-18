use std::collections::HashMap;

use aunty::{Entity, StrongEntity};

use super::transport::QuadPeerId;

#[derive(Debug, Default)]
pub struct SessionManager {
    sessions: HashMap<QuadPeerId, StrongEntity>,
}

impl SessionManager {
    pub fn add_peer(&mut self, id: QuadPeerId) {
        self.sessions.insert(
            id,
            StrongEntity::new()
                .with_debug_label(format_args!("peer @ {id:?}"))
                .with(SessionState { id }),
        );
    }

    pub fn remove_peer(&mut self, id: QuadPeerId) {
        self.sessions.remove(&id);
    }

    pub fn peer_by_id(&self, id: QuadPeerId) -> Entity {
        self.sessions[&id].entity()
    }

    pub fn peers(&self) -> impl Iterator<Item = Entity> + '_ {
        self.sessions.values().map(StrongEntity::entity)
    }
}

#[derive(Debug)]
pub struct SessionState {
    pub id: QuadPeerId,
}

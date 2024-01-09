use std::{cell::Cell, thread::panicking};

#[derive(Debug, Clone, Default)]
pub struct DespawnStep {
    #[cfg(debug_assertions)]
    dropped: Cell<bool>,
}

impl DespawnStep {
    pub fn mark(&self) {
        #[cfg(debug_assertions)]
        {
			assert!(!self.dropped.get(), "component was despawned more than once");
            self.dropped.set(true);
        }
    }
}

impl Drop for DespawnStep {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            if !panicking() {
                assert!(self.dropped.get(), "component was not despawned before being dropped");
            }
        }
    }
}

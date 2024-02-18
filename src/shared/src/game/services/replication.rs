use serde::{Deserialize, Serialize};

use crate::rpc_path;

rpc_path! {
    pub enum GameSceneRpcs {
        SetTile,
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameSceneSetTile {
    pub x: i32,
    pub y: i32,
    pub state: i32,
}

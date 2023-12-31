use std::{future::Future, pin::pin};

use giaw_client::{engine::scene::RenderHandler, game::entry::create_game};
use giaw_shared::game::services::actors::{DespawnHandler, UpdateHandler};
use macroquad::{
    input::{is_key_pressed, is_quit_requested},
    miniquad::KeyCode,
    window::next_frame,
};

fn main() {
    color_backtrace::install();

    // Macroquad calls into FFI, which AuToken can't trace. We work around this by creating a fake
    // call to the actual main function.
    if false {
        fn thin_air<T>() -> T {
            unreachable!()
        }
        let _ = pin!(amain()).poll(&mut thin_air());
    }

    macroquad::Window::new("Game In A Week", amain());
}

async fn amain() {
    let scene = create_game(None);

    while !is_quit_requested() {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        scene.get::<UpdateHandler>().call();
        scene.get::<RenderHandler>().call();
        next_frame().await;
    }

    scene.get::<DespawnHandler>().call();
    drop(scene);
}

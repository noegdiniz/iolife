use super::GameState;
use super::camera;
use super::coords::TILE_PX;
use super::runtime::GuiRuntimeState;
use bevy::prelude::*;

const PAN_SPEED: f32 = 4.0;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_input);
    }
}

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut scroll: EventReader<bevy::input::mouse::MouseWheel>,
    mut runtime: ResMut<GuiRuntimeState>,
    mut game: ResMut<GameState>,
    mut camera_q: Query<(&mut OrthographicProjection, &mut Transform), With<Camera>>,
) {
    let Ok((mut projection, mut transform)) = camera_q.get_single_mut() else {
        return;
    };

    let speed = TILE_PX as f32 * PAN_SPEED / runtime.ticks_per_second.max(1) as f32;

    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        transform.translation.y += speed;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        transform.translation.y -= speed;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        transform.translation.x -= speed;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        transform.translation.x += speed;
    }

    if keys.just_pressed(KeyCode::Escape) {
        std::process::exit(0);
    }

    if keys.just_pressed(KeyCode::Space) {
        runtime.paused = !runtime.paused;
        runtime.last_error = None;
    }

    if keys.just_pressed(KeyCode::Period) {
        runtime.step_once = true;
        runtime.paused = true;
    }

    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        runtime.increase_tps();
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        runtime.decrease_tps();
    }

    if keys.just_pressed(KeyCode::KeyS) {
        let save_result = {
            let GameState {
                sim, persistence, ..
            } = &mut *game;
            persistence.save(sim, "gui_manual")
        };
        match save_result {
            Ok(()) => runtime.last_save_tick = game.sim.total_ticks(),
            Err(error) => runtime.last_error = Some(format!("falha ao salvar: {error}")),
        }
    }

    for ev in scroll.read() {
        if ev.y > 0.0 {
            runtime.ticks_per_second = runtime.ticks_per_second.max(1);
            let zoom = current_zoom(projection.scale) + 0.3;
            camera::apply_zoom(&mut projection, zoom.min(8.0));
        } else {
            let zoom = current_zoom(projection.scale) - 0.3;
            camera::apply_zoom(&mut projection, zoom.max(0.5));
        }
    }
}

fn current_zoom(scale: f32) -> f32 {
    if scale <= 0.0 { 1.0 } else { 1.0 / scale }
}

use super::GameState;
use super::coords::{tile_distance_sq, world_tile};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub const PICK_RADIUS_TILES_SQ: i32 = 4;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, select_agent_with_mouse);
    }
}

fn select_agent_with_mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera>>,
    mut game: NonSendMut<GameState>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let spatial = game.sim.spatial();
    let Some(tile) = world_tile(
        world_pos,
        spatial.grid.width as u32,
        spatial.grid.height as u32,
    ) else {
        return;
    };

    if let Some(agent_id) = nearest_agent_to_tile(&mut *game, tile) {
        game.selected_agent_id = Some(agent_id);
    }
}

pub fn nearest_agent_to_tile(game: &mut GameState, tile: IVec2) -> Option<u64> {
    game.sim
        .agent_views()
        .into_iter()
        .filter_map(|view| {
            let agent_tile = IVec2::new(view.position.x, view.position.y);
            let distance = tile_distance_sq(tile, agent_tile);
            (distance <= PICK_RADIUS_TILES_SQ).then_some((distance, view.id))
        })
        .min_by_key(|(distance, id)| (*distance, *id))
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_adapter::MockLlmAdapter;
    use crate::persistence::Persistence;
    use crate::sim_core::{Simulation, SimulationConfig};
    use tempfile::tempdir;

    #[test]
    fn nearest_agent_selects_agent_inside_radius() {
        let dir = tempdir().unwrap();
        let persistence = Persistence::open(&dir.path().join("gui-selection.db")).unwrap();
        let sim = Simulation::seeded(SimulationConfig {
            max_agents: 4,
            ..SimulationConfig::default()
        });
        let mut game = GameState {
            sim,
            llm: Box::new(MockLlmAdapter),
            persistence,
            selected_agent_id: None,
        };
        let first = game.sim.agent_views().first().cloned().unwrap();
        let picked =
            nearest_agent_to_tile(&mut game, IVec2::new(first.position.x, first.position.y));
        assert_eq!(picked, Some(first.id));
    }
}

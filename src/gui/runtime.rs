use super::GameState;
use crate::sim_core::{DEFAULT_TICKS_PER_SECOND, MAX_TICKS_PER_SECOND, tick_interval_ms};
use bevy::prelude::*;

pub const GUI_AUTOSAVE_EVERY_TICKS: u64 = 60;

pub struct GuiRuntimePlugin;

impl Plugin for GuiRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuiRuntimeState>()
            .add_systems(Update, advance_simulation);
    }
}

#[derive(Resource, Debug, Clone)]
pub struct GuiRuntimeState {
    pub paused: bool,
    pub ticks_per_second: u32,
    pub accumulator_ms: f32,
    pub step_once: bool,
    pub last_error: Option<String>,
    pub last_save_tick: u64,
    pub map_dirty: bool,
}

impl Default for GuiRuntimeState {
    fn default() -> Self {
        Self {
            paused: true,
            ticks_per_second: DEFAULT_TICKS_PER_SECOND,
            accumulator_ms: 0.0,
            step_once: false,
            last_error: None,
            last_save_tick: 0,
            map_dirty: true,
        }
    }
}

impl GuiRuntimeState {
    pub fn increase_tps(&mut self) {
        self.ticks_per_second = (self.ticks_per_second + 1).min(MAX_TICKS_PER_SECOND);
    }

    pub fn decrease_tps(&mut self) {
        self.ticks_per_second = self.ticks_per_second.saturating_sub(1).max(1);
    }
}

fn advance_simulation(
    time: Res<Time>,
    mut game: ResMut<GameState>,
    mut runtime: ResMut<GuiRuntimeState>,
) {
    let interval_ms = tick_interval_ms(runtime.ticks_per_second) as f32;
    runtime.accumulator_ms += time.delta_secs() * 1000.0;

    let should_step =
        runtime.step_once || (!runtime.paused && runtime.accumulator_ms >= interval_ms);
    if !should_step {
        return;
    }

    runtime.step_once = false;
    runtime.accumulator_ms = 0.0;

    let tick_result = {
        let GameState { sim, llm, .. } = &mut *game;
        sim.tick(llm.as_ref())
    };

    match tick_result {
        Ok(()) => {
            runtime.last_error = None;
            runtime.map_dirty = true;
        }
        Err(error) => {
            runtime.paused = true;
            runtime.last_error = Some(error.to_string());
            return;
        }
    }

    let total_ticks = game.sim.total_ticks();
    if total_ticks.saturating_sub(runtime.last_save_tick) >= GUI_AUTOSAVE_EVERY_TICKS {
        let save_result = {
            let GameState {
                sim, persistence, ..
            } = &mut *game;
            persistence.save(sim, "gui_auto")
        };
        match save_result {
            Ok(()) => runtime.last_save_tick = total_ticks,
            Err(error) => {
                runtime.paused = true;
                runtime.last_error = Some(format!("falha ao salvar checkpoint GUI: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tps_adjustment_is_clamped() {
        let mut runtime = GuiRuntimeState::default();
        runtime.ticks_per_second = MAX_TICKS_PER_SECOND;
        runtime.increase_tps();
        assert_eq!(runtime.ticks_per_second, MAX_TICKS_PER_SECOND);
        runtime.ticks_per_second = 1;
        runtime.decrease_tps();
        assert_eq!(runtime.ticks_per_second, 1);
    }
}

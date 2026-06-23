use crate::llm_adapter::LlmAdapter;
use crate::persistence::Persistence;
use crate::sim_core::Simulation;
use agent_render::AgentRenderPlugin;
use anyhow::Result;
use bevy::prelude::*;
use camera::CameraPlugin;
use input::InputPlugin;
use map_render::MapRenderPlugin;
use palette::BACKGROUND;
use runtime::GuiRuntimePlugin;
use selection::SelectionPlugin;
use ui::GuiUiPlugin;

pub mod agent_render;
pub mod camera;
pub mod coords;
pub mod input;
pub mod map_render;
pub mod palette;
pub mod runtime;
pub mod selection;
pub mod ui;

#[derive(Resource)]
pub struct GameState {
    pub sim: Simulation,
    pub llm: Box<dyn LlmAdapter>,
    pub persistence: Persistence,
    pub selected_agent_id: Option<u64>,
}

pub fn run_gui(sim: Simulation, llm: Box<dyn LlmAdapter>, persistence: Persistence) -> Result<()> {
    let village_name = sim.village_name().to_string();
    let selected = sim.agent_views().first().map(|v| v.id);

    App::new()
        .add_plugins((DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("Vila Medieval - {}", village_name),
                    resolution: (1280.0, 800.0).into(),
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest()),))
        .insert_resource(ClearColor(Color::srgb(
            BACKGROUND.0 as f32 / 255.0,
            BACKGROUND.1 as f32 / 255.0,
            BACKGROUND.2 as f32 / 255.0,
        )))
        .insert_resource(GameState {
            sim,
            llm,
            persistence,
            selected_agent_id: selected,
        })
        .add_plugins((
            GuiRuntimePlugin,
            MapRenderPlugin,
            AgentRenderPlugin,
            CameraPlugin,
            InputPlugin,
            SelectionPlugin,
            GuiUiPlugin,
        ))
        .run();

    Ok(())
}

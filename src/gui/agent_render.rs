use super::GameState;
use super::coords::{TILE_PX, tile_world};
use super::palette;
use crate::world_model::AgentLifeStatus;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

const AGENT_SIZE: f32 = (TILE_PX as f32) * 0.75;
const SELECTED_AGENT_SIZE: f32 = (TILE_PX as f32) * 1.15;
const AGENT_Z: f32 = 10.0;

pub struct AgentRenderPlugin;

impl Plugin for AgentRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_all)
            .add_systems(Update, sync_positions);
    }
}

#[derive(Component)]
pub struct AgentMarker(pub u64);

fn spawn_all(mut commands: Commands, mut sim: NonSendMut<GameState>) {
    let spatial = sim.sim.spatial();
    let (grid_w, grid_h) = (spatial.grid.width as u32, spatial.grid.height as u32);
    for view in sim.sim.agent_views() {
        spawn_agent_sprite(&mut commands, &view, sim.selected_agent_id, grid_w, grid_h);
    }
}

fn sync_positions(
    mut commands: Commands,
    mut sim: NonSendMut<GameState>,
    mut query: Query<(Entity, &mut Transform, &mut Sprite, &AgentMarker)>,
) {
    let spatial = sim.sim.spatial();
    let (grid_w, grid_h) = (spatial.grid.width as u32, spatial.grid.height as u32);
    let views = sim.sim.agent_views();
    let view_by_id = views
        .iter()
        .map(|view| (view.id, view))
        .collect::<HashMap<_, _>>();
    let active_ids = view_by_id.keys().copied().collect::<HashSet<_>>();
    let mut existing = HashSet::new();

    for (entity, mut transform, mut sprite, marker) in query.iter_mut() {
        if !active_ids.contains(&marker.0) {
            commands.entity(entity).despawn();
            continue;
        }
        let Some(view) = view_by_id.get(&marker.0) else {
            continue;
        };
        existing.insert(marker.0);
        let pos = tile_world(view.position.x, view.position.y, grid_w, grid_h);
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        transform.translation.z = if Some(view.id) == sim.selected_agent_id {
            AGENT_Z + 1.0
        } else {
            AGENT_Z
        };
        sprite.color = agent_color(
            &view.role_id,
            view.life_status,
            view.id,
            sim.selected_agent_id,
            view.active_conversation_id.is_some(),
        );
        sprite.custom_size = Some(Vec2::splat(if Some(view.id) == sim.selected_agent_id {
            SELECTED_AGENT_SIZE
        } else {
            AGENT_SIZE
        }));
    }

    for view in views {
        if !existing.contains(&view.id) {
            spawn_agent_sprite(&mut commands, &view, sim.selected_agent_id, grid_w, grid_h);
        }
    }
}

fn spawn_agent_sprite(
    commands: &mut Commands,
    view: &crate::sim_core::AgentView,
    selected: Option<u64>,
    grid_w: u32,
    grid_h: u32,
) {
    let pos = tile_world(view.position.x, view.position.y, grid_w, grid_h);
    let color = agent_color(
        &view.role_id,
        view.life_status,
        view.id,
        selected,
        view.active_conversation_id.is_some(),
    );
    commands.spawn((
        Sprite {
            color,
            custom_size: Some(Vec2::splat(if Some(view.id) == selected {
                SELECTED_AGENT_SIZE
            } else {
                AGENT_SIZE
            })),
            ..default()
        },
        Transform::from_xyz(pos.x, pos.y, AGENT_Z),
        AgentMarker(view.id),
        Name::new(format!("Agente {}", view.id)),
    ));
}

fn agent_color(
    role_id: &str,
    life_status: AgentLifeStatus,
    id: u64,
    selected: Option<u64>,
    conversing: bool,
) -> Color {
    if Some(id) == selected {
        return rgb(palette::AGENT_SELECTED);
    }
    if conversing {
        return rgb(palette::AGENT_CONVERSING);
    }
    if life_status == AgentLifeStatus::Morto {
        return rgb(palette::AGENT_DEAD);
    }
    rgb(match role_id {
        "lider_local" => palette::AGENT_LEADER,
        "guarda" => palette::AGENT_GUARD,
        "campones" => palette::AGENT_FARMER,
        "ferreiro" => palette::AGENT_SMITH,
        "padeiro" => palette::AGENT_BAKER,
        "taverneiro" => palette::AGENT_TAVERN,
        _ => palette::AGENT_OTHER,
    })
}

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

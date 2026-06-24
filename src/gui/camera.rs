use super::GameState;
use super::coords::map_size_px;
use super::palette;
use bevy::prelude::*;

const INITIAL_ZOOM: f32 = 2.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera);
    }
}

fn spawn_camera(mut commands: Commands, game: NonSend<GameState>) {
    let mut proj = OrthographicProjection::default_2d();
    proj.scale = 1.0 / INITIAL_ZOOM;
    let (r, g, b) = palette::BACKGROUND;
    let spatial = game.sim.spatial();
    let map_size = map_size_px(spatial.grid.width as u32, spatial.grid.height as u32);

    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
            )),
            ..default()
        },
        Projection::Orthographic(proj),
        Transform::from_xyz(0.0, 0.0, 999.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        Name::new(format!("Camera mapa {}x{}", map_size.x, map_size.y)),
    ));
}

pub fn apply_zoom(projection: &mut Projection, zoom: f32) {
    if let Projection::Orthographic(projection) = projection {
        projection.scale = 1.0 / zoom.clamp(0.5, 8.0);
    }
}

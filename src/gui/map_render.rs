use super::GameState;
use super::coords::{TILE_PX, map_size_px};
use super::palette;
use super::runtime::GuiRuntimeState;
use crate::world_model::TileKind;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub struct MapRenderPlugin;

impl Plugin for MapRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapState>()
            .add_systems(Startup, setup)
            .add_systems(Update, redraw_if_dirty);
    }
}

#[derive(Resource, Default)]
struct MapState {
    handle: Option<Handle<Image>>,
    grid_w: u32,
    grid_h: u32,
}

fn setup(
    mut commands: Commands,
    sim: Res<GameState>,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<MapState>,
) {
    let spatial = sim.sim.spatial();
    state.grid_w = spatial.grid.width as u32;
    state.grid_h = spatial.grid.height as u32;
    let map_size = map_size_px(state.grid_w, state.grid_h);

    let image = build_map_image(&sim, state.grid_w, state.grid_h);
    let handle = images.add(image);

    commands.spawn((
        Sprite {
            image: handle.clone(),
            custom_size: Some(map_size),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
        Name::new("Mapa da vila"),
    ));

    state.handle = Some(handle);
}

fn redraw_if_dirty(
    sim: Res<GameState>,
    state: Res<MapState>,
    mut runtime: ResMut<GuiRuntimeState>,
    mut images: ResMut<Assets<Image>>,
) {
    if !runtime.map_dirty {
        return;
    }
    let Some(handle) = &state.handle else { return };
    let Some(image) = images.get_mut(handle) else {
        return;
    };
    paint_tiles(image, &sim, state.grid_w, state.grid_h);
    runtime.map_dirty = false;
}

fn build_map_image(sim: &GameState, gw: u32, gh: u32) -> Image {
    let img_w = gw * TILE_PX;
    let img_h = gh * TILE_PX;
    let mut image = Image::new_fill(
        Extent3d {
            width: img_w,
            height: img_h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0u8, 0, 0, 255],
    );
    image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    paint_tiles(&mut image, sim, gw, gh);
    image
}

fn paint_tiles(image: &mut Image, sim: &GameState, gw: u32, _gh: u32) {
    let Some(data) = image.data.as_mut() else {
        return;
    };
    let stride = (gw * TILE_PX) as usize * 4;
    for tile in &sim.sim.spatial().grid.tiles {
        let (r, g, b) = tile_color(tile.kind);
        let base_x = tile.coord.x as u32 * TILE_PX;
        let base_y = tile.coord.y as u32 * TILE_PX;
        for dy in 0..TILE_PX {
            let row_start = (base_y + dy) as usize * stride + base_x as usize * 4;
            for dx in 0..TILE_PX {
                let i = row_start + dx as usize * 4;
                if i + 3 < data.len() {
                    data[i] = r;
                    data[i + 1] = g;
                    data[i + 2] = b;
                    data[i + 3] = 255;
                }
            }
        }
    }
}

fn tile_color(kind: TileKind) -> (u8, u8, u8) {
    match kind {
        TileKind::Grass => palette::TILE_GRASS,
        TileKind::Road => palette::TILE_ROAD,
        TileKind::Floor => palette::TILE_FLOOR,
        TileKind::Wall => palette::TILE_WALL,
        TileKind::Door => palette::TILE_DOOR,
        TileKind::Field => palette::TILE_FIELD,
        TileKind::Forest => palette::TILE_FOREST,
        TileKind::Rock => palette::TILE_ROCK,
    }
}

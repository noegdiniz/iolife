use bevy::prelude::*;

pub const TILE_PX: u32 = 6;

pub fn map_size_px(grid_w: u32, grid_h: u32) -> Vec2 {
    Vec2::new((grid_w * TILE_PX) as f32, (grid_h * TILE_PX) as f32)
}

pub fn tile_world(tx: i32, ty: i32, grid_w: u32, grid_h: u32) -> Vec2 {
    let size = map_size_px(grid_w, grid_h);
    Vec2::new(
        tx as f32 * TILE_PX as f32 + TILE_PX as f32 / 2.0 - size.x / 2.0,
        size.y / 2.0 - (ty as f32 * TILE_PX as f32 + TILE_PX as f32 / 2.0),
    )
}

pub fn world_tile(world: Vec2, grid_w: u32, grid_h: u32) -> Option<IVec2> {
    let size = map_size_px(grid_w, grid_h);
    let x = ((world.x + size.x / 2.0) / TILE_PX as f32).floor() as i32;
    let y = ((size.y / 2.0 - world.y) / TILE_PX as f32).floor() as i32;
    (x >= 0 && y >= 0 && x < grid_w as i32 && y < grid_h as i32).then_some(IVec2::new(x, y))
}

pub fn tile_distance_sq(a: IVec2, b: IVec2) -> i32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_world_roundtrips_to_tile() {
        let world = tile_world(12, 7, 50, 30);
        assert_eq!(world_tile(world, 50, 30), Some(IVec2::new(12, 7)));
    }

    #[test]
    fn outside_world_returns_none() {
        assert_eq!(world_tile(Vec2::new(-999.0, 0.0), 20, 20), None);
    }
}

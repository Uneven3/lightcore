use bevy::prelude::*;

pub(crate) const GRID_W: i32 = 8;
pub(crate) const GRID_H: i32 = 8;
pub(crate) const TILE: f32 = 70.0;



#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct GridPos {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

pub(crate) fn to_world(p: GridPos) -> Vec3 {
    let ox = (GRID_W as f32 - 1.0) / 2.0;
    let oy = (GRID_H as f32 - 1.0) / 2.0;
    Vec3::new((p.x as f32 - ox) * TILE, (p.y as f32 - oy) * TILE, 0.0)
}

pub(crate) fn to_grid(world: Vec2) -> Option<GridPos> {
    let ox = (GRID_W as f32 - 1.0) / 2.0;
    let oy = (GRID_H as f32 - 1.0) / 2.0;
    let x = (world.x / TILE + ox).round() as i32;
    let y = (world.y / TILE + oy).round() as i32;
    ((0..GRID_W).contains(&x) && (0..GRID_H).contains(&y)).then_some(GridPos { x, y })
}

/// The 4 orthogonal neighbors of `p` (may fall outside the board — callers only use these to test
/// membership in an existing position set, so out-of-bounds entries are harmless).
pub(crate) fn orthogonal_neighbors(p: GridPos) -> [GridPos; 4] {
    [
        GridPos { x: p.x - 1, y: p.y },
        GridPos { x: p.x + 1, y: p.y },
        GridPos { x: p.x, y: p.y - 1 },
        GridPos { x: p.x, y: p.y + 1 },
    ]
}

#[derive(Resource, Default, Debug, Clone)]
pub(crate) struct ShadowSet(pub(crate) std::collections::HashSet<(i32, i32)>);


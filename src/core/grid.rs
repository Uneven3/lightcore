use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

pub(crate) const GRID_W: i32 = 8;
pub(crate) const GRID_H: i32 = 8;
pub(crate) const TILE: f32 = 70.0;

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct GridPos {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

/// Declarative geometry for a board. Coordinates are intentionally global: separate subgrids can
/// leave gaps between their x ranges, which prevents matches/swaps crossing from one grid to the
/// next while keeping the existing ECS `GridPos` component stable.
#[derive(Resource, Clone, Debug)]
pub(crate) struct GridLayout {
    cells: HashSet<GridPos>,
    fall_targets: HashMap<GridPos, GridPos>,
    spawn_entries: HashSet<GridPos>,
    spark_exits: HashSet<GridPos>,
}

impl Default for GridLayout {
    fn default() -> Self {
        Self::rectangles(&[(0, 0, GRID_W, GRID_H)])
    }
}

impl GridLayout {
    /// Builds any number of rectangular subgrids. `x/y` are global board coordinates; leaving a
    /// coordinate gap between rectangles keeps them logically disconnected.
    pub(crate) fn rectangles(rectangles: &[(i32, i32, i32, i32)]) -> Self {
        let mut cells = HashSet::new();
        let mut spawn_entries = HashSet::new();
        let mut spark_exits = HashSet::new();
        for &(x0, y0, width, height) in rectangles {
            for x in x0..x0 + width {
                for y in y0..y0 + height {
                    cells.insert(GridPos { x, y });
                }
                if height > 0 {
                    spawn_entries.insert(GridPos {
                        x,
                        y: y0 + height - 1,
                    });
                    spark_exits.insert(GridPos { x, y: y0 });
                }
            }
        }
        Self {
            cells,
            fall_targets: HashMap::new(),
            spawn_entries,
            spark_exits,
        }
    }

    pub(crate) fn contains(&self, pos: GridPos) -> bool {
        self.cells.contains(&pos)
    }

    pub(crate) fn cells(&self) -> &HashSet<GridPos> {
        &self.cells
    }

    /// Adds a directed gravity route. A portal is one presentation of this rule; the gravity
    /// system only knows that a piece at `from` continues falling at `to`.
    pub(crate) fn add_fall_route(&mut self, from: GridPos, to: GridPos) {
        assert!(self.contains(from) && self.contains(to));
        self.fall_targets.insert(from, to);
    }

    pub(crate) fn fall_target(&self, from: GridPos) -> Option<GridPos> {
        self.fall_targets.get(&from).copied()
    }

    pub(crate) fn spawn_entries(&self) -> impl Iterator<Item = GridPos> + '_ {
        self.spawn_entries.iter().copied()
    }

    pub(crate) fn cells_in_column(&self, x: i32) -> impl Iterator<Item = GridPos> + '_ {
        self.cells.iter().copied().filter(move |pos| pos.x == x)
    }

    /// Replaces the cells that consume falling sparks. A layout may expose only its terminal
    /// subgrid, rather than every bottom row, by setting this explicitly.
    pub(crate) fn set_spark_exits(&mut self, exits: impl IntoIterator<Item = GridPos>) {
        let exits: HashSet<_> = exits.into_iter().collect();
        assert!(exits.iter().all(|pos| self.contains(*pos)));
        self.spark_exits = exits;
    }

    pub(crate) fn spark_exits(&self) -> impl Iterator<Item = GridPos> + '_ {
        self.spark_exits.iter().copied()
    }

    pub(crate) fn is_spark_exit(&self, pos: GridPos) -> bool {
        self.spark_exits.contains(&pos)
    }

    pub(crate) fn top_cell_in_column(&self, x: i32) -> Option<GridPos> {
        self.cells_in_column(x).max_by_key(|pos| pos.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_and_spark_exits_are_layout_rules() {
        let mut layout = GridLayout::rectangles(&[(0, 1, 2, 3), (5, 1, 2, 3)]);
        let from = GridPos { x: 0, y: 1 };
        let to = GridPos { x: 5, y: 3 };
        layout.add_fall_route(from, to);
        layout.set_spark_exits([GridPos { x: 5, y: 1 }, GridPos { x: 6, y: 1 }]);

        assert_eq!(layout.fall_target(from), Some(to));
        assert!(layout.is_spark_exit(GridPos { x: 5, y: 1 }));
        assert!(!layout.is_spark_exit(GridPos { x: 0, y: 1 }));
    }
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
    Some(GridPos { x, y })
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

/// Snapshot of every cell that blocks gravity.  The snapshot is rebuilt from the
/// `BlocksGravity` capability and deliberately has no knowledge of obstacle kinds.
#[derive(Resource, Default, Debug, Clone)]
pub(crate) struct GravityBlockSet(pub(crate) std::collections::HashSet<(i32, i32)>);

use bevy::prelude::*;

pub(crate) const GRID_W: i32 = 8;
pub(crate) const GRID_H: i32 = 8;
pub(crate) const TILE: f32 = 70.0;

/// Master timing resource for the ray/bolt animation chain. All pop delays, bolt movement, and
/// TravelingLight travel times derive from these fields — changing one slider keeps the whole
/// chain synchronized without touching multiple files.
#[derive(Resource)]
pub(crate) struct RaySettings {
    /// px/s — speed of the bolt front; pop delay formula and bolt visual share this value.
    pub speed: f32,
    /// × TILE — half-length of the bolt capsule sprite.
    pub bolt_length_frac: f32,
    /// seconds — membrane fade duration (PopAnim). The pop delay formula uses this so that the
    /// light starts fading exactly when the bolt's front reaches it:
    ///   pop_delay(d) = (d − bolt_length − speed × pop_duration).max(0) / speed
    pub pop_duration: f32,
    /// seconds — how long a TravelingLight (Supernova/Starburst/Blackhole) takes to travel.
    pub trail_duration: f32,
    /// seconds — stagger between consecutive Starburst beam arrivals.
    pub stagger_secs: f32,
    /// seconds — cap on accumulated Starburst stagger.
    pub stagger_max: f32,
    /// × TILE — perpendicular glow halo width of the bolt sprite.
    pub bolt_width_frac: f32,
}

impl Default for RaySettings {
    fn default() -> Self {
        Self {
            speed: 750.0,
            bolt_length_frac: 0.8,
            pop_duration: 0.08,
            trail_duration: 0.32,
            stagger_secs: 0.035,
            stagger_max: 0.5,
            bolt_width_frac: 0.55,
        }
    }
}

impl RaySettings {
    pub fn bolt_length(&self) -> f32 {
        self.bolt_length_frac * TILE
    }
    pub fn bolt_width(&self) -> f32 {
        self.bolt_width_frac * TILE
    }
    /// Pop delay for a light at world-space distance `d` from the ray source.
    pub fn pop_delay(&self, d: f32) -> f32 {
        (d - self.bolt_length() - self.speed * self.pop_duration).max(0.0) / self.speed
    }
}

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

use bevy::prelude::*;

use super::grid::{GRID_W, GridPos};
use super::light::LightColor;

pub(crate) const MOVES: u32 = 30;

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum LevelGoal {
    Score(u32),
    Sparks,
    ClearShadow,
    /// Contrarreloj con meta: hay que capturar suficientes lightcores antes de que se acabe el
    /// reloj (duración en segundos). No termina por movimientos — ver `LevelTimer`.
    TimedScore {
        secs: f32,
        target: u32,
    },
    /// Recolectar una cantidad específica de lightcores de un color.
    CollectColor {
        color: LightColor,
        target: u32,
    },
}

#[derive(Resource, Clone)]
pub(crate) struct LevelConfig {
    pub(crate) level: u32,
    pub(crate) total_moves: u32,
    pub(crate) goal: LevelGoal,
    pub(crate) sparks_total: u32,
    pub(crate) shadow_positions: Vec<GridPos>,
    pub(crate) blocker_positions: Vec<GridPos>,
    pub(crate) grade_baseline: u32,
}

pub(crate) fn make_level(n: u32) -> LevelConfig {
    match n {
        1 => LevelConfig {
            level: 1,
            total_moves: 24,
            goal: LevelGoal::Score(150),
            sparks_total: 0,
            shadow_positions: vec![],
            blocker_positions: vec![],
            grade_baseline: 300,
        },
        2 => LevelConfig {
            level: 2,
            total_moves: 26,
            goal: LevelGoal::Sparks,
            sparks_total: 3,
            shadow_positions: vec![],
            blocker_positions: vec![],
            grade_baseline: 360,
        },
        3 => LevelConfig {
            level: 3,
            total_moves: 28,
            goal: LevelGoal::ClearShadow,
            sparks_total: 0,
            shadow_positions: (2i32..=6)
                .flat_map(|x| [2i32, 3i32].map(move |y| GridPos { x, y }))
                .collect(),
            blocker_positions: vec![],
            grade_baseline: 420,
        },
        4 => LevelConfig {
            level: 4,
            // El límite real es el reloj (`LevelTimer`), no los movimientos.
            total_moves: u32::MAX,
            goal: LevelGoal::TimedScore {
                secs: 90.0,
                target: 180,
            },
            sparks_total: 0,
            shadow_positions: vec![],
            blocker_positions: vec![],
            grade_baseline: 520,
        },
        5 => LevelConfig {
            level: 5,
            total_moves: 28,
            goal: LevelGoal::CollectColor {
                color: LightColor::Red,
                target: 40,
            },
            sparks_total: 0,
            shadow_positions: vec![],
            blocker_positions: vec![],
            grade_baseline: 300,
        },
        6 => LevelConfig {
            level: 6,
            total_moves: 30,
            goal: LevelGoal::CollectColor {
                color: LightColor::Blue,
                target: 40,
            },
            sparks_total: 0,
            shadow_positions: vec![],
            blocker_positions: vec![],
            grade_baseline: 300,
        },
        7 => LevelConfig {
            level: 7,
            total_moves: 18,
            goal: LevelGoal::Score(430),
            sparks_total: 0,
            shadow_positions: vec![],
            blocker_positions: vec![],
            grade_baseline: 520,
        },
        8 => LevelConfig {
            level: 8,
            total_moves: 30,
            goal: LevelGoal::Sparks,
            sparks_total: 4,
            shadow_positions: vec![],
            blocker_positions: vec![
                GridPos { x: 0, y: 3 },
                GridPos { x: 0, y: 4 },
                GridPos { x: 7, y: 3 },
                GridPos { x: 7, y: 4 },
                GridPos { x: 3, y: 2 },
                GridPos { x: 4, y: 5 },
            ],
            grade_baseline: 520,
        },
        9 => LevelConfig {
            level: 9,
            total_moves: 24,
            goal: LevelGoal::CollectColor {
                color: LightColor::Green,
                target: 46,
            },
            sparks_total: 0,
            shadow_positions: vec![],
            blocker_positions: vec![
                GridPos { x: 1, y: 1 },
                GridPos { x: 6, y: 1 },
                GridPos { x: 1, y: 6 },
                GridPos { x: 6, y: 6 },
                GridPos { x: 3, y: 3 },
                GridPos { x: 4, y: 4 },
            ],
            grade_baseline: 560,
        },
        _ => make_level(1),
    }
}

pub(crate) fn make_generated_level(depth: u32, seed: u64) -> LevelConfig {
    let depth = depth.max(1);
    if depth <= 9 {
        return make_level(depth);
    }

    let roll = ((seed as u32)
        .wrapping_add(depth.wrapping_mul(37))
        .wrapping_add((seed >> 32) as u32))
        % 5;
    let pressure = depth.saturating_sub(1);
    let moves = 24u32
        .saturating_add((depth % 3) * 2)
        .saturating_sub(pressure / 3);
    let color = LightColor::from_index((seed as usize + depth as usize) % 5);

    let goal = match roll {
        0 => LevelGoal::Score(145 + depth * 34),
        1 => LevelGoal::CollectColor {
            color,
            target: 22 + depth * 4,
        },
        2 => LevelGoal::Sparks,
        3 => LevelGoal::ClearShadow,
        _ => LevelGoal::TimedScore {
            secs: (82.0 - depth as f32 * 2.0).max(56.0),
            target: 130 + depth * 28,
        },
    };

    let shadow_positions = if goal == LevelGoal::ClearShadow {
        generated_shadow_positions(seed, depth)
    } else {
        vec![]
    };
    let blocker_positions = generated_blocker_positions(seed, depth, &goal);

    LevelConfig {
        level: depth,
        total_moves: if matches!(goal, LevelGoal::TimedScore { .. }) {
            u32::MAX
        } else {
            moves.max(18)
        },
        goal,
        sparks_total: 2 + (depth / 3).min(3),
        shadow_positions,
        blocker_positions,
        grade_baseline: 260 + depth * 58,
    }
}

fn generated_shadow_positions(seed: u64, depth: u32) -> Vec<GridPos> {
    let pattern = ((seed as u32) ^ depth) % 3;
    match pattern {
        0 => (2i32..=6)
            .flat_map(|x| [2i32, 3i32].map(move |y| GridPos { x, y }))
            .collect(),
        1 => (1i32..=7)
            .filter(|x| x % 2 == 1)
            .flat_map(|x| [2i32, 4i32].map(move |y| GridPos { x, y }))
            .collect(),
        _ => (0i32..GRID_W)
            .filter(|x| (x + depth as i32) % 2 == 0)
            .map(|x| GridPos {
                x,
                y: 2 + ((x + seed as i32).rem_euclid(3)),
            })
            .collect(),
    }
}

fn generated_blocker_positions(seed: u64, depth: u32, goal: &LevelGoal) -> Vec<GridPos> {
    if depth < 4 || matches!(goal, LevelGoal::ClearShadow | LevelGoal::TimedScore { .. }) {
        return vec![];
    }

    match ((seed as u32) ^ depth.wrapping_mul(11)) % 3 {
        0 => vec![
            GridPos { x: 0, y: 3 },
            GridPos { x: 0, y: 4 },
            GridPos { x: 7, y: 3 },
            GridPos { x: 7, y: 4 },
        ],
        1 => vec![
            GridPos { x: 2, y: 2 },
            GridPos { x: 5, y: 2 },
            GridPos { x: 2, y: 5 },
            GridPos { x: 5, y: 5 },
        ],
        _ => vec![
            GridPos { x: 3, y: 1 },
            GridPos { x: 4, y: 1 },
            GridPos { x: 3, y: 6 },
            GridPos { x: 4, y: 6 },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_levels_are_deterministic_for_seed_and_depth() {
        let a = make_generated_level(4, 12345);
        let b = make_generated_level(4, 12345);

        assert_eq!(a.goal, b.goal);
        assert_eq!(a.total_moves, b.total_moves);
        assert_eq!(a.shadow_positions, b.shadow_positions);
        assert_eq!(a.blocker_positions, b.blocker_positions);
    }

    #[test]
    fn generated_timed_levels_are_unbounded_by_moves() {
        let level = (1..20)
            .map(|depth| make_generated_level(depth, 7))
            .find(|level| matches!(level.goal, LevelGoal::TimedScore { .. }))
            .unwrap();

        assert_eq!(level.total_moves, u32::MAX);
    }
}

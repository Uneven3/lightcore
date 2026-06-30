use bevy::prelude::*;

use super::grid::GridPos;
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
            grade_baseline: 300,
        },
        2 => LevelConfig {
            level: 2,
            total_moves: 26,
            goal: LevelGoal::Sparks,
            sparks_total: 3,
            shadow_positions: vec![],
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
            grade_baseline: 300,
        },
        _ => make_level(1),
    }
}

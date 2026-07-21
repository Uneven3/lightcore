use bevy::prelude::*;

use crate::core::grid::TILE;

/// Timing contract for the swap→power→pop pipeline. Gameplay owns when consequences become
/// available; visual adapters use the same values to remain synchronized.
#[derive(Resource)]
pub(crate) struct MatchTiming {
    pub speed: f32,
    pub bolt_length_frac: f32,
    pub pop_duration: f32,
    pub trail_duration: f32,
    pub stagger_secs: f32,
    pub stagger_max: f32,
    pub bolt_width_frac: f32,
    pub combo_hold_secs: f32,
}

impl Default for MatchTiming {
    fn default() -> Self {
        Self {
            speed: 750.0,
            bolt_length_frac: 0.8,
            pop_duration: 0.08,
            trail_duration: 0.32,
            stagger_secs: 0.035,
            stagger_max: 0.5,
            bolt_width_frac: 0.55,
            combo_hold_secs: 0.22,
        }
    }
}

impl MatchTiming {
    pub(crate) fn bolt_length(&self) -> f32 {
        self.bolt_length_frac * TILE
    }

    pub(crate) fn bolt_width(&self) -> f32 {
        self.bolt_width_frac * TILE
    }

    pub(crate) fn pop_delay(&self, distance: f32) -> f32 {
        (distance - self.bolt_length() - self.speed * self.pop_duration).max(0.0) / self.speed
    }
}

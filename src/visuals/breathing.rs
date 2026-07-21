use bevy::prelude::*;
use std::f32::consts::TAU;

use crate::board::SparkNucleusPulse;

const BREATH_PERIOD_SECS: f32 = 6.0;
const HOLLOW_BREATH_MIN: f32 = 0.78;
const HOLLOW_BREATH_MAX: f32 = 1.0;

fn breath_norm(elapsed: f32, phase: f32) -> f32 {
    let freq = TAU / BREATH_PERIOD_SECS;
    0.5 + 0.5 * (elapsed * freq + phase).sin()
}

pub(crate) fn hollow_breath_factor(elapsed: f32, phase: f32) -> f32 {
    HOLLOW_BREATH_MIN + (HOLLOW_BREATH_MAX - HOLLOW_BREATH_MIN) * breath_norm(elapsed, phase)
}

pub(crate) fn pulse_spark_nucleus(
    time: Res<Time>,
    mut q: Query<(&mut Transform, &SparkNucleusPulse)>,
) {
    let t = time.elapsed_secs();
    for (mut transform, pulse) in &mut q {
        let wave = 0.5 + 0.5 * (t * 12.0 + pulse.phase).sin();
        transform.scale = pulse.base_scale * (0.86 + wave * 0.22);
    }
}

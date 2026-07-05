use bevy::prelude::*;
use std::f32::consts::TAU;

const BREATH_PERIOD_SECS: f32 = 4.5; // slow cycle, not a flicker
const BREATH_MIN: f32 = 0.55;
const BREATH_MAX: f32 = 1.0;

/// The shared breath waveform: a slow sine in `[BREATH_MIN, BREATH_MAX]`. Single source of truth
/// so a light's `LightCore` and its glow halo (see `glow.rs`) pulse in perfect lockstep when given
/// the same `phase`.
pub(crate) fn breath_factor(elapsed: f32, phase: f32) -> f32 {
    let freq = TAU / BREATH_PERIOD_SECS;
    BREATH_MIN + (BREATH_MAX - BREATH_MIN) * (0.5 + 0.5 * (elapsed * freq + phase).sin())
}

pub(crate) fn breath_factor_and_norm(elapsed: f32, phase: f32) -> (f32, f32) {
    let freq = TAU / BREATH_PERIOD_SECS;
    let norm = 0.5 + 0.5 * (elapsed * freq + phase).sin();
    (BREATH_MIN + (BREATH_MAX - BREATH_MIN) * norm, norm)
}

/// The breath phase of a light, stored on the `Light` entity so its `LightCore`(s) and its glow
/// halo can all breathe with the same phase. Randomized per-light so lights don't pulse together.
#[derive(Component)]
pub(crate) struct BreathPhase(pub(crate) f32);

/// Slow sine-wave brightness pulse on a `Sprite`-based entity (here, `LightCore`). `base` is the
/// unmodulated color captured at spawn time — each frame's color is derived fresh from it, so the
/// modulation never drifts/accumulates. Driving `Sprite::color` (a component) instead of a
/// per-entity `ColorMaterial` lets every core batch into one draw call.
#[derive(Component)]
pub(crate) struct Breathing {
    pub(crate) base: Srgba,
    pub(crate) phase: f32, // randomized per-light so cores don't all pulse in lockstep
}

#[derive(Component)]
pub(crate) struct SparkNucleusPulse {
    pub(crate) base_scale: Vec3,
    pub(crate) phase: f32,
}

pub(crate) fn breathe(time: Res<Time>, mut q: Query<(&mut Sprite, &Breathing)>) {
    let t = time.elapsed_secs();
    for (mut sprite, breathing) in &mut q {
        let factor = breath_factor(t, breathing.phase);
        let Srgba {
            red,
            green,
            blue,
            alpha,
        } = breathing.base;
        sprite.color = Color::srgb(red * factor, green * factor, blue * factor).with_alpha(alpha);
    }
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

use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::TAU;

use super::assets::VisualCache;
use super::particles::{ParticleSettings, spawn_membrane_pop};
use crate::core::grid::TILE;
use crate::core::prelude::LightColor;
use crate::gameplay::{
    ChainPop, DisplayedCollectedCores, DisplayedScore, LightPopped, ScoreAnchor, ScoreGlow,
};

const SHARDS_PER_POP: usize = 2;
const MAX_TOTAL_SHARDS: usize = 48;
/// How strongly each arriving shard drags the score's tint toward its own color.
const GLOW_BLEND: f32 = 0.18;

/// Visual parameters for the score-shard particles. Exposed as a `Resource` so the Options
/// screen can tweak them in real time via sliders.
#[derive(Resource)]
pub(crate) struct ShardSettings {
    /// Fraction of TILE — base sprite size of a score shard (radial glow).
    pub base_size_frac: f32,
    /// Fraction of TILE — maximum shard size (deep cascade chains).
    pub max_size_frac: f32,
    /// Extra size factor per cascade level above 1.
    pub growth: f32,
    /// Fraction of TILE — lateral bow of the Bézier control point; controls how much shards arc.
    pub curve_frac: f32,
    /// Seconds — minimum travel time from light to score.
    pub min_secs: f32,
    /// Seconds — maximum travel time from light to score.
    pub max_secs: f32,
    /// HDR brightness multiplier applied to the shard sprite color.
    pub hdr_boost: f32,
}

impl Default for ShardSettings {
    fn default() -> Self {
        Self {
            base_size_frac: 0.18,
            max_size_frac: 0.45,
            growth: 0.18,
            curve_frac: 1.3,
            min_secs: 0.68,
            max_secs: 1.08,
            hdr_boost: 2.5,
        }
    }
}

/// One core-shard: a bright mote of a collected light's own color. It springs out of the match
/// point and is drawn into the score readout along a curved, accelerating path, carrying its
/// share of the points it adds to `DisplayedScore` on arrival (see `gameplay::DisplayedScore`
/// for why this is decoupled from the real, immediate `Score`) and tinting `ScoreGlow`.
/// Rendered as a `Sprite` using `glow_image` (radial falloff) so it looks like a glowing orb,
/// not a flat disc — alpha is driven directly via `Sprite::color` in `tick_score_light`.
#[derive(Component)]
pub(crate) struct ScoreShard {
    from: Vec3,
    /// Quadratic-Bézier control point: bends the path so the shard arcs instead of sliding straight.
    ctrl: Vec3,
    to: Vec3,
    points: u32,
    /// Normalized linear RGB of this shard's light color, blended into `ScoreGlow` on arrival.
    tint: Vec3,
    timer: Timer,
    /// Waits for the light's PopDelay to expire before the shard starts moving — keeps it invisible
    /// until the light's core actually disappears (so shards don't appear before the pop).
    pop_delay: Timer,
    color: LightColor,
}

fn quad_bezier(p0: Vec3, p1: Vec3, p2: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
}

fn tint_of(color: LightColor) -> Vec3 {
    let lin = color.bevy_color().to_linear();
    Vec3::new(lin.red, lin.green, lin.blue)
}

pub(crate) fn on_chain_pop_score_light(
    trigger: On<ChainPop>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    anchor: Res<ScoreAnchor>,
    shards: Res<ShardSettings>,
) {
    if trigger.points == 0 || trigger.pops.is_empty() {
        return;
    }
    let to = anchor.0;

    let base_size = shards.base_size_frac * TILE;
    let max_size = shards.max_size_frac * TILE;

    // Cascade depth ≈ points per light; drives shard SIZE — deeper chains throw fatter motes.
    let cascade = trigger.points as f32 / trigger.pops.len() as f32;
    let size = (base_size * (1.0 + (cascade - 1.0) * shards.growth)).clamp(base_size, max_size);

    // Flatten pops → shard slots (a couple per light), dropping to 1 each if a huge clear would
    // otherwise spawn too many. Points are split across all slots so the readout ticks up in a
    // burst as they land, instead of one jump — front slots carry the remainder for an exact total.
    let per_pop = if trigger.pops.len() * SHARDS_PER_POP <= MAX_TOTAL_SHARDS {
        SHARDS_PER_POP
    } else {
        1
    };
    let mut slots: Vec<(Vec3, LightColor, f32)> = Vec::with_capacity(trigger.pops.len() * per_pop);
    for &(pos, c, delay) in &trigger.pops {
        for _ in 0..per_pop {
            slots.push((pos, c, delay));
        }
    }
    let n = slots.len() as u32;
    let base = trigger.points / n;
    let rem = trigger.points % n;

    let shard_curve = shards.curve_frac * TILE;
    let mut rng = rand::rng();
    for (i, &(pos, c, pop_delay_secs)) in slots.iter().enumerate() {
        let from = pos.with_z(2.0); // float above the board for the whole trip
        let to = to.with_z(6.0);
        let line = (to - from).truncate();
        let perp = Vec2::new(-line.y, line.x).normalize_or_zero();
        // Control point sits in the first part of the trip, kicked sideways (perp) and a little
        // radially — so the shard visibly springs out of the core before curving to the score.
        let along = rng.random_range(0.15..0.5);
        let side = rng.random_range(-1.0..1.0) * shard_curve;
        let radial =
            Vec2::from_angle(rng.random_range(0.0..TAU)) * rng.random_range(0.0..TILE * 0.6);
        let ctrl = from + (line * along + perp * side + radial).extend(0.0);
        let core_lin = c.glow_color().to_linear();
        let core_color = Color::linear_rgb(
            core_lin.red * 1.5,
            core_lin.green * 1.5,
            core_lin.blue * 1.5,
        );

        let glow_lin = c.ring_color().to_linear();
        let glow_color = Color::linear_rgb(
            glow_lin.red * 1.5,
            glow_lin.green * 1.5,
            glow_lin.blue * 1.5,
        );

        let parent = commands
            .spawn((
                ScoreShard {
                    from,
                    ctrl,
                    to,
                    points: base + if (i as u32) < rem { 1 } else { 0 },
                    tint: tint_of(c),
                    timer: Timer::from_seconds(
                        rng.random_range(shards.min_secs..shards.max_secs),
                        TimerMode::Once,
                    ),
                    pop_delay: Timer::from_seconds(pop_delay_secs, TimerMode::Once),
                    color: c,
                },
                Sprite {
                    image: cache.core_image.clone(),
                    color: core_color,
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::from_translation(from),
            ))
            .id();

        let glow_child = commands
            .spawn((
                Sprite {
                    image: cache.glow_image.clone(),
                    color: glow_color,
                    custom_size: Some(Vec2::splat(size * 2.5)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -0.1),
            ))
            .id();

        commands.entity(parent).add_child(glow_child);
    }
}

/// Deliberately not gated by `not(in_state(GameOver))` like the rest of `visuals` — a match can
/// fire `ChainPop` on the very move that ends the game, leaving shards mid-flight right as
/// `GameOver` is entered. Letting them keep ticking lets them finish their short trip and
/// despawn normally instead of freezing on screen forever.
pub(crate) fn tick_score_light(
    time: Res<Time>,
    mut commands: Commands,
    mut displayed: ResMut<DisplayedScore>,
    mut glow: ResMut<ScoreGlow>,
    mut displayed_cores: ResMut<DisplayedCollectedCores>,
    mut q: Query<(
        Entity,
        &mut ScoreShard,
        &mut Transform,
        &mut Sprite,
        Option<&Children>,
    )>,
    mut child_sprite_q: Query<&mut Sprite, Without<ScoreShard>>,
) {
    for (e, mut shard, mut t, mut sprite, children) in &mut q {
        if !shard.pop_delay.tick(time.delta()).is_finished() {
            sprite.color = sprite.color.with_alpha(0.0);
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(mut child_sprite) = child_sprite_q.get_mut(child) {
                        child_sprite.color = child_sprite.color.with_alpha(0.0);
                    }
                }
            }
            continue;
        }
        shard.timer.tick(time.delta());
        let frac = shard.timer.fraction();
        // ease-in (t²): the shard drifts out of the core slowly, then accelerates as it's pulled
        // into the score — "una curva que va acelerando".
        let eased = frac * frac;
        t.translation = quad_bezier(shard.from, shard.ctrl, shard.to, eased);
        // Shrink as it's absorbed, so arrival reads as the score "drinking" the light.
        t.scale = Vec3::splat(1.0 - 0.6 * frac);
        // Alpha: hold full for the first 65% of the trip, fade to 0 in the last 35% (comet tail).
        let alpha = if frac < 0.65 {
            1.0
        } else {
            (1.0 - frac) / 0.35
        };
        sprite.color = sprite.color.with_alpha(alpha);
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut child_sprite) = child_sprite_q.get_mut(child) {
                    child_sprite.color = child_sprite.color.with_alpha(alpha);
                }
            }
        }
        if shard.timer.is_finished() {
            displayed.0 += shard.points;
            displayed_cores.0[shard.color.index()] += shard.points;
            glow.rgb = glow.rgb.lerp(shard.tint, GLOW_BLEND); // score drifts toward collected colors
            glow.pulse = (glow.pulse + 0.5).min(1.0); // each arrival re-triggers the rapid pulse
            if let Some(children) = children {
                for child in children.iter() {
                    commands.entity(child).try_despawn();
                }
            }
            commands.entity(e).try_despawn();
        }
    }
}

/// Spawns the membrane-burst particles the moment a light's fade animation completes.
/// Fired by `gameplay::popping::tick_pop_anim` via `LightPopped` — decoupled from `ChainPop`
/// so particles appear AFTER the ring dissolves, not at the same time as the pop starts.
pub(crate) fn on_light_popped(
    trigger: On<LightPopped>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    particles: Res<ParticleSettings>,
) {
    spawn_membrane_pop(
        &mut commands,
        cache.core_image.clone(),
        trigger.pos,
        trigger.color.ring_color(),
        particles.membrane_radius,
    );
}

use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::TAU;

use super::assets::VisualCache;
use super::particles::{ParticleSettings, spawn_membrane_pop};
use crate::core::grid::TILE;
use crate::core::prelude::LightColor;
use crate::gameplay::{
    ChainPop, DisplayedCollectedCores, DisplayedScore, LightPopped, ScoreAnchor, ScoreDrained,
    ScoreGlow,
};

const SHARDS_PER_POP: usize = 2;
const MAX_TOTAL_SHARDS: usize = 48;
const SCORE_DRAIN_SHARDS: usize = 34;
const HOLLOW_LOCAL_DRAIN_SHARDS: usize = 8;
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
            base_size_frac: 0.22,
            max_size_frac: 0.40,
            growth: 0.10,
            curve_frac: 1.3,
            min_secs: 0.68,
            max_secs: 1.08,
            hdr_boost: 5.0,
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

#[derive(Component)]
pub(crate) struct ScoreShardScatter {
    velocity: Vec3,
    timer: Timer,
}

#[derive(Component)]
pub(crate) struct ScoreShardAbsorb {
    from: Vec3,
    ctrl: Vec3,
    to: Vec3,
    timer: Timer,
}

fn quad_bezier(p0: Vec3, p1: Vec3, p2: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
}

fn tint_of(color: LightColor) -> Vec3 {
    let lin = color.bevy_color().to_linear();
    Vec3::new(lin.red, lin.green, lin.blue)
}

fn shard_core_color(color: LightColor, hdr_boost: f32) -> Color {
    let lin = color.bevy_color().to_linear();
    Color::linear_rgb(
        lin.red * hdr_boost,
        lin.green * hdr_boost,
        lin.blue * hdr_boost,
    )
}

fn shard_halo_color(color: LightColor, hdr_boost: f32) -> Color {
    let lin = color.bevy_color().to_linear();
    let halo_boost = hdr_boost * 0.85;
    Color::linear_rgb(
        lin.red * halo_boost,
        lin.green * halo_boost,
        lin.blue * halo_boost,
    )
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
        let core_color = shard_core_color(c, shards.hdr_boost);
        let glow_color = shard_halo_color(c, shards.hdr_boost);

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
                    custom_size: Some(Vec2::splat(size * 2.8)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -0.1),
            ))
            .id();

        commands.entity(parent).add_child(glow_child);
    }
}

pub(crate) fn on_score_drained(
    trigger: On<ScoreDrained>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    anchor: Res<ScoreAnchor>,
    score_glow: Res<ScoreGlow>,
    mut q: Query<(Entity, &Transform, &mut Sprite, Option<&Children>), With<ScoreShard>>,
    mut child_sprite_q: Query<&mut Sprite, Without<ScoreShard>>,
) {
    if trigger.origins.is_empty() {
        return;
    }
    let mut rng = rand::rng();
    for (e, t, mut sprite, children) in &mut q {
        let from = t.translation;
        let to = nearest_origin(from, &trigger.origins)
            + Vec3::new(
                rng.random_range(-TILE * 0.08..TILE * 0.08),
                rng.random_range(-TILE * 0.08..TILE * 0.08),
                0.0,
            );
        let ctrl = drain_ctrl(from, to, &mut rng);
        sprite.color = sprite.color.with_alpha(1.0);
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut child_sprite) = child_sprite_q.get_mut(child) {
                    child_sprite.color = child_sprite.color.with_alpha(0.62);
                }
            }
        }
        commands.entity(e).remove::<ScoreShard>().insert((
            ScoreShardAbsorb {
                from,
                ctrl,
                to,
                timer: Timer::from_seconds(rng.random_range(0.42..0.68), TimerMode::Once),
            },
            Transform::from_translation(t.translation),
        ));
    }

    let score_color = Color::linear_rgb(
        score_glow.rgb.x * 4.2,
        score_glow.rgb.y * 4.2,
        score_glow.rgb.z * 4.2,
    );
    let score_halo = Color::linear_rgba(
        score_glow.rgb.x * 2.4,
        score_glow.rgb.y * 2.4,
        score_glow.rgb.z * 2.4,
        0.55,
    );
    for _ in 0..SCORE_DRAIN_SHARDS {
        let from = anchor.0
            + Vec3::new(
                rng.random_range(-TILE * 0.42..TILE * 0.42),
                rng.random_range(-TILE * 0.24..TILE * 0.24),
                0.0,
            );
        let to = trigger.origins[rng.random_range(0..trigger.origins.len())]
            + Vec3::new(
                rng.random_range(-TILE * 0.12..TILE * 0.12),
                rng.random_range(-TILE * 0.12..TILE * 0.12),
                0.0,
            );
        let parent = commands
            .spawn((
                ScoreShardAbsorb {
                    from,
                    ctrl: drain_ctrl(from, to, &mut rng),
                    to,
                    timer: Timer::from_seconds(rng.random_range(0.46..0.76), TimerMode::Once),
                },
                Sprite {
                    image: cache.core_image.clone(),
                    color: score_color,
                    custom_size: Some(Vec2::splat(rng.random_range(TILE * 0.10..TILE * 0.16))),
                    ..default()
                },
                Transform::from_translation(from.with_z(6.0)),
            ))
            .id();

        let glow = commands
            .spawn((
                Sprite {
                    image: cache.glow_image.clone(),
                    color: score_halo,
                    custom_size: Some(Vec2::splat(TILE * 0.42)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -0.1),
            ))
            .id();
        commands.entity(parent).add_child(glow);
    }

    for &origin in &trigger.origins {
        for _ in 0..HOLLOW_LOCAL_DRAIN_SHARDS {
            let angle = rng.random_range(0.0..TAU);
            let start = origin + (Vec2::from_angle(angle) * TILE * 0.34).extend(0.0);
            let path = start - origin;
            // Perpendicular offset creates a beautiful curved path
            let perp = Vec3::new(-path.y, path.x, 0.0).normalize_or_zero() * (TILE * 0.14);
            let ctrl = (start + origin) * 0.5 + perp;

            let parent = commands
                .spawn((
                    ScoreShardAbsorb {
                        from: start,
                        ctrl,
                        to: origin,
                        timer: Timer::from_seconds(rng.random_range(0.50..0.85), TimerMode::Once),
                    },
                    Sprite {
                        image: cache.core_image.clone(),
                        color: Color::srgba(0.0, 0.0, 0.0, 0.85),
                        custom_size: Some(Vec2::splat(TILE * 0.085)),
                        ..default()
                    },
                    Transform::from_translation(start),
                ))
                .id();

            let glow = commands
                .spawn((
                    Sprite {
                        image: cache.glow_image.clone(),
                        color: Color::srgba(0.0, 0.0, 0.0, 0.18),
                        custom_size: Some(Vec2::splat(TILE * 0.26)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, -0.1),
                ))
                .id();
            commands.entity(parent).add_child(glow);
        }
    }
}

fn nearest_origin(from: Vec3, origins: &[Vec3]) -> Vec3 {
    origins
        .iter()
        .copied()
        .min_by(|a, b| {
            from.distance_squared(*a)
                .total_cmp(&from.distance_squared(*b))
        })
        .unwrap_or(from)
}

fn drain_ctrl(from: Vec3, to: Vec3, rng: &mut impl Rng) -> Vec3 {
    let line = (to - from).truncate();
    let perp = Vec2::new(-line.y, line.x).normalize_or_zero();
    let side = rng.random_range(-TILE * 1.1..TILE * 1.1);
    from + (line * rng.random_range(0.35..0.68) + perp * side).extend(0.0)
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

pub(crate) fn tick_score_shard_scatter(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut ScoreShardScatter,
        &mut Transform,
        &mut Sprite,
        Option<&Children>,
    )>,
    mut child_sprite_q: Query<&mut Sprite, (Without<ScoreShard>, Without<ScoreShardScatter>)>,
) {
    for (e, mut scatter, mut t, mut sprite, children) in &mut q {
        scatter.timer.tick(time.delta());
        let dt = time.delta_secs();
        scatter.velocity += Vec3::new(0.0, -TILE * 3.2, 0.0) * dt;
        t.translation += scatter.velocity * dt;

        let frac = scatter.timer.fraction();
        let alpha = (1.0 - frac).powf(1.4);
        let scale = 1.0 + 0.45 * frac;
        t.scale = Vec3::splat(scale);
        sprite.color = sprite.color.with_alpha(alpha * 0.85);
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut child_sprite) = child_sprite_q.get_mut(child) {
                    child_sprite.color = child_sprite.color.with_alpha(alpha * 0.42);
                }
            }
        }

        if scatter.timer.is_finished() {
            if let Some(children) = children {
                for child in children.iter() {
                    commands.entity(child).try_despawn();
                }
            }
            commands.entity(e).try_despawn();
        }
    }
}

pub(crate) fn tick_score_shard_absorb(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut ScoreShardAbsorb,
        &mut Transform,
        &mut Sprite,
        Option<&Children>,
    )>,
    mut child_sprite_q: Query<&mut Sprite, (Without<ScoreShard>, Without<ScoreShardAbsorb>)>,
) {
    for (e, mut absorb, mut t, mut sprite, children) in &mut q {
        absorb.timer.tick(time.delta());
        let frac = absorb.timer.fraction();
        let eased = 1.0 - (1.0 - frac).powi(3);
        t.translation = quad_bezier(absorb.from, absorb.ctrl, absorb.to, eased);
        t.scale = Vec3::splat(1.0 - 0.82 * frac);

        let alpha = if frac < 0.78 {
            1.0
        } else {
            (1.0 - frac) / 0.22
        };
        sprite.color = sprite.color.with_alpha(alpha);
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut child_sprite) = child_sprite_q.get_mut(child) {
                    child_sprite.color = child_sprite.color.with_alpha(alpha * 0.55);
                }
            }
        }

        if absorb.timer.is_finished() {
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
        trigger.kind.visual_ring_color(trigger.color),
        particles.membrane_radius,
    );
}

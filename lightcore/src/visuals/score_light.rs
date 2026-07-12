use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::TAU;

use super::additive_material::AdditiveMaterial;
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
    /// Seconds — brief hover at the capture point before the shard launches into its curved
    /// flight, so the capture reads as "gather, then throw" instead of an instant snap into motion.
    pub hold_secs: f32,
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
            hold_secs: 0.24,
        }
    }
}

/// One core-shard: a bright mote of a collected light's own color. It springs out of the match
/// point and is drawn into the score readout along a curved, accelerating path, carrying its
/// share of the points it adds to `DisplayedScore` on arrival (see `gameplay::DisplayedScore`
/// for why this is decoupled from the real, immediate `Score`) and tinting `ScoreGlow`.
/// Rendered as `Mesh2d` + `AdditiveMaterial` (not `Sprite`) so overlapping shards SUM brightness
/// instead of alpha-blending into a muddy average — see `additive_material` for why. Alpha is
/// driven directly via the material's color in `tick_score_light`.
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
    /// Once `pop_delay` clears, the shard hovers at `from` for this long — a little gathered breath
    /// before it throws itself into the curve — instead of snapping straight into flight.
    hold: Timer,
    /// Random phase offset so each shard's glow twinkle is out of sync with its neighbors'.
    glow_phase: f32,
    color: LightColor,
    /// Base on-screen size in pixels, baked into `Transform::scale` at spawn (Mesh2d has no
    /// `Sprite::custom_size` equivalent) — animation systems must multiply by this, not overwrite it.
    base_size: f32,
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

/// Marks a `ScoreShardAbsorb` that was converted from a `ScoreShard` mid-flight (see
/// `on_score_drained`) and therefore renders via `Mesh2d` + `AdditiveMaterial` instead of `Sprite`
/// — ticked separately by `tick_score_shard_absorb_glow` since it needs `base_size` and a
/// different set of components than the plain-`Sprite` drain shards `tick_score_shard_absorb`
/// handles.
#[derive(Component)]
pub(crate) struct ScoreShardAbsorbGlow {
    base_size: f32,
}

fn quad_bezier(p0: Vec3, p1: Vec3, p2: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
}

/// Shared ease/alpha curve for a `ScoreShardAbsorb`'s flight, whether it's rendered via `Sprite`
/// (`tick_score_shard_absorb`) or `Mesh2d` + `AdditiveMaterial` (`tick_score_shard_absorb_glow`) —
/// ease-out cubic toward the target, full opacity for the first 78% of the trip then a quick fade.
/// Returns `(eased_t, alpha)`.
fn absorb_ease_and_alpha(frac: f32) -> (f32, f32) {
    let eased = 1.0 - (1.0 - frac).powi(3);
    let alpha = if frac < 0.78 {
        1.0
    } else {
        (1.0 - frac) / 0.22
    };
    (eased, alpha)
}

/// Despawns `e` and every one of its `children` — the "pop is finished, clean up" pattern shared by
/// every score-shard tick system.
fn despawn_with_children(commands: &mut Commands, e: Entity, children: Option<&Children>) {
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
    commands.entity(e).try_despawn();
}

fn tint_of(color: LightColor) -> Vec3 {
    let lin = color.bevy_color().to_linear();
    Vec3::new(lin.red, lin.green, lin.blue)
}

/// Uniform (hue-preserving) brightness multiplier for the shard's hot-core sprite — the hue
/// gradient itself now lives in the baked `shard_core_image` texture (see `visuals::assets`), so
/// this only needs to scale intensity for Bloom, not tint.
fn shard_core_boost(hdr_boost: f32) -> Color {
    Color::linear_rgb(hdr_boost, hdr_boost, hdr_boost)
}

fn shard_halo_color(color: LightColor, hdr_boost: f32) -> Color {
    let lin = color.bevy_color().to_linear();
    // Brighter than the old 0.85× — now that the halo also carries the trail streak, a smaller,
    // more concentrated shape reads as "dim" unless it's pushed hotter to match.
    let halo_boost = hdr_boost * 1.3;
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
    mut materials: ResMut<Assets<AdditiveMaterial>>,
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
        let core_color = shard_core_boost(shards.hdr_boost);
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
                    hold: Timer::from_seconds(
                        shards.hold_secs * rng.random_range(0.75..1.25),
                        TimerMode::Once,
                    ),
                    glow_phase: rng.random_range(0.0..TAU),
                    color: c,
                    base_size: size,
                },
                Mesh2d(cache.unit_quad_mesh.clone()),
                MeshMaterial2d(materials.add(AdditiveMaterial {
                    color: core_color.to_linear(),
                    texture: cache.shard_core_image(c),
                })),
                Transform::from_translation(from).with_scale(Vec3::splat(size)),
            ))
            .id();

        // Local scale is a RATIO relative to the parent, not a pixel size — the parent's own scale
        // (base_size, then base_size·shrink-fraction as it animates) is inherited via
        // GlobalTransform composition, so this child ends up at 2.8× the parent's *current* size
        // exactly like the old `Sprite::custom_size = size * 2.8` did (custom_size is also
        // multiplied by the inherited transform scale). Baking `size` in again here would square it.
        let glow_child = commands
            .spawn((
                Mesh2d(cache.unit_quad_mesh.clone()),
                MeshMaterial2d(materials.add(AdditiveMaterial {
                    color: glow_color.to_linear(),
                    texture: cache.glow_image.clone(),
                })),
                Transform::from_xyz(0.0, 0.0, -0.1).with_scale(Vec3::splat(2.8)),
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
    mut materials: ResMut<Assets<AdditiveMaterial>>,
    mut q: Query<
        (
            Entity,
            &ScoreShard,
            &Transform,
            &MeshMaterial2d<AdditiveMaterial>,
            Option<&Children>,
        ),
        With<ScoreShard>,
    >,
    child_material_q: Query<&MeshMaterial2d<AdditiveMaterial>, Without<ScoreShard>>,
) {
    if trigger.origins.is_empty() {
        return;
    }
    let mut rng = rand::rng();
    for (e, shard, t, material, children) in &mut q {
        let from = t.translation;
        let to = nearest_origin(from, &trigger.origins)
            + Vec3::new(
                rng.random_range(-TILE * 0.08..TILE * 0.08),
                rng.random_range(-TILE * 0.08..TILE * 0.08),
                0.0,
            );
        let ctrl = drain_ctrl(from, to, &mut rng);
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.color.alpha = 1.0;
        }
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(child_material) = child_material_q.get(child)
                    && let Some(mut mat) = materials.get_mut(&child_material.0)
                {
                    mat.color.alpha = 0.62;
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
            ScoreShardAbsorbGlow {
                base_size: shard.base_size,
            },
            Transform::from_translation(t.translation).with_scale(Vec3::splat(shard.base_size)),
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
    mut materials: ResMut<Assets<AdditiveMaterial>>,
    mut q: Query<(
        Entity,
        &mut ScoreShard,
        &mut Transform,
        &MeshMaterial2d<AdditiveMaterial>,
        Option<&Children>,
    )>,
    child_material_q: Query<&MeshMaterial2d<AdditiveMaterial>, Without<ScoreShard>>,
    mut child_transform_q: Query<&mut Transform, Without<ScoreShard>>,
) {
    for (e, mut shard, mut t, material, children) in &mut q {
        if !shard.pop_delay.tick(time.delta()).is_finished() {
            if let Some(mut mat) = materials.get_mut(&material.0) {
                mat.color.alpha = 0.0;
            }
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(child_material) = child_material_q.get(child)
                        && let Some(mut mat) = materials.get_mut(&child_material.0)
                    {
                        mat.color.alpha = 0.0;
                    }
                }
            }
            continue;
        }

        if !shard.hold.tick(time.delta()).is_finished() {
            // Hover at the capture point and pop into view instead of appearing already mid-flight
            // — a short gathered breath before the shard throws itself into the curve.
            let hold_frac = shard.hold.fraction();
            let pop_in = (hold_frac / 0.4).min(1.0);
            t.translation = shard.from;
            t.scale = Vec3::splat(shard.base_size * (0.55 + 0.45 * pop_in));
            if let Some(mut mat) = materials.get_mut(&material.0) {
                mat.color.alpha = pop_in;
            }
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(child_material) = child_material_q.get(child)
                        && let Some(mut mat) = materials.get_mut(&child_material.0)
                    {
                        mat.color.alpha = pop_in;
                    }
                }
            }
            continue;
        }

        shard.timer.tick(time.delta());
        let frac = shard.timer.fraction();
        // ease-in (t³): a strong hang-then-snap — the shard barely creeps for the first half of
        // the trip, then accelerates hard into the score, instead of the gentler t² curve.
        let eased = frac * frac * frac;
        t.translation = quad_bezier(shard.from, shard.ctrl, shard.to, eased);
        // Shrink as it's absorbed, so arrival reads as the score "drinking" the light.
        t.scale = Vec3::splat(shard.base_size * (1.0 - 0.6 * frac));
        // Full brightness for the whole flight — the light disappears ABRUPTLY on arrival (the
        // despawn below), not a gradual fade-out.
        let alpha = 1.0;
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.color.alpha = alpha;
        }
        // Cheap twinkle: modulate just the halo child's alpha with a per-shard-phased sine, so the
        // glow gently shimmers in flight instead of sitting at one flat brightness — no extra draw
        // calls or entities, just a sin() per shard per frame.
        let twinkle = 1.0 + 0.15 * (time.elapsed_secs() * 10.0 + shard.glow_phase).sin();
        // Bézier tangent at `eased`: gives the instantaneous direction of travel so the halo can
        // stretch into a streak pointing back along the path — a trail, without spawning any extra
        // entities. Grows with `frac` since the ease-in curve is fastest right before landing.
        // Kept small (vs. a wide smear) and boosted via `shard_halo_color`'s brighter multiplier so
        // it reads as a tight, hot streak rather than a diffuse smudge.
        let tangent = (2.0 * (1.0 - eased) * (shard.ctrl - shard.from)
            + 2.0 * eased * (shard.to - shard.ctrl))
            .truncate();
        let dir = tangent.normalize_or_zero();
        let trail_stretch = 1.0 + 0.9 * frac;
        // Shift the streak backward by half its added length (in the same child-local units as
        // `scale`, so it composes the same way through the parent's transform) — the elongation
        // trails BEHIND the shard instead of growing symmetrically through it.
        let trail_offset = -dir * (2.3 * (trail_stretch - 1.0) * 0.5);
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(child_material) = child_material_q.get(child)
                    && let Some(mut mat) = materials.get_mut(&child_material.0)
                {
                    mat.color.alpha = (alpha * twinkle).clamp(0.0, 1.0);
                }
                if let Ok(mut child_t) = child_transform_q.get_mut(child) {
                    child_t.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
                    child_t.scale = Vec3::new(2.3 * trail_stretch, 1.3, 1.0);
                    child_t.translation = trail_offset.extend(-0.1);
                }
            }
        }
        if shard.timer.is_finished() {
            displayed.0 += shard.points;
            displayed_cores.0[shard.color.index()] += shard.points;
            glow.rgb = glow.rgb.lerp(shard.tint, GLOW_BLEND); // score drifts toward collected colors
            glow.pulse = (glow.pulse + 0.5).min(1.0); // each arrival re-triggers the rapid pulse
            despawn_with_children(&mut commands, e, children);
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
            despawn_with_children(&mut commands, e, children);
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
        let (eased, alpha) = absorb_ease_and_alpha(frac);
        t.translation = quad_bezier(absorb.from, absorb.ctrl, absorb.to, eased);
        t.scale = Vec3::splat(1.0 - 0.82 * frac);

        sprite.color = sprite.color.with_alpha(alpha);
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut child_sprite) = child_sprite_q.get_mut(child) {
                    child_sprite.color = child_sprite.color.with_alpha(alpha * 0.55);
                }
            }
        }

        if absorb.timer.is_finished() {
            despawn_with_children(&mut commands, e, children);
        }
    }
}

/// Same animation as `tick_score_shard_absorb`, for the subset of `ScoreShardAbsorb` entities
/// converted mid-flight from a captured `ScoreShard` (see `on_score_drained`) — those render via
/// `Mesh2d` + `AdditiveMaterial`, not `Sprite`, so they need their own query shape and their
/// `base_size` baked into `Transform::scale` instead of a bare 0..1 factor.
pub(crate) fn tick_score_shard_absorb_glow(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<AdditiveMaterial>>,
    mut q: Query<(
        Entity,
        &mut ScoreShardAbsorb,
        &ScoreShardAbsorbGlow,
        &mut Transform,
        &MeshMaterial2d<AdditiveMaterial>,
        Option<&Children>,
    )>,
    child_material_q: Query<&MeshMaterial2d<AdditiveMaterial>, Without<ScoreShardAbsorb>>,
) {
    for (e, mut absorb, glow, mut t, material, children) in &mut q {
        absorb.timer.tick(time.delta());
        let frac = absorb.timer.fraction();
        let (eased, alpha) = absorb_ease_and_alpha(frac);
        t.translation = quad_bezier(absorb.from, absorb.ctrl, absorb.to, eased);
        t.scale = Vec3::splat(glow.base_size * (1.0 - 0.82 * frac));

        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.color.alpha = alpha;
        }
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(child_material) = child_material_q.get(child)
                    && let Some(mut mat) = materials.get_mut(&child_material.0)
                {
                    mat.color.alpha = alpha * 0.55;
                }
            }
        }

        if absorb.timer.is_finished() {
            despawn_with_children(&mut commands, e, children);
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

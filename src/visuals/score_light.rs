use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::TAU;

use super::additive_material::AdditiveMaterial;
use super::assets::VisualCache;
use super::particles::{ParticleSettings, spawn_membrane_pop};
use crate::core::grid::TILE;
use crate::core::prelude::{LightColor, LightKind, to_world};
use crate::gameplay::{
    CaptureBatch, CapturedCore, CollectedCores, LightPopped, ScoreDrained, ScoreGlow,
};
use crate::presentation::{
    CollectorRoute, ColorGoalCollectorPulse, DisplayedCollectedCores, LightcoreCollectorTargets,
};

const SHARDS_PER_POP: usize = 2;
const MAX_TOTAL_SHARDS: usize = 48;
const SCORE_DRAIN_SHARDS: usize = 34;
const HOLLOW_LOCAL_DRAIN_SHARDS: usize = 8;
/// The same score shard first rides this brief explosive arc out of a Supernova, then enters its
/// usual capture hold and curved trip to the HUD.
const SUPERNOVA_SHARD_DISTANCE_RANGE: std::ops::Range<f32> = 1.8..3.4;
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
/// point and is drawn into its collector along a curved, accelerating path. Authoritative score
/// remains independent of shard count. Color-objective presentation advances on physical arrival,
/// with exact semantic units distributed across the spawned shards.
/// Rendered as `Mesh2d` + `AdditiveMaterial` (not `Sprite`) so overlapping shards SUM brightness
/// instead of alpha-blending into a muddy average — see `additive_material` for why. Alpha is
/// driven directly via the material's color in `tick_score_light`.
#[derive(Component)]
pub(crate) struct ScoreShard {
    from: Vec3,
    /// Quadratic-Bézier control point: bends the path so the shard arcs instead of sliding straight.
    ctrl: Vec3,
    to: Vec3,
    collector_route: CollectorRoute,
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
    /// Portion of this capture's semantic color progress delivered by this physical arrival.
    arrival_units: u32,
    /// Base on-screen size in pixels, baked into `Transform::scale` at spawn (Mesh2d has no
    /// `Sprite::custom_size` equivalent) — animation systems must multiply by this, not overwrite it.
    base_size: f32,
    supernova_launch: Option<SupernovaShardLaunch>,
}

struct SupernovaShardLaunch {
    centre: Vec3,
    target: Vec3,
}

struct ShardSlot {
    position: Vec3,
    color: LightColor,
    arrival_units: u32,
    pop_delay_secs: f32,
    launch_centre: Option<Vec3>,
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

fn cubic_bezier(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

/// Moves at constant perceived speed across two perpendicular legs. The first leg is vertical,
/// so a shard on the right visibly rises before turning toward the score instead of cutting the
/// corner diagonally.
fn right_angle_path(from: Vec3, to: Vec3, frac: f32) -> Vec3 {
    let corner = Vec3::new(from.x, to.y, from.z);
    let first = from.distance(corner);
    let second = corner.distance(to);
    let total = (first + second).max(f32::EPSILON);
    let distance = frac * total;
    if distance <= first && first > f32::EPSILON {
        from.lerp(corner, distance / first)
    } else if second > f32::EPSILON {
        corner.lerp(to, (distance - first).max(0.0) / second)
    } else {
        to
    }
}

/// One broad, energetic bend for the Purple pentagon. A single break reads as a lightning bolt;
/// several tiny bends read as a stiff staircase at board scale.
fn lightning_path(from: Vec3, to: Vec3, frac: f32, phase: f32) -> Vec3 {
    let line = (to - from).truncate();
    let perpendicular = Vec2::new(-line.y, line.x).normalize_or_zero();
    let amplitude = TILE * (1.65 + 0.35 * phase.sin().abs());
    let points = [
        from,
        from.lerp(to, 0.46) + (perpendicular * amplitude).extend(0.0),
        to,
    ];
    let lengths = [points[0].distance(points[1]), points[1].distance(points[2])];
    let total: f32 = lengths.iter().sum::<f32>().max(f32::EPSILON);
    let mut remaining = frac * total;
    for index in 0..lengths.len() {
        if remaining <= lengths[index] || index == lengths.len() - 1 {
            return points[index].lerp(
                points[index + 1],
                remaining / lengths[index].max(f32::EPSILON),
            );
        }
        remaining -= lengths[index];
    }
    to
}

/// Yellow diamonds dive well below the board, with a small diagonal bias, before the score pulls
/// them back. This deliberately uses the available screen space instead of orbiting locally.
fn yellow_dive_path(from: Vec3, to: Vec3, frac: f32, phase: f32) -> Vec3 {
    let horizontal = if phase.cos() >= 0.0 { 1.0 } else { -1.0 };
    let dive = Vec3::new(horizontal * TILE * 1.55, -TILE * 4.6, 0.0);
    cubic_bezier(
        from,
        from + dive,
        from.lerp(to, 0.42) + dive * 1.08,
        to,
        frac,
    )
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
    // Only the travelling Lightcore itself gets hotter; its surrounding halo keeps the previous
    // intensity so score particles remain crisp rather than washing out the whole HUD.
    let boost = hdr_boost * 1.55;
    Color::linear_rgb(boost, boost, boost)
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

/// A color objective must always have a physical carrier for its semantic progress. Normally the
/// reward echo count already supplies one; score-reset effects may suppress score feedback, so a
/// single objective shard is retained in that case.
fn visible_feedback_copies(captured: &CapturedCore, collectors: &LightcoreCollectorTargets) -> u32 {
    if captured.feedback_copies > 0 {
        captured.feedback_copies
    } else if captured.capture_units > 0
        && collectors.route(captured.color).1 == CollectorRoute::ColorGoal
    {
        1
    } else {
        0
    }
}

pub(crate) fn on_capture_batch_score_light(
    trigger: On<CaptureBatch>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    collectors: Res<LightcoreCollectorTargets>,
    shards: Res<ShardSettings>,
    mut materials: ResMut<Assets<AdditiveMaterial>>,
) {
    if trigger.captures.is_empty()
        || trigger
            .captures
            .iter()
            .all(|capture| visible_feedback_copies(capture, &collectors) == 0)
    {
        return;
    }
    let base_size = shards.base_size_frac * TILE;
    let max_size = shards.max_size_frac * TILE;

    // Cascade depth ≈ points per light; drives shard SIZE — deeper chains throw fatter motes.
    let cascade = trigger.cascade_depth as f32;
    let size = (base_size * (1.0 + (cascade - 1.0) * shards.growth)).clamp(base_size, max_size);

    // Flatten semantic reward echoes → shard slots (a couple per echo), dropping to 1 each if a
    // huge clear would otherwise spawn too many. Exact objective units are divided among however
    // many physical slots survive that presentation budget.
    let supernova_origins: Vec<Vec3> = trigger
        .captures
        .iter()
        .filter(|captured| captured.kind == LightKind::Supernova)
        .map(|captured| to_world(captured.grid_position).with_z(2.0))
        .collect();
    let requested_slots: usize = trigger
        .captures
        .iter()
        .map(|captured| visible_feedback_copies(captured, &collectors) as usize)
        .sum::<usize>()
        * SHARDS_PER_POP;
    let per_pop = if requested_slots <= MAX_TOTAL_SHARDS {
        SHARDS_PER_POP
    } else {
        1
    };
    let mut slots = Vec::with_capacity(trigger.captures.len() * per_pop);
    for captured in &trigger.captures {
        let pos = to_world(captured.grid_position);
        let c = captured.color;
        // Only pops inside Supernova reach receive this phase. Other matches retain exactly their
        // existing score animation, and the nearest centre gives double-Supernovas a stable owner.
        let launch_centre = supernova_origins
            .iter()
            .copied()
            .filter(|centre| centre.distance(pos.with_z(2.0)) <= TILE * 1.55)
            .min_by(|a, b| {
                a.distance_squared(pos.with_z(2.0))
                    .total_cmp(&b.distance_squared(pos.with_z(2.0)))
            });
        let slot_count = per_pop * visible_feedback_copies(captured, &collectors) as usize;
        let units_per_slot = captured.capture_units / slot_count.max(1) as u32;
        let remainder = captured.capture_units % slot_count.max(1) as u32;
        for index in 0..slot_count {
            slots.push(ShardSlot {
                position: pos,
                color: c,
                arrival_units: units_per_slot + u32::from(index < remainder as usize),
                pop_delay_secs: captured.available_after_secs,
                launch_centre,
            });
        }
    }

    let shard_curve = shards.curve_frac * TILE;
    // Options keeps these ranges disjoint, but this guard also protects gameplay from a malformed
    // future save or a programmatic resource edit. `rand` panics when start >= end.
    let shard_time_range = safe_shard_time_range(&shards);
    let mut rng = rand::rng();
    for slot in &slots {
        let pos = slot.position;
        let c = slot.color;
        let pop_delay_secs = slot.pop_delay_secs;
        let launch_centre = slot.launch_centre;
        let source = pos.with_z(2.0);
        let (to, collector_route) = collectors.route(c);
        let to = to.with_z(6.0);
        let launch = launch_centre.map(|centre| {
            // The particle does not stop at the 3×3 edge: it is expelled well beyond it. The
            // centre cell has no natural radial direction, so give it one of its own.
            let radial = (source - centre).truncate();
            let direction = if radial.length_squared() > f32::EPSILON {
                radial.normalize()
            } else {
                Vec2::from_angle(rng.random_range(0.0..TAU))
            };
            let target = centre
                + (direction * TILE * rng.random_range(SUPERNOVA_SHARD_DISTANCE_RANGE)).extend(0.0);
            SupernovaShardLaunch { centre, target }
        });
        // A launched shard's score path starts at its farthest explosive point. This avoids the
        // old snap back to the destroyed cell and lets the HUD pull it in from the expanded field.
        let from = launch.as_ref().map_or(source, |launch| launch.target);
        let line = (to - from).truncate();
        let perp = Vec2::new(-line.y, line.x).normalize_or_zero();
        // Control point sits in the first part of the trip, kicked sideways (perp) and a little
        // radially — so the shard visibly springs out of the core before curving to the score.
        let along = rng.random_range(0.15..0.5);
        let side = rng.random_range(-1.0..1.0) * shard_curve;
        let radial =
            Vec2::from_angle(rng.random_range(0.0..TAU)) * rng.random_range(0.0..TILE * 0.6);
        let ctrl = match c {
            // Red's circle is the most fluid core: make its capture arc deliberate rather than a
            // shallow generic bend.
            LightColor::Red => {
                let direction = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
                // Quadratic Bézier reaches roughly half its control-point lateral distance, so
                // this deliberately overshoots several tile widths and takes the core outside
                // the board before the score reels it back in.
                from + (line * 0.18 + perp * shard_curve * 5.2 * direction).extend(0.0)
            }
            _ => from + (line * along + perp * side + radial).extend(0.0),
        };
        let core_color = shard_core_boost(shards.hdr_boost);
        let glow_color = shard_halo_color(c, shards.hdr_boost);

        let parent = commands
            .spawn((
                ScoreShard {
                    from,
                    ctrl,
                    to,
                    collector_route,
                    tint: tint_of(c),
                    timer: Timer::from_seconds(
                        rng.random_range(shard_time_range.clone())
                            * match c {
                                // Green is the arrow: quickest but still visibly readable.
                                LightColor::Green => 0.72,
                                // Purple is a fast electric strike; its wide single bend remains
                                // legible without slowing it into a staircase.
                                LightColor::Purple => 0.76,
                                _ => 1.0,
                            }
                            * if launch.is_some() {
                                rng.random_range(1.20..1.65)
                            } else {
                                1.0
                            },
                        TimerMode::Once,
                    ),
                    pop_delay: Timer::from_seconds(pop_delay_secs, TimerMode::Once),
                    hold: Timer::from_seconds(
                        shards.hold_secs * rng.random_range(0.75..1.25),
                        TimerMode::Once,
                    ),
                    glow_phase: rng.random_range(0.0..TAU),
                    color: c,
                    arrival_units: slot.arrival_units,
                    base_size: size,
                    supernova_launch: launch,
                },
                Mesh2d(cache.unit_quad_mesh.clone()),
                MeshMaterial2d(materials.add(AdditiveMaterial {
                    color: core_color.to_linear(),
                    texture: cache.shard_core_image(c),
                })),
                Transform::from_translation(launch_centre.unwrap_or(from))
                    .with_scale(Vec3::splat(size)),
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

/// Produces the only range handed to `rand` for score-shard travel. Keeping this invariant in the
/// visual system (not only in the menu) makes malformed values harmless.
fn safe_shard_time_range(settings: &ShardSettings) -> std::ops::Range<f32> {
    let min = settings.min_secs.clamp(0.30, 0.95);
    let max = settings.max_secs.clamp(1.00, 2.50).max(min + 0.05);
    min..max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_time_range_stays_valid_when_values_are_reversed() {
        let settings = ShardSettings {
            min_secs: 9.0,
            max_secs: 0.0,
            ..default()
        };
        let range = safe_shard_time_range(&settings);

        assert!(range.start < range.end);
        assert_eq!(range.start, 0.95);
        assert_eq!(range.end, 1.0);
    }
}

pub(crate) fn on_score_drained(
    trigger: On<ScoreDrained>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    collectors: Res<LightcoreCollectorTargets>,
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
                timer: Timer::from_seconds(rng.random_range(0.42..0.68) * 2.2, TimerMode::Once),
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
        let from = collectors.score
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
                    timer: Timer::from_seconds(rng.random_range(0.46..0.76) * 2.2, TimerMode::Once),
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
                        timer: Timer::from_seconds(
                            rng.random_range(0.50..0.85) * 2.2,
                            TimerMode::Once,
                        ),
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
/// fire `CaptureBatch` on the very move that ends the game, leaving shards mid-flight right as
/// `GameOver` is entered. Letting them keep ticking lets them finish their short trip and
/// despawn normally instead of freezing on screen forever.
pub(crate) fn tick_score_light(
    time: Res<Time>,
    mut commands: Commands,
    collectors: Res<LightcoreCollectorTargets>,
    collected: Res<CollectedCores>,
    mut displayed_cores: ResMut<DisplayedCollectedCores>,
    mut glow: ResMut<ScoreGlow>,
    mut goal_pulse: ResMut<ColorGoalCollectorPulse>,
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
        shard.to = collectors.route(shard.color).0.with_z(6.0);
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

        let is_supernova_shard = shard.supernova_launch.is_some();
        if !is_supernova_shard && !shard.hold.tick(time.delta()).is_finished() {
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

        let eased = if let Some(launch) = &shard.supernova_launch {
            // One continuous, inertia-like curve: the first control point keeps the shard moving
            // outward, then the last half bends it into the score's pull. There is no hold or
            // velocity reset at the far point, so it reads as dust drifting in vacuum.
            let outward_control = launch.centre.lerp(launch.target, 1.22);
            t.translation = cubic_bezier(
                launch.centre,
                outward_control,
                launch.target,
                shard.to,
                frac,
            );
            frac
        } else {
            match shard.color {
                LightColor::Green => {
                    // Green triangle: an arrow — direct, steady and uncurved.
                    let pulled = frac * frac;
                    t.translation = shard.from.lerp(shard.to, pulled);
                    pulled
                }
                LightColor::Blue => {
                    // Four-sided cores travel in two clean 90° legs, but the score's pull makes
                    // their pace accelerate instead of reading like a conveyor belt.
                    let pulled = frac * frac;
                    t.translation = right_angle_path(shard.from, shard.to, pulled);
                    pulled
                }
                LightColor::Yellow => {
                    let pulled = frac * frac;
                    t.translation =
                        yellow_dive_path(shard.from, shard.to, pulled, shard.glow_phase);
                    pulled
                }
                LightColor::Purple => {
                    let pulled = frac * frac;
                    t.translation = lightning_path(shard.from, shard.to, pulled, shard.glow_phase);
                    pulled
                }
                _ => {
                    // Standard curved flight path
                    let eased = frac * frac * frac;
                    t.translation = quad_bezier(shard.from, shard.ctrl, shard.to, eased);
                    eased
                }
            }
        };
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
            let color_index = shard.color.index();
            displayed_cores.0[color_index] = displayed_cores.0[color_index]
                .saturating_add(shard.arrival_units)
                .min(collected.0[color_index]);
            match shard.collector_route {
                CollectorRoute::Score => {
                    // The score drifts toward colors that physically landed in it.
                    glow.rgb = glow.rgb.lerp(shard.tint, GLOW_BLEND);
                    glow.pulse = (glow.pulse + 0.5).min(1.0);
                }
                CollectorRoute::ColorGoal => {
                    goal_pulse.0 = (goal_pulse.0 + 0.5).min(1.0);
                }
            }
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
/// Fired by `gameplay::popping::tick_pop_anim` via `LightPopped` — decoupled from `CaptureBatch`
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

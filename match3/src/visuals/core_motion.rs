use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};

use super::assets::VisualCache;
use super::breathing::{BreathPhase, Breathing};
use crate::core::components::{PopAnim, PopDelay};
use crate::core::prelude::*;

pub(crate) const CORE_SIZE: f32 = TILE * 0.10;
pub(crate) const TIER2_3_CORE_SIZE: f32 = TILE * 0.082;
pub(crate) const STAR_CORE_SIZE: f32 = TILE * 0.062; // the star's five cores are smaller, for the swarm feel
const CORE_Z: f32 = 0.5;

/// How a `LightCore` drifts inside its light. The number and pattern of cores is how the player
/// reads a power light — no more white marker shapes, just living light.
#[derive(Component, Clone, Copy)]
pub(crate) enum CorePattern {
    /// 1 core, gentle wander around the center (normal light).
    Wander,
    /// 3 cores strung along the horizontal axis, bobbing past each other (RayH).
    LineH,
    /// 3 cores along the vertical axis (RayV).
    LineV,
    /// 4 cores beating in and out from the center (Supernova).
    RadialPulse,
    /// 4 cores on the arms of a slowly-spinning shuriken: each shoots from the center out to its
    /// vertex (up/down/left/right) and back, staggered so it reads as a turning blade — telling the
    /// player this power sweeps a whole row AND column (Cross).
    Shuriken,
    /// 5 cores pinned to the points of a slowly-rotating five-pointed star, each twinkling in/out
    /// along its spoke (Starburst).
    Star,
    /// One core sits dead-center (the dark nucleus); the rest sweep around it on a steady ring like
    /// rays caught in orbit — a circle with a black eye (Blackhole).
    Blackhole,
}

#[derive(Component)]
pub(crate) struct CoreMotion {
    pattern: CorePattern,
    index: u8,
    count: u8,
    phase: f32,
    base: Vec2,
    radius: f32,
}

struct CoreSpec {
    base: Vec2,
    pattern: CorePattern,
    radius: f32,
}

/// The cluster of cores that identifies each light kind. Core size is per-kind (only Starburst is
/// smaller), so `rebuild_cores` picks the shared mesh from the kind, not from each spec.
fn core_layout(kind: LightKind) -> Vec<CoreSpec> {
    let s = TILE * 0.125; // line spacing
    match kind {
        // 1 core — a barely-there drift.
        LightKind::Normal => vec![CoreSpec {
            base: Vec2::ZERO,
            pattern: CorePattern::Wander,
            radius: 0.0,
        }],
        // 2 cores along the axis (the "haz"/beam).
        LightKind::RayH => vec![
            CoreSpec {
                base: Vec2::new(-s, 0.0),
                pattern: CorePattern::LineH,
                radius: 0.0,
            },
            CoreSpec {
                base: Vec2::new(s, 0.0),
                pattern: CorePattern::LineH,
                radius: 0.0,
            },
        ],
        LightKind::RayV => vec![
            CoreSpec {
                base: Vec2::new(0.0, -s),
                pattern: CorePattern::LineV,
                radius: 0.0,
            },
            CoreSpec {
                base: Vec2::new(0.0, s),
                pattern: CorePattern::LineV,
                radius: 0.0,
            },
        ],
        // 3 cores pulsing in/out.
        LightKind::Supernova => (0..3)
            .map(|_| CoreSpec {
                base: Vec2::ZERO,
                pattern: CorePattern::RadialPulse,
                radius: TILE * 0.135,
            })
            .collect(),
        // 4 cores on the shuriken's cardinal arms; each shoots from center out toward its spike and
        // back, reading as "row AND column". Radius stops short of the spike tip (membrane is at
        // TILE*0.46) so the core never reaches the narrow point — it stays visibly trapped inside.
        LightKind::Cross => (0..4)
            .map(|_| CoreSpec {
                base: Vec2::ZERO,
                pattern: CorePattern::Shuriken,
                radius: TILE * 0.33,
            })
            .collect(),
        // 5 cores riding the star's points, pulled just inside the spike tips (membrane spike at
        // TILE*0.44) so they stay contained within the silhouette.
        LightKind::Starburst => (0..5)
            .map(|_| CoreSpec {
                base: Vec2::ZERO,
                pattern: CorePattern::Star,
                radius: TILE * 0.32,
            })
            .collect(),
        // 6 cores: one dead-center dark nucleus + 5 rays orbiting it on a ring just inside the
        // circle membrane (the center core, index 0, is recolored dark in `rebuild_cores`).
        LightKind::Blackhole => (0..6)
            .map(|_| CoreSpec {
                base: Vec2::ZERO,
                pattern: CorePattern::Blackhole,
                radius: TILE * 0.30,
            })
            .collect(),
    }
}

/// Rebuilds a light's cores whenever its kind changes — which `Changed<LightKind>` catches on both
/// the initial spawn (the component is freshly added) and an in-place upgrade. Gameplay just sets
/// `LightKind`; the visual identity reacts here. Despawns only the old `LightCore` children (never
/// the glow halo, which is also a child).
pub(crate) fn rebuild_cores(
    mut commands: Commands,
    cache: Res<VisualCache>,
    changed: Query<
        (
            Entity,
            &LightKind,
            &LightColor,
            &BreathPhase,
            Option<&Children>,
        ),
        Changed<LightKind>,
    >,
    cores: Query<(), With<LightCore>>,
) {
    for (light, kind, color, phase, children) in &changed {
        if let Some(children) = children {
            for c in children.iter() {
                if cores.contains(c) {
                    commands.entity(c).try_despawn();
                }
            }
        }
        // The membrane shape encodes the kind for the top powers (shuriken/star/circle), so an
        // in-place upgrade must swap the body mesh too — not just the cores. Material (color) is
        // left as-is. For non-shaped kinds this re-sets the same per-color mesh (a no-op handle).
        commands
            .entity(light)
            .insert(Mesh2d(cache.light_mesh(*kind, *color)));
        // Shared core texture. The shaped powers (Cross/Starburst/Blackhole) use the smaller size so
        // their cores read as fine sparks riding the silhouette, never bulging past it.
        let core_size = if matches!(
            *kind,
            LightKind::Cross | LightKind::Starburst | LightKind::Blackhole
        ) {
            STAR_CORE_SIZE
        } else if matches!(
            *kind,
            LightKind::RayH | LightKind::RayV | LightKind::Supernova
        ) {
            TIER2_3_CORE_SIZE
        } else {
            CORE_SIZE
        };
        let specs = core_layout(*kind);
        let count = specs.len() as u8;
        for (i, spec) in specs.iter().enumerate() {
            // Blackhole's center core (index 0) is the dark nucleus: a dim void instead of a bright
            // light, and a touch larger so it reads as the eye the rays orbit. Everything else is
            // the light's normal glow color.
            let is_void_nucleus = matches!(*kind, LightKind::Blackhole) && i == 0;
            let base_color = if is_void_nucleus {
                Color::srgb(0.10, 0.02, 0.16) // deep violet "black light"
            } else {
                color.glow_color()
            };
            let size = if is_void_nucleus {
                core_size * 3.0
            } else {
                core_size * 2.0
            };
            let core = commands
                .spawn((
                    LightCore,
                    Breathing {
                        base: base_color,
                        phase: phase.0 + i as f32 * 0.25,
                    },
                    CoreMotion {
                        pattern: spec.pattern,
                        index: i as u8,
                        count,
                        phase: phase.0 + i as f32 * 0.8,
                        base: spec.base,
                        radius: spec.radius,
                    },
                    // A tinted SPRITE (shared `core_image`) instead of a per-core ColorMaterial mesh, so
                    // all cores batch into one draw call; `breathe` mutates `Sprite::color` (a cheap
                    // component change), not a per-entity material asset.
                    Sprite {
                        image: cache.core_image.clone(),
                        color: base_color,
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    Transform::from_xyz(spec.base.x, spec.base.y, CORE_Z),
                ))
                .id();
            commands.entity(light).add_child(core);
        }
    }
}

fn core_local(m: &CoreMotion, t: f32) -> Vec2 {
    let p = m.phase;
    // Motion gets faster and bigger up the power tiers: a basic light's core is nearly still; a
    // star's five cores swarm quickly. The cue isn't just count — it's how alive the cluster moves.
    match m.pattern {
        // Normal (tier 1): a barely-there drift, slow, hugging the center.
        CorePattern::Wander => {
            m.base + Vec2::new((t * 0.5 + p).sin(), (t * 0.63 + p * 1.7).sin()) * (TILE * 0.018)
        }
        // Ray (tier 2): bobbing along the axis.
        CorePattern::LineH => {
            let bob = (t * 3.2 + m.index as f32 * 1.5).sin() * TILE * 0.040;
            m.base + Vec2::new(bob, 0.0)
        }
        CorePattern::LineV => {
            let bob = (t * 3.2 + m.index as f32 * 1.5).sin() * TILE * 0.040;
            m.base + Vec2::new(0.0, bob)
        }
        // Supernova (tier 3): a brisker in↔out beat.
        CorePattern::RadialPulse => {
            let ang = m.index as f32 / m.count.max(1) as f32 * TAU + t * 0.8;
            let pulse = 0.45 + 0.55 * (0.5 + 0.5 * (t * 2.8 + p).sin());
            Vec2::from_angle(ang) * m.radius * pulse
        }
        // Cross (tier 4): the shuriken's blades. Four cores locked to the membrane's cardinal spike
        // directions (no spin — the mesh is static), each travelling from the center out to its
        // vertex and back in lockstep, so the eye reads a cross sweeping a full row AND column. The
        // `+ FRAC_PI_2` matches the membrane's first spike (pointing up).
        CorePattern::Shuriken => {
            let arm = FRAC_PI_2 + m.index as f32 / m.count.max(1) as f32 * TAU;
            let reach = 0.5 + 0.5 * (t * 2.6).sin(); // 0 = center, 1 = vertex; all arms together
            Vec2::from_angle(arm) * m.radius * reach
        }
        // Starburst (tier 5): one core pinned to each point of the static star membrane (matching
        // its spike angles), twinkling slightly in/out along its spoke without leaving the tip.
        CorePattern::Star => {
            let ang = FRAC_PI_2 + m.index as f32 / m.count.max(1) as f32 * TAU;
            let twinkle = 0.9 + 0.1 * (t * 3.0 + m.index as f32 * 1.7).sin();
            Vec2::from_angle(ang) * m.radius * twinkle
        }
        // Blackhole (tier 6): a dark eye ringed by orbiting rays. Index 0 is the nucleus, held dead
        // center; the rest sweep clockwise on a steady ring at a fixed radius — light caught in orbit
        // around the void.
        CorePattern::Blackhole => {
            if m.index == 0 {
                Vec2::ZERO
            } else {
                let ring = m.count.saturating_sub(1).max(1) as f32;
                let slot = (m.index - 1) as f32 / ring * TAU;
                let ang = slot - t * 0.9; // negative → clockwise
                let rad = m.radius * (0.92 + 0.08 * (t * 1.6 + p).sin());
                Vec2::from_angle(ang) * rad
            }
        }
    }
}

/// Animates each core's local position so the cluster is always in motion — the light feels alive.
pub(crate) fn animate_cores(time: Res<Time>, mut q: Query<(&CoreMotion, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (motion, mut tf) in &mut q {
        let pos = core_local(motion, t);
        tf.translation.x = pos.x;
        tf.translation.y = pos.y;
    }
}

/// Despawns CoreMotion children per-light when that light's pop actually starts, so cores vanish
/// individually as the bolt reaches each light — not all at once at activation time.
/// Two cases: (1) no-delay lights (source, d=0) fire on Added<PopAnim>+Without<PopDelay>;
/// (2) delayed lights fire when tick_pop_anim removes their PopDelay (timer expired).
pub(crate) fn despawn_cores_on_pop(
    mut commands: Commands,
    no_delay: Query<&Children, (Added<PopAnim>, Without<PopDelay>)>,
    mut removed_delay: RemovedComponents<PopDelay>,
    children_q: Query<&Children, With<PopAnim>>,
    cores: Query<(), With<CoreMotion>>,
) {
    for children in &no_delay {
        despawn_cores(&mut commands, children, &cores);
    }
    for entity in removed_delay.read() {
        if let Ok(children) = children_q.get(entity) {
            despawn_cores(&mut commands, children, &cores);
        }
    }
}

fn despawn_cores(
    commands: &mut Commands,
    children: &Children,
    cores: &Query<(), With<CoreMotion>>,
) {
    for &child in children {
        if cores.contains(child) {
            commands.entity(child).try_despawn();
        }
    }
}

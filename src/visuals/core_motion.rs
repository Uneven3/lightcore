use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::assets::VisualCache;
use super::breathing::hollow_breath_factor;
use crate::board::BreathPhase;
use crate::core::components::PopAnim;
use crate::core::prelude::*;

pub(crate) const CORE_SIZE: f32 = TILE * 0.10;
pub(crate) const TIER2_3_CORE_SIZE: f32 = TILE * 0.082;
pub(crate) const STAR_CORE_SIZE: f32 = TILE * 0.062; // the star's five cores are smaller, for the swarm feel
const CORE_Z: f32 = 0.5;

/// How a `LightCore` is arranged inside its light. The number and pattern of cores is how the player
/// reads a power light; only power cores move, while a normal core is a stable landmark.
#[derive(Component, Clone, Copy)]
pub(crate) enum CorePattern {
    /// 1 core fixed at the center (normal light). This is a gameplay landmark, so it must not
    /// shimmer from subpixel motion when the internal canvas is scaled.
    Static,
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

#[derive(Component)]
pub(crate) struct HollowFlowParticle {
    dir: Vec2,
    phase: f32,
}

/// Per-light white breathing state for a Hollow membrane. The material handle is also unique per
/// entity, so Hollows can breathe out of phase without mutating one global shared material.
#[derive(Component)]
pub(crate) struct HollowBreathing {
    base: Srgba,
    phase: f32,
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
        LightKind::Hollow => Vec::new(),
        // 1 core — fixed at the center for stable readability.
        LightKind::Normal => vec![CoreSpec {
            base: Vec2::ZERO,
            pattern: CorePattern::Static,
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
    mut materials: ResMut<Assets<ColorMaterial>>,
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
    transient_children: Query<(), Or<(With<LightCore>, With<HollowFlowParticle>)>>,
) {
    for (light, kind, color, phase, children) in &changed {
        if let Some(children) = children {
            for c in children.iter() {
                if transient_children.contains(c) {
                    commands.entity(c).try_despawn();
                }
            }
        }
        // The membrane shape encodes the kind for the top powers (shuriken/star/circle), so an
        // in-place upgrade must swap the body mesh too — not just the cores. Material (color) is
        // left as-is. For non-shaped kinds this re-sets the same per-color mesh (a no-op handle).
        let material = if matches!(*kind, LightKind::Hollow) {
            let base = Srgba::new(0.92, 0.96, 1.0, 1.0);
            commands.entity(light).insert(HollowBreathing {
                base,
                phase: phase.0,
            });
            materials.add(ColorMaterial::from_color(base))
        } else {
            commands.entity(light).remove::<HollowBreathing>();
            cache.light_mat(*kind, *color)
        };
        commands.entity(light).insert((
            Mesh2d(cache.light_mesh(*kind, *color)),
            MeshMaterial2d(material),
        ));
        if matches!(*kind, LightKind::Hollow) {
            spawn_hollow_flow(&mut commands, &cache, light, phase.0);
        }
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
            let size = if is_void_nucleus {
                core_size * 3.0
            } else {
                core_size * 2.0
            };
            // The void nucleus is a dim eye, not "this light's color" — it stays the plain circular
            // `core_image`. Every other core is shaped like the light's own color (circle/triangle/
            // square/diamond/pentagon, see `assets::shaped_core_image`), matching the app icon's
            // per-color shape language — even for the kind-shaped powers (Cross/Starburst/Blackhole),
            // whose RING silhouette shows the power instead, but whose core dots still read as
            // "this light's color".
            let base_color = if is_void_nucleus {
                Color::srgb(0.10, 0.02, 0.16)
            } else {
                color.glow_color()
            };
            let core_image = if is_void_nucleus {
                cache.core_image.clone()
            } else {
                cache.light_core_image(*color)
            };
            let core = commands
                .spawn((
                    LightCore,
                    CoreMotion {
                        pattern: spec.pattern,
                        index: i as u8,
                        count,
                        phase: phase.0 + i as f32 * 0.8,
                        base: spec.base,
                        radius: spec.radius,
                    },
                    Sprite {
                        image: core_image,
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

fn spawn_hollow_flow(commands: &mut Commands, cache: &VisualCache, light: Entity, base_phase: f32) {
    let dirs = [
        Vec2::new(1.0, 1.0).normalize(),
        Vec2::new(-1.0, 1.0).normalize(),
        Vec2::new(-1.0, -1.0).normalize(),
        Vec2::new(1.0, -1.0).normalize(),
    ];
    let radius = TILE * 0.34;
    for (arm, dir) in dirs.into_iter().enumerate() {
        for lane in 0..3 {
            let phase = (base_phase * 0.13 + arm as f32 * 0.17 + lane as f32 / 3.0).fract();
            let particle = commands
                .spawn((
                    HollowFlowParticle { dir, phase },
                    Sprite {
                        image: cache.core_image.clone(),
                        color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                        custom_size: Some(Vec2::splat(TILE * 0.060)),
                        ..default()
                    },
                    Transform::from_translation((dir * radius).extend(CORE_Z + 0.20)),
                ))
                .id();
            commands.entity(light).add_child(particle);
        }
    }
}

fn core_local(m: &CoreMotion, t: f32) -> Vec2 {
    let p = m.phase;
    // Motion gets faster and bigger up the power tiers: a basic light's core is nearly still; a
    // star's five cores swarm quickly. The cue isn't just count — it's how alive the cluster moves.
    match m.pattern {
        CorePattern::Static => m.base,
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

/// Animates power-core clusters. Normal cores deliberately skip all per-frame transform writes.
pub(crate) fn animate_cores(time: Res<Time>, mut q: Query<(&CoreMotion, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (motion, mut tf) in &mut q {
        // Avoid marking normal-core transforms changed every frame. Besides saving hierarchy work,
        // this keeps the small high-contrast shape locked to the same pixel footprint.
        if matches!(motion.pattern, CorePattern::Static) {
            continue;
        }
        let pos = core_local(motion, t);
        tf.translation.x = pos.x;
        tf.translation.y = pos.y;
    }
}

pub(crate) fn animate_hollow_flow(
    time: Res<Time>,
    mut q: Query<(&HollowFlowParticle, &mut Transform, &mut Sprite)>,
) {
    let t = time.elapsed_secs();
    let radius = TILE * 0.34;
    for (flow, mut tf, mut sprite) in &mut q {
        let frac = (t * 0.55 + flow.phase).fract();
        let eased = frac * frac;
        // Calculate a spiral curve by rotating the direction angle based on progress
        let base_ang = flow.dir.y.atan2(flow.dir.x);
        let ang = base_ang + (1.0 - eased) * 1.6;
        let pos = Vec2::from_angle(ang) * radius * (1.0 - eased);

        tf.translation.x = pos.x;
        tf.translation.y = pos.y;
        tf.scale = Vec3::splat(0.75 + 0.35 * (1.0 - frac));
        let alpha = (frac * PI).sin().max(0.0) * 0.82;
        sprite.color = Color::srgba(0.0, 0.0, 0.0, alpha);
    }
}

pub(crate) fn animate_hollow_breath(
    time: Res<Time>,
    hollows: Query<(&HollowBreathing, &MeshMaterial2d<ColorMaterial>), Without<PopAnim>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let t = time.elapsed_secs();
    for (breathing, material) in &hollows {
        let factor = hollow_breath_factor(t, breathing.phase);
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.color = Color::srgba(
                breathing.base.red * factor,
                breathing.base.green * factor,
                breathing.base.blue * factor,
                breathing.base.alpha,
            );
        }
    }
}

/// Keeps a light's nucleus and membrane on one visual lifetime. `PopAnim` does not advance while a
/// propagation `PopDelay` is pending, so the core remains fully visible until the effect actually
/// reaches the cell; it then fades over exactly the same interval as the membrane. The parent
/// despawn owns the final cleanup.
pub(crate) fn fade_cores_on_pop(
    popping: Query<(&PopAnim, &Children)>,
    mut cores: Query<&mut Sprite, With<LightCore>>,
) {
    for (anim, children) in &popping {
        let alpha = 1.0 - anim.0.fraction();
        for child in children.iter() {
            if let Ok(mut sprite) = cores.get_mut(child) {
                sprite.color = sprite.color.with_alpha(alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popping_core_fades_with_parent_instead_of_disappearing_early() {
        let mut app = App::new();
        app.add_systems(Update, fade_cores_on_pop);

        let mut timer = Timer::from_seconds(1.0, TimerMode::Once);
        timer.tick(std::time::Duration::from_millis(250));
        let parent = app.world_mut().spawn(PopAnim(timer)).id();
        let core = app
            .world_mut()
            .spawn((
                LightCore,
                Sprite {
                    color: Color::srgba(2.0, 1.0, 0.5, 1.0),
                    ..default()
                },
                ChildOf(parent),
            ))
            .id();

        app.update();

        let color = app.world().get::<Sprite>(core).unwrap().color.to_srgba();
        assert!((color.alpha - 0.75).abs() < 0.001);
        assert!((color.red - 2.0).abs() < 0.001);
    }
}

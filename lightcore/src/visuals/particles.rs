use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::TAU;

use crate::core::grid::TILE;

const PARTICLE_DRAG: f32 = 4.0;

const MEMBRANE_POP_COUNT: usize = 14;

/// Live-tunable particle parameters, edited from the Options screen. `burst_radius`/
/// `membrane_radius` size the shared meshes cached in `VisualCache` — `sync_particle_mesh_settings`
/// pushes edits into those meshes in place, so every burst (already spawned or future) updates
/// immediately. `pop_burst_count`/`trail_particle_count` are read at spawn time by their callers.
#[derive(Resource, Clone, Copy)]
pub(crate) struct ParticleSettings {
    pub(crate) pop_burst_count: usize,
    pub(crate) burst_radius: f32,
    pub(crate) membrane_radius: f32,
    pub(crate) trail_particle_count: usize,
}

impl Default for ParticleSettings {
    fn default() -> Self {
        Self {
            pop_burst_count: 6,
            burst_radius: TILE * 0.05,
            membrane_radius: TILE * 0.022,
            trail_particle_count: 2,
        }
    }
}

/// Pushes `ParticleSettings`' radii into the already-built shared meshes (`VisualCache::burst_mesh`/
/// `membrane_mesh`) whenever the resource changes — mutating the mesh asset in place updates every
/// particle that references the handle, past and future, with no respawn needed.
pub(crate) fn sync_particle_mesh_settings(
    settings: Res<ParticleSettings>,
    cache: Res<super::assets::VisualCache>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if let Some(mut mesh) = meshes.get_mut(&cache.burst_mesh) {
        *mesh = Circle::new(settings.burst_radius).into();
    }
    if let Some(mut mesh) = meshes.get_mut(&cache.membrane_mesh) {
        *mesh = Circle::new(settings.membrane_radius).into();
    }
}

/// Tiny lightcore-shaped fragments flung outward in a near-even ring with a hard short life — the
/// light's membrane bursting like a soap bubble the instant its core is collected. `image` is the
/// same per-color silhouette used by the lightcore itself (circle / triangle / square / diamond /
/// pentagon), while `color` is HDR-bright so Bloom reads every fragment as emitted energy.
pub(crate) fn spawn_membrane_pop(
    commands: &mut Commands,
    image: Handle<Image>,
    world_pos: Vec3,
    color: Color,
    radius: f32,
) {
    let mut rng = rand::rng();
    for i in 0..MEMBRANE_POP_COUNT {
        // Even ring + small jitter → a clean expanding shell, not a random spray.
        let angle = (i as f32 / MEMBRANE_POP_COUNT as f32) * TAU + rng.random_range(-0.12..0.12);
        let speed = rng.random_range(TILE * 2.2..TILE * 3.4);
        let velocity = (Vec2::from_angle(angle) * speed).extend(0.0);
        commands.spawn((
            Particle,
            ParticleVelocity(velocity),
            ParticleLifetime {
                timer: Timer::from_seconds(rng.random_range(0.24..0.39), TimerMode::Once),
                fade_start_frac: 0.0, // thin out from the very start as the shell expands
            },
            ParticleDrag(PARTICLE_DRAG),
            ParticleOrigin(world_pos),
            Sprite {
                image: image.clone(),
                color,
                custom_size: Some(Vec2::splat(radius * 2.0)),
                ..default()
            },
            Transform::from_translation(world_pos.with_z(0.95)),
        ));
    }
}

/// Marker shared by every short-lived lightcore particle. Motion is deliberately split into
/// independent components below so a power can change only the attributes it owns.
#[derive(Component)]
pub(crate) struct Particle;

/// World-space velocity. Supernova can replace or add to this without knowing anything about the
/// particle's sprite, lifetime, or rendering mesh.
#[derive(Component, Clone, Copy)]
pub(crate) struct ParticleVelocity(pub(crate) Vec3);

/// Lifetime and alpha curve, kept separate from movement so effects can send a particle farther
/// without extending every other particle's life by accident.
#[derive(Component)]
pub(crate) struct ParticleLifetime {
    timer: Timer,
    fade_start_frac: f32,
}

/// Per-particle damping. A Supernova particle can use a much lower drag than a normal membrane
/// pop and therefore visibly travel away from the blast centre.
#[derive(Component, Clone, Copy)]
pub(crate) struct ParticleDrag(pub(crate) f32);

/// Immutable birthplace of a particle. This is useful for explosion choreography even after its
/// transform has moved, and avoids inferring its origin from a mutable `Transform`.
#[derive(Component, Clone, Copy)]
pub(crate) struct ParticleOrigin(pub(crate) Vec3);

/// Optional radial acceleration. It is intentionally an additive component: ordinary particles
/// have no force, while a Supernova can attach this to precisely its own particles.
#[derive(Component, Clone, Copy)]
pub(crate) struct RadialParticleForce {
    pub(crate) centre: Vec3,
    pub(crate) acceleration: f32,
    pub(crate) max_speed: f32,
}

/// Spawns `count` small decelerating/fading circular energy particles radiating from `world_pos`.
/// `color` should already be HDR-boosted (e.g. `LightColor::glow_color()`) so Bloom picks
/// it up the same way it picks up `LightCore`.
pub(crate) fn spawn_burst(
    commands: &mut Commands,
    image: Handle<Image>,
    world_pos: Vec3,
    color: Color,
    count: usize,
    radius: f32,
) {
    let mut rng = rand::rng();
    for _ in 0..count {
        let angle = rng.random_range(0.0..std::f32::consts::TAU);
        let speed = rng.random_range(TILE * 1.3..TILE * 3.1);
        let velocity = (Vec2::from_angle(angle) * speed).extend(0.0);
        commands.spawn((
            Particle,
            ParticleVelocity(velocity),
            ParticleLifetime {
                timer: Timer::from_seconds(rng.random_range(0.38..0.68), TimerMode::Once),
                fade_start_frac: 0.35,
            },
            ParticleDrag(PARTICLE_DRAG),
            ParticleOrigin(world_pos),
            Sprite {
                image: image.clone(),
                color,
                custom_size: Some(Vec2::splat(radius * 2.0)),
                ..default()
            },
            Transform::from_translation(world_pos.with_z(0.9)),
        ));
    }
}

pub(crate) fn tick_particles(
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut Transform,
        &mut Sprite,
        &mut ParticleVelocity,
        &mut ParticleLifetime,
        Option<&ParticleDrag>,
        Option<&RadialParticleForce>,
        &ParticleOrigin,
    ), With<Particle>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (e, mut t, mut sprite, mut velocity, mut lifetime, drag, force, origin) in &mut q {
        lifetime.timer.tick(time.delta());
        if let Some(force) = force {
            let from_centre = (t.translation - force.centre).truncate();
            // Particles born exactly at the centre need a valid first direction before their
            // transform has had a chance to move. Their existing launch velocity provides that
            // direction; origin is the stable fallback for particles that are repositioned.
            let outward = if from_centre.length_squared() > f32::EPSILON {
                from_centre.normalize()
            } else {
                let origin_direction = (origin.0 - force.centre).truncate();
                if origin_direction.length_squared() > f32::EPSILON {
                    origin_direction.normalize()
                } else {
                    velocity.0.truncate().normalize_or_zero()
                }
            }
            .extend(0.0);
            velocity.0 += outward * force.acceleration * dt;
            let speed = velocity.0.length();
            if speed > force.max_speed {
                velocity.0 *= force.max_speed / speed;
            }
        }
        t.translation += velocity.0 * dt;
        let drag = drag.map_or(PARTICLE_DRAG, |drag| drag.0);
        velocity.0 *= (1.0 - drag * dt).max(0.0);

        let frac = lifetime.timer.fraction();
        if frac >= lifetime.fade_start_frac {
            let fade = (frac - lifetime.fade_start_frac)
                / (1.0 - lifetime.fade_start_frac).max(0.0001);
            sprite.color = sprite.color.with_alpha((1.0 - fade).max(0.0));
        }
        if lifetime.timer.is_finished() {
            commands.entity(e).try_despawn();
        }
    }
}

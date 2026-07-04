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

/// Tiny white specks flung outward in a near-even ring with a hard short life — the light's
/// hollow membrane bursting like a soap bubble the instant its core is collected. Separate from
/// `spawn_burst` (the colored core energy): these are smaller, faster, shorter and fade from the
/// very start, so they read as a thin shell popping, not a colored puff lingering.
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
            Particle {
                velocity,
                timer: Timer::from_seconds(rng.random_range(0.24..0.39), TimerMode::Once),
                fade_start_frac: 0.0, // thin out from the very start as the shell expands
            },
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

#[derive(Component)]
pub(crate) struct Particle {
    velocity: Vec3,
    timer: Timer,
    fade_start_frac: f32,
}

/// Spawns `count` small decelerating/fading circles radiating from `world_pos` in `color`.
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
            Particle {
                velocity,
                timer: Timer::from_seconds(rng.random_range(0.38..0.68), TimerMode::Once),
                fade_start_frac: 0.35,
            },
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
    mut q: Query<(Entity, &mut Transform, &mut Sprite, &mut Particle)>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (e, mut t, mut sprite, mut p) in &mut q {
        p.timer.tick(time.delta());
        t.translation += p.velocity * dt;
        p.velocity *= (1.0 - PARTICLE_DRAG * dt).max(0.0);

        let frac = p.timer.fraction();
        if frac >= p.fade_start_frac {
            let fade = (frac - p.fade_start_frac) / (1.0 - p.fade_start_frac).max(0.0001);
            sprite.color = sprite.color.with_alpha((1.0 - fade).max(0.0));
        }
        if p.timer.is_finished() {
            commands.entity(e).try_despawn();
        }
    }
}

use bevy::prelude::*;

use super::EffectAnim;
use super::assets::VisualCache;
use super::particles::{ParticleSettings, spawn_burst};
use super::RaySettings;
use crate::core::prelude::*;
use crate::gameplay::PowerBlastTrail;

const TRAIL_SPAWN_INTERVAL_SECS: f32 = 0.02;

/// A short bright rectangle that travels along one axis from the activator to the grid edge.
/// Two are spawned per RayH/RayV (one in each direction), four for Cross.
#[derive(Component)]
pub(crate) struct LaserBolt {
    from: Vec3,
    dir: Vec3,
    total: f32,
    traveled: f32,
    spawn_cooldown: Timer,
    color: Color,
    delay: Option<Timer>,
}

/// Spawns one directional bolt from `from` toward `to`. Uses a beam_image Sprite (bright along the
/// axis, soft perpendicular glow) instead of a flat rectangle — gives the "light in a tube" look.
fn spawn_bolt(
    commands: &mut Commands,
    cache: &VisualCache,
    ray: &RaySettings,
    from: Vec3,
    to: Vec3,
    horizontal: bool,
    delay_secs: f32,
) {
    let diff = to - from;
    let total = diff.length();
    if total < TILE * 0.5 {
        return;
    }
    let dir = diff / total;
    let particle_color = Color::srgb(3.5, 4.5, 6.0); // blue-white for trail particles
    // Neutral HDR white: beam_image encodes the warm-white→blue color gradient internally,
    // so the sprite color is just a brightness multiplier (no additional tint).
    let sprite_color = Color::linear_rgb(5.0, 5.0, 5.5);
    // beam_image is designed horizontal; vertical bolts rotate 90° so the same texture works.
    let beam_size = Vec2::new(ray.bolt_length() * 2.0, ray.bolt_width());
    let rotation = if horizontal {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
    };
    commands.spawn((
        LaserBolt {
            from,
            dir,
            total,
            traveled: 0.0,
            spawn_cooldown: Timer::from_seconds(0.025, TimerMode::Repeating),
            color: particle_color,
            delay: if delay_secs > 0.0 {
                Some(Timer::from_seconds(delay_secs, TimerMode::Once))
            } else {
                None
            },
        },
        Sprite {
            image: cache.beam_image.clone(),
            color: sprite_color.with_alpha(if delay_secs > 0.0 { 0.0 } else { 1.0 }),
            custom_size: Some(beam_size),
            ..default()
        },
        Transform::from_translation(from.with_z(1.5)).with_rotation(rotation),
    ));
}

/// Spawns two bolts (positive and negative direction) for one axis.
fn spawn_ray_bolts(
    commands: &mut Commands,
    cache: &VisualCache,
    ray: &RaySettings,
    source: Vec3,
    horizontal: bool,
    delay_secs: f32,
) {
    let half_w = GRID_W as f32 * TILE * 0.5;
    let half_h = GRID_H as f32 * TILE * 0.5;
    if horizontal {
        spawn_bolt(
            commands,
            cache,
            ray,
            source,
            Vec3::new(half_w, source.y, source.z),
            true,
            delay_secs,
        );
        spawn_bolt(
            commands,
            cache,
            ray,
            source,
            Vec3::new(-half_w, source.y, source.z),
            true,
            delay_secs,
        );
    } else {
        spawn_bolt(
            commands,
            cache,
            ray,
            source,
            Vec3::new(source.x, half_h, source.z),
            false,
            delay_secs,
        );
        spawn_bolt(
            commands,
            cache,
            ray,
            source,
            Vec3::new(source.x, -half_h, source.z),
            false,
            delay_secs,
        );
    }
}

/// Walks a sequence of world positions over `TRAIL_DURATION_SECS`, dropping a short trail of
/// particles along the way. Used by Supernova, Starburst, and Blackhole (not Rays — those use
/// `LaserBolt` instead).
#[derive(Component)]
pub(crate) struct TravelingLight {
    path: Vec<Vec3>,
    cumulative: Vec<f32>,
    total_len: f32,
    color: Color,
    timer: Timer,
    spawn_cooldown: Timer,
    delay: Timer,
}

fn make_beam(
    path: Vec<Vec3>,
    color: Color,
    delay_secs: f32,
    trail_duration: f32,
) -> TravelingLight {
    let mut cumulative = Vec::with_capacity(path.len());
    let mut acc = 0.0;
    cumulative.push(0.0);
    for i in 1..path.len() {
        acc += path[i].distance(path[i - 1]);
        cumulative.push(acc);
    }
    TravelingLight {
        path,
        cumulative,
        total_len: acc.max(0.0001),
        color,
        timer: Timer::from_seconds(trail_duration, TimerMode::Once),
        spawn_cooldown: Timer::from_seconds(TRAIL_SPAWN_INTERVAL_SECS, TimerMode::Repeating),
        delay: Timer::from_seconds(delay_secs, TimerMode::Once),
    }
}

pub(crate) fn on_power_blast_trail(
    trigger: On<PowerBlastTrail>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    ray: Res<RaySettings>,
) {
    let path = &trigger.path;
    if path.is_empty() {
        return;
    }
    let source = path[0];

    let delay = trigger.delay_secs;
    match trigger.kind {
        LightKind::RayH => {
            spawn_ray_bolts(&mut commands, &cache, &ray, source, true, delay);
        }
        LightKind::RayV => {
            spawn_ray_bolts(&mut commands, &cache, &ray, source, false, delay);
        }
        LightKind::Cross => {
            spawn_ray_bolts(&mut commands, &cache, &ray, source, true, delay);
            spawn_ray_bolts(&mut commands, &cache, &ray, source, false, delay);
        }
        kind => {
            // StarSupernova: orbes (TravelingLight) viajan al target y cuando llegan disparan
            // PowerBlastTrail { kind: Supernova, path: [target], delay_secs }. Spawneamos un
            // mini-glow con delay para que la explosión coincida con la llegada del orbe.
            if kind == LightKind::Supernova && path.len() == 1 {
                let glow_color = Color::linear_rgb(5.0, 3.2, 0.8);
                let start_scale = Vec3::splat(0.3);
                commands.spawn((
                    EffectAnim {
                        timer: Timer::from_seconds(0.52, TimerMode::Once),
                        start_scale,
                        end_scale: Vec3::splat(3.6),
                        base_alpha: 1.0,
                        fade_start_frac: 0.3,
                        delay: if delay > 0.0 {
                            Some(Timer::from_seconds(delay, TimerMode::Once))
                        } else {
                            None
                        },
                    },
                    Sprite {
                        image: cache.glow_image.clone(),
                        color: glow_color.with_alpha(if delay > 0.0 { 0.0 } else { 1.0 }),
                        custom_size: Some(Vec2::splat(TILE * 2.0)),
                        ..default()
                    },
                    Transform::from_translation(source.with_z(0.8)).with_scale(start_scale),
                ));
                return;
            }
            if path.len() < 2 {
                return;
            }
            let color = match kind {
                LightKind::Starburst => trigger
                    .color
                    .map(|c| c.glow_color())
                    .unwrap_or(Color::srgb(4.0, 4.0, 4.0)),
                LightKind::Supernova => Color::srgb(4.0, 2.6, 0.6),
                LightKind::Blackhole => Color::srgb(1.4, 2.2, 4.2),
                _ => Color::srgb(4.0, 4.0, 4.0),
            };
            if kind == LightKind::Starburst {
                for (i, &target) in path.iter().enumerate().skip(1) {
                    if target.distance(source) < 0.001 {
                        continue;
                    }
                    let delay = (ray.stagger_secs * (i - 1) as f32).min(ray.stagger_max);
                    // Sprite con glow_image para que el orbe sea visible viajando de la
                    // estrella al target. Alpha=0 hasta que el delay expira.
                    commands.spawn((
                        make_beam(vec![source, target], color, delay, ray.trail_duration),
                        Transform::from_translation(source.with_z(1.8)),
                        Sprite {
                            image: cache.glow_image.clone(),
                            color: color.with_alpha(0.0),
                            custom_size: Some(Vec2::splat(TILE * 0.55)),
                            ..default()
                        },
                    ));
                }
            } else {
                commands.spawn((
                    make_beam(path.clone(), color, delay, ray.trail_duration),
                    Transform::from_translation(source.with_z(1.8)),
                ));
            }
        }
    }
}

pub(crate) fn tick_laser_bolt(
    mut commands: Commands,
    cache: Res<VisualCache>,
    mut q: Query<(Entity, &mut LaserBolt, &mut Transform, &mut Sprite)>,
    particles: Res<ParticleSettings>,
    ray: Res<RaySettings>,
    time: Res<Time>,
) {
    for (e, bolt, mut t, mut sprite) in &mut q {
        let mut bolt = bolt;
        // Si hay delay activo, esperar antes de empezar a mover el bolt.
        if let Some(ref mut d) = bolt.delay {
            d.tick(time.delta());
            if !d.is_finished() {
                continue;
            }
            bolt.delay = None;
            sprite.color = sprite.color.with_alpha(1.0);
        }
        bolt.traveled += ray.speed * time.delta_secs();
        let center = bolt.from + bolt.dir * (bolt.traveled + ray.bolt_length() * 0.5);
        t.translation = center.with_z(1.5);
        let frac = (bolt.traveled / bolt.total).clamp(0.0, 1.0);
        let alpha = if frac > 0.8 { (1.0 - frac) / 0.2 } else { 1.0 };
        sprite.color = sprite.color.with_alpha(alpha.max(0.0));
        if bolt.traveled >= bolt.total {
            commands.entity(e).try_despawn();
            continue;
        }
        bolt.spawn_cooldown.tick(time.delta());
        if bolt.spawn_cooldown.just_finished() {
            spawn_burst(
                &mut commands,
                cache.core_image.clone(),
                t.translation,
                bolt.color,
                particles.trail_particle_count,
                particles.burst_radius,
            );
        }
    }
}

pub(crate) fn tick_traveling_light(
    mut commands: Commands,
    cache: Res<VisualCache>,
    mut q: Query<(
        Entity,
        &mut TravelingLight,
        &mut Transform,
        Option<&mut Sprite>,
    )>,
    time: Res<Time>,
    particles: Res<ParticleSettings>,
) {
    for (e, mut light, mut transform, sprite) in &mut q {
        if !light.delay.tick(time.delta()).is_finished() {
            continue;
        }
        light.timer.tick(time.delta());
        let target_len = light.timer.fraction() * light.total_len;
        let idx = light
            .cumulative
            .partition_point(|&c| c < target_len)
            .saturating_sub(1)
            .min(light.path.len() - 2);
        let (seg_start, seg_end) = (light.cumulative[idx], light.cumulative[idx + 1]);
        let seg_frac = if seg_end > seg_start {
            ((target_len - seg_start) / (seg_end - seg_start)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let head_pos = light.path[idx].lerp(light.path[idx + 1], seg_frac);

        // Mover la entidad con el head del beam (permite que el Sprite del Starburst se vea viajar)
        transform.translation = head_pos.with_z(1.8);

        // Si tiene Sprite (Starburst beams): aparecer al inicio, desvanecerse al llegar al target
        if let Some(mut spr) = sprite {
            let frac = light.timer.fraction();
            let alpha = if frac < 0.7 { 1.0 } else { (1.0 - frac) / 0.3 };
            spr.color = spr.color.with_alpha(alpha.max(0.0));
        }

        light.spawn_cooldown.tick(time.delta());
        if light.spawn_cooldown.just_finished() {
            spawn_burst(
                &mut commands,
                cache.core_image.clone(),
                head_pos,
                light.color,
                particles.trail_particle_count,
                particles.burst_radius,
            );
        }
        if light.timer.is_finished() {
            commands.entity(e).try_despawn();
        }
    }
}

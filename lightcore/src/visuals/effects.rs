use bevy::prelude::*;

use super::EffectAnim;
use super::assets::VisualCache;
use crate::core::prelude::*;
use crate::gameplay::{PowerCombo, PowerConsumed};

pub(crate) fn on_power_light_consumed(
    trigger: On<PowerConsumed>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    spawn_power_effect(
        &mut commands,
        &cache,
        &mut materials,
        trigger.kind,
        trigger.pos,
        trigger.color,
    );
}

pub(crate) fn on_power_combo(
    trigger: On<PowerCombo>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    spawn_combo_effect(
        &mut commands,
        &cache,
        &mut materials,
        trigger.kind,
        trigger.a_pos,
        trigger.b_pos,
        trigger.color,
    );
}

/// The base mesh for a power light's blast effect (the transient entity is *scaled* over its life,
/// so the mesh itself is constant per kind). Built once into `visuals::assets::VisualCache`.
/// `Normal` has no effect; it returns a degenerate mesh the cache never stores.
pub(crate) fn build_effect_mesh(kind: LightKind) -> Mesh {
    match kind {
        LightKind::RayH => Rectangle::new(TILE * GRID_W as f32, TILE * 0.34).into(),
        LightKind::RayV => Rectangle::new(TILE * 0.34, TILE * GRID_H as f32).into(),
        LightKind::Supernova => Annulus::new(TILE * 0.42, TILE * 0.6).into(),
        LightKind::Starburst => Circle::new(TILE * 0.45).into(),
        // Cross has no own mesh (drawn as a RayH + RayV pair); Blackhole has its own dedicated
        // void/rim meshes built straight into `VisualCache` (see `assets::build_cache`), not via
        // this function. Neither ever reaches `build_cache`'s call into `build_effect_mesh` —
        // both degenerate the same as Normal.
        LightKind::Cross | LightKind::Normal | LightKind::Hollow | LightKind::Blackhole => {
            Circle::new(1.0).into()
        }
    }
}

fn spawn_power_effect(
    commands: &mut Commands,
    cache: &VisualCache,
    materials: &mut Assets<ColorMaterial>,
    kind: LightKind,
    pos: GridPos,
    color: Option<LightColor>,
) {
    // The Cross is two crossed rays — LaserBolts handle the beams; only spawn the center flourish.
    if kind == LightKind::Cross {
        spawn_cross_flourish(commands, cache, materials, pos);
        return;
    }

    // RayH/RayV beams are handled entirely by LaserBolt in light_trail.rs — no static flash here.
    if matches!(kind, LightKind::RayH | LightKind::RayV) {
        return;
    }

    let world = to_world(pos);

    // Blackhole has its own two-part choreography (void + event-horizon rim), not a single
    // scaled flash — handled entirely by its own helper.
    if kind == LightKind::Blackhole {
        spawn_blackhole_effect(commands, cache, materials, world);
        return;
    }

    let Some(mesh) = cache.effect_mesh(kind) else {
        return;
    }; // Normal: no blast
    // All colors are HDR-overbright (>1.0) so the camera's Bloom turns them into glowing light
    // instead of flat shapes — cosmic, not poster-paint.
    let (mat_color, world_pos, start_scale, end_scale, duration, fade_start_frac) = match kind {
        // A thin HDR beam that flashes along the whole row/column (thickness blooms in, then
        // fades); the streaking particle heads are added by `light_trail::TravelingLight`.
        LightKind::RayH => (
            Color::srgb(2.2, 2.6, 4.0), // blue-white starlight
            Vec3::new(0.0, world.y, 0.8),
            Vec3::new(1.0, 0.2, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            0.42,
            0.35,
        ),
        LightKind::RayV => (
            Color::srgb(2.2, 2.6, 4.0),
            Vec3::new(world.x, 0.0, 0.8),
            Vec3::new(0.2, 1.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            0.42,
            0.35,
        ),
        // Supernova: usa glow_image (brillo central difuminado) en vez del Annulus plano —
        // nace como punto brillante y se expande como esfera de luz dorada.
        LightKind::Supernova => {
            spawn_supernova_glow(commands, cache, world);
            return;
        }
        // A bright core flash at the star itself — the seeking beams to each colored light are
        // spawned separately by `light_trail` (one traveling beam per target).
        LightKind::Starburst => (
            color
                .map(|c| c.glow_color())
                .unwrap_or(Color::srgb(4.0, 4.0, 4.0)),
            world.with_z(0.85),
            Vec3::splat(0.3),
            Vec3::splat(1.8),
            0.45,
            0.35,
        ),
        // Cross and Blackhole are intercepted above (their own choreography); Normal has no blast.
        LightKind::Cross | LightKind::Normal | LightKind::Hollow | LightKind::Blackhole => return,
    };
    let base_alpha = mat_color.alpha();
    commands.spawn((
        EffectAnim {
            timer: Timer::from_seconds(duration, TimerMode::Once),
            start_scale,
            end_scale,
            base_alpha,
            fade_start_frac,
            delay: None,
        },
        Mesh2d(mesh),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(mat_color))),
        Transform::from_translation(world_pos).with_scale(start_scale),
    ));
}

/// One unified choreography per power-vs-power interaction, so a combo reads as a single event
/// rather than two coincidental single-power effects. Composes the per-kind blast builders
/// (`spawn_power_effect`), the crossing-point flash (`spawn_cross_flourish`), the void/rim collapse
/// (`spawn_blackhole_effect`) and a scaled supernova ring (`spawn_combo_ring`). `a` is the anchor
/// the gameplay layer chose (Starburst / Supernova / board centre — see `gameplay::PowerCombo`),
/// `b` the partner; `color` is the cleared color for Starburst combos.
fn spawn_combo_effect(
    commands: &mut Commands,
    cache: &VisualCache,
    materials: &mut Assets<ColorMaterial>,
    kind: ComboKind,
    a: GridPos,
    b: GridPos,
    color: Option<LightColor>,
) {
    let mid = GridPos {
        x: (a.x + b.x) / 2,
        y: (a.y + b.y) / 2,
    };
    match kind {
        // Both rows + both columns sweep as one grid wipe, bound by a single flash at the centre.
        ComboKind::DoubleLine => {
            spawn_power_effect(commands, cache, materials, LightKind::RayH, a, None);
            if b.y != a.y {
                spawn_power_effect(commands, cache, materials, LightKind::RayH, b, None);
            }
            spawn_power_effect(commands, cache, materials, LightKind::RayV, a, None);
            if b.x != a.x {
                spawn_power_effect(commands, cache, materials, LightKind::RayV, b, None);
            }
            spawn_cross_flourish(commands, cache, materials, mid);
        }
        // The grid wipe plus a gold burst at the supernova — a 3-wide cross band.
        ComboKind::LineSupernova => {
            spawn_power_effect(commands, cache, materials, LightKind::RayH, a, None);
            if b.y != a.y {
                spawn_power_effect(commands, cache, materials, LightKind::RayH, b, None);
            }
            spawn_power_effect(commands, cache, materials, LightKind::RayV, a, None);
            if b.x != a.x {
                spawn_power_effect(commands, cache, materials, LightKind::RayV, b, None);
            }
            spawn_combo_ring(
                commands,
                cache,
                materials,
                to_world(a),
                Color::srgb(4.0, 2.6, 0.8),
                2.6,
                0.52,
            );
        }
        // Cada supernova detona por separado (ring pequeño) y se fusionan en un gran shockwave central.
        ComboKind::DoubleSupernova => {
            spawn_combo_ring(
                commands,
                cache,
                materials,
                to_world(a),
                Color::srgb(4.0, 2.6, 0.8),
                2.6,
                0.4,
            );
            spawn_combo_ring(
                commands,
                cache,
                materials,
                to_world(b),
                Color::srgb(4.0, 2.6, 0.8),
                2.6,
                0.4,
            );
            spawn_combo_ring(
                commands,
                cache,
                materials,
                to_world(mid),
                Color::srgb(4.0, 2.8, 1.0),
                5.4,
                0.63,
            );
            spawn_cross_flourish(commands, cache, materials, mid);
        }
        // Star core flash tinted to the cleared color — every affected light gets its own
        // priming flash + synchronized detonation via `PowerTransformPulse`/`PowerBlastTrail`
        // (see `gameplay::vfx::trigger_star_line`), so no static flash at `b` here.
        ComboKind::StarLine => {
            spawn_power_effect(commands, cache, materials, LightKind::Starburst, a, color);
        }
        // Star core flash tinted to the cleared color + a gold burst at the partner.
        ComboKind::StarSupernova => {
            spawn_power_effect(commands, cache, materials, LightKind::Starburst, a, color);
            spawn_combo_ring(
                commands,
                cache,
                materials,
                to_world(b),
                Color::srgb(4.0, 2.8, 1.0),
                2.6,
                0.52,
            );
        }
        // A single tinted star flash — clears one whole color.
        ComboKind::StarColor => {
            spawn_power_effect(commands, cache, materials, LightKind::Starburst, a, color);
        }
        // Both colors gone at once → a bright board-wide white shockwave.
        ComboKind::StarStar => {
            spawn_combo_ring(
                commands,
                cache,
                materials,
                to_world(mid),
                Color::srgb(3.4, 3.6, 4.2),
                12.0,
                0.75,
            );
            spawn_cross_flourish(commands, cache, materials, mid);
        }
        // The tier-6 collapse — its own void+rim choreography.
        ComboKind::Blackhole => {
            spawn_blackhole_effect(commands, cache, materials, to_world(mid));
        }
        // 3+ powers at once → a triumphant gold board-wide burst, distinct from the dark Blackhole.
        ComboKind::SuperCombo => {
            spawn_combo_ring(
                commands,
                cache,
                materials,
                to_world(a),
                Color::srgb(4.2, 3.4, 1.2),
                12.0,
                0.82,
            );
            spawn_cross_flourish(commands, cache, materials, a);
        }
    }
}

/// Supernova's own effect: a radial glow disc (glow_image) that starts as a bright golden point
/// and expands into a diffuse sphere — same style as the other lights (bright center, soft
/// falloff), driven by EffectAnim's scale lerp + alpha fade.
fn spawn_supernova_glow(commands: &mut Commands, cache: &VisualCache, world: Vec3) {
    let color = Color::linear_rgb(5.0, 3.2, 0.8); // dorado HDR — Bloom lo convierte en glow
    let start_scale = Vec3::splat(0.3);
    commands.spawn((
        EffectAnim {
            timer: Timer::from_seconds(0.52, TimerMode::Once),
            start_scale,
            end_scale: Vec3::splat(3.6),
            base_alpha: 1.0,
            fade_start_frac: 0.3,
            delay: None,
        },
        Sprite {
            image: cache.glow_image.clone(),
            color,
            custom_size: Some(Vec2::splat(TILE * 2.0)),
            ..default()
        },
        Transform::from_translation(world.with_z(0.8)).with_scale(start_scale),
    ));
}

/// A single scaled supernova-ring shockwave for the combos that want one big burst (rather than
/// the fixed-size `spawn_power_effect(Supernova, …)`). Reuses the cached annulus mesh; HDR color so
/// Bloom turns it into glowing light.
fn spawn_combo_ring(
    commands: &mut Commands,
    cache: &VisualCache,
    materials: &mut Assets<ColorMaterial>,
    world: Vec3,
    color: Color,
    end_scale: f32,
    duration: f32,
) {
    let Some(mesh) = cache.effect_mesh(LightKind::Supernova) else {
        return;
    };
    let start_scale = Vec3::splat(0.3);
    commands.spawn((
        EffectAnim {
            timer: Timer::from_seconds(duration, TimerMode::Once),
            start_scale,
            end_scale: Vec3::splat(end_scale),
            base_alpha: color.alpha(),
            fade_start_frac: 0.4,
            delay: None,
        },
        Mesh2d(mesh),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
        Transform::from_translation(world.with_z(0.84)).with_scale(start_scale),
    ));
}

/// A brief bright spark at a Cross's crossing point — reuses Starburst's cached circle mesh (no
/// new asset) tinted warm white-gold, so the activation reads as one unified tier-4 power instead
/// of two Rays that happen to overlap.
fn spawn_cross_flourish(
    commands: &mut Commands,
    cache: &VisualCache,
    materials: &mut Assets<ColorMaterial>,
    pos: GridPos,
) {
    let Some(mesh) = cache.effect_mesh(LightKind::Starburst) else {
        return;
    };
    let mat_color = Color::srgb(3.6, 3.2, 1.8);
    let start_scale = Vec3::splat(0.2);
    commands.spawn((
        EffectAnim {
            timer: Timer::from_seconds(0.24, TimerMode::Once),
            start_scale,
            end_scale: Vec3::splat(1.2),
            base_alpha: mat_color.alpha(),
            fade_start_frac: 0.3,
            delay: None,
        },
        Mesh2d(mesh),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(mat_color))),
        Transform::from_translation(to_world(pos).with_z(0.86)).with_scale(start_scale),
    ));
}

/// Blackhole's own detonation: a dark void disc swallowing the board, with a brighter
/// event-horizon rim riding its edge — color-matched to the background ripple
/// (`visuals::space_background`) and the travelling collapse beam (`visuals::light_trail`).
fn spawn_blackhole_effect(
    commands: &mut Commands,
    cache: &VisualCache,
    materials: &mut Assets<ColorMaterial>,
    world: Vec3,
) {
    const DURATION: f32 = 1.5; // matches the background pulse's DUR in space_background.rs
    const START_SCALE: Vec3 = Vec3::splat(0.15);
    const END_SCALE: Vec3 = Vec3::splat(11.0);

    let void_color = Color::srgba(0.0, 0.01, 0.03, 0.94);
    commands.spawn((
        EffectAnim {
            timer: Timer::from_seconds(DURATION, TimerMode::Once),
            start_scale: START_SCALE,
            end_scale: END_SCALE,
            base_alpha: void_color.alpha(),
            fade_start_frac: 0.5,
            delay: None,
        },
        Mesh2d(cache.blackhole_void_mesh.clone()),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(void_color))),
        Transform::from_translation(world.with_z(0.88)).with_scale(START_SCALE),
    ));

    let rim_color = Color::srgb(1.4, 2.2, 4.2); // HDR blue-white — Bloom turns this into a bright ring
    commands.spawn((
        EffectAnim {
            timer: Timer::from_seconds(DURATION, TimerMode::Once),
            start_scale: START_SCALE,
            end_scale: END_SCALE,
            base_alpha: rim_color.alpha(),
            fade_start_frac: 0.45,
            delay: None,
        },
        Mesh2d(cache.blackhole_rim_mesh.clone()),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(rim_color))),
        Transform::from_translation(world.with_z(0.90)).with_scale(START_SCALE),
    ));
}

pub(crate) fn tick_effect_anim(
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut Transform,
        Option<&MeshMaterial2d<ColorMaterial>>,
        &mut EffectAnim,
        Option<&mut Sprite>,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    time: Res<Time>,
) {
    for (e, mut t, mat_handle, mut anim, sprite) in &mut q {
        let just_activated = if let Some(d) = &mut anim.delay {
            d.tick(time.delta());
            if !d.is_finished() {
                continue;
            }
            anim.delay = None;
            true
        } else {
            false
        };
        anim.timer.tick(time.delta());
        let frac = anim.timer.fraction();
        t.scale = anim.start_scale.lerp(anim.end_scale, frac);
        if just_activated || frac >= anim.fade_start_frac {
            let alpha = if frac >= anim.fade_start_frac {
                let fade = (frac - anim.fade_start_frac) / (1.0 - anim.fade_start_frac).max(0.0001);
                anim.base_alpha * (1.0 - fade).max(0.0)
            } else {
                anim.base_alpha
            };
            if let Some(handle) = mat_handle
                && let Some(mut mat) = materials.get_mut(&handle.0)
            {
                mat.color.set_alpha(alpha);
            }
            if let Some(mut spr) = sprite {
                spr.color = spr.color.with_alpha(alpha);
            }
        }
        if frac >= 1.0 {
            commands.entity(e).try_despawn();
        }
    }
}

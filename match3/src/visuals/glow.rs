use bevy::prelude::*;

use super::assets::VisualCache;
use super::breathing::{BreathPhase, breath_factor, breath_norm};
use crate::core::prelude::*;

/// Live-tunable parameters for a light's halo, edited from the Options screen. The halo is built from
/// STACKED layers, not one pool, so it reads as a real emitter even WITHOUT camera bloom (which on a
/// bandwidth-starved GPU costs ~70% of our framerate — measured). On the near-black space board,
/// alpha-blended bright halos behave almost like additive light (`dst≈0` ⇒ `src·a + dst·(1−a) ≈
/// src·a`), and stacking a wide-faint pool under a tight-bright one fakes the gaussian spread that
/// made bloom feel luminous. All layers share `glow_image`, so they still batch into ONE draw call.
///
/// `flicker` reads this resource every frame, so dragging the Options sliders retunes the live board.
#[derive(Resource, Clone, Copy)]
pub(crate) struct GlowSettings {
    /// Color brightness multiplier shared by both layers (how hot the light burns).
    pub(crate) brightness: f32,
    /// Wide soft bleed onto the darkness — the "bloom" spread. Radius as a multiple of `TILE`.
    pub(crate) outer_radius: f32,
    pub(crate) outer_alpha: f32,
    /// Tight bright pool hugging the core — the emitter itself.
    pub(crate) inner_radius: f32,
    pub(crate) inner_alpha: f32,
}

impl Default for GlowSettings {
    fn default() -> Self {
        Self {
            brightness: 2.3,
            outer_radius: 0.7,
            outer_alpha: 0.1,
            inner_radius: 0.3,
            inner_alpha: 0.75,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GlowLayer {
    Outer,
    Inner,
}

impl GlowSettings {
    /// (radius as a multiple of `TILE`, alpha) for the given layer.
    fn layer(&self, layer: GlowLayer) -> (f32, f32) {
        match layer {
            GlowLayer::Outer => (self.outer_radius, self.outer_alpha),
            GlowLayer::Inner => (self.inner_radius, self.inner_alpha),
        }
    }
}

/// Local z: behind the membrane ring and core (parent z and +0.5), in front of the space background
/// (z = -10), so the pool bleeds onto the dark space and grid. The inner layer sits slightly in
/// front of the outer one so the bright pool composites over the soft bleed.
const GLOW_Z: f32 = -2.0;

/// One stacked layer of a `Light`'s halo. Child of the light entity, so it moves, shrinks (during
/// the pop) and despawns together with its source. Stores the light's RAW color and which layer it
/// is, so `flicker` can recompute the tinted color/size live from `GlowSettings` every frame.
#[derive(Component)]
pub(crate) struct GlowPool {
    layer: GlowLayer,
    /// The light's untinted color at brightness 1.0 — `flicker` scales it by `GlowSettings`.
    raw: Srgba,
    phase: f32,
}

/// Gives every freshly-spawned light its stacked halo (outer soft bleed + inner bright pool). Runs on
/// `Added<Light>` so it covers the initial board, refills and shuffles alike. Each layer is a tinted
/// SPRITE sharing `glow_image`, so all halos of all lights batch into one draw call.
pub(crate) fn attach_glow_pools(
    mut commands: Commands,
    cache: Res<VisualCache>,
    settings: Res<GlowSettings>,
    new_lights: Query<(Entity, &LightColor, &LightKind, &BreathPhase), Added<Light>>,
) {
    for (e, color, kind, breath) in &new_lights {
        let Srgba {
            red, green, blue, ..
        } = kind.visual_base_color(*color).to_srgba();
        let raw = Srgba {
            red,
            green,
            blue,
            alpha: 1.0,
        };
        for (i, layer) in [GlowLayer::Outer, GlowLayer::Inner].into_iter().enumerate() {
            let (radius, alpha) = settings.layer(layer);
            let pool = commands
                .spawn((
                    // Share the light's breath phase so every layer pulses in lockstep with its core.
                    GlowPool {
                        layer,
                        raw,
                        phase: breath.0,
                    },
                    Sprite {
                        image: cache.glow_image.clone(),
                        color: tint(raw, settings.brightness, alpha),
                        custom_size: Some(Vec2::splat(TILE * radius * 2.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, GLOW_Z + i as f32 * 0.1),
                ))
                .id();
            commands.entity(e).add_child(pool);
        }
    }
}

/// Tints a raw color by a brightness multiplier, preserving its hue by preventing channel clipping.
fn tint(raw: Srgba, brightness: f32, alpha: f32) -> Color {
    let r = raw.red * brightness;
    let g = raw.green * brightness;
    let b = raw.blue * brightness;
    let max_val = r.max(g).max(b);
    if max_val > 1.0 {
        Color::srgb(r / max_val, g / max_val, b / max_val).with_alpha((alpha * max_val).min(1.0))
    } else {
        Color::srgb(r, g, b).with_alpha(alpha)
    }
}

/// Every halo breathes in lockstep with its `LightCore` — same slow waveform and phase. Re-reads
/// `GlowSettings` each frame so the Options sliders retune the live board: brightness uses the breath
/// factor; radius/alpha come straight from the settings; size pulses subtly with the same crest.
/// Nearby `ScoreShard`s add a proximity-based brightness boost (dynamic lighting) while they travel.
pub(crate) fn flicker(
    time: Res<Time>,
    settings: Res<GlowSettings>,
    mut q: Query<(&mut Sprite, &GlowPool, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    for (mut sprite, pool, mut tf) in &mut q {
        let (radius, alpha) = settings.layer(pool.layer);
        let f = breath_factor(t, pool.phase);
        sprite.color = tint(pool.raw, settings.brightness * f, alpha);
        sprite.custom_size = Some(Vec2::splat(TILE * radius * 2.0));
        tf.scale = Vec3::splat(0.9 + 0.15 * breath_norm(t, pool.phase));
    }
}

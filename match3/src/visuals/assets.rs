use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::FRAC_PI_2;

use super::effects::build_effect_mesh;
use super::particles::ParticleSettings;
use crate::core::prelude::*;

/// Cache of every *constant* mesh/material the game reuses, built once in `PreStartup`. Before this,
/// `spawn_light` / `spawn_burst` / `rebuild_cores` / … each called `meshes.add(...)` /
/// `materials.add(...)` per spawn, recreating byte-identical assets thousands of times during play
/// and flooding the GPU with redundant uploads. Sharing one handle per distinct asset removes that
/// churn (and lets same-mesh+same-material entities batch into one draw call).
///
/// Only assets that are **never** mutated per-entity live here. The per-instance materials that
/// `breathe` / `flicker` / particle-fade mutate every frame are still created at spawn time — moving
/// those off per-entity materials is a separate, less-obvious optimization (see the perf discussion).
#[derive(Resource)]
pub(crate) struct VisualCache {
    /// Membrane ring mesh + material per `LightColor`, indexed by `LightColor::index()`. The ring
    /// material is constant per color (selection scales the `Transform`, it never recolors).
    ring_mesh: [Handle<Mesh>; 5],
    ring_mat: [Handle<ColorMaterial>; 5],
    pub(crate) spark_mesh: Handle<Mesh>,
    pub(crate) spark_mat: Handle<ColorMaterial>,
    pub(crate) shadow_mesh: Handle<Mesh>,
    pub(crate) shadow_mat: Handle<ColorMaterial>,
    pub(crate) burst_mesh: Handle<Mesh>,
    pub(crate) membrane_mesh: Handle<Mesh>,
    /// Shared textures for the per-instance-tinted SPRITES (cores + glow pools). Sprites batch by
    /// texture regardless of tint, so every core/pool collapses into ~1 draw call each — the big
    /// win over the old per-entity `ColorMaterial` meshes. `core_image` is a soft solid disc (sized
    /// per kind via `Sprite::custom_size`); `glow_image` is a radial falloff for the halo.
    pub(crate) core_image: Handle<Image>,
    pub(crate) glow_image: Handle<Image>,
    /// Horizontal beam texture: alpha = (1-dy²)² along the full length, gentle tip fade at ends.
    /// Used by LaserBolt — scale to (length, glow_width); rotate 90° for vertical bolts.
    pub(crate) beam_image: Handle<Image>,
    /// Kind-shaped membrane meshes for the three top powers, so their *body* (not just their cores)
    /// distinguishes them at a glance: Cross = a 4-bladed shuriken, Starburst = a 5-pointed star,
    /// Blackhole = a clean circle. Color is still carried by the per-color `ring_mat` material, so a
    /// red Cross is a red shuriken, etc. Everything else keeps its per-color shape (`ring_mesh`).
    cross_mesh: Handle<Mesh>,
    starburst_mesh: Handle<Mesh>,
    blackhole_mesh: Handle<Mesh>,
    /// Blast meshes for RayH / RayV / Supernova / Starburst (Normal has none).
    effect: [Handle<Mesh>; 4],
    /// Blackhole's own detonation meshes — a dark void disc and a brighter rim riding its edge,
    /// both scaled up together by `EffectAnim` (see `visuals::effects::spawn_power_effect`).
    pub(crate) blackhole_void_mesh: Handle<Mesh>,
    pub(crate) blackhole_rim_mesh: Handle<Mesh>,
}

impl VisualCache {
    pub(crate) fn ring_mesh(&self, c: LightColor) -> Handle<Mesh> {
        self.ring_mesh[c.index()].clone()
    }
    pub(crate) fn ring_mat(&self, c: LightColor) -> Handle<ColorMaterial> {
        self.ring_mat[c.index()].clone()
    }
    /// The membrane mesh for a light: the kind-shape for the three top powers (so they read apart by
    /// silhouette), else the per-color shape. Pairs with `ring_mat(color)` for the tint.
    pub(crate) fn light_mesh(&self, kind: LightKind, color: LightColor) -> Handle<Mesh> {
        match kind {
            LightKind::Cross => self.cross_mesh.clone(),
            LightKind::Starburst => self.starburst_mesh.clone(),
            LightKind::Blackhole => self.blackhole_mesh.clone(),
            _ => self.ring_mesh(color),
        }
    }
    /// The blast mesh for a power kind, or `None` for `Normal`.
    pub(crate) fn effect_mesh(&self, kind: LightKind) -> Option<Handle<Mesh>> {
        Some(match kind {
            LightKind::RayH => self.effect[0].clone(),
            LightKind::RayV => self.effect[1].clone(),
            LightKind::Supernova => self.effect[2].clone(),
            LightKind::Starburst => self.effect[3].clone(),
            // Cross is drawn as a RayH+RayV pair, and Blackhole has its own dedicated
            // void/rim meshes (`blackhole_void_mesh`/`blackhole_rim_mesh`) — neither uses this
            // single-mesh slot.
            LightKind::Cross | LightKind::Normal | LightKind::Blackhole => return None,
        })
    }
}

/// Builds the cache in `PreStartup` so it's ready before any `Startup` system spawns the board.
/// Builds a `size`×`size` RGBA white texture whose alpha is `falloff(d)` for normalized radius
/// `d` (0 at center, 1 at the rim) — a tintable radial sprite. Used for the core disc and the glow
/// halo. White RGB so the per-sprite (HDR) `color` tint fully controls the hue and blooms.
fn radial_image(
    images: &mut Assets<Image>,
    size: u32,
    falloff: impl Fn(f32) -> f32,
) -> Handle<Image> {
    let r = size as f32 / 2.0;
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - r;
            let dy = y as f32 + 0.5 - r;
            let d = ((dx * dx + dy * dy).sqrt() / r).min(1.0);
            let a = (falloff(d).clamp(0.0, 1.0) * 255.0) as u8;
            let i = ((y * size + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            data[i + 3] = a;
        }
    }
    images.add(Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

/// Beam texture for LaserBolt: capsule shape with rounded ends, and a warm-white core that fades
/// to cool blue at the edges. The RGB channels encode the color gradient; alpha encodes the soft
/// perpendicular falloff. Sprite::color acts as the HDR brightness multiplier.
///
/// Shape: rectangular body for the inner 60% of the half-length, then semicircular ends.
/// Color: center=(255,255,245) warm white → edge=(60,115,255) blue.
fn make_beam_image(images: &mut Assets<Image>, size: u32) -> Handle<Image> {
    let r = size as f32 / 2.0;
    let mut data = vec![0u8; (size * size * 4) as usize];
    // Beyond this fraction of half-length the tip becomes a semicircle.
    const CORNER: f32 = 0.60;
    // Center color (warm white) and edge color (blue) in sRGB 0-255.
    const CC: (f32, f32, f32) = (255.0, 255.0, 245.0);
    const EC: (f32, f32, f32) = (60.0, 115.0, 255.0);

    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5 - r) / r; // -1..1 along beam axis
            let dy = (y as f32 + 0.5 - r) / r; // -1..1 perpendicular

            // Capsule boundary: full height in the body, rounded semicircle at each tip.
            let abs_dx = dx.abs();
            let dy_limit = if abs_dx <= CORNER {
                1.0_f32
            } else {
                let t = (abs_dx - CORNER) / (1.0 - CORNER);
                (1.0 - t * t).max(0.0).sqrt()
            };

            let i = ((y * size + x) * 4) as usize;
            if dy_limit < 0.001 {
                // Outside the capsule → fully transparent.
                data[i + 3] = 0;
                continue;
            }

            // dy normalized to capsule boundary (-1..1 within the rounded shape).
            let dy_norm = (dy / dy_limit).clamp(-1.0, 1.0);
            // Soft perpendicular falloff: (1 - dy_norm²)²
            let falloff = (1.0 - dy_norm * dy_norm).powi(2);

            // Color gradient: t=0 at center axis, t=1 at the capsule edge.
            let t = dy_norm.abs();
            let cr = CC.0 + (EC.0 - CC.0) * t;
            let cg = CC.1 + (EC.1 - CC.1) * t;
            let cb = CC.2 + (EC.2 - CC.2) * t;

            data[i] = cr as u8;
            data[i + 1] = cg as u8;
            data[i + 2] = cb as u8;
            data[i + 3] = (falloff * 255.0) as u8;
        }
    }
    images.add(Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

/// Smoothstep, for the disc's anti-aliased rim.
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn build_cache(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    particles: Res<ParticleSettings>,
) {
    let ring_mesh = std::array::from_fn(|i| LightColor::from_index(i).mesh(&mut meshes));
    let ring_mat = std::array::from_fn(|i| {
        materials.add(ColorMaterial::from_color(
            LightColor::from_index(i).ring_color(),
        ))
    });
    let effect = [
        meshes.add(build_effect_mesh(LightKind::RayH)),
        meshes.add(build_effect_mesh(LightKind::RayV)),
        meshes.add(build_effect_mesh(LightKind::Supernova)),
        meshes.add(build_effect_mesh(LightKind::Starburst)),
    ];
    // Kind-shaped membranes (color comes from the material, so radii match the per-color rings'
    // reach). The stars use a fairly bold wall so the spikes read clearly at tile size.
    let cross_mesh = meshes.add(star_ring_mesh(4, TILE * 0.46, TILE * 0.16, FRAC_PI_2, 0.72));
    let starburst_mesh = meshes.add(star_ring_mesh(5, TILE * 0.44, TILE * 0.18, FRAC_PI_2, 0.72));
    let blackhole_mesh = meshes.add(circle_ring_mesh(TILE * 0.40));
    commands.insert_resource(VisualCache {
        ring_mesh,
        ring_mat,
        cross_mesh,
        starburst_mesh,
        blackhole_mesh,
        // Spark: orange hexagon. Shadow: translucent blue tile. Both constant.
        spark_mesh: meshes.add(RegularPolygon::new(TILE * 0.35, 6)),
        spark_mat: materials.add(ColorMaterial::from_color(Color::srgb(1.0, 0.6, 0.1))),
        shadow_mesh: meshes.add(Rectangle::new(TILE * 0.95, TILE * 0.95)),
        shadow_mat: materials.add(ColorMaterial::from_color(Color::srgba(0.2, 0.5, 0.9, 0.45))),
        burst_mesh: meshes.add(Circle::new(particles.burst_radius)),
        membrane_mesh: meshes.add(Circle::new(particles.membrane_radius)),
        // Solid disc with a soft anti-aliased rim (last 12% fades out).
        core_image: radial_image(&mut images, 256, |d| 1.0 - smoothstep(0.88, 1.0, d)),
        // Radial halo: bright center easing to nothing at the rim (squared = center-weighted).
        glow_image: radial_image(&mut images, 256, |d| (1.0 - d) * (1.0 - d)),
        // Horizontal beam: uniform brightness along length, soft perpendicular falloff.
        beam_image: make_beam_image(&mut images, 256),
        effect,
        blackhole_void_mesh: meshes.add(Circle::new(TILE * 0.5)),
        blackhole_rim_mesh: meshes.add(Annulus::new(TILE * 0.5, TILE * 0.62)),
    });
}

use bevy::asset::RenderAssetUsages;
use bevy::color::Srgba;
use bevy::image::Image;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

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
    hollow_mesh: Handle<Mesh>,
    pub(crate) hollow_mat: Handle<ColorMaterial>,
    pub(crate) spark_mesh: Handle<Mesh>,
    pub(crate) spark_mat: Handle<ColorMaterial>,
    pub(crate) shadow_mesh: Handle<Mesh>,
    pub(crate) shadow_mat: Handle<ColorMaterial>,
    /// "Jalea ultra dura" tint — a deeper violet than the plain `shadow_mat` blue, so hard shadows
    /// read apart from normal ones at a glance.
    pub(crate) hard_shadow_mat: Handle<ColorMaterial>,
    pub(crate) blocker_mesh: Handle<Mesh>,
    pub(crate) blocker_mat: Handle<ColorMaterial>,
    pub(crate) burst_mesh: Handle<Mesh>,
    pub(crate) membrane_mesh: Handle<Mesh>,
    /// Shared textures for the per-instance-tinted SPRITES (cores + glow pools). Sprites batch by
    /// texture regardless of tint, so every core/pool collapses into ~1 draw call each — the big
    /// win over the old per-entity `ColorMaterial` meshes. `core_image` is a soft solid disc (sized
    /// per kind via `Sprite::custom_size`); `glow_image` is a radial falloff for the halo.
    pub(crate) core_image: Handle<Image>,
    pub(crate) glow_image: Handle<Image>,
    /// Per-`LightColor` hot-core disc for score shards (see `radial_hot_core_image`) — white at the
    /// center, fading to the light's own hue toward the rim, so a captured light reads as a real
    /// emitter instead of a flat tinted dot.
    shard_core_image: [Handle<Image>; 5],
    /// Per-`LightColor` shaped core disc for a light's own `LightCore` nucleus dots (see
    /// `shaped_core_image`) — circle/triangle/square/diamond/pentagon matching that color's ring,
    /// instead of a plain circular dot.
    light_core_image: [Handle<Image>; 5],
    /// Plain 1×1 quad, UVs 0..1: the `Mesh2d` counterpart of a `Sprite`'s implicit quad, for
    /// entities that need a custom `Material2d` (e.g. `AdditiveMaterial`) instead of `Sprite`'s
    /// hardcoded alpha blend. Scale the `Transform` to size it, same as `Sprite::custom_size` did.
    pub(crate) unit_quad_mesh: Handle<Mesh>,
    /// Horizontal beam texture: alpha = (1-dy²)² along the full length, gentle tip fade at ends.
    /// Used by LaserBolt — scale to (length, glow_width); rotate 90° for vertical bolts.
    pub(crate) beam_image: Handle<Image>,
    /// Flat opaque square, tintable to any color/alpha — used by the HUD goal icon for
    /// square-shaped goals (e.g. `LevelGoal::ClearShadow`) so it doesn't depend on font glyph
    /// coverage the way a text icon would.
    pub(crate) square_image: Handle<Image>,
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
    pub(crate) fn shard_core_image(&self, c: LightColor) -> Handle<Image> {
        self.shard_core_image[c.index()].clone()
    }
    pub(crate) fn light_core_image(&self, c: LightColor) -> Handle<Image> {
        self.light_core_image[c.index()].clone()
    }
    pub(crate) fn light_mat(&self, kind: LightKind, color: LightColor) -> Handle<ColorMaterial> {
        if kind.is_hollow() {
            self.hollow_mat.clone()
        } else {
            self.ring_mat(color)
        }
    }
    /// The membrane mesh for a light: the kind-shape for the three top powers (so they read apart by
    /// silhouette), else the per-color shape. Pairs with `ring_mat(color)` for the tint.
    pub(crate) fn light_mesh(&self, kind: LightKind, color: LightColor) -> Handle<Mesh> {
        match kind {
            LightKind::Hollow => self.hollow_mesh.clone(),
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
            LightKind::Cross | LightKind::Normal | LightKind::Hollow | LightKind::Blackhole => {
                return None;
            }
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

/// Radial "hot core" disc, one per `LightColor`: pure white at the very center, blending out to
/// the light's own color by `white_frac` of the radius, then solid color out to the same
/// anti-aliased rim as `core_image`. A plain color-tinted white disc (the old `core_image` +
/// per-instance `Sprite::color` approach) reads as a flat blob of paint; real bright light sources
/// clip to white at the emitter and only show their true hue where the intensity has fallen off, so
/// baking that gradient into the texture is what makes a score shard read as an actual light instead
/// of a colored dot. RGB is baked per-color at cache-build time (`LightColor` is a fixed palette);
/// `Sprite::color` still applies a uniform (hue-preserving) HDR brightness multiplier at spawn time.
fn radial_hot_core_image(
    images: &mut Assets<Image>,
    size: u32,
    color: Color,
    white_frac: f32,
) -> Handle<Image> {
    let Srgba {
        red, green, blue, ..
    } = color.to_srgba();
    let r = size as f32 / 2.0;
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - r;
            let dy = y as f32 + 0.5 - r;
            let d = ((dx * dx + dy * dy).sqrt() / r).min(1.0);
            let t = smoothstep(0.0, white_frac, d);
            let cr = 1.0 + (red - 1.0) * t;
            let cg = 1.0 + (green - 1.0) * t;
            let cb = 1.0 + (blue - 1.0) * t;
            let a = 1.0 - smoothstep(0.80, 1.0, d);
            let i = ((y * size + x) * 4) as usize;
            data[i] = (cr.clamp(0.0, 1.0) * 255.0) as u8;
            data[i + 1] = (cg.clamp(0.0, 1.0) * 255.0) as u8;
            data[i + 2] = (cb.clamp(0.0, 1.0) * 255.0) as u8;
            data[i + 3] = (a.clamp(0.0, 1.0) * 255.0) as u8;
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

/// Distance-ratio from center to a regular N-gon's boundary at `angle`, normalized so a vertex
/// sits exactly at `1.0` (matching the circumradius) and the middle of an edge dips to
/// `cos(π/sides)` (the apothem/circumradius ratio) — the standard shape of a regular polygon's
/// radius as a function of angle. Dividing a plain circle-normalized radius by this factor "cuts
/// in" the flat edges between vertices, turning a circular falloff into a polygon one with the
/// same vertex count/orientation as [`LightColor::mesh`]'s ring — see `shaped_core_image`.
fn polygon_shape_factor(angle: f32, sides: u32, start_angle: f32) -> f32 {
    let step = TAU / sides as f32;
    let rel = (angle - start_angle).rem_euclid(step) - step * 0.5;
    (step * 0.5).cos() / rel.cos()
}

/// Per-`LightColor` SHAPE alpha mask — white RGB, like `radial_image`, but polygon-shaped instead
/// of circular (matching `sides`/`start_angle` from [`LightColor::ring_sides`]/
/// [`LightColor::shape_start_angle`]) — so a light's own glowing `LightCore` nucleus dots read as
/// "this light's shape" (circle/triangle/square/diamond/pentagon), matching the game's app icon,
/// which uses the same per-color shape language. White RGB deliberately: hue/brightness stays
/// driven entirely by `Sprite::color` (see `visuals::breathing::breathe`, which overwrites it every
/// frame from `Breathing::base`), exactly like the old plain-circle `core_image` did — baking a hue
/// into this texture too would double-tint against `breathe`'s own color.
fn shaped_core_image(
    images: &mut Assets<Image>,
    size: u32,
    sides: u32,
    start_angle: f32,
) -> Handle<Image> {
    let r = size as f32 / 2.0;
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - r;
            let dy = y as f32 + 0.5 - r;
            let raw = (dx * dx + dy * dy).sqrt() / r;
            // Negate dy: texture data is row-major top-down (row 0 = top), but `start_angle`/
            // `ring_polygon_points` use the math convention (+Y = up) the mesh's own vertices are
            // built in — without the flip, every shape rendered upside-down relative to the ring.
            let shape = polygon_shape_factor((-dy).atan2(dx), sides, start_angle);
            // A little headroom above 1.0 so the anti-aliased rim isn't clipped right at a vertex.
            let d = (raw / shape).min(1.2);
            let a = (1.0 - smoothstep(0.80, 1.0, d)).clamp(0.0, 1.0);
            let i = ((y * size + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            data[i + 3] = (a * 255.0) as u8;
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

pub(crate) fn build_cache(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
    particles: Res<ParticleSettings>,
) {
    // Overwrite the default font handle with Roboto-Regular to prevent empty boxes/squares on accented Spanish characters.
    let font_bytes = include_bytes!("../../assets/fonts/Roboto-Regular.ttf");
    let font = Font::from_bytes(font_bytes.to_vec());
    let _ = fonts.insert(AssetId::<Font>::default(), font);

    let ring_mesh = std::array::from_fn(|i| {
        meshes.add(build_light_color_mesh(LightColor::from_index(i)))
    });
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
        hollow_mesh: meshes.add(x_mark_mesh(TILE * 0.36, TILE * 0.14)),
        hollow_mat: materials.add(ColorMaterial::from_color(
            LightKind::Hollow.visual_ring_color(LightColor::Red),
        )),
        cross_mesh,
        starburst_mesh,
        blackhole_mesh,
        // Spark ingredient: amber hex shell; the dark pulsing nucleus is spawned as children.
        spark_mesh: meshes.add(RegularPolygon::new(TILE * 0.40, 6)),
        spark_mat: materials.add(ColorMaterial::from_color(Color::srgb(1.25, 0.48, 0.08))),
        shadow_mesh: meshes.add(Rectangle::new(TILE * 0.95, TILE * 0.95)),
        shadow_mat: materials.add(ColorMaterial::from_color(Color::srgba(0.2, 0.5, 0.9, 0.45))),
        hard_shadow_mat: materials.add(ColorMaterial::from_color(Color::srgba(
            0.62, 0.16, 0.68, 0.62,
        ))),
        blocker_mesh: meshes.add(Rectangle::new(TILE * 0.96, TILE * 0.96)),
        blocker_mat: materials.add(ColorMaterial::from_color(Color::srgba(
            0.015, 0.018, 0.030, 0.92,
        ))),
        burst_mesh: meshes.add(Circle::new(particles.burst_radius)),
        membrane_mesh: meshes.add(Circle::new(particles.membrane_radius)),
        // Solid disc with a soft anti-aliased rim (last 20% fades out, optimized for 32x32 size).
        core_image: radial_image(&mut images, 32, |d| 1.0 - smoothstep(0.80, 1.0, d)),
        // Radial halo: bright center easing to nothing at the rim, optimized for 128x128 size.
        glow_image: radial_image(&mut images, 128, |d| (1.0 - d) * (1.0 - d)),
        shard_core_image: std::array::from_fn(|i| {
            radial_hot_core_image(
                &mut images,
                32,
                LightColor::from_index(i).bevy_color(),
                0.45,
            )
        }),
        light_core_image: std::array::from_fn(|i| {
            let color = LightColor::from_index(i);
            shaped_core_image(
                &mut images,
                32,
                color.ring_sides(),
                color.shape_start_angle(),
            )
        }),
        unit_quad_mesh: meshes.add(Rectangle::new(1.0, 1.0)),
        // Horizontal beam: uniform brightness along length, soft perpendicular falloff.
        beam_image: make_beam_image(&mut images, 256),
        square_image: images.add(Image::new(
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![255u8; 8 * 8 * 4],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        )),
        effect,
        blackhole_void_mesh: meshes.add(Circle::new(TILE * 0.5)),
        blackhole_rim_mesh: meshes.add(Annulus::new(TILE * 0.5, TILE * 0.62)),
    });
}

// ─── Mesh Generation Helper Functions ─────────────────────────────────────────

const RING_THICKNESS_PX: f32 = 3.5;

fn ring_inset_for_sides(n: u32, r: f32) -> f32 {
    let apothem_factor = (PI / n as f32).cos();
    1.0 - RING_THICKNESS_PX / (r * apothem_factor)
}

fn ring_polygon_points(n: u32, r: f32, start_angle: f32) -> Vec<Vec2> {
    let step = TAU / n as f32;
    (0..n)
        .map(|i| {
            let theta = start_angle + i as f32 * step;
            Vec2::new(r * theta.cos(), r * theta.sin())
        })
        .collect()
}

fn transformed_polygon_points(
    n: u32,
    r: f32,
    start_angle: f32,
    scale: Vec2,
    offset: Vec2,
) -> Vec<Vec2> {
    ring_polygon_points(n, r, start_angle)
        .into_iter()
        .map(|p| p * scale + offset)
        .collect()
}

fn build_ring_mesh(outer: &[Vec2], inset: f32) -> Mesh {
    let n = outer.len();
    let r_norm = outer
        .iter()
        .map(|p| p.length())
        .fold(0.0f32, f32::max)
        .max(0.0001);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * 2);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(n * 2);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(n * 2);
    for &p in outer {
        positions.push([p.x, p.y, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([0.5 * (p.x / r_norm + 1.0), 1.0 - 0.5 * (p.y / r_norm + 1.0)]);
    }
    for &p in outer {
        let inner = p * inset;
        positions.push([inner.x, inner.y, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([
            0.5 * (inner.x / r_norm + 1.0),
            1.0 - 0.5 * (inner.y / r_norm + 1.0),
        ]);
    }
    let mut indices: Vec<u32> = Vec::with_capacity(n * 6);
    for i in 0..n as u32 {
        let (o0, o1) = (i, (i + 1) % n as u32);
        let (i0, i1) = (n as u32 + i, n as u32 + (i + 1) % n as u32);
        indices.extend_from_slice(&[o0, o1, i1, o0, i1, i0]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_indices(Indices::U32(indices))
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}

fn star_polygon_points(points: u32, outer_r: f32, inner_r: f32, start_angle: f32) -> Vec<Vec2> {
    let n = points * 2;
    let step = TAU / n as f32;
    (0..n)
        .map(|i| {
            let r = if i % 2 == 0 { outer_r } else { inner_r };
            let theta = start_angle + i as f32 * step;
            Vec2::new(r * theta.cos(), r * theta.sin())
        })
        .collect()
}

pub(crate) fn star_ring_mesh(
    points: u32,
    outer_r: f32,
    inner_r: f32,
    start_angle: f32,
    inset: f32,
) -> Mesh {
    build_ring_mesh(
        &star_polygon_points(points, outer_r, inner_r, start_angle),
        inset,
    )
}

pub(crate) fn circle_ring_mesh(r: f32) -> Mesh {
    let inset = ring_inset_for_sides(48, r);
    build_ring_mesh(&ring_polygon_points(48, r, FRAC_PI_2), inset)
}

pub(crate) fn x_mark_mesh(half_len: f32, thickness: f32) -> Mesh {
    let half_w = thickness * 0.5;
    let mut quads = Vec::with_capacity(2);
    for angle in [FRAC_PI_4, -FRAC_PI_4] {
        let axis = Vec2::from_angle(angle);
        let perp = Vec2::new(-axis.y, axis.x);
        quads.push([
            axis * -half_len + perp * -half_w,
            axis * half_len + perp * -half_w,
            axis * half_len + perp * half_w,
            axis * -half_len + perp * half_w,
        ]);
    }

    let mut positions = Vec::with_capacity(8);
    let mut normals = Vec::with_capacity(8);
    let mut uvs = Vec::with_capacity(8);
    let mut indices = Vec::with_capacity(12);
    for quad in quads {
        let base = positions.len() as u32;
        for p in quad {
            positions.push([p.x, p.y, 0.0]);
            normals.push([0.0, 0.0, 1.0]);
            uvs.push([
                0.5 * (p.x / half_len + 1.0),
                1.0 - 0.5 * (p.y / half_len + 1.0),
            ]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_indices(Indices::U32(indices))
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}

fn build_light_color_mesh(color: LightColor) -> Mesh {
    let r = TILE * 0.40;
    let angle = color.shape_start_angle();
    let outer = match color {
        LightColor::Red => ring_polygon_points(32, r, angle),
        LightColor::Green => transformed_polygon_points(
            3,
            r,
            angle,
            Vec2::new(1.08, 1.06),
            Vec2::new(0.0, -TILE * 0.02),
        ),
        LightColor::Blue => ring_polygon_points(4, r, angle),
        LightColor::Yellow => ring_polygon_points(4, r, angle),
        LightColor::Purple => ring_polygon_points(5, r, angle),
    };
    let inset = ring_inset_for_sides(color.ring_sides(), r);
    build_ring_mesh(&outer, inset)
}

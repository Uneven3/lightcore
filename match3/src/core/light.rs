use bevy::asset::RenderAssetUsages;
use bevy::color::Srgba;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use super::grid::TILE;

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub(crate) enum LightColor {
    Red,
    Green,
    Blue,
    Yellow,
    Purple,
}

impl LightColor {
    pub(crate) fn random(rng: &mut impl Rng) -> Self {
        match rng.random_range(0..5u8) {
            0 => Self::Red,
            1 => Self::Green,
            2 => Self::Blue,
            3 => Self::Yellow,
            _ => Self::Purple,
        }
    }

    pub(crate) fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Red,
            1 => Self::Green,
            2 => Self::Blue,
            3 => Self::Yellow,
            _ => Self::Purple,
        }
    }

    /// Inverse of [`from_index`](Self::from_index) — used to index the per-color ring mesh/material
    /// arrays in `visuals::assets::VisualCache`.
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Green => 1,
            Self::Blue => 2,
            Self::Yellow => 3,
            Self::Purple => 4,
        }
    }

    pub(crate) fn bevy_color(self) -> Color {
        match self {
            Self::Red => Color::srgb(0.92, 0.25, 0.30),
            Self::Green => Color::srgb(0.30, 0.80, 0.40),
            Self::Blue => Color::srgb(0.25, 0.50, 0.95),
            Self::Yellow => Color::srgb(0.95, 0.85, 0.25),
            Self::Purple => Color::srgb(0.65, 0.35, 0.85),
        }
    }

    /// Builds the light's membrane as a real ring/annulus mesh — a thin, neon-like contour
    /// with a genuine geometric hole in the middle (not simulated via alpha), so the light
    /// core reads as a sharp emitter floating in real empty space, not a fog-filled disc.
    pub(crate) fn mesh(self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        let r = TILE * 0.40;
        let outer = match self {
            Self::Red => ring_polygon_points(32, r, FRAC_PI_2),
            Self::Green => transformed_polygon_points(
                3,
                r,
                FRAC_PI_2,
                Vec2::new(1.08, 1.06),
                Vec2::new(0.0, -TILE * 0.02),
            ),
            // A square is just a regular 4-gon rotated 45° from the diamond (`Yellow`) — using
            // the same circumradius `r` (not `r`-as-half-extent, which used to make this shape
            // stick out ~41% farther than the other 4) keeps every shape's reach identical.
            Self::Blue => ring_polygon_points(4, r, FRAC_PI_4),
            Self::Yellow => ring_polygon_points(4, r, FRAC_PI_2),
            Self::Purple => ring_polygon_points(5, r, FRAC_PI_2),
        };
        let inset = ring_inset_for_sides(self.ring_sides(), r);
        meshes.add(build_ring_mesh(&outer, inset))
    }

    fn ring_sides(self) -> u32 {
        match self {
            Self::Red => 32,
            Self::Green => 3,
            Self::Blue | Self::Yellow => 4,
            Self::Purple => 5,
        }
    }

    const GLOW_BOOST: f32 = 4.0;
    const RING_BOOST: f32 = 1.8;

    /// HDR-overbright version of `bevy_color()`, picked up by the camera's Bloom pass to
    /// render a glowing core at the light's center.
    pub(crate) fn glow_color(self) -> Color {
        let Srgba {
            red, green, blue, ..
        } = self.bevy_color().to_srgba();
        Color::srgb(
            red * Self::GLOW_BOOST,
            green * Self::GLOW_BOOST,
            blue * Self::GLOW_BOOST,
        )
    }

    /// Modest HDR boost for the body's neon ring outline — bright enough that Bloom picks it
    /// up softly, but well under `glow_color()`'s boost so the core stays the visually
    /// dominant emitter and the ring reads as an outline, not a second light source.
    pub(crate) fn ring_color(self) -> Color {
        let Srgba {
            red, green, blue, ..
        } = self.bevy_color().to_srgba();
        Color::srgb(
            red * Self::RING_BOOST,
            green * Self::RING_BOOST,
            blue * Self::RING_BOOST,
        )
    }
}

/// Target ring wall thickness in pixels, the same for all 5 shapes. The naive approach (one
/// shared `inset` ratio for every shape) gives wildly different *visible* thickness per shape,
/// because the perpendicular wall thickness of an N-gon ring scaled by `inset` is
/// `r * cos(pi/N) * (1 - inset)`, not `r * (1 - inset)` — `cos(pi/N)` ranges from 0.5
/// (triangle) to ~1.0 (circle), so a triangle's wall ends up roughly half as thick as a
/// circle's for the same `inset`. `ring_inset_for_sides` below solves for the `inset` that
/// gives each shape this same absolute thickness instead.
const RING_THICKNESS_PX: f32 = 3.5;

fn ring_inset_for_sides(n: u32, r: f32) -> f32 {
    let apothem_factor = (PI / n as f32).cos();
    1.0 - RING_THICKNESS_PX / (r * apothem_factor)
}

/// N points around a regular polygon (or a fine-enough approximation of a circle when N is
/// large) starting at `start_angle`, proceeding counter-clockwise — same convention Bevy's own
/// EllipseMeshBuilder/RegularPolygonMeshBuilder use when `start_angle == FRAC_PI_2`.
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

/// Builds a flat ring/annulus mesh: an outer boundary (`outer`) and an inner boundary scaled
/// toward the center by `inset`, connected by a strip of quads — a real geometric hole in the
/// middle, not a simulated one via alpha. Works identically for every convex polygon this
/// project uses, including the axis-aligned `Rectangle` (whose corners/edges sit at different
/// distances from center than the regular polygons' fan-vertices do — irrelevant here, since
/// we scale real vertex positions, not a UV-radius threshold).
fn build_ring_mesh(outer: &[Vec2], inset: f32) -> Mesh {
    let n = outer.len();
    // Normalize UV by the shape's own max extent, not a fixed constant — keeps UV in [0,1]
    // even though it's currently unused (ColorMaterial has no texture to sample here).
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

/// `2*points` vertices alternating between `outer_r` (spike tips) and `inner_r` (valleys) — the
/// outline of a `points`-pointed star. Powers whose membrane shape encodes their *kind* (not their
/// color) use this: Starburst (5-pointed star) and Cross (a 4-bladed shuriken, sharp valleys).
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

/// A star-shaped ring outline (see [`star_polygon_points`]). `inset` controls wall thickness as a
/// fraction of each vertex's radius — kept fairly bold so the spiky silhouette reads at a glance.
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

/// A smooth circular ring outline — the Blackhole's kind-shape membrane (a clean circle, distinct
/// from the spiky Cross/Starburst and from the polygonal color-shapes).
pub(crate) fn circle_ring_mesh(r: f32) -> Mesh {
    let inset = ring_inset_for_sides(48, r);
    build_ring_mesh(&ring_polygon_points(48, r, FRAC_PI_2), inset)
}

/// How powerful a [`Light`](crate::core::components::Light) is, read by the player as its number
/// of **lightcores**. `Normal` lights just match; the rest are "power lights" forged by larger
/// matches (see [`LightKind::from_line`] / [`LightKind::from_intersection`]) and detonate in a
/// different shape each. This is the canonical tier table (corelights → kind → detonation), one
/// source of truth for the whole game:
///
/// | corelights | kind        | detonation                          |
/// |------------|-------------|-------------------------------------|
/// | 1          | Normal      | none (just matches)                 |
/// | 2          | RayH / RayV | sweeps its whole row / column       |
/// | 3          | Supernova   | bursts a 3×3 area                    |
/// | 4          | Cross       | sweeps its whole row **and** column |
/// | 5          | Starburst   | clears every light of one color     |
/// | 6          | Blackhole   | clears the entire board             |
#[derive(Component, Clone, Copy, PartialEq, Default, Debug)]
pub(crate) enum LightKind {
    #[default]
    Normal,
    /// Horizontal ray (2 cores) — sweeps its whole row.
    RayH,
    /// Vertical ray (2 cores) — sweeps its whole column.
    RayV,
    /// Supernova (3 cores) — bursts a 3×3 area around it.
    Supernova,
    /// Cross (4 cores) — two crossed rays: sweeps its whole row *and* column.
    Cross,
    /// Starburst (5 cores) — clears every light of one color at once.
    Starburst,
    /// Blackhole (6 cores) — clears every light on the board, any color. The rarest forge: an
    /// intersection where one arm is already Starburst-worthy (5+) *and* crosses another run.
    Blackhole,
}

impl LightKind {
    /// The power forged by a straight run with no perpendicular intersection. A run of exactly 3
    /// just clears (the caller filters that out before calling this); 4 forges a directional Ray,
    /// 5+ always forges a Starburst — straight is the easiest shape to read on the board, so it's
    /// the most generous one to reward.
    pub(crate) fn from_line(len: usize, is_horizontal: bool) -> LightKind {
        if len >= 5 {
            LightKind::Starburst
        } else if is_horizontal {
            LightKind::RayH
        } else {
            LightKind::RayV
        }
    }

    /// The power forged by two runs crossing at one shared cell. An arm that's already
    /// Starburst-worthy on its own (5+) AND also crosses another run is the rarest, most powerful
    /// shape on the board — it forges a Blackhole outright, more potent than a Starburst reached
    /// by a freestanding line. Otherwise, shape sets the baseline: a **corner** (`is_corner` — the
    /// shared cell sits at an endpoint of BOTH arms, an "L") forges a Cross; anything else
    /// (mid-run on at least one arm — a "T" or a "+") forges a Supernova. A bigger match should
    /// never feel weaker than a smaller one of the same shape, so if the total pieces consumed
    /// (`h_len + v_len − 1`) would justify Cross on the old `tier = pieces − 2` scale, that wins.
    pub(crate) fn from_intersection(h_len: usize, v_len: usize, is_corner: bool) -> LightKind {
        if h_len >= 5 || v_len >= 5 {
            return LightKind::Blackhole;
        }
        let pieces_tier = (h_len + v_len - 1).saturating_sub(2).clamp(2, 4);
        let shape_tier = if is_corner { 4 } else { 3 };
        if pieces_tier.max(shape_tier) >= 4 {
            LightKind::Cross
        } else {
            LightKind::Supernova
        }
    }

    /// The next power up the tier ladder, or `None` if already at the top (`Blackhole`). Used only
    /// by the shop's "subir tier" booster — it walks one rung up the table above, picking a default
    /// orientation (`RayH`) for the Normal→Ray step. Both rays share the next rung (Supernova).
    pub(crate) fn next_tier(self) -> Option<LightKind> {
        Some(match self {
            LightKind::Normal => LightKind::RayH,
            LightKind::RayH | LightKind::RayV => LightKind::Supernova,
            LightKind::Supernova => LightKind::Cross,
            LightKind::Cross => LightKind::Starburst,
            LightKind::Starburst => LightKind::Blackhole,
            LightKind::Blackhole => return None,
        })
    }
}

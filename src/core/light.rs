use bevy::color::Srgba;
use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LightColor {
    Red,
    Green,
    Blue,
    Yellow,
    Purple,
}

impl LightColor {
    pub(crate) fn random_weighted(rng: &mut impl Rng, weights: [f32; 5]) -> Self {
        let total_weight: f32 = weights.iter().sum();
        let mut r = rng.random_range(0.0..total_weight);
        let mut selected_idx = 0;
        for (idx, &w) in weights.iter().enumerate() {
            if r < w {
                selected_idx = idx;
                break;
            }
            r -= w;
        }
        Self::from_index(selected_idx)
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

    /// Number of sides of this color's membrane polygon (a "32-gon" for Red reads as a circle).
    /// Also drives the shape of the light's core dots (see `visuals::assets::shaped_core_image`),
    /// so ring and core silhouettes always agree — same as this game's app icon.
    pub(crate) fn ring_sides(self) -> u32 {
        match self {
            Self::Red => 32,
            Self::Green => 3,
            Self::Blue | Self::Yellow => 4,
            Self::Purple => 5,
        }
    }

    /// First-vertex angle of this color's membrane polygon (see `mesh`, `ring_polygon_points`) —
    /// e.g. `Blue` and `Yellow` are both 4-gons, but a 45°-rotated first vertex reads as an upright
    /// square vs. a diamond.
    pub(crate) fn shape_start_angle(self) -> f32 {
        match self {
            Self::Blue => FRAC_PI_4,
            _ => FRAC_PI_2,
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
    /// Hollow hazard — matches with other Hollows and drains the score instead of granting points.
    /// It is a kind, not a power: it has no lightcore, does not detonate, and does not combine.
    Hollow,
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
    pub(crate) fn is_power(self) -> bool {
        !matches!(self, LightKind::Normal | LightKind::Hollow)
    }

    pub(crate) fn is_hollow(self) -> bool {
        matches!(self, LightKind::Hollow)
    }

    pub(crate) fn visual_ring_color(self, color: LightColor) -> Color {
        if self.is_hollow() {
            Color::srgb(2.4, 2.5, 2.6)
        } else {
            color.ring_color()
        }
    }

    pub(crate) fn visual_base_color(self, color: LightColor) -> Color {
        if self.is_hollow() {
            Color::srgb(0.92, 0.96, 1.0)
        } else {
            color.bevy_color()
        }
    }

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

    /// Position on the canonical tier table above, as the number of **corelights** the player
    /// reads on the light (`Hollow` is 0: no core at all). Chained detonations resolve in salvos
    /// of descending corelights (see `chain::check_chain_matches` FASE 1), so this is a gameplay
    /// ordering, not just flavor.
    pub(crate) fn corelights(self) -> u32 {
        match self {
            LightKind::Hollow => 0,
            LightKind::Normal => 1,
            LightKind::RayH | LightKind::RayV => 2,
            LightKind::Supernova => 3,
            LightKind::Cross => 4,
            LightKind::Starburst => 5,
            LightKind::Blackhole => 6,
        }
    }

    /// The next power up the tier ladder, or `None` if already at the top (`Blackhole`). Used only
    /// by the shop's "subir tier" booster — it walks one rung up the table above, picking a default
    /// orientation (`RayH`) for the Normal→Ray step. Both rays share the next rung (Supernova).
    pub(crate) fn next_tier(self) -> Option<LightKind> {
        Some(match self {
            LightKind::Normal => LightKind::RayH,
            LightKind::Hollow => return None,
            LightKind::RayH | LightKind::RayV => LightKind::Supernova,
            LightKind::Supernova => LightKind::Cross,
            LightKind::Cross => LightKind::Starburst,
            LightKind::Starburst => LightKind::Blackhole,
            LightKind::Blackhole => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `corelights` and `next_tier` describe the same ladder from two angles — climbing one rung
    /// must always mean strictly more corelights, or the descending-salvo ordering in
    /// `chain::check_chain_matches` would fire tiers out of sequence.
    #[test]
    fn corelights_ascend_the_next_tier_ladder() {
        let all = [
            LightKind::Normal,
            LightKind::Hollow,
            LightKind::RayH,
            LightKind::RayV,
            LightKind::Supernova,
            LightKind::Cross,
            LightKind::Starburst,
            LightKind::Blackhole,
        ];
        for kind in all {
            if let Some(next) = kind.next_tier() {
                assert!(
                    next.corelights() > kind.corelights(),
                    "{kind:?} ({}) -> {next:?} ({}) no asciende",
                    kind.corelights(),
                    next.corelights(),
                );
            }
        }
    }
}

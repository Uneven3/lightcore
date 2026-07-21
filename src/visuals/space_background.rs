//! Static star field behind the board. Replaces the old procedural `space.wgsl` fullscreen shader,
//! which ran a per-pixel fragment program every frame — wasteful once HDR/Bloom already saturates
//! the GPU's memory bandwidth. The stars are scattered once at startup and never updated; the bright
//! ones bloom via the camera, so the field still feels alive for free. The reactive light/shadow
//! effects the shader used to do (black-hole pull/ripple, camera-shake shimmer, the dotted grid) are
//! being rebuilt on the board grid instead.

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::visuals::assets::VisualCache;

pub(crate) struct SpaceBackgroundPlugin;

impl Plugin for SpaceBackgroundPlugin {
    fn build(&self, app: &mut App) {
        // PreStartup builds the `VisualCache`; spawn the stars in Startup so `glow_image` exists.
        app.add_systems(Startup, setup_stars);
    }
}

/// How many stars to scatter, and the half-extents (world units) they cover — generous enough to
/// fill past a 1080p viewport so window resizing / camera shake never reveal an empty edge.
const STAR_COUNT: usize = 260;
const FIELD_HALF_W: f32 = 1100.0;
const FIELD_HALF_H: f32 = 700.0;

fn setup_stars(mut commands: Commands, cache: Res<VisualCache>) {
    // Fixed seed → the field is identical every run (no stars "jumping" between launches). The stars
    // are static: no per-frame system touches them, and sharing `glow_image` batches them into ~1
    // draw call.
    let mut rng = StdRng::seed_from_u64(0x5740_C0DE);
    for _ in 0..STAR_COUNT {
        let x = rng.random_range(-FIELD_HALF_W..FIELD_HALF_W);
        let y = rng.random_range(-FIELD_HALF_H..FIELD_HALF_H);
        // Most stars are dim; a rare few are HDR-bright (>1.0) so the camera Bloom turns them into
        // soft glints. Slight warm/cool tint variation keeps the field from looking uniform.
        let roll = rng.random::<f32>();
        let bright = roll > 0.92;
        let intensity = if bright {
            rng.random_range(1.6..2.6)
        } else {
            rng.random_range(0.22..0.7)
        };
        let size = rng.random_range(2.0..4.5) * if bright { 1.7 } else { 1.0 };
        let warm = rng.random_range(0.85..1.0);
        commands.spawn((
            Sprite {
                image: cache.glow_image.clone(),
                color: Color::srgb(warm * intensity, warm * intensity, intensity),
                custom_size: Some(Vec2::splat(size)),
                ..default()
            },
            Transform::from_xyz(x, y, -10.0), // behind the board (z=0) and glow pools (z=-2)
        ));
    }
}

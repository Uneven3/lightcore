use bevy::prelude::*;

use crate::visuals::grid_water::GridWaterSettings;
use crate::visuals::particles::ParticleSettings;
use crate::visuals::render_target::RenderScale;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeviceClass {
    Desktop,
    Tablet,
    Phone,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PerfTier {
    Low,
    Medium,
    High,
}

#[derive(Resource)]
pub(crate) struct PlatformProfile {
    pub(crate) performance_tier: PerfTier,
    /// Si `false`, las opciones solo relevantes en desktop (fullscreen, etc.) se ocultan.
    pub(crate) show_desktop_options: bool,
}

pub(crate) struct PlatformPlugin;

impl Plugin for PlatformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (detect_platform, apply_platform_defaults).chain());
    }
}

fn detect_platform(mut commands: Commands, _window: Single<&Window>) {
    // Plataformas nativas móviles — detectadas en compile time.
    #[cfg(target_os = "android")]
    let class = DeviceClass::Phone;
    #[cfg(target_os = "ios")]
    let class = DeviceClass::Tablet;
    // Desktop: siempre usar la clase Desktop para evitar que una ventana inicial de 720p se catalogue como Tablet/Low-Perf
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let class = DeviceClass::Desktop;

    // Override de desarrollo: MATCH3_PERF=low|medium|high
    let tier = match std::env::var("MATCH3_PERF").as_deref() {
        Ok("low") => PerfTier::Low,
        Ok("medium") => PerfTier::Medium,
        Ok("high") => PerfTier::High,
        _ => match class {
            DeviceClass::Phone => PerfTier::Low,
            DeviceClass::Tablet => PerfTier::Medium,
            DeviceClass::Desktop => PerfTier::High,
        },
    };

    commands.insert_resource(PlatformProfile {
        performance_tier: tier,
        show_desktop_options: matches!(class, DeviceClass::Desktop),
    });
}

fn apply_platform_defaults(
    profile: Res<PlatformProfile>,
    mut render_scale: ResMut<RenderScale>,
    mut particles: ResMut<ParticleSettings>,
    mut grid_water: ResMut<GridWaterSettings>,
) {
    match profile.performance_tier {
        PerfTier::Low => {
            render_scale.internal_height = 720;
            particles.trail_particle_count = 2;
            #[cfg(target_os = "android")]
            {
                grid_water.enabled = true;
            }
            #[cfg(not(target_os = "android"))]
            {
                grid_water.enabled = false;
            }
        }
        PerfTier::Medium => {
            render_scale.internal_height = 900;
            particles.trail_particle_count = 4;
            grid_water.enabled = true;
        }
        PerfTier::High => {
            render_scale.internal_height = 1080;
            particles.trail_particle_count = 6;
            grid_water.enabled = true;
        }
    }
}

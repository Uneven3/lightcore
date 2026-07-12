use bevy::prelude::*;

use crate::visuals::grid_water::GridWaterSettings;
use crate::visuals::particles::ParticleSettings;

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
        // App icon (taskbar/alt-tab on Windows, window-manager icon on X11 — see
        // `set_desktop_window_icon`'s doc comment for why it's scoped to these two platforms only).
        // Android's icon comes from `res/mipmap-*` (Cargo.toml's `[package.metadata.android]`); the
        // web build's favicon comes from `index.html`'s `data-trunk rel="icon"` tag.
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        app.add_systems(Update, set_desktop_window_icon);
    }
}

/// Sets the OS window icon from the app's own icon (`assets/icon_64.rgba`, a raw 64×64 RGBA8 blob
/// pre-baked from the source PNG — avoids pulling in a PNG decoder just for this one icon). Runs in
/// `Update` (not `Startup`) and gates itself on a `Local<bool>` because the underlying winit window
/// isn't necessarily created yet the first time a `Startup` system would run; this just waits until
/// it exists, sets the icon once, and goes idle forever after.
///
/// `WinitWindows` isn't a regular `NonSend` *resource* in this Bevy version — it's the thread-local
/// `bevy::winit::WINIT_WINDOWS` (see its own doc comment: temporary until upstream issue #17667
/// lands proper `!Send` resource storage), so accessing it needs `.with_borrow(...)` plus a
/// `NonSendMarker` parameter to force this system onto the main thread (the same pattern
/// `bevy_winit`'s own internal systems use, e.g. `changed_windows`).
///
/// winit only supports this on Windows and X11 (its own docs: "iOS / Android / Web / Wayland /
/// macOS / Orbital: Unsupported" — a no-op there, not a crash) — matches the `not(android, wasm32)`
/// gate above, which still includes macOS; that's fine, `set_window_icon` simply does nothing there.
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn set_desktop_window_icon(
    primary: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut done: Local<bool>,
    _non_send_marker: bevy::ecs::system::NonSendMarker,
) {
    if *done {
        return;
    }
    let Ok(entity) = primary.single() else {
        return;
    };
    let applied = bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
        let Some(window) = windows.get_window(entity) else {
            return false;
        };
        const ICON_SIZE: u32 = 64;
        static ICON_RGBA: &[u8] = include_bytes!("../assets/icon_64.rgba");
        if let Ok(icon) = winit::window::Icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIZE, ICON_SIZE) {
            window.set_window_icon(Some(icon));
        }
        true
    });
    if applied {
        *done = true;
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
    mut particles: ResMut<ParticleSettings>,
    mut grid_water: ResMut<GridWaterSettings>,
) {
    match profile.performance_tier {
        PerfTier::Low => {
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
            particles.trail_particle_count = 4;
            grid_water.enabled = true;
        }
        PerfTier::High => {
            particles.trail_particle_count = 6;
            grid_water.enabled = true;
        }
    }
}

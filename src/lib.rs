#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

pub(crate) mod audio;
pub(crate) mod board;
pub(crate) mod core;
pub(crate) mod debug;
pub(crate) mod embedded;
pub(crate) mod gameplay;
pub(crate) mod input;
pub(crate) mod menu;
pub(crate) mod platform;
pub(crate) mod presentation;
pub(crate) mod settings;
pub(crate) mod state;
pub(crate) mod ui;
pub(crate) mod visuals;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // Log+continue instead of panicking on a failed command (e.g. acting on an
        // already-despawned entity) — preferable to crashing for a non-critical game.
        app.set_error_handler(bevy::ecs::error::warn);
        embedded_asset!(app, "", "../assets/bevy_bird_dark.png");
        embedded_asset!(app, "", "../assets/icons/back.png");
        embedded_asset!(app, "", "../assets/icons/play.png");
        embedded_asset!(app, "", "../assets/icons/settings.png");
        embedded_asset!(app, "", "../assets/icons/power.png");
        app.init_state::<state::Screen>()
            .add_sub_state::<state::MatchPhase>()
            .init_state::<state::Overlay>()
            .init_resource::<state::TutorialModalState>()
            .add_plugins((
                settings::SettingsPlugin,
                core::run::RunPlugin,
                core::campaign::CampaignPlugin,
                platform::PlatformPlugin,
                presentation::PresentationPlugin,
                input::InputPlugin,
            ));
        app.add_plugins(audio::AudioPlugin);
        app.add_plugins((
            ui::UiPlugin,
            visuals::VisualsPlugin,
            gameplay::GameplayPlugin,
            menu::MenuPlugin,
            debug::DebugOverlayPlugin,
        ));
        // More feature plugins get added here as the migration proceeds.
    }
}

pub fn run_game() {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
    }

    // Modo de presentación. En desktop usamos Mailbox porque en esta máquina (PRIME: render en la
    // Radeon, pantalla en la Intel) FIFO/AutoVsync castiga cualquier dip bajo 60 cuantizando a
    // 30/20/15 fps. En móvil dejamos que Android/iOS gobiernen el frame pacing con AutoVsync.
    // Overrides para diagnóstico vía env.
    let present_mode = match std::env::var("MATCH3_PRESENT").as_deref() {
        Ok("vsync") | Ok("fifo") => PresentMode::AutoVsync,
        Ok("immediate") => PresentMode::Immediate,
        Ok("novsync") => PresentMode::AutoNoVsync,
        _ if std::env::var("MATCH3_NOVSYNC").is_ok() => PresentMode::AutoNoVsync,
        #[cfg(any(target_os = "android", target_os = "ios"))]
        _ => PresentMode::AutoVsync,
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        _ => PresentMode::Mailbox,
    };
    // Diagnóstico: MATCH3_RES=1920x1080 arranca a esa resolución para medir coste de fillrate a
    // resolución alta, como cuando maximizas. En uso normal arranca a 1280x720.
    let (rw, rh) = std::env::var("MATCH3_RES")
        .ok()
        .and_then(|s| {
            s.split_once('x')
                .map(|(w, h)| (w.trim().to_string(), h.trim().to_string()))
        })
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1280u32, 720u32));
    let default_plugins = DefaultPlugins
        .set(WindowPlugin {
            // Configure the primary window up front so fullscreen/resolution toggling from the
            // Options screen has a known starting point (it mutates this same `Window`).
            primary_window: Some(Window {
                title: "Lightcore".into(),
                // No forzamos scale_factor 1.0: dejamos que la ventana use la densidad nativa
                // del sistema para que el HUD/texto salgan nítidos. Esta resolución es solo el
                // tamaño lógico inicial de la ventana.
                resolution: WindowResolution::new(rw, rh),
                present_mode,
                ..default()
            }),
            ..default()
        })
        .disable::<bevy::animation::AnimationPlugin>()
        .disable::<bevy::gizmos::GizmoPlugin>()
        .disable::<bevy::gizmos_render::GizmoRenderPlugin>()
        .disable::<bevy::gltf::GltfPlugin>()
        .disable::<bevy::light::LightPlugin>()
        .disable::<bevy::pbr::PbrPlugin>();
    #[cfg(target_os = "android")]
    let default_plugins = default_plugins.disable::<bevy::gilrs::GilrsPlugin>();

    App::new().add_plugins((default_plugins, GamePlugin)).run();
}

// `#[bevy_main]` expands into `android_main` (the symbol `NativeActivity` dlopen's on launch),
// gated `#[cfg(target_os = "android")]` internally. It must live in this crate's `cdylib` build
// (this file), not in `main.rs` — that's a separate `bin` compilation unit cargo-apk never links
// into the `.so`, which is why the APK could install but crashed with `UnsatisfiedLinkError:
// cannot locate symbol "android_main"` before this existed.
#[bevy_main]
fn main() {
    run_game();
}

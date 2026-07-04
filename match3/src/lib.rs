#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::asset::embedded_asset;
use bevy::prelude::bevy_main;
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
        app.init_state::<state::GameState>()
            .init_resource::<core::run::RunState>()
            .add_plugins((
                core::campaign::CampaignPlugin,
                platform::PlatformPlugin,
                input::InputPlugin,
                audio::AudioPlugin,
                ui::UiPlugin,
                visuals::VisualsPlugin,
                gameplay::GameplayPlugin,
                menu::MenuPlugin,
                debug::DebugOverlayPlugin,
            ));
        // More feature plugins get added here as the migration proceeds.
    }
}

#[bevy_main]
fn main() {
    run_game();
}

pub fn run_game() {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
    }

    // Modo de presentación. Por defecto Mailbox (triple buffer): sin tearing y SIN el precipicio del
    // FIFO/AutoVsync, que en esta máquina (PRIME: render en la Radeon, pantalla en la Intel) castiga
    // cualquier dip bajo 60 cuantizando a 30/20/15 fps. Mailbox muestra el framerate real y suave.
    // Si una GPU no soporta Mailbox, wgpu recae en Fifo. Overrides para diagnóstico vía env.
    let present_mode = match std::env::var("MATCH3_PRESENT").as_deref() {
        Ok("vsync") | Ok("fifo") => PresentMode::AutoVsync,
        Ok("immediate") => PresentMode::Immediate,
        Ok("novsync") => PresentMode::AutoNoVsync,
        _ if std::env::var("MATCH3_NOVSYNC").is_ok() => PresentMode::AutoNoVsync,
        _ => PresentMode::Mailbox,
    };
    // Diagnóstico: MATCH3_RES=1920x1080 arranca a esa resolución (para medir el coste de fillrate de
    // HDR+Bloom a resolución alta, como cuando maximizas). En uso normal arranca a 1280x720.
    let (rw, rh) = std::env::var("MATCH3_RES")
        .ok()
        .and_then(|s| {
            s.split_once('x')
                .map(|(w, h)| (w.trim().to_string(), h.trim().to_string()))
        })
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1280u32, 720u32));
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                // Configure the primary window up front so fullscreen/resolution toggling from the
                // Options screen has a known starting point (it mutates this same `Window`).
                primary_window: Some(Window {
                    title: "Lightcore".into(),
                    // NOTA: ya NO forzamos scale_factor 1.0. El RTT (visuals/render_target.rs) desacopla
                    // el coste del bloom del tamaño de ventana —el mundo se rasteriza siempre a la
                    // resolución interna fija—, así que la ventana puede ir a su densidad NATIVA (Retina
                    // incluido) y el HUD/texto sale nítido sin penalizar los FPS. Esta resolución es solo
                    // el tamaño LÓGICO inicial de la ventana; el coste de render lo gobierna RenderScale.
                    resolution: WindowResolution::new(rw, rh),
                    present_mode,
                    ..default()
                }),
                ..default()
            }),
            GamePlugin,
        ))
        .run();
}

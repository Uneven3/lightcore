use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::ui_widgets::{
    ScrollArea, Slider, SliderRange, SliderThumb, SliderValue, TrackClick, slider_self_update,
};
use bevy::window::{MonitorSelection, WindowMode};

use super::{BTN_IDLE, MenuActivated, MenuButton, OptionsReturn, activated, button_hover_system};
use crate::core::grid::{RaySettings, TILE};
use crate::core::locale::{Language, TrKey};
use crate::input::InputActions;
use crate::platform::PlatformProfile;
use crate::state::GameState;
use crate::visuals::camera::FpsTarget;
use crate::visuals::glow::GlowSettings;
use crate::visuals::grid_water::GridWaterSettings;
use crate::visuals::particles::ParticleSettings;
use crate::visuals::render_target::InternalResolution;
use crate::visuals::score_light::ShardSettings;

/// Settings screen — glow and particle parameters (otherwise hardcoded constants)
/// plus volume, all edited live via Bevy's native headless `Slider` widget. Changes apply
/// immediately to the underlying resources; nothing is persisted to disk this pass.
pub(crate) struct OptionsPlugin;

impl Plugin for OptionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(slider_self_update)
            .init_resource::<WindowSettings>()
            .add_systems(OnEnter(GameState::Options), spawn_options)
            .add_systems(OnExit(GameState::Options), despawn_options)
            .add_systems(
                Update,
                (
                    button_hover_system,
                    back_button_system,
                    position_thumbs,
                    apply_slider_values,
                    update_slider_value_labels,
                    fps_button_system,
                    show_fps_button_system,
                    internal_resolution_button_system,
                    grid_water_button_system,
                    fullscreen_button_system,
                    device_button_system,
                    tutorial_button_system,
                    language_button_system,
                    update_settings_labels,
                    update_options_static_labels,
                    scroll_drag_system,
                )
                    .run_if(in_state(GameState::Options)),
            );
    }
}

#[derive(Component)]
struct OptionsRoot;

#[derive(Component)]
struct BackButton;

#[derive(Component)]
struct FpsButton;

#[derive(Component)]
struct FpsLabel;

#[derive(Component)]
struct ShowFpsButton;

#[derive(Component)]
struct ShowFpsLabel;

#[derive(Component)]
struct InternalResolutionButton;

#[derive(Component)]
struct InternalResolutionLabel;

#[derive(Component)]
struct GridWaterButton;

#[derive(Component)]
struct GridWaterLabel;
#[derive(Component)]
struct TutorialButton;
#[derive(Component)]
struct TutorialLabel;
#[derive(Component)]
struct LanguageButton;
#[derive(Component)]
struct LanguageLabel;

#[derive(Component)]
struct OptionsTitleLabel;

#[derive(Component)]
struct BackButtonLabel;

#[derive(Component)]
struct SliderLabel(SliderTarget);

#[derive(Component)]
struct FullscreenButton;

#[derive(Component)]
struct FullscreenLabel;

#[derive(Component)]
struct DeviceButton;

#[derive(Component)]
struct DeviceLabel;

/// Marker on the numeric `Text` shown to the right of each slider track.
#[derive(Component, Clone, Copy)]
struct SliderValueLabel(SliderTarget);

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum DeviceMode {
    #[default]
    Desktop,
    Mobile,
}

impl DeviceMode {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Desktop => Self::Mobile,
            Self::Mobile => Self::Desktop,
        }
    }
    pub(crate) fn label(self, lang: Language) -> String {
        let prefix = lang.tr(TrKey::DeviceLabel);
        let mode = match self {
            Self::Desktop => lang.tr(TrKey::DeviceDesktop),
            Self::Mobile => lang.tr(TrKey::DeviceMobile),
        };
        format!("{}: {}", prefix, mode)
    }
}

#[derive(Resource)]
pub(crate) struct WindowSettings {
    pub(crate) device_mode: DeviceMode,
    pub(crate) internal_resolution: InternalResolution,
    pub(crate) tutorial_enabled: bool,
    pub(crate) show_fps_watermark: bool,
    pub(crate) language: Language,
}

impl Default for WindowSettings {
    fn default() -> Self {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let mode = DeviceMode::Mobile;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let mode = DeviceMode::Desktop;
        let internal_resolution = InternalResolution::Native;

        Self {
            device_mode: mode,
            internal_resolution,
            tutorial_enabled: true,
            show_fps_watermark: true,
            language: Language::default(),
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
enum SliderTarget {
    GlowBrightness,
    GlowOuterRadius,
    GlowOuterAlpha,
    GlowInnerRadius,
    GlowInnerAlpha,
    PopBurstCount,
    BurstRadius,
    MembraneRadius,
    TrailParticleCount,
    Volume,
    // Rayos
    RaySpeed,
    PopDuration,
    StarStagger,
    BoltWidth,
    TrailDuration,
    // Shards
    ShardMinSecs,
    ShardMaxSecs,
    ShardBaseSize,
    ShardCurve,
    ShardHdrBoost,
    ShardHold,
}

fn spawn_options(
    mut commands: Commands,
    settings: Res<WindowSettings>,
    glow: Res<GlowSettings>,
    particles: Res<ParticleSettings>,
    gv: Res<GlobalVolume>,
    fps_target: Res<FpsTarget>,
    ray: Res<RaySettings>,
    shards: Res<ShardSettings>,
    profile: Res<PlatformProfile>,
) {
    commands
        .spawn((
            OptionsRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                padding: UiRect::axes(Val::Px(0.0), Val::Px(28.0)),
                ..default()
            },
            ScrollArea,
            bevy::ui::ScrollPosition::default(),
        ))
        .with_children(|root| {
            root.spawn((
                OptionsTitleLabel,
                Text::new("Opciones"),
                TextFont {
                    font_size: FontSize::Px(40.0),
                    ..default()
                },
                TextColor(Color::srgb(1.4, 1.6, 2.2)),
            ));

            root.spawn((
                Button,
                BackButton,
                MenuButton { index: 0 },
                Node {
                    width: Val::Px(180.0),
                    height: Val::Px(46.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(BTN_IDLE),
            ))
            .with_children(|b| {
                b.spawn((
                    BackButtonLabel,
                    Text::new("Volver"),
                    TextFont {
                        font_size: FontSize::Px(22.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });

            let mut button_index = 1;
            spawn_text_button(
                root,
                DeviceButton,
                DeviceLabel,
                button_index,
                "Dispositivo: Escritorio",
            );
            button_index += 1;
            if profile.show_desktop_options {
                spawn_text_button(
                    root,
                    FullscreenButton,
                    FullscreenLabel,
                    button_index,
                    "Fullscreen: OFF",
                );
                button_index += 1;
            }
            spawn_text_button(root, FpsButton, FpsLabel, button_index, fps_target.label());
            button_index += 1;
            spawn_text_button(
                root,
                InternalResolutionButton,
                InternalResolutionLabel,
                button_index,
                settings.internal_resolution.label(),
            );
            button_index += 1;
            spawn_text_button(
                root,
                ShowFpsButton,
                ShowFpsLabel,
                button_index,
                "Mostrar FPS: ON",
            );
            button_index += 1;
            spawn_text_button(
                root,
                GridWaterButton,
                GridWaterLabel,
                button_index,
                "Grid agua: ON",
            );
            button_index += 1;
            spawn_text_button(
                root,
                TutorialButton,
                TutorialLabel,
                button_index,
                if settings.tutorial_enabled {
                    "Tutorial: ON"
                } else {
                    "Tutorial: OFF"
                },
            );
            button_index += 1;
            spawn_text_button(
                root,
                LanguageButton,
                LanguageLabel,
                button_index,
                settings.language.label(),
            );

            spawn_slider(
                root,
                "Glow brillo",
                SliderTarget::GlowBrightness,
                0.5,
                4.0,
                glow.brightness,
            );
            spawn_slider(
                root,
                "Glow radio ext",
                SliderTarget::GlowOuterRadius,
                0.3,
                2.5,
                glow.outer_radius,
            );
            spawn_slider(
                root,
                "Glow alpha ext",
                SliderTarget::GlowOuterAlpha,
                0.0,
                0.6,
                glow.outer_alpha,
            );
            spawn_slider(
                root,
                "Glow radio int",
                SliderTarget::GlowInnerRadius,
                0.15,
                1.2,
                glow.inner_radius,
            );
            spawn_slider(
                root,
                "Glow alpha int",
                SliderTarget::GlowInnerAlpha,
                0.0,
                1.0,
                glow.inner_alpha,
            );
            spawn_slider(
                root,
                "Pop burst count",
                SliderTarget::PopBurstCount,
                1.0,
                20.0,
                particles.pop_burst_count as f32,
            );
            spawn_slider(
                root,
                "Burst radius",
                SliderTarget::BurstRadius,
                TILE * 0.01,
                TILE * 0.15,
                particles.burst_radius,
            );
            spawn_slider(
                root,
                "Membrane radius",
                SliderTarget::MembraneRadius,
                TILE * 0.005,
                TILE * 0.06,
                particles.membrane_radius,
            );
            spawn_slider(
                root,
                "Trail particles",
                SliderTarget::TrailParticleCount,
                0.0,
                6.0,
                particles.trail_particle_count as f32,
            );
            spawn_slider(
                root,
                "Volumen",
                SliderTarget::Volume,
                0.0,
                1.5,
                gv.volume.to_linear(),
            );

            // ── Rayos ───────────────────────────────────────────────────────────────────
            spawn_slider(
                root,
                "Velocidad rayo",
                SliderTarget::RaySpeed,
                200.0,
                1500.0,
                ray.speed,
            );
            spawn_slider(
                root,
                "Duracion pop",
                SliderTarget::PopDuration,
                0.03,
                0.35,
                ray.pop_duration,
            );
            spawn_slider(
                root,
                "Stagger estrella",
                SliderTarget::StarStagger,
                0.005,
                0.12,
                ray.stagger_secs,
            );
            spawn_slider(
                root,
                "Ancho bolt",
                SliderTarget::BoltWidth,
                0.2,
                1.0,
                ray.bolt_width_frac,
            );
            spawn_slider(
                root,
                "Duracion trail",
                SliderTarget::TrailDuration,
                0.1,
                0.8,
                ray.trail_duration,
            );

            // ── Shards ──────────────────────────────────────────────────────────────────
            spawn_slider(
                root,
                "Shard vel. min",
                SliderTarget::ShardMinSecs,
                0.3,
                1.5,
                shards.min_secs,
            );
            spawn_slider(
                root,
                "Shard vel. max",
                SliderTarget::ShardMaxSecs,
                0.5,
                2.5,
                shards.max_secs,
            );
            spawn_slider(
                root,
                "Escala shard",
                SliderTarget::ShardBaseSize,
                0.15,
                1.2,
                shards.base_size_frac,
            );
            spawn_slider(
                root,
                "Shard curva",
                SliderTarget::ShardCurve,
                0.3,
                3.0,
                shards.curve_frac,
            );
            spawn_slider(
                root,
                "Shard brillo HDR",
                SliderTarget::ShardHdrBoost,
                1.0,
                8.0,
                shards.hdr_boost,
            );
            spawn_slider(
                root,
                "Shard pausa",
                SliderTarget::ShardHold,
                0.0,
                0.4,
                shards.hold_secs,
            );
        });
}

fn format_slider_value(target: SliderTarget, v: f32) -> String {
    match target {
        SliderTarget::PopBurstCount | SliderTarget::TrailParticleCount => {
            format!("{}", v.round() as u32)
        }
        SliderTarget::Volume => format!("{:.0}%", (v / 1.5 * 100.0).min(100.0)),
        _ => format!("{:.2}", v),
    }
}

fn spawn_slider(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    target: SliderTarget,
    min: f32,
    max: f32,
    value: f32,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .with_children(|col| {
            col.spawn((
                SliderLabel(target),
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::srgb(0.75, 0.85, 1.0)),
                Node {
                    margin: UiRect::left(Val::Px(4.0)),
                    ..default()
                },
            ));
            col.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Slider {
                        track_click: TrackClick::Drag,
                        ..default()
                    },
                    SliderValue(value),
                    SliderRange::new(min, max),
                    target,
                    Node {
                        width: Val::Px(240.0),
                        height: Val::Px(20.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.22)),
                ))
                .with_children(|track| {
                    track.spawn((
                        SliderThumb,
                        Node {
                            width: Val::Px(14.0),
                            height: Val::Px(20.0),
                            position_type: PositionType::Absolute,
                            left: Val::Percent(0.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.5, 0.8, 1.0)),
                    ));
                });
                row.spawn((
                    SliderValueLabel(target),
                    Text::new(format_slider_value(target, value)),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.9, 1.0)),
                    Node {
                        width: Val::Px(58.0),
                        ..default()
                    },
                ));
            });
        });
}

fn position_thumbs(
    sliders: Query<(&SliderValue, &SliderRange, &Children), With<Slider>>,
    mut thumbs: Query<&mut Node, With<SliderThumb>>,
) {
    for (value, range, children) in &sliders {
        let pct = (range.thumb_position(value.0) * 100.0).clamp(0.0, 100.0);
        for &child in children {
            if let Ok(mut node) = thumbs.get_mut(child) {
                node.left = Val::Percent(pct);
            }
        }
    }
}

fn update_slider_value_labels(
    sliders: Query<(&SliderTarget, &SliderValue), Changed<SliderValue>>,
    mut labels: Query<(&SliderValueLabel, &mut Text)>,
) {
    for (target, value) in &sliders {
        for (label, mut text) in &mut labels {
            if label.0 == *target {
                **text = format_slider_value(*target, value.0);
            }
        }
    }
}

fn apply_slider_values(
    sliders: Query<(&SliderTarget, &SliderValue), Changed<SliderValue>>,
    mut glow: ResMut<GlowSettings>,
    mut particles: ResMut<ParticleSettings>,
    mut gv: ResMut<GlobalVolume>,
    mut ray: ResMut<RaySettings>,
    mut shards: ResMut<ShardSettings>,
) {
    for (target, value) in &sliders {
        match target {
            SliderTarget::GlowBrightness => glow.brightness = value.0,
            SliderTarget::GlowOuterRadius => glow.outer_radius = value.0,
            SliderTarget::GlowOuterAlpha => glow.outer_alpha = value.0,
            SliderTarget::GlowInnerRadius => glow.inner_radius = value.0,
            SliderTarget::GlowInnerAlpha => glow.inner_alpha = value.0,
            SliderTarget::PopBurstCount => {
                particles.pop_burst_count = value.0.round().max(1.0) as usize
            }
            SliderTarget::BurstRadius => particles.burst_radius = value.0,
            SliderTarget::MembraneRadius => particles.membrane_radius = value.0,
            SliderTarget::TrailParticleCount => {
                particles.trail_particle_count = value.0.round().max(0.0) as usize
            }
            SliderTarget::Volume => gv.volume = Volume::Linear(value.0),
            SliderTarget::RaySpeed => ray.speed = value.0,
            SliderTarget::PopDuration => ray.pop_duration = value.0,
            SliderTarget::StarStagger => ray.stagger_secs = value.0,
            SliderTarget::BoltWidth => ray.bolt_width_frac = value.0,
            SliderTarget::TrailDuration => ray.trail_duration = value.0,
            SliderTarget::ShardMinSecs => shards.min_secs = value.0,
            SliderTarget::ShardMaxSecs => shards.max_secs = value.0,
            SliderTarget::ShardBaseSize => shards.base_size_frac = value.0,
            SliderTarget::ShardCurve => shards.curve_frac = value.0,
            SliderTarget::ShardHdrBoost => shards.hdr_boost = value.0,
            SliderTarget::ShardHold => shards.hold_secs = value.0,
        }
    }
}

fn spawn_text_button<B: Component, L: Component>(
    parent: &mut ChildSpawnerCommands,
    btn: B,
    label: L,
    index: usize,
    text: &str,
) {
    parent
        .spawn((
            Button,
            btn,
            MenuButton { index },
            Node {
                width: Val::Px(280.0),
                height: Val::Px(40.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                margin: UiRect::top(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(BTN_IDLE),
        ))
        .with_children(|b| {
            b.spawn((
                label,
                Text::new(text),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn fps_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<FpsButton>>,
    menu_activated: Res<MenuActivated>,
    mut fps_target: ResMut<FpsTarget>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        *fps_target = fps_target.next();
    }
}

fn fullscreen_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<FullscreenButton>>,
    menu_activated: Res<MenuActivated>,
    mut window: Single<&mut Window>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        window.mode = match window.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
}

fn grid_water_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<GridWaterButton>>,
    menu_activated: Res<MenuActivated>,
    mut settings: ResMut<GridWaterSettings>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        settings.enabled = !settings.enabled;
    }
}

fn show_fps_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<ShowFpsButton>>,
    menu_activated: Res<MenuActivated>,
    mut settings: ResMut<WindowSettings>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        settings.show_fps_watermark = !settings.show_fps_watermark;
    }
}

fn internal_resolution_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<InternalResolutionButton>>,
    menu_activated: Res<MenuActivated>,
    mut settings: ResMut<WindowSettings>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        settings.internal_resolution = settings.internal_resolution.next();
    }
}

fn tutorial_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<TutorialButton>>,
    menu_activated: Res<MenuActivated>,
    mut settings: ResMut<WindowSettings>,
    mut label: Query<&mut Text, With<TutorialLabel>>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        settings.tutorial_enabled = !settings.tutorial_enabled;
        for mut t in &mut label {
            t.0 = format!(
                "Tutorial: {}",
                if settings.tutorial_enabled {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }
    }
}

fn language_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<LanguageButton>>,
    menu_activated: Res<MenuActivated>,
    mut settings: ResMut<WindowSettings>,
    mut label: Query<&mut Text, With<LanguageLabel>>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        settings.language = settings.language.next();
        for mut t in &mut label {
            t.0 = settings.language.label().to_string();
        }
    }
}

fn device_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<DeviceButton>>,
    menu_activated: Res<MenuActivated>,
    mut settings: ResMut<WindowSettings>,
) {
    if interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated))
    {
        let prev = settings.device_mode;
        settings.device_mode = settings.device_mode.next();
        info!(
            "Cambiando modo de dispositivo de {:?} a {:?}",
            prev, settings.device_mode
        );
    }
}

fn update_settings_labels(
    window: Single<&Window>,
    settings: Res<WindowSettings>,
    grid_water: Res<GridWaterSettings>,
    fps_target: Res<FpsTarget>,
    mut fs: Query<
        &mut Text,
        (
            With<FullscreenLabel>,
            Without<ShowFpsLabel>,
            Without<GridWaterLabel>,
            Without<FpsLabel>,
            Without<DeviceLabel>,
            Without<InternalResolutionLabel>,
        ),
    >,
    mut gw: Query<
        &mut Text,
        (
            With<GridWaterLabel>,
            Without<FullscreenLabel>,
            Without<ShowFpsLabel>,
            Without<FpsLabel>,
            Without<DeviceLabel>,
            Without<InternalResolutionLabel>,
        ),
    >,
    mut show_fps_q: Query<
        &mut Text,
        (
            With<ShowFpsLabel>,
            Without<FullscreenLabel>,
            Without<GridWaterLabel>,
            Without<FpsLabel>,
            Without<DeviceLabel>,
            Without<InternalResolutionLabel>,
        ),
    >,
    mut internal_resolution_q: Query<
        &mut Text,
        (
            With<InternalResolutionLabel>,
            Without<FullscreenLabel>,
            Without<ShowFpsLabel>,
            Without<GridWaterLabel>,
            Without<FpsLabel>,
            Without<DeviceLabel>,
        ),
    >,
    mut fps_q: Query<
        &mut Text,
        (
            With<FpsLabel>,
            Without<FullscreenLabel>,
            Without<ShowFpsLabel>,
            Without<GridWaterLabel>,
            Without<DeviceLabel>,
            Without<InternalResolutionLabel>,
        ),
    >,
    mut ds: Query<
        &mut Text,
        (
            With<DeviceLabel>,
            Without<FullscreenLabel>,
            Without<ShowFpsLabel>,
            Without<GridWaterLabel>,
            Without<FpsLabel>,
            Without<InternalResolutionLabel>,
        ),
    >,
) {
    let lang = settings.language;
    let fs_on = !matches!(window.mode, WindowMode::Windowed);
    for mut t in &mut fs {
        **t = format!(
            "{}: {}",
            lang.tr(TrKey::Fullscreen),
            if fs_on { "ON" } else { "OFF" }
        );
    }
    for mut t in &mut gw {
        **t = format!(
            "{}: {}",
            lang.tr(TrKey::GridWater),
            if grid_water.enabled { "ON" } else { "OFF" }
        );
    }
    for mut t in &mut show_fps_q {
        **t = format!(
            "Mostrar FPS: {}",
            if settings.show_fps_watermark {
                "ON"
            } else {
                "OFF"
            }
        );
    }
    for mut t in &mut fps_q {
        **t = fps_target.label().to_string();
    }
    for mut t in &mut internal_resolution_q {
        **t = settings.internal_resolution.label().to_string();
    }
    for mut t in &mut ds {
        **t = settings.device_mode.label(lang);
    }
}

fn get_slider_label_text(target: SliderTarget, lang: Language) -> &'static str {
    match target {
        SliderTarget::GlowBrightness => lang.tr(TrKey::SliderGlowBrightness),
        SliderTarget::GlowOuterRadius => lang.tr(TrKey::SliderGlowOuterRadius),
        SliderTarget::GlowOuterAlpha => lang.tr(TrKey::SliderGlowOuterAlpha),
        SliderTarget::GlowInnerRadius => lang.tr(TrKey::SliderGlowInnerRadius),
        SliderTarget::GlowInnerAlpha => lang.tr(TrKey::SliderGlowInnerAlpha),
        SliderTarget::PopBurstCount => lang.tr(TrKey::SliderPopBurstCount),
        SliderTarget::BurstRadius => lang.tr(TrKey::SliderBurstRadius),
        SliderTarget::MembraneRadius => lang.tr(TrKey::SliderMembraneRadius),
        SliderTarget::TrailParticleCount => lang.tr(TrKey::SliderTrailParticleCount),
        SliderTarget::RaySpeed => lang.tr(TrKey::SliderRaySpeed),
        SliderTarget::PopDuration => lang.tr(TrKey::SliderPopDuration),
        SliderTarget::StarStagger => lang.tr(TrKey::SliderStarStagger),
        SliderTarget::BoltWidth => lang.tr(TrKey::SliderBoltWidth),
        SliderTarget::TrailDuration => lang.tr(TrKey::SliderTrailDuration),
        SliderTarget::ShardMinSecs => lang.tr(TrKey::SliderShardMinSecs),
        SliderTarget::ShardMaxSecs => lang.tr(TrKey::SliderShardMaxSecs),
        SliderTarget::ShardBaseSize => lang.tr(TrKey::SliderShardBaseSize),
        SliderTarget::ShardCurve => lang.tr(TrKey::SliderShardCurve),
        SliderTarget::ShardHdrBoost => lang.tr(TrKey::SliderShardHdrBoost),
        SliderTarget::ShardHold => lang.tr(TrKey::SliderShardHold),
        SliderTarget::Volume => lang.tr(TrKey::SliderVolume),
    }
}

fn update_options_static_labels(
    settings: Res<WindowSettings>,
    mut title: Query<&mut Text, With<OptionsTitleLabel>>,
    mut back: Query<&mut Text, (With<BackButtonLabel>, Without<OptionsTitleLabel>)>,
    mut sliders: Query<
        (&SliderLabel, &mut Text),
        (Without<OptionsTitleLabel>, Without<BackButtonLabel>),
    >,
) {
    let lang = settings.language;
    for mut t in &mut title {
        **t = lang.tr(TrKey::OptionsTitle).to_string();
    }
    for mut t in &mut back {
        **t = lang.tr(TrKey::Back).to_string();
    }
    for (slider, mut t) in &mut sliders {
        **t = get_slider_label_text(slider.0, lang).to_string();
    }
}

fn back_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<BackButton>>,
    actions: Res<InputActions>,
    menu_activated: Res<MenuActivated>,
    options_return: Res<OptionsReturn>,
    mut next: ResMut<NextState<GameState>>,
) {
    let clicked = interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated));
    if clicked || actions.menu_back() {
        next.set(options_return.0.clone());
    }
}

fn despawn_options(mut commands: Commands, q: Query<Entity, With<OptionsRoot>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

#[derive(Default)]
struct DragScrollState {
    active: bool,
    start_pointer_y: f32,
    start_scroll_y: f32,
}

fn scroll_drag_system(
    pointer: Res<crate::input::pointer::PointerInput>,
    mut scroll_area: Single<&mut bevy::ui::ScrollPosition, With<ScrollArea>>,
    mut state: Local<DragScrollState>,
) {
    if pointer.just_pressed {
        if let Some(pos) = pointer.position_window {
            state.active = true;
            state.start_pointer_y = pos.y;
            state.start_scroll_y = scroll_area.0.y;
        }
    } else if pointer.held && state.active {
        if let Some(pos) = pointer.position_window {
            let delta = pos.y - state.start_pointer_y;
            scroll_area.0.y = (state.start_scroll_y - delta).max(0.0);
        }
    } else {
        state.active = false;
    }
}

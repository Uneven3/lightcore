use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::ui_widgets::{
    ScrollArea, Slider, SliderRange, SliderThumb, SliderValue, TrackClick, slider_self_update,
};
use bevy::window::{MonitorSelection, WindowMode};

use super::{BTN_HOVER, BTN_IDLE, OptionsReturn};
use crate::core::grid::TILE;
use crate::core::locale::{Language, TrKey};
use crate::gameplay::MatchTiming;
use crate::input::InputActions;
use crate::platform::PlatformProfile;
use crate::presentation::{PresentationSettings, ViewportMode};
use crate::settings::UserSettings;
use crate::state::Overlay;
use crate::visuals::camera::FpsTarget;
use crate::visuals::glow::GlowSettings;
use crate::visuals::particles::ParticleSettings;
use crate::visuals::score_light::ShardSettings;

/// Tabbed settings screen — Audio / Graphics / Interface, each a panel toggled by the tab bar. The
/// technical visual/timing sliders live in a collapsible "Advanced" section inside Graphics rather
/// than a separate screen. Every label is routed through `lang.tr(...)` so switching language
/// re-localizes the whole screen. Pointer-driven (mouse + touch) so it works identically on desktop
/// and mobile; sliders are the native headless `Slider` widget. Changes apply live to the
/// underlying resources; `UserSettings` persists via its own plugin.
pub(crate) struct OptionsPlugin;

impl Plugin for OptionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OptionsTab>()
            .init_resource::<AdvancedExpanded>()
            .add_observer(slider_self_update)
            .add_systems(OnEnter(Overlay::Options), spawn_options)
            .add_systems(OnExit(Overlay::Options), despawn_options)
            .add_systems(
                Update,
                (
                    option_button_hover,
                    back_button_system,
                    tab_button_system,
                    apply_tab_visibility,
                    advanced_toggle_system,
                    apply_advanced_visibility,
                    fps_button_system,
                    show_fps_button_system,
                    internal_resolution_button_system,
                    fullscreen_button_system,
                    device_button_system,
                    tutorial_button_system,
                    language_button_system,
                    position_thumbs,
                    apply_slider_values,
                    update_slider_value_labels,
                    refresh_option_labels,
                    refresh_slider_labels,
                    scroll_drag_system,
                )
                    .run_if(in_state(Overlay::Options)),
            );
    }
}

/// Which options tab is showing. Persists across open/close so reopening returns to the last tab.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
enum OptionsTab {
    Audio,
    #[default]
    Graphics,
    Interface,
}

/// Whether the collapsible technical-sliders section inside Graphics is expanded.
#[derive(Resource, Default)]
struct AdvancedExpanded(bool);

#[derive(Component)]
struct OptionsRoot;
#[derive(Component)]
struct BackButton;
/// Marker for every pointer-driven button in the screen (tabs, toggles, back) so one hover system
/// paints them all — except tab buttons, whose active/idle colour is owned by `apply_tab_visibility`.
#[derive(Component)]
struct OptionButton;
#[derive(Component)]
struct TabButton(OptionsTab);
#[derive(Component)]
struct TabPanel(OptionsTab);
#[derive(Component)]
struct AdvancedToggleButton;
#[derive(Component)]
struct AdvancedContainer;

#[derive(Component)]
struct FpsButton;
#[derive(Component)]
struct ShowFpsButton;
#[derive(Component)]
struct InternalResolutionButton;
#[derive(Component)]
struct FullscreenButton;
#[derive(Component)]
struct DeviceButton;
#[derive(Component)]
struct TutorialButton;
#[derive(Component)]
struct LanguageButton;

/// The kind of dynamic label a `Text` node carries, so a single system re-localizes them all from
/// current state instead of one disjoint `Query` per label type.
#[derive(Component, Clone, Copy)]
enum OptLabel {
    Title,
    Tab(OptionsTab),
    Fullscreen,
    InternalResolution,
    FpsLimit,
    ShowFps,
    Device,
    Tutorial,
    Language,
    AdvancedToggle,
}

#[derive(Component)]
struct SliderLabel(SliderTarget);

/// Marker on the numeric `Text` shown to the right of each slider track.
#[derive(Component, Clone, Copy)]
struct SliderValueLabel(SliderTarget);

impl ViewportMode {
    pub(crate) fn label(self, lang: Language) -> String {
        let (prefix, mode) = match (lang, self) {
            (Language::English, Self::Auto) => ("Viewport", "Auto"),
            (Language::English, Self::PortraitPreview) => ("Viewport", "Mobile 9:16"),
            (_, Self::Auto) => ("Vista", "Automática"),
            (_, Self::PortraitPreview) => ("Vista", "Móvil 9:16"),
        };
        format!("{}: {}", prefix, mode)
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

fn on_off(v: bool) -> &'static str {
    if v { "ON" } else { "OFF" }
}

#[allow(clippy::too_many_arguments)]
fn spawn_options(
    mut commands: Commands,
    settings: Res<UserSettings>,
    presentation: Res<PresentationSettings>,
    gv: Res<GlobalVolume>,
    fps_target: Res<FpsTarget>,
    profile: Res<PlatformProfile>,
    asset_server: Res<AssetServer>,
    glow: Res<GlowSettings>,
    particles: Res<ParticleSettings>,
    ray: Res<MatchTiming>,
    shards: Res<ShardSettings>,
) {
    let lang = settings.language;
    let desktop = profile.show_desktop_options;
    let back_icon = asset_server.load(crate::embedded::back_icon_path());
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
                padding: UiRect::axes(Val::Px(0.0), Val::Px(24.0)),
                ..default()
            },
            ScrollArea,
            bevy::ui::ScrollPosition::default(),
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                OptLabel::Title,
                Text::new(lang.tr(TrKey::OptionsTitle)),
                TextFont {
                    font_size: FontSize::Px(38.0),
                    ..default()
                },
                TextColor(Color::srgb(1.4, 1.6, 2.2)),
            ));

            // Back button (icon)
            root.spawn((
                Button,
                BackButton,
                OptionButton,
                Node {
                    width: Val::Px(150.0),
                    height: Val::Px(42.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(BTN_IDLE),
                BorderColor::all(Color::srgba(0.25, 0.6, 1.0, 0.45)),
            ))
            .with_children(|b| {
                b.spawn((
                    ImageNode {
                        image: back_icon,
                        color: Color::srgb(0.78, 0.92, 1.0),
                        ..default()
                    },
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        ..default()
                    },
                ));
            });

            // Tab bar
            root.spawn((Node {
                width: Val::Percent(92.0),
                max_width: Val::Px(360.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                justify_content: JustifyContent::Center,
                margin: UiRect::vertical(Val::Px(4.0)),
                ..default()
            },))
                .with_children(|bar| {
                    for tab in [
                        OptionsTab::Audio,
                        OptionsTab::Graphics,
                        OptionsTab::Interface,
                    ] {
                        spawn_tab(bar, tab, tab_label(tab, lang));
                    }
                });

            // Content: the three panels stacked, only the active one displayed.
            root.spawn((Node {
                width: Val::Percent(92.0),
                max_width: Val::Px(360.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(8.0),
                ..default()
            },))
                .with_children(|content| {
                    // ── Audio ──────────────────────────────────────────────
                    content
                        .spawn((TabPanel(OptionsTab::Audio), panel_node()))
                        .with_children(|panel| {
                            spawn_slider(
                                panel,
                                get_slider_label_text(SliderTarget::Volume, lang),
                                SliderTarget::Volume,
                                0.0,
                                1.5,
                                gv.volume.to_linear(),
                            );
                        });

                    // ── Graphics ───────────────────────────────────────────
                    content
                        .spawn((TabPanel(OptionsTab::Graphics), panel_node()))
                        .with_children(|panel| {
                            if desktop {
                                spawn_opt_button(
                                    panel,
                                    FullscreenButton,
                                    OptLabel::Fullscreen,
                                    &format!("{}: OFF", lang.tr(TrKey::Fullscreen)),
                                );
                            }
                            spawn_opt_button(
                                panel,
                                InternalResolutionButton,
                                OptLabel::InternalResolution,
                                &presentation.internal_resolution.label(lang),
                            );
                            spawn_opt_button(
                                panel,
                                FpsButton,
                                OptLabel::FpsLimit,
                                &fps_target.label(lang),
                            );
                            // Collapsible technical sliders.
                            spawn_opt_button(
                                panel,
                                AdvancedToggleButton,
                                OptLabel::AdvancedToggle,
                                &format!("{} [+]", lang.tr(TrKey::AdvancedSection)),
                            );
                            panel
                                .spawn((
                                    AdvancedContainer,
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Column,
                                        align_items: AlignItems::Stretch,
                                        row_gap: Val::Px(6.0),
                                        display: Display::None,
                                        ..default()
                                    },
                                ))
                                .with_children(|adv| {
                                    spawn_advanced_sliders(adv, lang, &glow, &particles, &ray, &shards);
                                });
                        });

                    // ── Interface ──────────────────────────────────────────
                    content
                        .spawn((TabPanel(OptionsTab::Interface), panel_node()))
                        .with_children(|panel| {
                            spawn_opt_button(
                                panel,
                                LanguageButton,
                                OptLabel::Language,
                                settings.language.label(),
                            );
                            spawn_opt_button(
                                panel,
                                TutorialButton,
                                OptLabel::Tutorial,
                                &format!(
                                    "{}: {}",
                                    lang.tr(TrKey::Tutorial),
                                    on_off(settings.tutorial_enabled)
                                ),
                            );
                            spawn_opt_button(
                                panel,
                                ShowFpsButton,
                                OptLabel::ShowFps,
                                &format!(
                                    "{}: {}",
                                    lang.tr(TrKey::ShowFps),
                                    on_off(settings.show_fps_watermark)
                                ),
                            );
                            if desktop {
                                spawn_opt_button(
                                    panel,
                                    DeviceButton,
                                    OptLabel::Device,
                                    &presentation.viewport_mode.label(lang),
                                );
                            }
                        });
                });
        });
}

fn tab_label(tab: OptionsTab, lang: Language) -> &'static str {
    lang.tr(match tab {
        OptionsTab::Audio => TrKey::TabAudio,
        OptionsTab::Graphics => TrKey::TabGraphics,
        OptionsTab::Interface => TrKey::TabInterface,
    })
}

fn panel_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Stretch,
        row_gap: Val::Px(6.0),
        display: Display::None,
        ..default()
    }
}

fn spawn_tab(parent: &mut ChildSpawnerCommands, tab: OptionsTab, initial: &str) {
    parent
        .spawn((
            Button,
            OptionButton,
            TabButton(tab),
            Node {
                flex_grow: 1.0,
                height: Val::Px(38.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.5)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(BTN_IDLE),
            BorderColor::all(Color::srgba(0.25, 0.6, 1.0, 0.45)),
        ))
        .with_children(|b| {
            b.spawn((
                OptLabel::Tab(tab),
                Text::new(initial),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_opt_button<M: Component>(
    parent: &mut ChildSpawnerCommands,
    marker: M,
    label: OptLabel,
    initial: &str,
) {
    parent
        .spawn((
            Button,
            OptionButton,
            marker,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(40.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.5)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(BTN_IDLE),
            BorderColor::all(Color::srgba(0.25, 0.6, 1.0, 0.45)),
        ))
        .with_children(|b| {
            b.spawn((
                label,
                Text::new(initial),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_advanced_sliders(
    root: &mut ChildSpawnerCommands,
    lang: Language,
    glow: &GlowSettings,
    particles: &ParticleSettings,
    ray: &MatchTiming,
    shards: &ShardSettings,
) {
    for (target, min, max, value) in [
        (SliderTarget::GlowBrightness, 0.5, 4.0, glow.brightness),
        (SliderTarget::GlowOuterRadius, 0.3, 2.5, glow.outer_radius),
        (SliderTarget::GlowOuterAlpha, 0.0, 0.6, glow.outer_alpha),
        (SliderTarget::GlowInnerRadius, 0.15, 1.2, glow.inner_radius),
        (SliderTarget::GlowInnerAlpha, 0.0, 1.0, glow.inner_alpha),
        (
            SliderTarget::PopBurstCount,
            1.0,
            20.0,
            particles.pop_burst_count as f32,
        ),
        (
            SliderTarget::BurstRadius,
            TILE * 0.01,
            TILE * 0.15,
            particles.burst_radius,
        ),
        (
            SliderTarget::MembraneRadius,
            TILE * 0.005,
            TILE * 0.06,
            particles.membrane_radius,
        ),
        (
            SliderTarget::TrailParticleCount,
            0.0,
            6.0,
            particles.trail_particle_count as f32,
        ),
        (SliderTarget::RaySpeed, 200.0, 1500.0, ray.speed),
        (SliderTarget::PopDuration, 0.03, 0.35, ray.pop_duration),
        (SliderTarget::StarStagger, 0.005, 0.12, ray.stagger_secs),
        (SliderTarget::BoltWidth, 0.2, 1.0, ray.bolt_width_frac),
        (SliderTarget::TrailDuration, 0.1, 0.8, ray.trail_duration),
        // Non-overlapping limits guarantee min < max from the UI itself.
        (SliderTarget::ShardMinSecs, 0.30, 0.95, shards.min_secs),
        (SliderTarget::ShardMaxSecs, 1.00, 2.50, shards.max_secs),
        (SliderTarget::ShardBaseSize, 0.15, 1.2, shards.base_size_frac),
        (SliderTarget::ShardCurve, 0.3, 3.0, shards.curve_frac),
        (SliderTarget::ShardHdrBoost, 1.0, 8.0, shards.hdr_boost),
        (SliderTarget::ShardHold, 0.0, 0.4, shards.hold_secs),
    ] {
        spawn_slider(root, get_slider_label_text(target, lang), target, min, max, value);
    }
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
            margin: UiRect::top(Val::Px(4.0)),
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
    mut ray: ResMut<MatchTiming>,
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
            // The two controls deliberately have non-overlapping UI ranges; clamp here as a
            // second line of defence for values restored/changed by code rather than the slider.
            SliderTarget::ShardMinSecs => shards.min_secs = value.0.clamp(0.30, 0.95),
            SliderTarget::ShardMaxSecs => shards.max_secs = value.0.clamp(1.00, 2.50),
            SliderTarget::ShardBaseSize => shards.base_size_frac = value.0,
            SliderTarget::ShardCurve => shards.curve_frac = value.0,
            SliderTarget::ShardHdrBoost => shards.hdr_boost = value.0,
            SliderTarget::ShardHold => shards.hold_secs = value.0,
        }
    }
}

fn option_button_hover(
    mut q: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<OptionButton>, Without<TabButton>),
    >,
) {
    for (interaction, mut bg) in &mut q {
        bg.0 = match interaction {
            Interaction::Hovered | Interaction::Pressed => BTN_HOVER,
            Interaction::None => BTN_IDLE,
        };
    }
}

fn tab_button_system(
    interactions: Query<(&Interaction, &TabButton), Changed<Interaction>>,
    mut tab: ResMut<OptionsTab>,
) {
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed {
            *tab = button.0;
        }
    }
}

fn apply_tab_visibility(
    tab: Res<OptionsTab>,
    mut panels: Query<(&TabPanel, &mut Node)>,
    mut buttons: Query<(&TabButton, &mut BackgroundColor)>,
) {
    for (panel, mut node) in &mut panels {
        node.display = if panel.0 == *tab {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (button, mut bg) in &mut buttons {
        bg.0 = if button.0 == *tab { BTN_HOVER } else { BTN_IDLE };
    }
}

fn advanced_toggle_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<AdvancedToggleButton>)>,
    mut expanded: ResMut<AdvancedExpanded>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            expanded.0 = !expanded.0;
        }
    }
}

fn apply_advanced_visibility(
    expanded: Res<AdvancedExpanded>,
    mut container: Query<&mut Node, With<AdvancedContainer>>,
) {
    for mut node in &mut container {
        node.display = if expanded.0 {
            Display::Flex
        } else {
            Display::None
        };
    }
}

fn fps_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<FpsButton>)>,
    mut fps_target: ResMut<FpsTarget>,
) {
    if interactions.iter().any(|i| *i == Interaction::Pressed) {
        *fps_target = fps_target.next();
    }
}

fn fullscreen_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<FullscreenButton>)>,
    mut window: Single<&mut Window>,
) {
    if interactions.iter().any(|i| *i == Interaction::Pressed) {
        window.mode = match window.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
}

fn show_fps_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ShowFpsButton>)>,
    mut settings: ResMut<UserSettings>,
) {
    if interactions.iter().any(|i| *i == Interaction::Pressed) {
        settings.show_fps_watermark = !settings.show_fps_watermark;
    }
}

fn internal_resolution_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<InternalResolutionButton>)>,
    mut presentation: ResMut<PresentationSettings>,
) {
    if interactions.iter().any(|i| *i == Interaction::Pressed) {
        presentation.internal_resolution = presentation.internal_resolution.next();
    }
}

fn tutorial_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<TutorialButton>)>,
    mut settings: ResMut<UserSettings>,
) {
    if interactions.iter().any(|i| *i == Interaction::Pressed) {
        settings.tutorial_enabled = !settings.tutorial_enabled;
    }
}

fn language_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<LanguageButton>)>,
    mut settings: ResMut<UserSettings>,
) {
    if interactions.iter().any(|i| *i == Interaction::Pressed) {
        settings.language = settings.language.next();
    }
}

fn device_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<DeviceButton>)>,
    mut presentation: ResMut<PresentationSettings>,
) {
    if interactions.iter().any(|i| *i == Interaction::Pressed) {
        presentation.viewport_mode = presentation.viewport_mode.next();
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

#[allow(clippy::too_many_arguments)]
fn refresh_option_labels(
    settings: Res<UserSettings>,
    window: Single<&Window>,
    presentation: Res<PresentationSettings>,
    fps_target: Res<FpsTarget>,
    expanded: Res<AdvancedExpanded>,
    mut q: Query<(&OptLabel, &mut Text)>,
) {
    let lang = settings.language;
    let fs_on = !matches!(window.mode, WindowMode::Windowed);
    for (kind, mut text) in &mut q {
        **text = match kind {
            OptLabel::Title => lang.tr(TrKey::OptionsTitle).to_string(),
            OptLabel::Tab(tab) => tab_label(*tab, lang).to_string(),
            OptLabel::Fullscreen => {
                format!("{}: {}", lang.tr(TrKey::Fullscreen), on_off(fs_on))
            }
            OptLabel::InternalResolution => presentation.internal_resolution.label(lang),
            OptLabel::FpsLimit => fps_target.label(lang),
            OptLabel::ShowFps => format!(
                "{}: {}",
                lang.tr(TrKey::ShowFps),
                on_off(settings.show_fps_watermark)
            ),
            OptLabel::Device => presentation.viewport_mode.label(lang),
            OptLabel::Tutorial => format!(
                "{}: {}",
                lang.tr(TrKey::Tutorial),
                on_off(settings.tutorial_enabled)
            ),
            OptLabel::Language => settings.language.label().to_string(),
            OptLabel::AdvancedToggle => format!(
                "{} {}",
                lang.tr(TrKey::AdvancedSection),
                if expanded.0 { "[-]" } else { "[+]" }
            ),
        };
    }
}

fn refresh_slider_labels(
    settings: Res<UserSettings>,
    mut sliders: Query<(&SliderLabel, &mut Text)>,
) {
    let lang = settings.language;
    for (slider, mut text) in &mut sliders {
        **text = get_slider_label_text(slider.0, lang).to_string();
    }
}

fn back_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<BackButton>)>,
    actions: Res<InputActions>,
    options_return: Res<OptionsReturn>,
    mut next: ResMut<NextState<Overlay>>,
) {
    let clicked = interactions.iter().any(|i| *i == Interaction::Pressed);
    if clicked || actions.menu_back() {
        next.set(options_return.0);
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

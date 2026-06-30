use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::{BTN_HOVER, MenuFocus};
use crate::core::campaign::CampaignProgress;
use crate::gameplay::GameMode;
use crate::input::pointer::PointerInput;
use crate::input::{InputActions, LastInputDevice};
use crate::menu::options::{DeviceMode, WindowSettings};
use crate::state::GameState;
use crate::visuals::assets::VisualCache;
use crate::visuals::render_target::WorldCamera;

pub(crate) struct LevelMenuPlugin;

impl Plugin for LevelMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelMapState>()
            .add_systems(OnEnter(GameState::LevelMenu), spawn_level_menu)
            .add_systems(OnExit(GameState::LevelMenu), despawn_level_menu)
            .add_systems(
                Update,
                (
                    level_menu_nav_system,
                    map_scroll_zoom_system,
                    map_touch_zoom_system,
                    map_drag_and_select_system,
                    auto_center_selected_system,
                    clamp_map_state_system,
                    sync_map_transform_system,
                    update_connections_system,
                    update_node_sprite_system,
                    update_node_label_system,
                    update_info_panel_system,
                    update_play_button_visuals_system,
                    update_back_button_visuals_system,
                    play_button_system,
                    keyboard_launch_selected_system,
                    back_button_system,
                )
                    .run_if(in_state(GameState::LevelMenu)),
            );
    }
}

#[derive(Component)]
struct LevelMenuUiRoot;

#[derive(Component)]
struct LevelMenuWorldRoot;

#[derive(Component)]
struct BackButton;

#[derive(Component)]
struct PlayButton;

#[derive(Component, Clone, Copy)]
enum InfoTextKind {
    Title,
    Objective,
    Progress,
    Hint,
}

#[derive(Component, Clone, Copy)]
struct MapConnection {
    from: usize,
    to: usize,
}

#[derive(Component, Clone, Copy)]
enum NodeSpriteKind {
    Outline,
    Fill,
    Glow,
}

#[derive(Component, Clone, Copy)]
struct NodeSpritePart {
    index: usize,
    kind: NodeSpriteKind,
}

#[derive(Component, Clone, Copy)]
enum NodeLabelKind {
    Icon,
    Badge,
}

#[derive(Component, Clone, Copy)]
struct NodeLabelPart {
    index: usize,
    kind: NodeLabelKind,
}

#[derive(Clone, Copy)]
enum MenuEntryKind {
    Campaign(u32),
    ConsumeAll,
    Sandbox,
}

#[derive(Clone, Copy)]
struct MenuEntry {
    title: &'static str,
    blurb: &'static str,
    pos: Vec2,
    accent: [f32; 3],
    kind: MenuEntryKind,
}

#[derive(Resource)]
struct LevelMapState {
    selected: usize,
    offset: Vec2,
    zoom: f32,
    drag_anchor_window: Option<Vec2>,
    drag_last_world: Option<Vec2>,
    dragged: bool,
    pinch_distance: Option<f32>,
}

impl Default for LevelMapState {
    fn default() -> Self {
        Self {
            selected: 0,
            offset: Vec2::ZERO,
            zoom: 1.0,
            drag_anchor_window: None,
            drag_last_world: None,
            dragged: false,
            pinch_distance: None,
        }
    }
}

const MAP_NODE_SIZE: f32 = 86.0;
const MAP_NODE_FILL: f32 = 72.0;
const MAP_GLOW_SIZE: f32 = 122.0;
const MAP_PICK_RADIUS: f32 = 56.0;
const MAP_ZOOM_MIN: f32 = 0.75;
const MAP_ZOOM_MAX: f32 = 1.80;
const ENTRY_CONNECTIONS: &[(usize, usize)] = &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)];
const MENU_ENTRIES: [MenuEntry; 8] = [
    MenuEntry {
        title: "Orbita Base",
        blurb: "Entrada limpia. Aprende el ritmo y alcanza el score objetivo.",
        pos: Vec2::new(-624.0, -60.0),
        accent: [0.40, 0.80, 1.25],
        kind: MenuEntryKind::Campaign(1),
    },
    MenuEntry {
        title: "Ruta de Chispas",
        blurb: "Abre caminos y baja ingredientes sin romper la cadencia del tablero.",
        pos: Vec2::new(-248.0, 220.0),
        accent: [0.55, 1.00, 0.72],
        kind: MenuEntryKind::Campaign(2),
    },
    MenuEntry {
        title: "Nucleo Velado",
        blurb: "La presion pasa a ser espacial: importa donde cae cada explosion.",
        pos: Vec2::new(136.0, -130.0),
        accent: [1.05, 0.72, 1.20],
        kind: MenuEntryKind::Campaign(3),
    },
    MenuEntry {
        title: "Cinturon Rojo",
        blurb: "Contrarreloj. El throughput manda y el reloj no perdona.",
        pos: Vec2::new(520.0, 160.0),
        accent: [1.28, 0.54, 0.54],
        kind: MenuEntryKind::Campaign(4),
    },
    MenuEntry {
        title: "Cosecha Roja",
        blurb: "Cosecha lightcores rojos para alimentar el reactor central.",
        pos: Vec2::new(760.0, -40.0),
        accent: [0.92, 0.25, 0.30],
        kind: MenuEntryKind::Campaign(5),
    },
    MenuEntry {
        title: "Cosecha Azul",
        blurb: "Cosecha lightcores azules para estabilizar la órbita.",
        pos: Vec2::new(960.0, 180.0),
        accent: [0.25, 0.50, 0.95],
        kind: MenuEntryKind::Campaign(6),
    },
    MenuEntry {
        title: "ConsumeAll",
        blurb: "Modo especial. Vacia el tablero completo para ganar, sin campaign gating.",
        pos: Vec2::new(358.0, -10.0),
        accent: [1.24, 0.92, 0.42],
        kind: MenuEntryKind::ConsumeAll,
    },
    MenuEntry {
        title: "Sandbox",
        blurb: "Modo libre siempre disponible. Board cargado de powers para probar interacciones y VFX.",
        pos: Vec2::new(598.0, -122.0),
        accent: [0.48, 1.12, 1.16],
        kind: MenuEntryKind::Sandbox,
    },
];

fn spawn_level_menu(
    mut commands: Commands,
    progress: Res<CampaignProgress>,
    settings: Res<WindowSettings>,
    cache: Res<VisualCache>,
    mut focus: ResMut<MenuFocus>,
    mut map_state: ResMut<LevelMapState>,
) {
    let highest_index = highest_unlocked_index(&progress);
    focus.0 = highest_index;
    map_state.selected = highest_index;
    map_state.zoom = 1.0;
    map_state.offset = centered_offset(highest_index, map_state.zoom);
    map_state.drag_anchor_window = None;
    map_state.drag_last_world = None;
    map_state.dragged = false;
    map_state.pinch_distance = None;

    commands
        .spawn((
            LevelMenuWorldRoot,
            Transform::from_translation(map_state.offset.extend(0.0))
                .with_scale(Vec3::splat(map_state.zoom)),
            GlobalTransform::default(),
            Visibility::Visible,
        ))
        .with_children(|root| {
            spawn_menu_stars(root, &cache);

            for &(from, to) in ENTRY_CONNECTIONS {
                let a = entry_world_pos(from);
                let b = entry_world_pos(to);
                let delta = b - a;
                let length = delta.length();
                let angle = delta.y.atan2(delta.x);
                root.spawn((
                    MapConnection { from, to },
                    Sprite {
                        color: Color::srgba(0.33, 0.36, 0.42, 0.30),
                        custom_size: Some(Vec2::new(length, 8.0)),
                        ..default()
                    },
                    Transform::from_translation(((a + b) * 0.5).extend(-5.0))
                        .with_rotation(Quat::from_rotation_z(angle)),
                ));
            }

            for (index, entry) in MENU_ENTRIES.iter().enumerate() {
                root.spawn((
                    Transform::from_translation(entry_world_pos(index).extend(0.0)),
                    GlobalTransform::default(),
                    Visibility::Visible,
                ))
                .with_children(|node_root| {
                    node_root.spawn((
                        NodeSpritePart {
                            index,
                            kind: NodeSpriteKind::Glow,
                        },
                        Sprite {
                            image: cache.glow_image.clone(),
                            color: Color::srgba(0.4, 0.8, 1.0, 0.0),
                            custom_size: Some(Vec2::splat(MAP_GLOW_SIZE)),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, -1.0),
                    ));
                    node_root.spawn((
                        NodeSpritePart {
                            index,
                            kind: NodeSpriteKind::Outline,
                        },
                        Sprite {
                            color: Color::srgb(0.86, 0.88, 0.92),
                            custom_size: Some(Vec2::splat(MAP_NODE_SIZE)),
                            ..default()
                        },
                    ));
                    node_root.spawn((
                        NodeSpritePart {
                            index,
                            kind: NodeSpriteKind::Fill,
                        },
                        Sprite {
                            color: Color::srgba(
                                entry.accent[0],
                                entry.accent[1],
                                entry.accent[2],
                                0.20,
                            ),
                            custom_size: Some(Vec2::splat(MAP_NODE_FILL)),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, 0.2),
                    ));
                    node_root.spawn((
                        NodeLabelPart {
                            index,
                            kind: NodeLabelKind::Icon,
                        },
                        Text2d::new(node_icon_text(index)),
                        TextFont {
                            font_size: FontSize::Px(26.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        Transform::from_xyz(0.0, -2.0, 1.0),
                    ));
                    node_root.spawn((
                        NodeLabelPart {
                            index,
                            kind: NodeLabelKind::Badge,
                        },
                        Text2d::new(""),
                        TextFont {
                            font_size: FontSize::Px(34.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.82, 0.90, 1.0)),
                        Transform::from_xyz(34.0, 34.0, 1.0),
                    ));
                });
            }
        });

    let panel_width = match settings.device_mode {
        DeviceMode::Mobile => Val::Percent(92.0),
        DeviceMode::Desktop => Val::Px(360.0),
    };
    let panel_left = match settings.device_mode {
        DeviceMode::Mobile => Val::Percent(4.0),
        DeviceMode::Desktop => Val::Px(24.0),
    };

    commands
        .spawn((
            LevelMenuUiRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: panel_left,
                    right: if settings.device_mode == DeviceMode::Mobile {
                        Val::Percent(4.0)
                    } else {
                        Val::Auto
                    },
                    bottom: Val::Px(20.0),
                    width: panel_width,
                    padding: UiRect::all(Val::Px(18.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.03, 0.05, 0.08, 0.88)),
                BorderColor::all(Color::srgb(0.74, 0.78, 0.88)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    InfoTextKind::Title,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(28.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.96, 0.98, 1.0)),
                ));
                panel.spawn((
                    InfoTextKind::Objective,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(18.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.78, 0.88)),
                ));
                panel.spawn((
                    InfoTextKind::Progress,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.62, 0.68, 0.78)),
                ));
                panel
                    .spawn((
                        Button,
                        PlayButton,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(46.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.0)),
                            margin: UiRect::top(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.11, 0.18, 0.28, 0.96)),
                        BorderColor::all(Color::srgb(0.42, 0.58, 0.88)),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new("Jugar"),
                            TextFont {
                                font_size: FontSize::Px(22.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
            });

            root.spawn((
                Button,
                BackButton,
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(22.0),
                    bottom: Val::Px(20.0),
                    width: Val::Px(92.0),
                    height: Val::Px(92.0),
                    border: UiRect::all(Val::Px(1.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.72, 0.72, 0.76, 0.80)),
                BorderColor::all(Color::srgb(0.94, 0.94, 0.98)),
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new("<-"),
                    TextFont {
                        font_size: FontSize::Px(32.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.22, 0.22, 0.26)),
                ));
            });

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(18.0),
                    left: Val::Px(20.0),
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.74)),
                BorderColor::all(Color::srgb(0.44, 0.48, 0.56)),
            ))
            .with_children(|hint| {
                hint.spawn((
                    InfoTextKind::Hint,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.90, 0.96)),
                ));
            });
        });
}

fn spawn_menu_stars(root: &mut ChildSpawnerCommands, cache: &VisualCache) {
    let mut rng = StdRng::seed_from_u64(0x71EC_19A0);
    for _ in 0..110 {
        let x = rng.random_range(-760.0..760.0);
        let y = rng.random_range(-440.0..440.0);
        let bright = rng.random::<f32>() > 0.92;
        let size = if bright {
            rng.random_range(5.0..7.0)
        } else {
            rng.random_range(2.0..4.0)
        };
        let intensity = if bright { 0.9 } else { 0.45 };
        root.spawn((
            Sprite {
                image: cache.glow_image.clone(),
                color: Color::srgba(0.86, 0.90, 1.0, intensity),
                custom_size: Some(Vec2::splat(size)),
                ..default()
            },
            Transform::from_xyz(x, y, -20.0),
        ));
    }
}

fn level_menu_nav_system(
    actions: Res<InputActions>,
    progress: Res<CampaignProgress>,
    mut focus: ResMut<MenuFocus>,
    mut map_state: ResMut<LevelMapState>,
) {
    let highest_unlocked = highest_unlocked_index(&progress);
    if focus.0 >= MENU_ENTRIES.len() {
        focus.0 = highest_unlocked;
    }

    let dir = if actions.up {
        Some(Vec2::new(0.0, 1.0))
    } else if actions.down {
        Some(Vec2::new(0.0, -1.0))
    } else if actions.left {
        Some(Vec2::new(-1.0, 0.0))
    } else if actions.right {
        Some(Vec2::new(1.0, 0.0))
    } else {
        None
    };

    let Some(dir) = dir else {
        return;
    };

    let current = entry_world_pos(focus.0);
    let mut best_candidate = None;
    let mut best_score = f32::MAX;
    for index in 0..MENU_ENTRIES.len() {
        if index == focus.0 {
            continue;
        }
        if !entry_is_unlocked(index, &progress) {
            continue;
        }
        let delta = entry_world_pos(index) - current;
        let len = delta.length();
        if len <= f32::EPSILON {
            continue;
        }
        let alignment = delta.normalize().dot(dir);
        if alignment <= 0.20 {
            continue;
        }
        let heuristic = len * (2.0 - alignment);
        if heuristic < best_score {
            best_score = heuristic;
            best_candidate = Some(index);
        }
    }

    if let Some(index) = best_candidate {
        focus.0 = index;
        map_state.selected = index;
    }
}

fn map_scroll_zoom_system(
    mut wheel: MessageReader<MouseWheel>,
    mut map_state: ResMut<LevelMapState>,
    last_input: Res<LastInputDevice>,
) {
    if *last_input == LastInputDevice::Touch {
        return;
    }
    for event in wheel.read() {
        let delta = match event.unit {
            MouseScrollUnit::Line => event.y * 0.08,
            MouseScrollUnit::Pixel => event.y * 0.0025,
        };
        map_state.zoom = (map_state.zoom * (1.0 + delta)).clamp(MAP_ZOOM_MIN, MAP_ZOOM_MAX);
    }
}

fn map_touch_zoom_system(
    touches: Res<bevy::input::touch::Touches>,
    mut map_state: ResMut<LevelMapState>,
) {
    let mut active: Vec<_> = touches.iter().collect();
    active.sort_by_key(|touch| touch.id());
    if active.len() < 2 {
        map_state.pinch_distance = None;
        return;
    }
    let dist = active[0].position().distance(active[1].position());
    if let Some(last) = map_state.pinch_distance
        && last > 0.0
    {
        map_state.zoom = (map_state.zoom * (dist / last)).clamp(MAP_ZOOM_MIN, MAP_ZOOM_MAX);
    }
    map_state.pinch_distance = Some(dist);
    map_state.drag_anchor_window = None;
    map_state.drag_last_world = None;
    map_state.dragged = true;
}

fn map_drag_and_select_system(
    progress: Res<CampaignProgress>,
    pointer: Res<PointerInput>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    map_root: Single<&Transform, With<LevelMenuWorldRoot>>,
    mut map_state: ResMut<LevelMapState>,
    mut focus: ResMut<MenuFocus>,
) {
    let active_touches =
        pointer.source == crate::input::pointer::PointerSource::Touch && pointer.held;
    let Some(window_pos) = pointer.position_window else {
        return;
    };
    let Some(world_pos) = pointer.position_world else {
        return;
    };
    let _ = camera;

    if pointer.just_pressed {
        map_state.drag_anchor_window = Some(window_pos);
        map_state.drag_last_world = Some(world_pos);
        map_state.dragged = false;
    }

    if pointer.held
        && map_state.pinch_distance.is_none()
        && let (Some(anchor), Some(last_world)) =
            (map_state.drag_anchor_window, map_state.drag_last_world)
    {
        if window_pos.distance(anchor) > 8.0 || active_touches {
            map_state.dragged = true;
        }
        if map_state.dragged {
            map_state.offset += world_pos - last_world;
            map_state.drag_last_world = Some(world_pos);
        }
    }

    if pointer.just_released {
        if !map_state.dragged {
            let local =
                world_to_map_local(world_pos, map_root.translation.truncate(), map_root.scale.x);
            if let Some(index) = hit_test_node(local, &progress) {
                map_state.selected = index;
                focus.0 = index;
            }
        }
        map_state.drag_anchor_window = None;
        map_state.drag_last_world = None;
        map_state.dragged = false;
    }
}

fn auto_center_selected_system(
    last_input: Res<LastInputDevice>,
    pointer: Res<PointerInput>,
    mut map_state: ResMut<LevelMapState>,
) {
    if *last_input != LastInputDevice::Cursor || pointer.held || map_state.dragged {
        return;
    }
    let target = centered_offset(map_state.selected, map_state.zoom);
    map_state.offset = map_state.offset.lerp(target, 0.22);
}

fn clamp_map_state_system(mut map_state: ResMut<LevelMapState>) {
    let (min, max) = map_bounds();
    let margin = Vec2::new(220.0, 180.0) * map_state.zoom;
    map_state.offset.x = map_state
        .offset
        .x
        .clamp(-max.x - margin.x, -min.x + margin.x);
    map_state.offset.y = map_state
        .offset
        .y
        .clamp(-max.y - margin.y, -min.y + margin.y);
}

fn sync_map_transform_system(
    map_state: Res<LevelMapState>,
    mut root: Single<&mut Transform, With<LevelMenuWorldRoot>>,
) {
    root.translation.x = map_state.offset.x;
    root.translation.y = map_state.offset.y;
    root.scale = Vec3::splat(map_state.zoom);
}

fn update_connections_system(
    progress: Res<CampaignProgress>,
    selection: Res<LevelMapState>,
    mut connections: Query<(&MapConnection, &mut Sprite)>,
) {
    for (connection, mut sprite) in &mut connections {
        let fully_open = entry_is_unlocked(connection.to, &progress);
        let touching_selected =
            connection.from == selection.selected || connection.to == selection.selected;
        sprite.color = if fully_open {
            Color::srgba(
                0.74,
                0.82,
                0.95,
                if touching_selected { 0.52 } else { 0.34 },
            )
        } else {
            Color::srgba(
                0.34,
                0.36,
                0.42,
                if touching_selected { 0.34 } else { 0.22 },
            )
        };
    }
}

fn update_node_sprite_system(
    progress: Res<CampaignProgress>,
    map_state: Res<LevelMapState>,
    mut sprites: Query<(&NodeSpritePart, &mut Sprite)>,
) {
    for (part, mut sprite) in &mut sprites {
        let entry = &MENU_ENTRIES[part.index];
        let unlocked = entry_is_unlocked(part.index, &progress);
        let selected = map_state.selected == part.index;
        let completed = entry_is_completed(part.index, &progress);
        match part.kind {
            NodeSpriteKind::Outline => {
                sprite.custom_size = Some(Vec2::splat(if selected {
                    MAP_NODE_SIZE + 8.0
                } else {
                    MAP_NODE_SIZE
                }));
                sprite.color = if !unlocked {
                    Color::srgba(0.52, 0.52, 0.56, 0.28)
                } else if selected {
                    Color::srgb(0.96, 0.97, 1.0)
                } else if completed {
                    Color::srgb(0.74, 0.90, 0.88)
                } else {
                    Color::srgb(0.86, 0.88, 0.92)
                };
            }
            NodeSpriteKind::Fill => {
                let accent = Color::srgba(entry.accent[0], entry.accent[1], entry.accent[2], 0.18);
                sprite.color = if !unlocked {
                    Color::srgba(0.12, 0.13, 0.16, 0.74)
                } else if completed {
                    Color::srgba(0.32, 0.54, 0.46, if selected { 0.62 } else { 0.44 })
                } else if selected {
                    Color::srgba(entry.accent[0], entry.accent[1], entry.accent[2], 0.42)
                } else {
                    accent
                };
            }
            NodeSpriteKind::Glow => {
                sprite.color = if !unlocked {
                    Color::srgba(0.0, 0.0, 0.0, 0.0)
                } else if selected {
                    Color::srgba(entry.accent[0], entry.accent[1], entry.accent[2], 0.24)
                } else {
                    Color::srgba(entry.accent[0], entry.accent[1], entry.accent[2], 0.08)
                };
            }
        }
    }
}

fn update_node_label_system(
    progress: Res<CampaignProgress>,
    map_state: Res<LevelMapState>,
    mut labels: Query<(&NodeLabelPart, &mut Text2d, &mut TextColor)>,
) {
    for (part, mut text, mut color) in &mut labels {
        let unlocked = entry_is_unlocked(part.index, &progress);
        let best = entry_best_score(part.index, &progress);
        match part.kind {
            NodeLabelKind::Icon => {
                text.0 = node_icon_text(part.index).to_string();
                color.0 = if !unlocked {
                    Color::srgba(0.54, 0.54, 0.58, 0.60)
                } else if map_state.selected == part.index {
                    Color::srgb(0.98, 1.0, 1.0)
                } else {
                    Color::srgb(0.90, 0.94, 0.98)
                };
            }
            NodeLabelKind::Badge => {
                text.0 = node_badge_text(part.index, best, unlocked);
                color.0 = if !unlocked {
                    Color::srgba(0.46, 0.46, 0.50, 0.0)
                } else if best > 0 {
                    Color::srgb(0.74, 0.90, 1.0)
                } else {
                    Color::srgb(0.56, 0.64, 0.80)
                };
            }
        }
    }
}

fn update_info_panel_system(
    progress: Res<CampaignProgress>,
    settings: Res<WindowSettings>,
    map_state: Res<LevelMapState>,
    mut texts: Query<(&InfoTextKind, &mut Text)>,
) {
    let entry = &MENU_ENTRIES[map_state.selected];
    let best = entry_best_score(map_state.selected, &progress);
    let unlocked = entry_is_unlocked(map_state.selected, &progress);
    let progress_line = if unlocked {
        if let Some(level) = entry_level(map_state.selected) {
            if best > 0 {
                format!("Mejor score: {}  ·  {}", best, grade_summary(level, best))
            } else {
                "Disponible para jugar".to_string()
            }
        } else {
            "Modo especial · disponible siempre".to_string()
        }
    } else {
        let prev = entry_level(map_state.selected.saturating_sub(1)).unwrap_or(1);
        format!("Bloqueado · completa Nivel {:02} para desbloquear", prev)
    };
    let hint = match settings.device_mode {
        DeviceMode::Mobile => "Desliza para mover · pellizca para zoom".to_string(),
        DeviceMode::Desktop => "Arrastra para mover · rueda para zoom".to_string(),
    };

    for (kind, mut text) in &mut texts {
        **text = match kind {
            InfoTextKind::Title => info_title_text(map_state.selected),
            InfoTextKind::Objective => entry.blurb.to_string(),
            InfoTextKind::Progress => progress_line.clone(),
            InfoTextKind::Hint => hint.clone(),
        };
    }
}

fn update_play_button_visuals_system(
    progress: Res<CampaignProgress>,
    map_state: Res<LevelMapState>,
    mut play_button: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        With<PlayButton>,
    >,
) {
    if let Ok((interaction, mut bg, mut border)) = play_button.single_mut() {
        let unlocked = entry_is_unlocked(map_state.selected, &progress);
        let hovered = matches!(*interaction, Interaction::Hovered | Interaction::Pressed);
        bg.0 = if !unlocked {
            Color::srgba(0.16, 0.16, 0.18, 0.72)
        } else if hovered {
            BTN_HOVER
        } else {
            Color::srgba(0.11, 0.18, 0.28, 0.96)
        };
        *border = BorderColor::all(if unlocked {
            Color::srgb(0.42, 0.58, 0.88)
        } else {
            Color::srgb(0.24, 0.24, 0.28)
        });
    }
}

fn update_back_button_visuals_system(
    mut back_button: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        With<BackButton>,
    >,
) {
    if let Ok((interaction, mut bg, mut border)) = back_button.single_mut() {
        let hovered = matches!(*interaction, Interaction::Hovered | Interaction::Pressed);
        bg.0 = if hovered {
            Color::srgba(0.86, 0.86, 0.90, 0.94)
        } else {
            Color::srgba(0.72, 0.72, 0.76, 0.80)
        };
        *border = BorderColor::all(if hovered {
            Color::WHITE
        } else {
            Color::srgb(0.94, 0.94, 0.98)
        });
    }
}

fn play_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<PlayButton>)>,
    progress: Res<CampaignProgress>,
    map_state: Res<LevelMapState>,
    mut mode: ResMut<GameMode>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !entry_is_unlocked(map_state.selected, &progress) {
        return;
    }
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            *mode = entry_mode(map_state.selected);
            next.set(GameState::Loading);
        }
    }
}

fn keyboard_launch_selected_system(
    actions: Res<InputActions>,
    progress: Res<CampaignProgress>,
    map_state: Res<LevelMapState>,
    mut mode: ResMut<GameMode>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !actions.confirm {
        return;
    }
    if entry_is_unlocked(map_state.selected, &progress) {
        *mode = entry_mode(map_state.selected);
        next.set(GameState::Loading);
    }
}

fn back_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<BackButton>)>,
    actions: Res<InputActions>,
    mut next: ResMut<NextState<GameState>>,
) {
    let clicked = interactions
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed);
    if clicked || actions.menu_back() {
        next.set(GameState::MainMenu);
    }
}

fn despawn_level_menu(
    mut commands: Commands,
    ui_root: Query<Entity, With<LevelMenuUiRoot>>,
    world_root: Query<Entity, With<LevelMenuWorldRoot>>,
    mut map_state: ResMut<LevelMapState>,
) {
    for entity in &ui_root {
        commands.entity(entity).try_despawn();
    }
    for entity in &world_root {
        commands.entity(entity).try_despawn();
    }
    map_state.drag_anchor_window = None;
    map_state.drag_last_world = None;
    map_state.dragged = false;
    map_state.pinch_distance = None;
}

fn highest_unlocked_index(progress: &CampaignProgress) -> usize {
    MENU_ENTRIES
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches!(entry.kind, MenuEntryKind::Campaign(_)))
        .rev()
        .find_map(|(index, _)| entry_is_unlocked(index, progress).then_some(index))
        .unwrap_or(0)
}

fn entry_world_pos(index: usize) -> Vec2 {
    MENU_ENTRIES[index].pos
}

fn centered_offset(index: usize, zoom: f32) -> Vec2 {
    -entry_world_pos(index) * zoom
}

fn world_to_map_local(world: Vec2, offset: Vec2, zoom: f32) -> Vec2 {
    (world - offset) / zoom.max(0.001)
}

fn hit_test_node(local: Vec2, progress: &CampaignProgress) -> Option<usize> {
    let mut best = None;
    let mut best_dist = f32::MAX;
    for index in 0..MENU_ENTRIES.len() {
        let dist = entry_world_pos(index).distance(local);
        if dist <= MAP_PICK_RADIUS && dist < best_dist {
            best_dist = dist;
            best = Some(index);
        }
        let _ = progress;
    }
    best
}

fn map_bounds() -> (Vec2, Vec2) {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for index in 0..MENU_ENTRIES.len() {
        let p = entry_world_pos(index);
        min = min.min(p);
        max = max.max(p);
    }
    (min, max)
}

fn node_icon_text(index: usize) -> &'static str {
    match MENU_ENTRIES[index].kind {
        MenuEntryKind::Campaign(1) => "CL",
        MenuEntryKind::Campaign(2) => "SP",
        MenuEntryKind::Campaign(3) => "JL",
        MenuEntryKind::Campaign(4) => "TT",
        MenuEntryKind::ConsumeAll => "CA",
        MenuEntryKind::Sandbox => "SB",
        _ => "LV",
    }
}

fn node_badge_text(level: usize, best: u32, unlocked: bool) -> String {
    if !unlocked {
        return String::new();
    }
    if entry_level(level).is_none() {
        return "MOD".to_string();
    }
    if best == 0 {
        return format!("{:02}", entry_level(level).unwrap_or_default());
    }
    grade_summary(entry_level(level).unwrap_or_default(), best)
}

fn entry_level(index: usize) -> Option<u32> {
    match MENU_ENTRIES[index].kind {
        MenuEntryKind::Campaign(level) => Some(level),
        MenuEntryKind::ConsumeAll | MenuEntryKind::Sandbox => None,
    }
}

fn entry_mode(index: usize) -> GameMode {
    match MENU_ENTRIES[index].kind {
        MenuEntryKind::Campaign(level) => GameMode::Classic(level),
        MenuEntryKind::ConsumeAll => GameMode::ConsumeAll,
        MenuEntryKind::Sandbox => GameMode::Sandbox,
    }
}

fn entry_best_score(index: usize, progress: &CampaignProgress) -> u32 {
    entry_level(index)
        .map(|level| progress.best_score(level))
        .unwrap_or(0)
}

fn entry_is_completed(index: usize, progress: &CampaignProgress) -> bool {
    entry_best_score(index, progress) > 0
}

fn entry_is_unlocked(index: usize, progress: &CampaignProgress) -> bool {
    match MENU_ENTRIES[index].kind {
        MenuEntryKind::Campaign(level) => progress.is_unlocked(level),
        MenuEntryKind::ConsumeAll | MenuEntryKind::Sandbox => true,
    }
}

fn info_title_text(index: usize) -> String {
    match MENU_ENTRIES[index].kind {
        MenuEntryKind::Campaign(level) => {
            format!("Nivel {:02} · {}", level, MENU_ENTRIES[index].title)
        }
        MenuEntryKind::ConsumeAll | MenuEntryKind::Sandbox => MENU_ENTRIES[index].title.to_string(),
    }
}

fn grade_summary(level: u32, best: u32) -> String {
    let config = crate::core::level::make_level(level);
    let baseline = config.grade_baseline;
    if best >= baseline + 180 {
        "S".to_string()
    } else if best >= baseline + 80 {
        "A".to_string()
    } else if best >= baseline {
        "B".to_string()
    } else {
        "C".to_string()
    }
}

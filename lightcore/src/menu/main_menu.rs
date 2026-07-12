use bevy::prelude::*;

use super::{BTN_IDLE, MenuActivated, MenuButton, OptionsReturn, activated, button_hover_system};
use crate::core::locale::TrKey;
use crate::menu::options::{DeviceMode, WindowSettings};
use crate::state::GameState;

/// Title screen — the app boots here. Only "Jugar" and "Opciones". Persistence (continuing an
/// active run, seeing it dynamically labeled "Continuar run", or abandoning/restarting it) lives
/// entirely inside `LevelMenu` — the ONE unified map — not duplicated here. This screen used to
/// also offer "Continuar"/"Nuevo run"/"Modo Debug" as separate buttons, which just meant several
/// different ways to reach the same place instead of one clear entry point.
pub(crate) struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GameState::MainMenu), despawn_main_menu)
            .add_systems(
                Update,
                (
                    button_hover_system,
                    nav_button_system,
                    quit_button_system,
                    main_menu_tutorial_button_system,
                    update_main_menu_tutorial_text,
                )
                    .run_if(in_state(GameState::MainMenu)),
            );
    }
}

#[derive(Component)]
struct MainMenuRoot;

#[derive(Component, Clone)]
struct NavButton(GameState);

#[derive(Component)]
struct QuitButton;

fn spawn_main_menu(mut commands: Commands, settings: Res<WindowSettings>) {
    let lang = settings.language;
    let compact = settings.device_mode == DeviceMode::Mobile;
    let desktop = settings.device_mode == DeviceMode::Desktop;
    let row_gap = if compact { 10.0 } else { 26.0 };
    let title_font = if compact { 52.0 } else { 64.0 };
    let button_width = if compact { 260.0 } else { 280.0 };
    let button_height = if compact { 54.0 } else { 66.0 };
    let button_font = if compact { 24.0 } else { 30.0 };
    // The tutorial toggle is a small chip, not a full nav button — it's a minor settings flip,
    // not a primary menu action, and shouldn't compete visually with Jugar/Opciones.
    let tutorial_width = button_width * 0.6;
    let tutorial_height = button_height * 0.6;
    let tutorial_font = if compact { 15.0 } else { 17.0 };
    commands
        .spawn((
            MainMenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(row_gap),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("LIGHTCORE"),
                TextFont {
                    font_size: FontSize::Px(title_font),
                    ..default()
                },
                TextColor(Color::srgb(1.6, 1.8, 2.6)), // HDR → blooms
            ));
            let mut index = 0;
            for (label, target) in [
                (lang.tr(TrKey::Play), GameState::LevelMenu),
                (lang.tr(TrKey::Options), GameState::Options),
            ] {
                root.spawn((
                    Button,
                    NavButton(target),
                    MenuButton { index },
                    Node {
                        width: Val::Px(button_width),
                        height: Val::Px(button_height),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BTN_IDLE),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(label),
                        TextFont {
                            font_size: FontSize::Px(button_font),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
                index += 1;
            }

            // Tutorial toggle — compact chip, see `tutorial_width`/`tutorial_height` above.
            root.spawn((
                Button,
                MainMenuTutorialButton,
                MenuButton { index },
                Node {
                    width: Val::Px(tutorial_width),
                    height: Val::Px(tutorial_height),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(BTN_IDLE),
            ))
            .with_children(|b| {
                b.spawn((
                    MainMenuTutorialText,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(tutorial_font),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
            index += 1;

            // Desktop only — quitting isn't a meaningful action on mobile (there's no "close the
            // app" gesture players expect from inside it; the OS/home-button owns that).
            if desktop {
                root.spawn((
                    Button,
                    QuitButton,
                    MenuButton { index },
                    Node {
                        width: Val::Px(button_width),
                        height: Val::Px(button_height),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BTN_IDLE),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(lang.tr(TrKey::Quit)),
                        TextFont {
                            font_size: FontSize::Px(button_font),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }
        });
}

fn nav_button_system(
    interactions: Query<(Entity, Ref<Interaction>, &NavButton)>,
    menu_activated: Res<MenuActivated>,
    mut next: ResMut<NextState<GameState>>,
    mut options_return: ResMut<OptionsReturn>,
) {
    for (entity, interaction, btn) in &interactions {
        if activated(&interaction, entity, &menu_activated) {
            // Options opened from the title returns to the title (the paused match sets its own).
            if btn.0 == GameState::Options {
                options_return.0 = GameState::MainMenu;
            }
            next.set(btn.0.clone());
        }
    }
}

fn quit_button_system(
    interactions: Query<(Entity, Ref<Interaction>), With<QuitButton>>,
    menu_activated: Res<MenuActivated>,
    keys: Res<ButtonInput<KeyCode>>,
    // Bevy 0.19: buffered events are "messages" — `AppExit` derives `Message`, sent via MessageWriter.
    mut exit: MessageWriter<AppExit>,
) {
    let pressed_quit = interactions
        .iter()
        .any(|(e, i)| activated(&i, e, &menu_activated));

    let pressed_back = keys.just_pressed(KeyCode::Escape);

    if pressed_quit || pressed_back {
        exit.write(AppExit::Success);
    }
}

fn despawn_main_menu(mut commands: Commands, q: Query<Entity, With<MainMenuRoot>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

#[derive(Component)]
struct MainMenuTutorialButton;

#[derive(Component)]
struct MainMenuTutorialText;

fn main_menu_tutorial_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<MainMenuTutorialButton>)>,
    mut settings: ResMut<WindowSettings>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            settings.tutorial_enabled = !settings.tutorial_enabled;
        }
    }
}

fn update_main_menu_tutorial_text(
    settings: Res<WindowSettings>,
    mut q: Query<&mut Text, With<MainMenuTutorialText>>,
) {
    let lang = settings.language;
    if let Ok(mut text) = q.single_mut() {
        if settings.tutorial_enabled {
            text.0 = lang.tr(TrKey::TutorialOn).to_string();
        } else {
            text.0 = lang.tr(TrKey::TutorialOff).to_string();
        }
    }
}

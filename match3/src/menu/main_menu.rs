use bevy::prelude::*;

use super::{BTN_IDLE, MenuActivated, MenuButton, OptionsReturn, activated, button_hover_system};
use crate::core::campaign::CampaignProgress;
use crate::core::locale::TrKey;
use crate::core::run::RunState;
use crate::gameplay::{CoreReserve, CoresSpent, GameMode};
use crate::menu::options::WindowSettings;
use crate::state::GameState;

/// Title screen — the app boots here. "Jugar" → `LevelMenu`, "Opciones" → `Options`.
pub(crate) struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GameState::MainMenu), despawn_main_menu)
            .add_systems(
                Update,
                (
                    button_hover_system,
                    run_button_system,
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

#[derive(Component, Clone, Copy)]
enum RunMenuButton {
    Continue,
    New,
    Debug,
}

#[derive(Component)]
struct QuitButton;

fn spawn_main_menu(mut commands: Commands, run: Res<RunState>, settings: Res<WindowSettings>) {
    let lang = settings.language;
    commands
        .spawn((
            MainMenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(26.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("LIGHTCORE"),
                TextFont {
                    font_size: FontSize::Px(64.0),
                    ..default()
                },
                TextColor(Color::srgb(1.6, 1.8, 2.6)), // HDR → blooms
            ));
            let mut index = 0;
            if run.active {
                spawn_run_button(
                    root,
                    index,
                    lang.tr(TrKey::Continue),
                    RunMenuButton::Continue,
                );
                index += 1;
            }
            spawn_run_button(root, index, lang.tr(TrKey::NewRun), RunMenuButton::New);
            index += 1;

            spawn_run_button(root, index, lang.tr(TrKey::DebugMode), RunMenuButton::Debug);
            index += 1;

            for (label, target) in [
                (lang.tr(TrKey::Levels), GameState::LevelMenu),
                (lang.tr(TrKey::Options), GameState::Options),
            ] {
                root.spawn((
                    Button,
                    NavButton(target),
                    MenuButton { index },
                    Node {
                        width: Val::Px(280.0),
                        height: Val::Px(66.0),
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
                            font_size: FontSize::Px(30.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
                index += 1;
            }
            // Botón de Tutorial en el Main Menu (Checkbox / Toggle)
            root.spawn((
                Button,
                MainMenuTutorialButton,
                MenuButton { index },
                Node {
                    width: Val::Px(280.0),
                    height: Val::Px(66.0),
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
                        font_size: FontSize::Px(28.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
            index += 1;

            root.spawn((
                Button,
                QuitButton,
                MenuButton { index },
                Node {
                    width: Val::Px(280.0),
                    height: Val::Px(66.0),
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
                        font_size: FontSize::Px(30.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn spawn_run_button(
    root: &mut ChildSpawnerCommands,
    index: usize,
    label: &str,
    action: RunMenuButton,
) {
    root.spawn((
        Button,
        action,
        MenuButton { index },
        Node {
            width: Val::Px(280.0),
            height: Val::Px(66.0),
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
                font_size: FontSize::Px(30.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ));
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

fn run_button_system(
    interactions: Query<(Entity, Ref<Interaction>, &RunMenuButton)>,
    menu_activated: Res<MenuActivated>,
    mut run: ResMut<RunState>,
    mut reserve: ResMut<CoreReserve>,
    mut spent: ResMut<CoresSpent>,
    mut mode: ResMut<GameMode>,
    mut progress: ResMut<CampaignProgress>,
    mut next: ResMut<NextState<GameState>>,
) {
    for (entity, interaction, action) in &interactions {
        if !activated(&interaction, entity, &menu_activated) {
            continue;
        }
        match action {
            RunMenuButton::Continue if run.active => {
                *mode = GameMode::Run(run.depth);
                next.set(GameState::Loading);
            }
            RunMenuButton::New => {
                run.abandon();
                reserve.0 = 0;
                spent.0 = 0;
                *progress = CampaignProgress::default();
                *mode = GameMode::Run(1);
                next.set(GameState::Loading);
            }
            RunMenuButton::Debug => {
                run.abandon();
                reserve.0 = 0;
                spent.0 = 0;
                progress.unlock_all();
                next.set(GameState::LevelMenu);
            }
            RunMenuButton::Continue => {}
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

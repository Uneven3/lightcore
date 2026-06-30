use bevy::prelude::*;

use super::{BTN_IDLE, MenuActivated, MenuButton, OptionsReturn, activated, button_hover_system};
use crate::state::GameState;

/// Title screen — the app boots here. "Jugar" → `LevelMenu`, "Opciones" → `Options`.
pub(crate) struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GameState::MainMenu), despawn_main_menu)
            .add_systems(
                Update,
                (button_hover_system, nav_button_system, quit_button_system)
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

fn spawn_main_menu(mut commands: Commands) {
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
            for (index, (label, target)) in [
                ("Jugar", GameState::LevelMenu),
                ("Opciones", GameState::Options),
            ]
            .into_iter()
            .enumerate()
            {
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
            }
            root.spawn((
                Button,
                QuitButton,
                MenuButton { index: 2 },
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
                    Text::new("Salir"),
                    TextFont {
                        font_size: FontSize::Px(30.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
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

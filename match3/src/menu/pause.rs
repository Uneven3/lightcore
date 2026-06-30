use bevy::prelude::*;

use super::{BTN_IDLE, MenuActivated, MenuButton, OptionsReturn, activated, button_hover_system};
use crate::input::InputActions;
use crate::state::GameState;

/// In-match pause overlay. `pause` (Esc / Start) from `Playing` opens it; the board is left intact
/// behind a dimming panel, so Options can be tuned with the board visible. Buttons: Reanudar
/// (→ `Playing`), Opciones (→ `Options`, returning here), Salir al menú (→ `LevelMenu`, which tears
/// the match down).
pub(crate) struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Paused), spawn_pause)
            .add_systems(OnExit(GameState::Paused), despawn_pause)
            .add_systems(Update, open_pause.run_if(in_state(GameState::Playing)))
            .add_systems(
                Update,
                (button_hover_system, pause_button_system, close_pause)
                    .run_if(in_state(GameState::Paused)),
            );
    }
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy)]
enum PauseButton {
    Resume,
    Options,
    Quit,
}

fn open_pause(actions: Res<InputActions>, mut next: ResMut<NextState<GameState>>) {
    if actions.pause {
        next.set(GameState::Paused);
    }
}

fn close_pause(actions: Res<InputActions>, mut next: ResMut<NextState<GameState>>) {
    // Esc/Start (pause) or B/Backspace (cancel) both resume.
    if actions.pause || actions.cancel {
        next.set(GameState::Playing);
    }
}

fn spawn_pause(mut commands: Commands) {
    commands
        .spawn((
            PauseRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(18.0),
                ..default()
            },
            // Dim the live board behind the overlay without hiding it.
            BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.6)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Pausa"),
                TextFont {
                    font_size: FontSize::Px(52.0),
                    ..default()
                },
                TextColor(Color::srgb(1.5, 1.7, 2.4)), // HDR → blooms
            ));
            for (index, (label, action)) in [
                ("Reanudar", PauseButton::Resume),
                ("Opciones", PauseButton::Options),
                ("Salir al menu", PauseButton::Quit),
            ]
            .into_iter()
            .enumerate()
            {
                root.spawn((
                    Button,
                    action,
                    MenuButton { index },
                    Node {
                        width: Val::Px(260.0),
                        height: Val::Px(56.0),
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
                            font_size: FontSize::Px(26.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }
        });
}

fn pause_button_system(
    interactions: Query<(Entity, Ref<Interaction>, &PauseButton)>,
    menu_activated: Res<MenuActivated>,
    mut next: ResMut<NextState<GameState>>,
    mut options_return: ResMut<OptionsReturn>,
) {
    for (entity, interaction, btn) in &interactions {
        if activated(&interaction, entity, &menu_activated) {
            match btn {
                PauseButton::Resume => next.set(GameState::Playing),
                PauseButton::Options => {
                    options_return.0 = GameState::Paused;
                    next.set(GameState::Options);
                }
                PauseButton::Quit => next.set(GameState::LevelMenu),
            }
        }
    }
}

fn despawn_pause(mut commands: Commands, q: Query<Entity, With<PauseRoot>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

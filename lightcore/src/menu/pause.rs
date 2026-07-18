use bevy::prelude::*;

use super::{BTN_IDLE, MenuActivated, MenuButton, OptionsReturn, activated, button_hover_system};
use crate::core::locale::TrKey;
use crate::input::InputActions;
use crate::menu::options::WindowSettings;
use crate::state::{MatchPhase, Overlay, Screen};

/// In-match pause overlay. `pause` (Esc / Start) from `Playing` opens it; the board is left intact
/// behind a dimming panel, so Options can be tuned with the board visible. Buttons: Reanudar
/// (→ `Playing`), Opciones (→ `Options`, returning here), Salir al menú (→ `LevelMenu`, which tears
/// the match down).
pub(crate) struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Overlay::Paused), spawn_pause)
            .add_systems(OnExit(Overlay::Paused), despawn_pause)
            .add_systems(
                Update,
                open_pause.run_if(in_state(MatchPhase::Playing).and_then(in_state(Overlay::None))),
            )
            .add_systems(
                Update,
                (button_hover_system, pause_button_system, close_pause)
                    .run_if(in_state(Overlay::Paused)),
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

fn open_pause(actions: Res<InputActions>, mut next: ResMut<NextState<Overlay>>) {
    if actions.pause {
        next.set(Overlay::Paused);
    }
}

fn close_pause(actions: Res<InputActions>, mut next: ResMut<NextState<Overlay>>) {
    // Esc/Start (pause) or B/Backspace (cancel) both resume.
    if actions.pause || actions.cancel {
        next.set(Overlay::None);
    }
}

fn spawn_pause(
    mut commands: Commands,
    settings: Res<WindowSettings>,
    asset_server: Res<AssetServer>,
) {
    let lang = settings.language;
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
                Text::new(lang.tr(TrKey::PauseTitle)),
                TextFont {
                    font_size: FontSize::Px(52.0),
                    ..default()
                },
                TextColor(Color::srgb(1.5, 1.7, 2.4)), // HDR → blooms
            ));
            root.spawn((Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(24.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },))
            .with_children(|row| {
                for (index, (icon_path, action, tint)) in [
                    ("icons/play.png", PauseButton::Resume, Color::srgb(0.78, 0.92, 1.0)),
                    ("icons/settings.png", PauseButton::Options, Color::srgb(0.78, 0.92, 1.0)),
                    ("icons/power.png", PauseButton::Quit, Color::srgb(1.0, 0.72, 0.76)),
                ]
                .into_iter()
                .enumerate()
                {
                    row.spawn((
                        Button,
                        action,
                        MenuButton { index },
                        Node {
                            width: Val::Px(78.0),
                            height: Val::Px(78.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.5)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(BTN_IDLE),
                        BorderColor::all(Color::srgba(0.25, 0.6, 1.0, 0.45)),
                    ))
                    .with_children(|b| {
                        let icon = asset_server.load(icon_path);
                        b.spawn((
                            ImageNode {
                                image: icon,
                                color: tint,
                                ..default()
                            },
                            Node {
                                width: Val::Px(40.0),
                                height: Val::Px(40.0),
                                ..default()
                            },
                        ));
                    });
                }
            });
        });
}

fn pause_button_system(
    interactions: Query<(Entity, Ref<Interaction>, &PauseButton)>,
    menu_activated: Res<MenuActivated>,
    mut next_overlay: ResMut<NextState<Overlay>>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut options_return: ResMut<OptionsReturn>,
) {
    for (entity, interaction, btn) in &interactions {
        if activated(&interaction, entity, &menu_activated) {
            match btn {
                PauseButton::Resume => next_overlay.set(Overlay::None),
                PauseButton::Options => {
                    options_return.0 = Overlay::Paused;
                    next_overlay.set(Overlay::Options);
                }
                PauseButton::Quit => {
                    // Leaving the match must also drop the overlay, or the next match would
                    // start "paused" with no pause panel on screen.
                    next_overlay.set(Overlay::None);
                    next_screen.set(Screen::LevelMenu);
                }
            }
        }
    }
}

fn despawn_pause(mut commands: Commands, q: Query<Entity, With<PauseRoot>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

use bevy::prelude::*;

use crate::input::{InputActions, LastInputDevice};
use crate::state::{Overlay, Screen};

mod level_menu;
mod main_menu;
pub(crate) mod options;
mod pause;

/// The whole menu flow: `MainMenu` (title) → `LevelMenu` (pick a mode) → `Loading`/`Playing`, with
/// `Options` reachable from `MainMenu` *and* from the in-match `Paused` overlay. Each screen is a
/// focused submodule; this plugin just wires them together, mirroring `gameplay::GameplayPlugin`'s
/// aggregator pattern.
pub(crate) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OptionsReturn>()
            .init_resource::<MenuFocus>()
            .init_resource::<MenuActivated>()
            .add_plugins((
                main_menu::MainMenuPlugin,
                level_menu::LevelMenuPlugin,
                options::OptionsPlugin,
                pause::PausePlugin,
            ))
            // Reset focus to the first button whenever a menu screen or overlay opens —
            // `Overlay::None` included, because closing Options over the title screen respawns
            // the main menu buttons.
            .add_systems(OnEnter(Screen::MainMenu), reset_focus)
            .add_systems(OnEnter(Screen::LevelMenu), reset_focus)
            .add_systems(OnEnter(Overlay::Options), reset_focus)
            .add_systems(OnEnter(Overlay::AdvancedOptions), reset_focus)
            .add_systems(OnEnter(Overlay::Paused), reset_focus)
            .add_systems(OnEnter(Overlay::None), reset_focus)
            .add_systems(
                Update,
                // Any open overlay is navigable; the title screen only while no overlay covers it
                // (its buttons despawn under Options, but don't run the nav on a stale frame).
                menu_nav.run_if(not(in_state(Overlay::None)).or_else(in_state(Screen::MainMenu))),
            );
    }
}

/// Which overlay the Options screen should return to when the player backs out — set by whoever
/// opened it, read by `options::back_button_system`. From the title screen that's `Overlay::None`
/// (revealing the main menu again); from a paused match it's `Overlay::Paused`, so backing out
/// lands on the pause panel (and the live board), not the title screen.
#[derive(Resource)]
pub(super) struct OptionsReturn(pub(super) Overlay);

impl Default for OptionsReturn {
    fn default() -> Self {
        Self(Overlay::None)
    }
}

pub(super) const BTN_IDLE: Color = Color::srgba(0.06, 0.08, 0.14, 0.88);
pub(super) const BTN_HOVER: Color = Color::srgba(0.12, 0.22, 0.45, 0.95);

// ─── Keyboard/gamepad menu navigation ────────────────────────────────────────
//
// Each screen tags its buttons with `MenuButton { index }` (spawn order). `menu_nav` (below) runs
// for every menu state, moves a shared focus with nav, highlights the focused button, and on
// `confirm` records the focused entity in `MenuActivated`. Each screen's existing press-handler
// then treats "mouse-pressed OR activated-by-nav" as a click via the `activated` helper. Only one
// screen's buttons carry `MenuButton` at a time (screens despawn on exit), so a single global query
// is unambiguous. (Options' sliders stay mouse-only this pass; the buttons/cyclers are navigable.)

/// Tag + ordering for a navigable menu button.
#[derive(Component)]
pub(super) struct MenuButton {
    pub(super) index: usize,
}

/// Which `MenuButton` (by index) is focused on the current screen. Reset to 0 on each screen enter.
#[derive(Resource, Default)]
pub(super) struct MenuFocus(pub(super) usize);

/// Set by `menu_nav` for one frame to the entity the player confirmed via keyboard/gamepad; read by
/// screen press-handlers through [`activated`].
#[derive(Resource, Default)]
pub(super) struct MenuActivated(pub(super) Option<Entity>);

/// True on the FRAME a button is activated — by a mouse press *edge* (`Ref::is_changed` so a held
/// button fires once, not every frame: critical for toggles/cyclers like fullscreen/resolution) or
/// by a keyboard/gamepad confirm (`MenuActivated`, already one-shot via `menu_nav`).
pub(super) fn activated(interaction: &Ref<Interaction>, me: Entity, act: &MenuActivated) -> bool {
    (interaction.is_changed() && **interaction == Interaction::Pressed) || act.0 == Some(me)
}

fn reset_focus(mut focus: ResMut<MenuFocus>) {
    focus.0 = 0;
}

fn menu_nav(
    actions: Res<InputActions>,
    last: Res<LastInputDevice>,
    mut focus: ResMut<MenuFocus>,
    mut activated_res: ResMut<MenuActivated>,
    mut buttons: Query<(Entity, &MenuButton, &mut BackgroundColor)>,
) {
    activated_res.0 = None;

    let mut list: Vec<(usize, Entity)> = buttons.iter().map(|(e, b, _)| (b.index, e)).collect();
    if list.is_empty() {
        return;
    }
    list.sort_by_key(|(i, _)| *i);
    let n = list.len();
    if focus.0 >= n {
        focus.0 = 0;
    }

    if actions.down || actions.right {
        focus.0 = (focus.0 + 1) % n;
    }
    if actions.up || actions.left {
        focus.0 = (focus.0 + n - 1) % n;
    }
    if actions.confirm {
        activated_res.0 = Some(list[focus.0].1);
    }

    // Paint the focus only while a keyboard/gamepad is in use, so it doesn't fight the mouse's
    // hover highlight (`button_hover_system`).
    if *last == LastInputDevice::Cursor {
        let focused = list[focus.0].1;
        for (e, _, mut bg) in &mut buttons {
            bg.0 = if e == focused { BTN_HOVER } else { BTN_IDLE };
        }
    }
}

/// Shared button-hover/press visuals for any menu screen's `Button` entities — spawn the button
/// with `BackgroundColor(BTN_IDLE)` and run this in `Update` alongside the screen's own
/// press-handling system.
pub(super) fn button_hover_system(
    mut interactions: Query<
        (&Interaction, &mut BackgroundColor, Option<&mut BorderColor>),
        (Changed<Interaction>, With<MenuButton>),
    >,
) {
    for (interaction, mut bg, border) in &mut interactions {
        match interaction {
            Interaction::Pressed => {}
            Interaction::Hovered => {
                bg.0 = BTN_HOVER;
                if let Some(mut b) = border {
                    *b = BorderColor::all(Color::srgb(1.8, 2.6, 4.0)); // glowing cyan-white
                }
            }
            Interaction::None => {
                bg.0 = BTN_IDLE;
                if let Some(mut b) = border {
                    *b = BorderColor::all(Color::srgba(0.25, 0.6, 1.0, 0.45)); // glowing cyan
                }
            }
        }
    }
}

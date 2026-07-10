//! Device-agnostic input layer. The rest of the game reads abstract *actions* from
//! [`InputActions`] instead of polling `ButtonInput<KeyCode>`/`Gamepad`/mouse directly, so the
//! board and menus can be driven by keyboard, gamepad **or** mouse with one set of consumers.
//!
//! Default bindings (no remapping UI yet — that's a future pass):
//! - **Keyboard:** Arrows/WASD = nav · Space/Enter = confirm · Esc = pause · Backspace = cancel.
//! - **Gamepad:** D-pad + left stick = nav · South(A) = confirm · East(B) = cancel · Start = pause.
//! - **Mouse:** the existing drag gesture (`gameplay::input::handle_input`) is the "pointer"
//!   paradigm and is untouched; here the mouse only flips [`LastInputDevice`] back to `Pointer`.

use bevy::input::InputSystems;
use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

pub(crate) mod pointer;

pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputActions>()
            .init_resource::<LastInputDevice>()
            .init_resource::<AnalogNav>()
            .init_resource::<pointer::PointerInput>()
            // Chain: gather_input primero (teclado/gamepad → InputActions + LastInputDevice),
            // luego gather_pointer_input (mouse/touch → PointerInput; overrides Last a Touch si aplica).
            .add_systems(
                PreUpdate,
                (gather_input, pointer::gather_pointer_input)
                    .after(InputSystems)
                    .chain(),
            );
    }
}

/// Abstract per-frame actions, all edge-triggered ("happened this frame"). Recomputed every frame
/// by [`gather_input`]; consumers just read the booleans they care about.
#[derive(Resource, Default)]
pub(crate) struct InputActions {
    pub(crate) up: bool,
    pub(crate) down: bool,
    pub(crate) left: bool,
    pub(crate) right: bool,
    /// Select / swap / activate a button.
    pub(crate) confirm: bool,
    /// Drop a picked piece / go back.
    pub(crate) cancel: bool,
    /// Open the pause menu (in a match) or close it.
    pub(crate) pause: bool,
}

impl InputActions {
    /// A directional nav happened this frame (any of the four).
    pub(crate) fn any_nav(&self) -> bool {
        self.up || self.down || self.left || self.right
    }
    /// The signed grid delta for this frame's nav (y up = +1), or `None` if no nav.
    pub(crate) fn nav_delta(&self) -> Option<IVec2> {
        let d = IVec2::new(
            self.right as i32 - self.left as i32,
            self.up as i32 - self.down as i32,
        );
        (d != IVec2::ZERO).then_some(d)
    }
    /// "Back/cancel" for menus: either the dedicated cancel or the pause key (Esc) both go back.
    pub(crate) fn menu_back(&self) -> bool {
        self.cancel || self.pause
    }
}

/// Which input paradigm the player last used. Drives whether the board's keyboard/gamepad cursor
/// highlight is shown (hidden while playing with the mouse, so it doesn't clutter).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LastInputDevice {
    #[default]
    Pointer,
    Cursor,
    Touch,
}

/// Internal state for turning the analog stick into discrete, repeatable nav steps.
#[derive(Resource)]
struct AnalogNav {
    last: IVec2,
    repeat: Timer,
}

impl Default for AnalogNav {
    fn default() -> Self {
        Self {
            last: IVec2::ZERO,
            repeat: Timer::from_seconds(0.16, TimerMode::Once),
        }
    }
}

fn gather_input(
    mut actions: ResMut<InputActions>,
    mut last: ResMut<LastInputDevice>,
    mut analog: ResMut<AnalogNav>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    gamepads: Query<&Gamepad>,
    touches: Res<bevy::input::touch::Touches>,
    state: Res<State<crate::state::GameState>>,
) {
    let mut a = InputActions::default();

    // ── Keyboard ────────────────────────────────────────────────────────────
    a.up |= keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]);
    a.down |= keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]);
    a.left |= keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]);
    a.right |= keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD]);
    a.confirm |= keys.any_just_pressed([KeyCode::Space, KeyCode::Enter]);
    a.cancel |= keys.just_pressed(KeyCode::Backspace);
    a.pause |= keys.just_pressed(KeyCode::Escape);

    let pointer_just_pressed =
        mouse.just_pressed(MouseButton::Left) || touches.iter_just_pressed().next().is_some();
    if pointer_just_pressed {
        let current_state = state.get();
        if *current_state == crate::state::GameState::LevelComplete
            || *current_state == crate::state::GameState::GameOver
        {
            a.confirm = true;
        }
    }

    // ── Gamepad buttons (OR across all connected pads) ──────────────────────
    let mut stick = IVec2::ZERO;
    for gp in &gamepads {
        a.up |= gp.just_pressed(GamepadButton::DPadUp);
        a.down |= gp.just_pressed(GamepadButton::DPadDown);
        a.left |= gp.just_pressed(GamepadButton::DPadLeft);
        a.right |= gp.just_pressed(GamepadButton::DPadRight);
        a.confirm |= gp.just_pressed(GamepadButton::South);
        a.cancel |= gp.just_pressed(GamepadButton::East);
        a.pause |= gp.just_pressed(GamepadButton::Start);

        let sx = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        let sy = gp.get(GamepadAxis::LeftStickY).unwrap_or(0.0); // up is positive
        const DEAD: f32 = 0.5;
        if sx.abs() > sy.abs() {
            if sx > DEAD {
                stick = IVec2::new(1, 0);
            } else if sx < -DEAD {
                stick = IVec2::new(-1, 0);
            }
        } else if sy > DEAD {
            stick = IVec2::new(0, 1);
        } else if sy < -DEAD {
            stick = IVec2::new(0, -1);
        }
    }

    // ── Analog stick → discrete nav, with auto-repeat while held ────────────
    analog.repeat.tick(time.delta());
    let fire = if stick == IVec2::ZERO {
        false
    } else if stick != analog.last {
        analog.repeat.reset();
        true // fresh flick: fire immediately
    } else {
        analog.repeat.is_finished()
    };
    if fire {
        analog.repeat.reset();
        if stick.x > 0 {
            a.right = true;
        }
        if stick.x < 0 {
            a.left = true;
        }
        if stick.y > 0 {
            a.up = true;
        }
        if stick.y < 0 {
            a.down = true;
        }
    }
    analog.last = stick;

    // ── Which device is in control? A deliberate keyboard/gamepad action wins the frame; the
    //    mouse only reclaims focus on frames where no such action fired (so a stray mouse bump
    //    mid-keypress doesn't hide the cursor). ──────────────────────────────
    let cursor_action = a.any_nav() || a.confirm || a.cancel || a.pause;
    let mouse_active =
        mouse.get_just_pressed().next().is_some() || mouse_motion.delta != Vec2::ZERO;
    if cursor_action {
        *last = LastInputDevice::Cursor;
    } else if mouse_active {
        *last = LastInputDevice::Pointer;
    }

    *actions = a;
}

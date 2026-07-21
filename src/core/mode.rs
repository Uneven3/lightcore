//! `GameMode` — which ruleset the current match runs under.
//!
//! Lives in `core` (not `gameplay`) so the dependency direction stays one-way: `gameplay`, `menu`,
//! `ui` and `core::run` all read it, yet none of them has to depend *upward* on the gameplay
//! pipeline just to name a mode. It is a plain data enum with no gameplay behaviour of its own —
//! the systems that diverge per mode live in `gameplay`.

use bevy::prelude::*;

/// Which game the player picked from the level menu. Set by `menu`, read by the systems that
/// diverge between modes (board setup and the HUD). `Classic(level)` is one of the 4 isolated
/// modes shown in the level menu (Clásico/Ingredientes/Jalea/Contrarreloj — levels 1-4, each a
/// distinct `LevelGoal`); completing or restarting one repeats that same level, there's no 1→2→3→4
/// progression anymore. `ConsumeAll` is the "Blackhole" sandbox — runs the same Classic gameplay
/// pipeline with no win/lose yet, a bench for iterating the feel (tier growth + a final clear-the-
/// board win condition are deferred). Renamed from `GameMode::Blackhole` to free that name for
/// `LightKind::Blackhole`, the tier-6 power that actually clears the board.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GameMode {
    Classic(u32),
    Run(u32),
    ConsumeAll,
    /// VFX test bench: the board spawns with *random* `LightKind`s (powers everywhere) so adjacent
    /// powers can be swapped to exercise every combo animation. Same unbounded, no-win/lose loop as
    /// `ConsumeAll` (infinite moves, Esc returns to the menu); unlike `ConsumeAll` it has no
    /// clear-the-board win — it's purely a place to watch interactions.
    Sandbox,
    /// One specific combo interaction, isolated and guaranteed on the very first move: the board is
    /// a fixed (not random) layout — every cell is `Normal` except one adjacent pair of `LightKind`s
    /// placed at a known spot, positioned so swapping them immediately fires the intended
    /// `ComboKind` (see `gameplay::lifecycle::DEBUG_SCENARIOS`). Where `Sandbox` is "watch whatever
    /// combo happens to come up", this is "watch THIS exact combo, right now" — for tuning the feel
    /// of one interaction at a time instead of hunting for it on a random board. Same unbounded,
    /// no-win/lose loop as `Sandbox`/`ConsumeAll`.
    Debug(u8),
    TeleportTest,
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::Classic(1)
    }
}

impl GameMode {
    /// Sandbox modes run the Classic pipeline with infinite moves and no win/lose (Esc/Space just
    /// returns to the menu). Used to gate the unbounded-loop branches shared by `ConsumeAll` and
    /// `Sandbox`. Note: the clear-the-board *win* is still `ConsumeAll`-only (see
    /// `lifecycle::check_board_consumed`).
    pub(crate) fn is_sandbox(self) -> bool {
        matches!(
            self,
            GameMode::ConsumeAll | GameMode::Sandbox | GameMode::Debug(_) | GameMode::TeleportTest
        )
    }

    pub(crate) fn is_run(self) -> bool {
        matches!(self, GameMode::Run(_))
    }
}

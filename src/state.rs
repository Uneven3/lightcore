use bevy::prelude::*;

/// Ownership marker for every entity whose lifetime is one match session. Each adapter tags the
/// entities it creates; lifecycle cleanup depends only on this neutral contract, never on concrete
/// board, UI, audio or VFX component types.
#[derive(Component)]
pub(crate) struct MatchScoped;

/// Ownership marker for entities whose lifetime is one attempt within a match session. Retrying
/// clears this narrower scope while preserving match-level composition such as the grid backdrop.
/// Every attempt-scoped entity is also [`MatchScoped`] so leaving the match remains a single,
/// complete cleanup operation.
#[derive(Component)]
pub(crate) struct AttemptScoped;

/// Cross-adapter interaction lock for the tutorial modal. Gameplay only needs to know whether
/// input is blocked; it does not depend on any UI entity or widget implementation.
#[derive(Resource, Default)]
pub(crate) struct TutorialModalState {
    pub(crate) open: bool,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MatchFrameSet {
    VisualPosition,
}

/// Top-level screens — where the player *is*. Exactly one is active; switching screens tears down
/// the previous screen's UI (and, leaving `Match`, the whole board via `MatchPhase` disappearing).
/// Menus/overlays that show *on top* of a screen (pause, options, future story dialogue) are NOT
/// screens — they live in [`Overlay`], so the screen underneath stays alive.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Screen {
    /// Title screen — the app boots here. "Jugar" → `LevelMenu`, "Opciones" → `Overlay::Options`.
    #[default]
    MainMenu,
    /// The mode picker (Clásico/Ingredientes/Jalea/Contrarreloj/Blackhole). The board doesn't
    /// exist yet; picking a mode enters `Match` (whose `MatchPhase` starts at `Loading`).
    /// Returning from a match (Esc) comes straight back here, skipping `MainMenu`.
    LevelMenu,
    /// A match is live. The actual flow within the match is [`MatchPhase`], a sub-state that only
    /// exists while this screen is active — leaving `Match` destroys it (and with it the guarantee
    /// that any `Res<State<MatchPhase>>` exists: systems that can run outside the match must take
    /// `Option<Res<State<MatchPhase>>>`).
    Match,
}

/// The match flow — only exists while `Screen::Match` is active. Re-created at `Loading` every
/// time a match starts, so board population always runs exactly once per match.
#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(Screen = Screen::Match)]
pub(crate) enum MatchPhase {
    /// One-shot board setup for the chosen `GameMode`. Runs its `OnEnter` system, then immediately
    /// advances to `Playing`. Kept separate from `Playing` because `Playing` is re-entered on every
    /// cascade settle — populating there would rebuild the board mid-match.
    #[default]
    Loading,
    Playing,
    SwapAnimating,
    Popping,
    Falling,
    Spawning,
    CheckingChain,
    LevelComplete,
    GameOver,
}

/// UI layered *on top* of the current [`Screen`] without tearing it down — the pause dim panel
/// over the live board, and the Options screens (reachable from `MainMenu` and from `Paused`;
/// `OptionsReturn` remembers which overlay to go back to). Gameplay/menu input systems that should
/// freeze under an overlay gate themselves with `in_state(Overlay::None)`. This is also the seam
/// for future overlays (story dialogue over a live board).
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Overlay {
    #[default]
    None,
    /// In-match pause (Reanudar / Opciones / Salir al menú). The board is NOT torn down — it stays
    /// visible (and breathing) behind the dim panel, so graphics can be tuned live in Options.
    Paused,
    /// Settings screen (Bloom/shake/partículas/volumen).
    Options,
    /// Technical visual/timing controls, reached from the regular Options screen. Kept as a
    /// separate overlay so it has its own back navigation.
    AdvancedOptions,
}

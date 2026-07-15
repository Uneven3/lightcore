use bevy::prelude::*;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub(crate) enum GameState {
    /// Title screen — the app boots here. "Jugar" → `LevelMenu`, "Opciones" → `Options`.
    #[default]
    MainMenu,
    /// The mode picker (Clásico/Ingredientes/Jalea/Contrarreloj/Blackhole). The board doesn't
    /// exist yet; picking a mode transitions through `Loading` (which populates the board once)
    /// into `Playing`. Returning from a match (Esc) comes straight back here, skipping `MainMenu`.
    LevelMenu,
    /// Settings screen (Bloom/shake/partículas/volumen), reachable from `MainMenu`.
    Options,
    /// Technical visual/timing controls, reached from the regular Options screen. Kept as a
    /// separate state so it has its own back navigation and cannot leave both menus in the UI tree.
    AdvancedOptions,
    /// One-shot board setup for the chosen `GameMode`. Runs its `OnEnter` system, then immediately
    /// advances to `Playing`. Kept separate from `Playing` because `Playing` is re-entered on every
    /// cascade settle — populating there would rebuild the board mid-match.
    Loading,
    Playing,
    /// In-match pause overlay (Reanudar / Opciones / Salir al menú). The board is NOT torn down —
    /// it stays visible (and breathing) behind the overlay, so graphics can be tuned live in
    /// Options. Reached from `Playing` via the `pause` action; `Reanudar` returns to `Playing`.
    Paused,
    SwapAnimating,
    Popping,
    Falling,
    Spawning,
    CheckingChain,
    LevelComplete,
    GameOver,
}

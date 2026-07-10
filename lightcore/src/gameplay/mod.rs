use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::VecDeque;

use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::state::GameState;
use crate::visuals::motion::lerp_visual_pos;

pub(crate) mod chain;
pub(crate) mod falling;
pub(crate) mod input;
pub(crate) mod lifecycle;
pub(crate) mod popping;
pub(crate) mod shop;
pub(crate) mod spawning;
pub(crate) mod swap;
pub(crate) mod vfx;

pub(crate) struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, input::setup_board_cursor)
            .add_systems(Update, input::update_board_cursor)
            .init_resource::<GameMode>()
            .insert_resource(Score(0))
            .insert_resource(DisplayedScore(0))
            .insert_resource(CoreReserve(0))
            .insert_resource(CoresSpent(0))
            .init_resource::<CollectedCores>()
            .init_resource::<DisplayedCollectedCores>()
            .init_resource::<ScoreGlow>()
            .init_resource::<ScoreAnchor>()
            .init_resource::<StatsBook>()
            .init_resource::<StatsPopupOpen>()
            .insert_resource(MovesLeft(crate::core::level::MOVES))
            .insert_resource(make_level(1))
            .init_resource::<PendingSwap>()
            .init_resource::<RevertingSwap>()
            .init_resource::<GravitySettled>()
            .init_resource::<DragState>()
            .init_resource::<CascadeDepth>()
            .init_resource::<SparksCollected>()
            .init_resource::<ShadowCount>()
            .init_resource::<ShadowSet>()
            .init_resource::<lifecycle::LevelRewardOffer>()
            .add_systems(Update, falling::update_shadow_set)
            .init_resource::<LevelTimer>()
            .init_resource::<PowerActivationQueue>()
            .init_resource::<SuperComboPending>()
            .init_resource::<shop::Shop>()
            .init_resource::<input::BoardCursor>()
            .add_observer(swap::on_swap_happened)
            .add_observer(falling::on_fall_complete)
            .add_observer(spawning::on_spawn_complete)
            .add_systems(OnEnter(GameState::Loading), lifecycle::setup_match)
            .add_systems(OnEnter(GameState::LevelMenu), lifecycle::teardown_match)
            .add_systems(
                OnEnter(GameState::Falling),
                // Win-check first: in ConsumeAll, a fully cleared board ends the level here,
                // before gravity/refill runs (see `lifecycle::check_board_consumed`).
                (lifecycle::check_board_consumed, falling::reset_gravity).chain(),
            )
            .add_systems(OnEnter(GameState::Spawning), spawning::spawn_new_lights)
            .add_systems(
                OnEnter(GameState::CheckingChain),
                chain::check_chain_matches,
            )
            .add_systems(OnEnter(GameState::GameOver), lifecycle::show_game_over)
            .add_systems(
                OnEnter(GameState::LevelComplete),
                lifecycle::show_level_complete,
            )
            .add_systems(
                Update,
                // `shop_targeting` runs first so an armed booster consumes the click before the
                // drag-swap (`handle_input` bails while `Shop::is_armed`).
                (
                    shop::shop_targeting,
                    input::handle_input,
                    input::board_cursor_input,
                    input::highlight_selected,
                )
                    .chain()
                    .before(lerp_visual_pos)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (
                    shop::shop_button_system.before(shop::shop_targeting),
                    shop::update_shop_buttons.run_if(
                        resource_changed::<CoreReserve>
                            .or_else(resource_changed::<RunState>)
                            .or_else(resource_changed::<shop::Shop>),
                    ),
                )
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(OnExit(GameState::Playing), shop::reset_shop)
            .add_systems(
                Update,
                input::check_swap_visual_done.run_if(in_state(GameState::SwapAnimating)),
            )
            .add_systems(
                Update,
                (
                    popping::clone_pop_material,
                    popping::tick_pop_anim,
                    popping::check_popping_done,
                )
                    .chain()
                    .run_if(in_state(GameState::Popping)),
            )
            .add_systems(
                Update,
                falling::apply_gravity
                    .run_if(in_state(GameState::Falling))
                    .after(lerp_visual_pos),
            )
            .add_systems(
                Update,
                spawning::wait_for_spawn_settle
                    .run_if(in_state(GameState::Spawning))
                    .after(lerp_visual_pos),
            )
            .add_systems(
                Update,
                lifecycle::handle_restart.run_if(in_state(GameState::GameOver)),
            )
            .add_systems(
                Update,
                (
                    lifecycle::level_reward_button_system,
                    lifecycle::handle_level_advance,
                )
                    .chain()
                    .run_if(in_state(GameState::LevelComplete)),
            )
            .add_systems(
                Update,
                lifecycle::tick_level_timer.run_if(
                    not(in_state(GameState::GameOver))
                        .and_then(not(in_state(GameState::LevelComplete)))
                        // Pausing must stop the Contrarreloj clock — don't tick behind the overlay.
                        .and_then(not(in_state(GameState::Paused))),
                ),
            );
    }
}

// ─── Resources shared across the swap→pop→fall→spawn→chain pipeline ───────────
//
// These are read/written by systems on both sides of nearly every module boundary in
// `gameplay/` (and a couple, like `DragState`, are also read by `visuals` for rendering
// purposes) — see CONSTITUTION.md, Decision 1, for why this whole pipeline is one Plugin.

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
        matches!(self, GameMode::ConsumeAll | GameMode::Sandbox)
    }

    pub(crate) fn is_run(self) -> bool {
        matches!(self, GameMode::Run(_))
    }
}

#[derive(Resource, Default)]
pub(crate) struct Score(pub(crate) u32);

#[derive(Event)]
pub(crate) struct ScoreDrained {
    pub(crate) origins: Vec<Vec3>,
}

/// What the HUD actually shows — lags behind `Score` on purpose. Only
/// `visuals::score_light::tick_score_light` increments it, when a traveling light reaches the
/// score readout, so the displayed number visually "catches up" instead of jumping immediately.
/// `Score` itself stays authoritative and immediate (read by `chain.rs`'s win-condition check).
#[derive(Resource, Default)]
pub(crate) struct DisplayedScore(pub(crate) u32);

#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct CollectedCores(pub(crate) [u32; 5]);

#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct DisplayedCollectedCores(pub(crate) [u32; 5]);

/// Lightcores currently available to bend the rules with boosters. This is the spendable reserve:
/// it grows when lights are captured, but unlike `Score` it goes down when the player buys help.
#[derive(Resource, Default)]
pub(crate) struct CoreReserve(pub(crate) u32);

#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct StatsBook {
    pub(crate) reds: u32,
    pub(crate) greens: u32,
    pub(crate) blues: u32,
    pub(crate) yellows: u32,
    pub(crate) purples: u32,
    pub(crate) lightkinds: u32,
    pub(crate) max_cascade: u32,
    pub(crate) total_chains: u32,
}

#[derive(Resource, Default)]
pub(crate) struct StatsPopupOpen(pub(crate) bool);

/// Session-only bookkeeping for debug logs: how many captured lightcores were spent on boosters.
#[derive(Resource, Default)]
pub(crate) struct CoresSpent(pub(crate) u32);

/// The score readout's living color. `visuals::score_light` blends `rgb` toward the hue of each
/// core-shard as it lands and bumps `pulse`; `ui` renders the score (a world-space `Text2d`, so
/// the camera's Bloom makes it glow neon) from this — the number drifts toward whatever colors
/// are being collected and flickers as it "drinks" them.
#[derive(Resource)]
pub(crate) struct ScoreGlow {
    /// Normalized linear RGB (each channel ~0..1) the score is currently tinted.
    pub(crate) rgb: Vec3,
    /// 0 at rest, bumped toward 1 on each shard arrival; decays fast → rapid pulsing while absorbing.
    pub(crate) pulse: f32,
}

impl Default for ScoreGlow {
    /// Cool neon white-blue at rest.
    fn default() -> Self {
        Self {
            rgb: Vec3::new(0.65, 0.85, 1.0),
            pulse: 0.0,
        }
    }
}

/// Current world-space position of the score readout, refreshed every frame
/// by `ui::position_score`. Read by `visuals::score_light` for the shard target.
#[derive(Resource, Default)]
pub(crate) struct ScoreAnchor(pub(crate) Vec3);

#[derive(Resource)]
pub(crate) struct MovesLeft(pub(crate) u32);

#[derive(Resource, Default)]
pub(crate) struct PendingSwap(pub(crate) Option<SwapData>);

#[derive(Resource, Default)]
pub(crate) struct RevertingSwap(pub(crate) Vec<Entity>);

pub(crate) struct SwapData {
    pub(crate) a: Entity,
    pub(crate) b: Option<Entity>,
    pub(crate) a_pos: GridPos,
    pub(crate) b_pos: GridPos,
    /// A shop "swap" booster forces two arbitrary lights to trade places. Such a swap costs no
    /// move and, crucially, must NOT revert when it forms no match (`on_swap_happened`) — the
    /// player paid lightcores to break the adjacency/match rules, so the new arrangement stays.
    pub(crate) free: bool,
}

#[derive(Resource, Default)]
pub(crate) struct GravitySettled(pub(crate) bool);

#[derive(Resource, Default)]
pub(crate) struct CascadeDepth(pub(crate) u32);

#[derive(Resource, Default)]
pub(crate) struct SparksCollected(pub(crate) u32);

#[derive(Resource, Default)]
pub(crate) struct ShadowCount(pub(crate) u32);

/// `None` outside timed levels. `Some` ticks down for the duration of a timed score level; see
/// `lifecycle::tick_level_timer`.
#[derive(Resource, Default)]
pub(crate) struct LevelTimer(pub(crate) Option<Timer>);

#[derive(Resource, Default)]
pub(crate) struct PowerActivationQueue(pub(crate) VecDeque<PowerActivation>);

#[derive(Resource, Default)]
pub(crate) struct SuperComboPending(pub(crate) Vec<LightKind>);

/// Drag-gesture tracking. Owned/written by `gameplay::input`, but also read by
/// `visuals::motion` (to know which entities are mid-drag and should skip the
/// normal lerp/constrained-offset treatment).
#[derive(Resource, Default)]
pub(crate) struct DragState {
    pub(crate) active: bool,
    pub(crate) start_world: Vec2,
    pub(crate) last_world: Option<Vec2>,
    pub(crate) start_grid: Option<GridPos>,
    pub(crate) start_entity: Option<Entity>,
    pub(crate) locked_axis: Option<IVec2>,
    pub(crate) neighbor_entity: Option<Entity>,
    pub(crate) neighbor_grid: Option<GridPos>,
    pub(crate) neighbor_is_empty: bool,
}

#[derive(SystemParam)]
pub(crate) struct PowerComboParams<'w> {
    pub(crate) queue: ResMut<'w, PowerActivationQueue>,
    pub(crate) super_combo: ResMut<'w, SuperComboPending>,
}

/// Groups the resources that need resetting on level change/restart to stay under Bevy's
/// 16-system-param limit in `handle_restart` and `handle_level_advance`.
#[derive(SystemParam)]
pub(crate) struct ResetParams<'w> {
    pub(crate) score: ResMut<'w, Score>,
    pub(crate) displayed: ResMut<'w, DisplayedScore>,
    pub(crate) reserve: ResMut<'w, CoreReserve>,
    pub(crate) spent: ResMut<'w, CoresSpent>,
    pub(crate) moves: ResMut<'w, MovesLeft>,
    pub(crate) pending: ResMut<'w, PendingSwap>,
    pub(crate) reverting: ResMut<'w, RevertingSwap>,
    pub(crate) drag: ResMut<'w, DragState>,
    pub(crate) settled: ResMut<'w, GravitySettled>,
    pub(crate) cascade: ResMut<'w, CascadeDepth>,
    pub(crate) collected: ResMut<'w, SparksCollected>,
    pub(crate) shadow: ResMut<'w, ShadowCount>,
    pub(crate) level_timer: ResMut<'w, LevelTimer>,
    pub(crate) queue: ResMut<'w, PowerActivationQueue>,
    pub(crate) super_combo: ResMut<'w, SuperComboPending>,
    pub(crate) collected_cores: ResMut<'w, CollectedCores>,
    pub(crate) displayed_cores: ResMut<'w, DisplayedCollectedCores>,
    pub(crate) stats: ResMut<'w, StatsBook>,
    pub(crate) popup_open: ResMut<'w, StatsPopupOpen>,
}

// ─── Events ─────────────────────────────────────────────────────────────────────

#[derive(Event)]
pub(crate) struct SwapHappened;

#[derive(Event)]
pub(crate) struct FallComplete;

#[derive(Event)]
pub(crate) struct SpawnComplete;

#[derive(Event, Clone, Copy)]
pub(crate) struct PowerConsumed {
    pub(crate) kind: LightKind,
    pub(crate) pos: GridPos,
    pub(crate) color: Option<LightColor>,
}

/// Sibling event to `PowerConsumed`, fired alongside it: carries the ordered blast path
/// (world-space) so `visuals::light_trail` can animate a light traveling along the activation.
#[derive(Event, Clone)]
pub(crate) struct PowerBlastTrail {
    pub(crate) kind: LightKind,
    pub(crate) color: Option<LightColor>,
    pub(crate) path: Vec<Vec3>,
    /// Segundos antes de que el efecto empiece a reproducirse. 0.0 = inmediato.
    pub(crate) delay_secs: f32,
}

/// Fired in place of two separate `PowerConsumed` when power lights **combine** — so the visual
/// layer plays one unified animation per interaction (see `core::matching::ComboKind`) instead of
/// two coincidental single-power effects. `a_pos` is the choreography's anchor (the Starburst for
/// star combos, the Supernova for line+supernova; for whole-board combos it's the board centre);
/// `color` is the target color for Starburst combos, `None` otherwise.
#[derive(Event, Clone, Copy)]
pub(crate) struct PowerCombo {
    pub(crate) kind: ComboKind,
    pub(crate) a_pos: GridPos,
    pub(crate) b_pos: GridPos,
    pub(crate) color: Option<LightColor>,
}

#[derive(Event, Clone)]
pub(crate) struct ChainPop {
    pub(crate) removed: u32,
    pub(crate) points: u32,
    pub(crate) hollow: bool,
    /// (world position, color, pop_delay_secs) of each light. The delay matches PopDelay so
    /// score shards start flying when the light's pop actually begins, not all at once.
    pub(crate) pops: Vec<(Vec3, LightColor, f32)>,
}

/// A swap produced no match and was reverted. Fired so `AudioPlugin` can play the "nope" sound
/// without `gameplay` knowing about audio.
#[derive(Event)]
pub(crate) struct SwapFailed;

/// The pop-fade animation for one light finished — membrane fully dissolved.
/// Fired so `VisualsPlugin` can spawn the membrane-burst particles at exactly this moment.
#[derive(Event, Clone, Copy)]
pub(crate) struct LightPopped {
    pub(crate) pos: Vec3,
    pub(crate) color: LightColor,
    pub(crate) kind: LightKind,
}

/// A normal light was upgraded to a power light this frame (via match-3, cascade, or shop booster).
/// Fired so `AudioPlugin` can react without `gameplay` knowing about audio.
/// (No payload: the single audio cue is kind-independent. Re-add a `kind` field when per-kind
/// audio assets are available.)
#[derive(Event, Clone, Copy)]
pub(crate) struct PowerCreated;

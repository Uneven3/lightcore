use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::VecDeque;

use crate::core::grid::GravityBlockSet;
use crate::core::prelude::*;
pub(crate) use crate::core::run::CoreReserve;
use crate::core::run::RunState;
use crate::state::MatchFrameSet;
use crate::state::{MatchPhase, Overlay, Screen};

pub(crate) mod chain;
pub(crate) mod falling;
pub(crate) mod input;
pub(crate) mod lifecycle;
pub(crate) mod popping;
mod rewards;
pub(crate) mod shop;
pub(crate) mod spawning;
pub(crate) mod swap;
pub(crate) mod timing;
pub(crate) mod vfx;

pub(crate) use timing::MatchTiming;

pub(crate) struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, input::setup_board_cursor)
            .add_systems(Update, input::update_board_cursor)
            .init_resource::<GameMode>()
            .insert_resource(Score(0))
            .insert_resource(CoresSpent(0))
            .init_resource::<CollectedCores>()
            .init_resource::<ScoreGlow>()
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
            .init_resource::<GravityBlockSet>()
            .init_resource::<GridLayout>()
            .init_resource::<lifecycle::LevelRewardOffer>()
            .init_resource::<lifecycle::LevelCompletionReport>()
            .add_systems(
                Update,
                falling::update_gravity_block_set.before(falling::apply_gravity),
            )
            .add_systems(Update, lifecycle::account_removed_stasis)
            .add_systems(Update, lifecycle::despawn_orphan_stasis_covers)
            .init_resource::<LevelTimer>()
            .init_resource::<PowerActivationQueue>()
            .init_resource::<SuperComboPending>()
            .init_resource::<spawning::RefillQueue>()
            .init_resource::<MatchTiming>()
            .init_resource::<shop::Shop>()
            .init_resource::<shop::SpecialMoveInventory>()
            .init_resource::<input::BoardCursor>()
            .add_observer(swap::on_swap_happened)
            .add_observer(falling::on_fall_complete)
            .add_observer(spawning::on_spawn_complete)
            .add_observer(shop::on_shop_purchase_requested)
            .add_observer(shop::on_special_move_toggle_requested)
            .add_observer(shop::on_boon_sell_requested)
            .add_observer(lifecycle::on_level_reward_purchase_requested)
            .add_systems(OnEnter(MatchPhase::Loading), lifecycle::setup_match)
            .add_systems(OnEnter(Screen::LevelMenu), lifecycle::teardown_match)
            .add_systems(
                OnEnter(MatchPhase::Falling),
                // Win-check first: in ConsumeAll, a fully cleared board ends the level here,
                // before gravity/refill runs (see `lifecycle::check_board_consumed`).
                (lifecycle::check_board_consumed, falling::reset_gravity).chain(),
            )
            .add_systems(OnEnter(MatchPhase::Spawning), spawning::spawn_new_lights)
            .add_systems(
                OnEnter(MatchPhase::CheckingChain),
                chain::check_chain_matches,
            )
            .add_systems(OnEnter(MatchPhase::GameOver), lifecycle::finalize_game_over)
            .add_systems(
                OnEnter(MatchPhase::LevelComplete),
                lifecycle::finalize_level_complete,
            )
            .add_systems(
                Update,
                // `shop_targeting` runs first so an armed booster consumes the click before the
                // drag-swap (`handle_input` bails while `Shop::is_armed`).
                (
                    shop::shop_targeting,
                    input::handle_input,
                    input::board_cursor_input,
                    input::on_light_selected,
                    input::tick_select_jelly,
                    input::highlight_selected,
                )
                    .chain()
                    .run_if(crate::state::match_active),
            )
            .add_systems(OnExit(MatchPhase::Playing), shop::reset_shop)
            // Pausing used to leave `Playing` (disarming any armed booster via the OnExit above);
            // now that pause is an overlay the phase stays `Playing`, so disarm explicitly.
            .add_systems(OnEnter(Overlay::Paused), shop::reset_shop)
            .add_systems(
                Update,
                input::check_swap_visual_done.run_if(in_state(MatchPhase::SwapAnimating)),
            )
            .add_systems(
                Update,
                (
                    popping::clone_pop_material,
                    popping::tick_pop_anim,
                    popping::check_popping_done,
                )
                    .chain()
                    .run_if(in_state(MatchPhase::Popping)),
            )
            .add_systems(
                Update,
                vfx::tick_pending_light_transform.run_if(in_state(MatchPhase::Popping)),
            )
            // Impact jelly can deliberately outlive Popping: a Supernova's outer shockwave
            // starts only after its destroyed cells have finished dissolving.
            .add_systems(Update, vfx::tick_impact_jelly)
            .add_systems(
                Update,
                falling::apply_gravity
                    .run_if(in_state(MatchPhase::Falling))
                    .after(MatchFrameSet::VisualPosition),
            )
            .add_systems(
                Update,
                (
                    spawning::emit_refill_drop,
                    spawning::wait_for_spawn_settle
                        .after(spawning::emit_refill_drop)
                        .after(MatchFrameSet::VisualPosition),
                )
                    .chain()
                    .run_if(in_state(MatchPhase::Spawning)),
            )
            .add_systems(
                Update,
                lifecycle::handle_restart.run_if(in_state(MatchPhase::GameOver)),
            )
            .add_systems(
                Update,
                (lifecycle::handle_level_advance,)
                    .chain()
                    .run_if(in_state(MatchPhase::LevelComplete)),
            )
            .add_systems(
                Update,
                lifecycle::tick_level_timer.run_if(
                    not(in_state(MatchPhase::GameOver))
                        .and_then(not(in_state(MatchPhase::LevelComplete)))
                        // Any overlay must stop the Contrarreloj clock — pause, and also Options
                        // opened from pause (which previously kept ticking behind both panels).
                        .and_then(in_state(Overlay::None)),
                ),
            );
    }
}

// ─── Resources shared across the swap→pop→fall→spawn→chain pipeline ───────────
//
// These are read/written by systems on both sides of nearly every module boundary in
// `gameplay/` (and a couple, like `DragState`, are also read by `visuals` for rendering
// purposes) — see CONSTITUTION.md, Decision 1, for why this whole pipeline is one Plugin.

/// `GameMode` is a domain concept (which ruleset a match runs under), so it lives in `core::mode`
/// to keep the dependency direction one-way — `core::run` reads it without depending upward on
/// this pipeline. Re-exported here so the many `crate::gameplay::GameMode` / `super::GameMode` call
/// sites across gameplay, menu and ui keep resolving unchanged.
pub(crate) use crate::core::mode::GameMode;

#[derive(Resource, Default)]
pub(crate) struct Score(pub(crate) u32);

#[derive(Event)]
pub(crate) struct ScoreDrained {
    pub(crate) origins: Vec<Vec3>,
}

#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct CollectedCores(pub(crate) [u32; 5]);

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

/// The economy resources every removal-wave path mutates, bundled as one nested `SystemParam` so
/// `swap.rs` and `chain.rs` embed it instead of re-declaring the same six `ResMut`s, and build a
/// [`rewards::EconomyState`] through [`EconomyParams::state`] instead of repeating the struct
/// literal at each call site.
#[derive(SystemParam)]
pub(crate) struct EconomyParams<'w> {
    pub(crate) score: ResMut<'w, Score>,
    pub(crate) reserve: ResMut<'w, CoreReserve>,
    pub(crate) collected_cores: ResMut<'w, CollectedCores>,
    pub(crate) stats: ResMut<'w, StatsBook>,
    pub(crate) moves: ResMut<'w, MovesLeft>,
    pub(crate) run: ResMut<'w, RunState>,
    pub(crate) level: Res<'w, LevelConfig>,
}

impl EconomyParams<'_> {
    /// Borrows the bundle as the `&mut`-of-fields view the shared `rewards` helpers consume. Must
    /// stay a temporary at the call site (`&mut params.state()`) so the borrow of `self` is
    /// released the moment the helper returns.
    pub(crate) fn state(&mut self) -> rewards::EconomyState<'_> {
        rewards::EconomyState {
            score: &mut self.score,
            reserve: &mut self.reserve,
            collected_cores: &mut self.collected_cores,
            stats: &mut self.stats,
            moves: &mut self.moves,
            run: &mut self.run,
            color_values: self.level.color_values,
        }
    }
}

/// Groups the resources that need resetting on level change/restart to stay under Bevy's
/// 16-system-param limit in `handle_restart` and `handle_level_advance`.
#[derive(SystemParam)]
pub(crate) struct ResetParams<'w> {
    pub(crate) score: ResMut<'w, Score>,
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
    pub(crate) stats: ResMut<'w, StatsBook>,
    pub(crate) popup_open: ResMut<'w, StatsPopupOpen>,
    pub(crate) special_moves: ResMut<'w, shop::SpecialMoveInventory>,
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
    pub(crate) delay_secs: f32,
}

#[derive(Event, Clone)]
pub(crate) struct CaptureBatch {
    pub(crate) removed: u32,
    pub(crate) cascade_depth: u32,
    pub(crate) hollow: bool,
    /// Captured lights whose particles communicate both score and objective progress.
    pub(crate) captures: Vec<CapturedCore>,
}

#[derive(Clone, Copy)]
pub(crate) struct CapturedCore {
    pub(crate) grid_position: GridPos,
    pub(crate) color: LightColor,
    pub(crate) kind: LightKind,
    /// Domain chronology: this capture becomes available after the resolving power reaches it.
    pub(crate) available_after_secs: f32,
    /// Exact amount contributed to a color-collection objective. This is deliberately distinct
    /// from score points: capture boons and color values follow different economy rules.
    pub(crate) capture_units: u32,
    /// Number of reward echoes presentation should emit for this core. Gameplay resolves boon
    /// semantics once; VFX does not query `RunState` or duplicate economy rules.
    pub(crate) feedback_copies: u32,
}

/// A swap produced no match and was reverted. Fired so `AudioPlugin` can play the "nope" sound
/// without `gameplay` knowing about audio.
#[derive(Event)]
pub(crate) struct SwapFailed;

#[derive(Event, Clone, Copy)]
pub(crate) struct ManualLightEliminated {
    pub(crate) pos: GridPos,
    pub(crate) color: LightColor,
}

/// One endpoint of the paid Move special changed cells instantaneously. Gameplay owns the board
/// mutation; presentation listens to this fact to render departure/arrival teleport feedback.
#[derive(Event, Clone, Copy)]
pub(crate) struct LightTeleported {
    pub(crate) from: GridPos,
    pub(crate) to: GridPos,
    pub(crate) color: LightColor,
}

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

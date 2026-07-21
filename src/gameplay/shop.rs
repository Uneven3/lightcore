//! In-game shop: boosters the player buys with lightcores in reserve to bend the board's rules,
//! Candy-Crush style. Three abilities, armed from a bottom HUD bar (spawned in `ui::setup_ui`) and
//! then aimed by clicking lights:
//!
//! - **Swap** — force two lights to trade places, even non-adjacent / non-matching (a `free`
//!   `SwapData`, so the normal "snap back on no match" never fires).
//! - **Eliminate** — destroy a whole targeted cell (light plus shadow obstacle), then let the usual
//!   `Popping→Falling→…` pipeline refill it.
//! - **Upgrade** — raise a light one tier (`LightKind::next_tier`); `visuals::core_motion::
//!   rebuild_cores` reacts to the kind change and rebuilds its body + cores.
//!
//! Capturing lights always feeds the main lightcore counter (`Score`) and the spendable reserve
//! (`CoreReserve`). Buying a booster spends only the reserve — `Score` (the level's goal progress)
//! is untouched — and in `GameMode::Run`, `CoreReserve` is the run's persistent currency: it
//! carries over between levels instead of resetting, so boons bought early still cost the wallet
//! that funds later levels' shopping (see `gameplay::lifecycle::setup_match`).

use bevy::prelude::*;

use super::{
    CaptureBatch, CapturedCore, CascadeDepth, CoreReserve, CoresSpent, LightTeleported,
    ManualLightEliminated, PendingSwap, PowerCreated, ShadowCount, SwapData,
};
use crate::board::clear_shadow_cell;
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RunState};
use crate::input::pointer::PointerInput;
use crate::state::MatchPhase;
use crate::state::TutorialModalState;

/// The three boosters and the extra life sold by the in-match shop.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ShopItem {
    Swap,
    Eliminate,
    Upgrade,
    Life,
    Boon(BoonKind),
}

impl ShopItem {
    /// Bar order (left→right), also used to spawn the buttons.
    #[allow(dead_code)]
    pub(crate) const ALL: [ShopItem; 4] = [
        ShopItem::Swap,
        ShopItem::Eliminate,
        ShopItem::Upgrade,
        ShopItem::Life,
    ];

    pub(crate) fn cost(self, run: &RunState) -> Option<u32> {
        match self {
            ShopItem::Swap => Some(200),
            ShopItem::Eliminate => Some(450),
            ShopItem::Upgrade => Some(900),
            ShopItem::Life => Some(800),
            ShopItem::Boon(boon) => run.boon_cost(boon),
        }
    }

    pub(crate) fn is_boon(self) -> bool {
        matches!(self, ShopItem::Boon(_))
    }
}

/// In-game shop state. `armed` is the booster the player picked off the bar and is now aiming;
/// `first_pick` holds the light currently being dragged by Move.
#[derive(Resource, Default)]
pub(crate) struct Shop {
    armed: Option<ShopItem>,
    first_pick: Option<Entity>,
    ignore_board_press: bool,
    pending_purchase: Option<ShopItem>,
    pub(crate) open: bool,
}

/// Interaction marker consumed by the visual adapter to make the picked Move light follow the
/// pointer without mutating its authoritative `GridPos` before drop.
#[derive(Component)]
pub(crate) struct MoveDragPreview;

/// Consumable special moves bought from the shop. They are inventory, not an immediate targeting
/// mode: buying one increments the HUD counter; selecting that counter arms it; successful use
/// finally consumes one copy.
#[derive(Resource, Default)]
pub(crate) struct SpecialMoveInventory {
    counts: [u32; 3],
}

impl SpecialMoveInventory {
    fn index(item: ShopItem) -> Option<usize> {
        match item {
            ShopItem::Swap => Some(0),
            ShopItem::Eliminate => Some(1),
            ShopItem::Upgrade => Some(2),
            ShopItem::Life | ShopItem::Boon(_) => None,
        }
    }

    pub(crate) fn count(&self, item: ShopItem) -> u32 {
        Self::index(item).map_or(0, |index| self.counts[index])
    }

    fn add(&mut self, item: ShopItem) {
        if let Some(index) = Self::index(item) {
            self.counts[index] = self.counts[index].saturating_add(1);
        }
    }

    fn consume(&mut self, item: ShopItem) -> bool {
        let Some(index) = Self::index(item) else {
            return false;
        };
        if self.counts[index] == 0 {
            return false;
        }
        self.counts[index] -= 1;
        true
    }

    pub(crate) fn clear(&mut self) {
        self.counts = [0; 3];
    }
}

impl Shop {
    /// Whether a booster is currently aiming — `gameplay::input::handle_input` bails when so, so
    /// the drag-swap doesn't fight the targeting click.
    pub(crate) fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    /// A targeting special owns the primary pointer until release. This prevents the same press
    /// that consumed a special from falling through into the board's ordinary drag handler after
    /// the special disarms itself.
    pub(crate) fn blocks_board_input(&self) -> bool {
        self.is_armed() || self.ignore_board_press
    }

    pub(crate) fn armed_item(&self) -> Option<ShopItem> {
        self.armed
    }

    pub(crate) fn has_first_pick(&self) -> bool {
        self.first_pick.is_some()
    }

    pub(crate) fn pending_purchase_item(&self) -> Option<ShopItem> {
        self.pending_purchase
    }
}

/// Semantic command emitted by the UI adapter. Gameplay owns validation and the transaction; the
/// request contains no widget entity, interaction state or presentation component.
#[derive(Event, Clone, Copy)]
pub(crate) struct ShopPurchaseRequested(pub(crate) ShopItem);

/// Semantic command for arming/cancelling an already-owned special move.
#[derive(Event, Clone, Copy)]
pub(crate) struct SpecialMoveToggleRequested(pub(crate) ShopItem);

/// Semantic command emitted by the boon HUD once a sale is confirmed (its two-tap confirmation is
/// UI state and stays in `ui`). Gameplay owns the economy transaction — mirroring how purchases go
/// through `ShopPurchaseRequested` — so authoritative `RunState`/`CoreReserve` mutation never lives
/// in a UI system.
#[derive(Event, Clone, Copy)]
pub(crate) struct BoonSellRequested(pub(crate) BoonKind);

/// Spend only from the booster reserve — `Score` is the level's goal progress (and, for a Run,
/// what gets recorded to `CampaignProgress`), so buying a booster never claws that back. `reserve`
/// is the run's persistent wallet (see `RunState`'s doc comment on `lifecycle::setup_match`).
fn spend(reserve: &mut CoreReserve, spent: &mut CoresSpent, cost: u32) {
    reserve.0 = reserve.0.saturating_sub(cost);
    spent.0 += cost;
}

/// Clears a half-finished Swap pick: forgets `first_pick` and strips the `Selected` highlight from
/// whatever light still carries it (during armed targeting only the picked light is `Selected`).
fn clear_pick(commands: &mut Commands, shop: &mut Shop, selected: &Query<Entity, With<Selected>>) {
    shop.first_pick = None;
    for e in selected.iter() {
        commands
            .entity(e)
            .remove::<Selected>()
            .remove::<MoveDragPreview>();
    }
}

fn disarm(commands: &mut Commands, shop: &mut Shop, selected: &Query<Entity, With<Selected>>) {
    shop.armed = None;
    clear_pick(commands, shop, selected);
}

fn arm(shop: &mut Shop, item: ShopItem) {
    shop.armed = Some(item);
    // UI requests are observed through deferred commands, so their originating press is never a
    // board-targeting press that must be swallowed on the following frame.
    shop.ignore_board_press = false;
}

/// Purchases one special move into inventory. Targeting is deliberately separate: it starts only
/// when the player taps an owned counter in the status panel.
pub(crate) fn on_shop_purchase_requested(
    trigger: On<ShopPurchaseRequested>,
    mut shop: ResMut<Shop>,
    mut reserve: ResMut<CoreReserve>,
    mut spent: ResMut<CoresSpent>,
    mut run: ResMut<RunState>,
    mut inventory: ResMut<SpecialMoveInventory>,
) {
    let item = trigger.0;
    let Some(cost) = item.cost(&run) else {
        return;
    };
    if item == ShopItem::Life {
        shop.pending_purchase = None;
        if reserve.0 >= cost {
            run.lives += 1;
            spend(&mut reserve, &mut spent, cost);
            shop.open = false;
        }
        return;
    }
    if item.is_boon() {
        // Boons deliberately have no in-level purchase path. `ShopItem::Boon` remains a
        // presentation model for tooltips/reward cards, while the completion overlay owns
        // the only transaction that may call `RunState::buy`.
        return;
    }
    if reserve.0 < cost {
        shop.pending_purchase = None;
        return;
    }
    if shop.pending_purchase != Some(item) {
        shop.pending_purchase = Some(item);
        return;
    }
    shop.pending_purchase = None;
    inventory.add(item);
    spend(&mut reserve, &mut spent, cost);
    shop.open = false;
}

/// Applies a confirmed boon sale: refunds the most recently purchased rank into the reserve and
/// drops the boon one level. Gameplay owns this transaction; the UI only decides *when* to confirm.
pub(crate) fn on_boon_sell_requested(
    trigger: On<BoonSellRequested>,
    mut run: ResMut<RunState>,
    mut reserve: ResMut<CoreReserve>,
) {
    if let Some(refund) = run.sell(trigger.0) {
        reserve.0 = reserve.0.saturating_add(refund);
    }
}

/// Arms one already-owned special move. The counter is only decremented by `shop_targeting` after
/// a valid board action, so cancelling or changing target never burns a purchase.
pub(crate) fn on_special_move_toggle_requested(
    trigger: On<SpecialMoveToggleRequested>,
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    inventory: Res<SpecialMoveInventory>,
    selected: Query<Entity, With<Selected>>,
) {
    let item = trigger.0;
    if inventory.count(item) == 0 {
        return;
    }
    shop.pending_purchase = None;
    if shop.armed == Some(item) {
        disarm(&mut commands, &mut shop, &selected);
    } else {
        if shop.is_armed() {
            disarm(&mut commands, &mut shop, &selected);
        }
        clear_pick(&mut commands, &mut shop, &selected);
        arm(&mut shop, item);
    }
}

/// While a booster is armed, a board press applies it. Most specials require a light; Eliminate
/// also accepts a shadow-only cell. Runs before `handle_input`, which bails whenever
/// `Shop::is_armed`. Cancel via the UI button (cross-platform, works on touch).
#[allow(clippy::too_many_arguments)]
pub(crate) fn shop_targeting(
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    mut inventory: ResMut<SpecialMoveInventory>,
    mut cascade: ResMut<CascadeDepth>,
    mut shadow_count: ResMut<ShadowCount>,
    mut next_state: ResMut<NextState<MatchPhase>>,
    mut pending: ResMut<PendingSwap>,
    pointer: Res<PointerInput>,
    mut lights: Query<
        (Entity, &mut GridPos, &LightColor, &mut LightKind),
        (With<Light>, Without<AdjacentMatchDamage>),
    >,
    mut shadows: Query<
        (Entity, &GridPos, Option<&mut HardShadow>),
        (With<AdjacentMatchDamage>, Without<Light>),
    >,
    movable: Query<(), (With<Movable>, Without<BlocksInteraction>)>,
    selected: Query<Entity, With<Selected>>,
    tutorial: Res<TutorialModalState>,
) {
    if shop.ignore_board_press {
        // Keep ownership throughout mouse/touch hold. Once released (or no longer down), clear the
        // latch; returning early this frame also guarantees no stale press reaches a new ability.
        if pointer.just_released || (!pointer.held && !pointer.just_pressed) {
            shop.ignore_board_press = false;
        }
        return;
    }
    if tutorial.open {
        return;
    }
    let Some(item) = shop.armed else {
        return;
    };

    if item == ShopItem::Swap {
        if pointer.just_pressed {
            let Some(world) = pointer.position_world else {
                return;
            };
            let Some(target) = to_grid(world).and_then(|position| {
                lights
                    .iter()
                    .find(|(_, light_position, _, _)| **light_position == position)
                    .map(|(entity, _, _, _)| entity)
            }) else {
                return;
            };
            if movable.get(target).is_err() {
                return;
            }
            clear_pick(&mut commands, &mut shop, &selected);
            shop.first_pick = Some(target);
            commands.entity(target).insert((Selected, MoveDragPreview));
            return;
        }

        if pointer.just_released {
            let Some(first) = shop.first_pick else {
                return;
            };
            let target = pointer
                .position_world
                .and_then(to_grid)
                .and_then(|position| {
                    lights
                        .iter()
                        .find(|(_, light_position, _, _)| **light_position == position)
                        .map(|(entity, _, _, _)| entity)
                });
            let Some(target) =
                target.filter(|target| *target != first && movable.get(*target).is_ok())
            else {
                // Invalid drop: return the preview to its authoritative cell but keep Move armed
                // so a missed touch does not consume the paid special.
                clear_pick(&mut commands, &mut shop, &selected);
                return;
            };

            let Ok((_, a_pos, a_color, _)) = lights.get(first) else {
                clear_pick(&mut commands, &mut shop, &selected);
                return;
            };
            let (a_pos, a_color) = (*a_pos, *a_color);
            let Ok((_, b_pos, b_color, _)) = lights.get(target) else {
                clear_pick(&mut commands, &mut shop, &selected);
                return;
            };
            let (b_pos, b_color) = (*b_pos, *b_color);

            if let Ok((_, mut position, _, _)) = lights.get_mut(first) {
                position.set_if_neq(b_pos);
            }
            if let Ok((_, mut position, _, _)) = lights.get_mut(target) {
                position.set_if_neq(a_pos);
            }
            // Move is a teleport, not a long-distance slide. `VisualPos` snaps at the same
            // semantic moment as `GridPos`; particles make both endpoints legible.
            commands.entity(first).insert(VisualPos(to_world(b_pos)));
            commands.entity(target).insert(VisualPos(to_world(a_pos)));
            commands.trigger(LightTeleported {
                from: a_pos,
                to: b_pos,
                color: a_color,
            });
            commands.trigger(LightTeleported {
                from: b_pos,
                to: a_pos,
                color: b_color,
            });
            pending.0 = Some(SwapData {
                a: first,
                b: Some(target),
                a_pos,
                b_pos,
                free: true,
            });
            inventory.consume(item);
            clear_pick(&mut commands, &mut shop, &selected);
            shop.armed = None;
            next_state.set(MatchPhase::SwapAnimating);
        }
        return;
    }

    if !pointer.just_pressed {
        return;
    }

    let Some(world) = pointer.position_world else {
        return;
    };
    let Some(gp) = to_grid(world) else {
        return;
    };
    let target = lights
        .iter()
        .find(|(_, p, _, _)| **p == gp)
        .map(|(e, _, _, _)| e);
    let has_shadow = shadows.iter_mut().any(|(_, position, _)| *position == gp);
    if target.is_none() && !(item == ShopItem::Eliminate && has_shadow) {
        return;
    }

    match item {
        ShopItem::Boon(_) | ShopItem::Life => {
            disarm(&mut commands, &mut shop, &selected);
        }
        ShopItem::Eliminate => {
            cascade.0 = 1;
            clear_shadow_cell(gp, &mut commands, &mut shadows, &mut shadow_count.0);

            if let Some(target) = target {
                // Pop the light through the normal pipeline. A Stasis shadow is attached to this
                // light and is therefore removed by the existing orphan-cover/accounting systems;
                // a separate DeepShadow in the same coordinate was cleared above.
                let Ok((_, pos, color, _)) = lights.get(target) else {
                    return;
                };
                let pos = *pos;
                let color = *color;
                commands
                    .entity(target)
                    .insert(PopAnim(Timer::from_seconds(0.18, TimerMode::Once)));
                commands.trigger(ManualLightEliminated { pos, color });
                commands.trigger(CaptureBatch {
                    removed: 1,
                    cascade_depth: 1,
                    hollow: false,
                    captures: vec![CapturedCore {
                        grid_position: pos,
                        color,
                        kind: LightKind::Normal,
                        available_after_secs: 0.0,
                        capture_units: 0,
                        feedback_copies: 0,
                    }],
                });
            }
            inventory.consume(item);
            disarm(&mut commands, &mut shop, &selected);
            shop.ignore_board_press = true;
            next_state.set(if target.is_some() {
                MatchPhase::Popping
            } else {
                MatchPhase::Falling
            });
        }
        ShopItem::Upgrade => {
            if let Some(target) = target
                && let Ok((_, _, _, mut kind)) = lights.get_mut(target)
            {
                if let Some(next) = kind.next_tier() {
                    // `rebuild_cores` reacts to the `LightKind` change (body shape + cores).
                    *kind = next;
                    commands.trigger(PowerCreated);
                    inventory.consume(item);
                    disarm(&mut commands, &mut shop, &selected);
                    shop.ignore_board_press = true;
                }
            }
        }
        ShopItem::Swap => unreachable!("Move drag is handled before press-targeted specials"),
    }
}

/// Leaving `Playing` (e.g. into Pause) disarms any booster so a stale armed state and `Selected`
/// highlight don't linger across the overlay.
pub(crate) fn reset_shop(
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    selected: Query<Entity, With<Selected>>,
) {
    disarm(&mut commands, &mut shop, &selected);
    shop.pending_purchase = None;
    shop.open = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_moves_are_inventory_until_they_are_used() {
        let mut inventory = SpecialMoveInventory::default();
        inventory.add(ShopItem::Swap);
        inventory.add(ShopItem::Swap);

        assert_eq!(inventory.count(ShopItem::Swap), 2);
        assert!(inventory.consume(ShopItem::Swap));
        assert_eq!(inventory.count(ShopItem::Swap), 1);
        assert!(inventory.consume(ShopItem::Swap));
        assert!(!inventory.consume(ShopItem::Swap));
    }

    #[test]
    fn life_and_boons_cannot_enter_special_inventory() {
        let mut inventory = SpecialMoveInventory::default();
        inventory.add(ShopItem::Life);
        inventory.add(ShopItem::Boon(BoonKind::RedValue));

        assert_eq!(inventory.count(ShopItem::Life), 0);
        assert_eq!(inventory.count(ShopItem::Boon(BoonKind::RedValue)), 0);
    }

    #[test]
    fn special_move_and_life_prices_are_scaled_by_ten() {
        let run = RunState::default();

        assert_eq!(ShopItem::Swap.cost(&run), Some(200));
        assert_eq!(ShopItem::Eliminate.cost(&run), Some(450));
        assert_eq!(ShopItem::Upgrade.cost(&run), Some(900));
        assert_eq!(ShopItem::Life.cost(&run), Some(800));
    }

    #[test]
    fn special_move_purchase_requires_a_second_confirmation_press() {
        let mut app = App::new();
        app.init_resource::<Shop>()
            .insert_resource(CoreReserve(1_000))
            .insert_resource(CoresSpent(0))
            .init_resource::<RunState>()
            .init_resource::<SpecialMoveInventory>()
            .add_observer(on_shop_purchase_requested);

        app.world_mut()
            .trigger(ShopPurchaseRequested(ShopItem::Swap));
        assert_eq!(
            app.world().resource::<Shop>().pending_purchase_item(),
            Some(ShopItem::Swap)
        );
        assert_eq!(app.world().resource::<CoreReserve>().0, 1_000);
        assert_eq!(
            app.world()
                .resource::<SpecialMoveInventory>()
                .count(ShopItem::Swap),
            0
        );

        app.world_mut()
            .trigger(ShopPurchaseRequested(ShopItem::Swap));

        assert_eq!(app.world().resource::<Shop>().pending_purchase_item(), None);
        assert_eq!(app.world().resource::<CoreReserve>().0, 800);
        assert_eq!(
            app.world()
                .resource::<SpecialMoveInventory>()
                .count(ShopItem::Swap),
            1
        );
    }

    #[test]
    fn life_purchase_revives_an_exhausted_run_and_spends_its_reserve() {
        let mut run = RunState::default();
        run.start_new();
        run.lives = 0;
        let mut app = App::new();
        app.init_resource::<Shop>()
            .insert_resource(CoreReserve(1_000))
            .insert_resource(CoresSpent(0))
            .insert_resource(run)
            .init_resource::<SpecialMoveInventory>()
            .add_observer(on_shop_purchase_requested);

        app.world_mut()
            .trigger(ShopPurchaseRequested(ShopItem::Life));

        assert!(app.world().resource::<RunState>().active);
        assert_eq!(app.world().resource::<RunState>().lives, 1);
        assert_eq!(app.world().resource::<CoreReserve>().0, 200);
        assert_eq!(app.world().resource::<CoresSpent>().0, 800);
    }

    #[test]
    fn move_special_swaps_only_after_drag_release() {
        let mut app = App::new();
        app.init_resource::<Shop>()
            .init_resource::<SpecialMoveInventory>()
            .init_resource::<CascadeDepth>()
            .init_resource::<ShadowCount>()
            .init_resource::<PendingSwap>()
            .init_resource::<PointerInput>()
            .init_resource::<TutorialModalState>()
            .insert_resource(NextState::<MatchPhase>::default())
            .add_systems(Update, shop_targeting);

        let a_pos = GridPos { x: 1, y: 2 };
        let b_pos = GridPos { x: 6, y: 5 };
        let a = app
            .world_mut()
            .spawn((
                Light,
                Movable,
                a_pos,
                VisualPos(to_world(a_pos)),
                LightColor::Red,
                LightKind::Normal,
            ))
            .id();
        let b = app
            .world_mut()
            .spawn((
                Light,
                Movable,
                b_pos,
                VisualPos(to_world(b_pos)),
                LightColor::Blue,
                LightKind::Normal,
            ))
            .id();
        app.world_mut()
            .resource_mut::<SpecialMoveInventory>()
            .add(ShopItem::Swap);
        app.world_mut().resource_mut::<Shop>().armed = Some(ShopItem::Swap);

        {
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.held = true;
            pointer.position_world = Some(to_world(a_pos).xy());
        }
        app.update();

        assert_eq!(app.world().resource::<Shop>().first_pick, Some(a));
        assert_eq!(*app.world().get::<GridPos>(a).unwrap(), a_pos);
        assert!(app.world().get::<MoveDragPreview>(a).is_some());

        {
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = false;
            pointer.just_released = true;
            pointer.held = false;
            pointer.position_world = Some(to_world(b_pos).xy());
        }
        app.update();

        assert_eq!(*app.world().get::<GridPos>(a).unwrap(), b_pos);
        assert_eq!(*app.world().get::<GridPos>(b).unwrap(), a_pos);
        assert_eq!(
            app.world()
                .resource::<SpecialMoveInventory>()
                .count(ShopItem::Swap),
            0
        );
        let pending = app.world().resource::<PendingSwap>().0.as_ref().unwrap();
        assert!(pending.free);
        assert_eq!(pending.a, a);
        assert_eq!(pending.b, Some(b));
        assert!(app.world().resource::<Shop>().armed.is_none());
    }

    #[test]
    fn eliminate_press_does_not_fall_through_into_normal_drag() {
        let mut app = App::new();
        app.init_resource::<Shop>()
            .init_resource::<SpecialMoveInventory>()
            .init_resource::<CascadeDepth>()
            .init_resource::<ShadowCount>()
            .init_resource::<PendingSwap>()
            .init_resource::<crate::gameplay::DragState>()
            .init_resource::<GridLayout>()
            .init_resource::<PointerInput>()
            .init_resource::<TutorialModalState>()
            .insert_resource(NextState::<MatchPhase>::default())
            .add_systems(
                Update,
                (shop_targeting, crate::gameplay::input::handle_input).chain(),
            );

        let target = GridPos { x: 3, y: 4 };
        app.world_mut().spawn((
            Light,
            Movable,
            target,
            LightColor::Yellow,
            LightKind::Normal,
        ));
        app.world_mut()
            .resource_mut::<SpecialMoveInventory>()
            .add(ShopItem::Eliminate);
        app.world_mut().resource_mut::<Shop>().armed = Some(ShopItem::Eliminate);
        {
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.held = true;
            pointer.position_world = Some(to_world(target).xy());
        }

        app.update();

        assert!(app.world().resource::<Shop>().blocks_board_input());
        assert!(!app.world().resource::<crate::gameplay::DragState>().active);
    }

    #[test]
    fn upgrade_booster_system_upgrades_targeted_light() {
        let mut app = App::new();
        app.init_resource::<Shop>();
        app.init_resource::<SpecialMoveInventory>();
        app.init_resource::<CascadeDepth>();
        app.init_resource::<ShadowCount>();
        app.init_resource::<PendingSwap>();
        app.init_resource::<PointerInput>();
        app.init_resource::<TutorialModalState>();
        app.insert_resource(NextState::<MatchPhase>::default());

        app.add_systems(Update, shop_targeting);

        let target_entity = app
            .world_mut()
            .spawn((
                Light,
                GridPos { x: 2, y: 3 },
                LightColor::Red,
                LightKind::Normal,
            ))
            .id();

        // 1. Upgrade: Normal -> RayH
        {
            let mut inventory = app.world_mut().resource_mut::<SpecialMoveInventory>();
            inventory.add(ShopItem::Upgrade);
            let mut shop = app.world_mut().resource_mut::<Shop>();
            shop.armed = Some(ShopItem::Upgrade);
            shop.ignore_board_press = false;
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.position_world = Some(to_world(GridPos { x: 2, y: 3 }).xy());
        }
        app.update();
        assert_eq!(
            *app.world().get::<LightKind>(target_entity).unwrap(),
            LightKind::RayH
        );

        // 2. Upgrade: RayH -> Supernova
        {
            let mut inventory = app.world_mut().resource_mut::<SpecialMoveInventory>();
            inventory.add(ShopItem::Upgrade);
            let mut shop = app.world_mut().resource_mut::<Shop>();
            shop.armed = Some(ShopItem::Upgrade);
            shop.ignore_board_press = false;
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.position_world = Some(to_world(GridPos { x: 2, y: 3 }).xy());
        }
        app.update();
        assert_eq!(
            *app.world().get::<LightKind>(target_entity).unwrap(),
            LightKind::Supernova
        );

        // 3. Upgrade: Supernova -> Cross
        {
            let mut inventory = app.world_mut().resource_mut::<SpecialMoveInventory>();
            inventory.add(ShopItem::Upgrade);
            let mut shop = app.world_mut().resource_mut::<Shop>();
            shop.armed = Some(ShopItem::Upgrade);
            shop.ignore_board_press = false;
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.position_world = Some(to_world(GridPos { x: 2, y: 3 }).xy());
        }
        app.update();
        assert_eq!(
            *app.world().get::<LightKind>(target_entity).unwrap(),
            LightKind::Cross
        );

        // 4. Upgrade: Cross -> Starburst
        {
            let mut inventory = app.world_mut().resource_mut::<SpecialMoveInventory>();
            inventory.add(ShopItem::Upgrade);
            let mut shop = app.world_mut().resource_mut::<Shop>();
            shop.armed = Some(ShopItem::Upgrade);
            shop.ignore_board_press = false;
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.position_world = Some(to_world(GridPos { x: 2, y: 3 }).xy());
        }
        app.update();
        assert_eq!(
            *app.world().get::<LightKind>(target_entity).unwrap(),
            LightKind::Starburst
        );

        // 5. Upgrade: Starburst -> Blackhole
        {
            let mut inventory = app.world_mut().resource_mut::<SpecialMoveInventory>();
            inventory.add(ShopItem::Upgrade);
            let mut shop = app.world_mut().resource_mut::<Shop>();
            shop.armed = Some(ShopItem::Upgrade);
            shop.ignore_board_press = false;
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.position_world = Some(to_world(GridPos { x: 2, y: 3 }).xy());
        }
        app.update();
        assert_eq!(
            *app.world().get::<LightKind>(target_entity).unwrap(),
            LightKind::Blackhole
        );

        // 6. Upgrade: Blackhole -> should not change (top tier)
        {
            let mut inventory = app.world_mut().resource_mut::<SpecialMoveInventory>();
            inventory.add(ShopItem::Upgrade);
            let mut shop = app.world_mut().resource_mut::<Shop>();
            shop.armed = Some(ShopItem::Upgrade);
            shop.ignore_board_press = false;
            let mut pointer = app.world_mut().resource_mut::<PointerInput>();
            pointer.just_pressed = true;
            pointer.position_world = Some(to_world(GridPos { x: 2, y: 3 }).xy());
        }
        app.update();
        assert_eq!(
            *app.world().get::<LightKind>(target_entity).unwrap(),
            LightKind::Blackhole
        );
    }
}

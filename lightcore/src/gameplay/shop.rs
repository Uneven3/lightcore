//! In-game shop: boosters the player buys with lightcores in reserve to bend the board's rules,
//! Candy-Crush style. Three abilities, armed from a bottom HUD bar (spawned in `ui::setup_ui`) and
//! then aimed by clicking lights:
//!
//! - **Swap** — force two lights to trade places, even non-adjacent / non-matching (a `free`
//!   `SwapData`, so the normal "snap back on no match" never fires).
//! - **Eliminate** — pop one light, then the usual `Popping→Falling→…` pipeline refills.
//! - **Upgrade** — raise a light one tier (`LightKind::next_tier`); `visuals::core_motion::
//!   rebuild_cores` reacts to the kind change and rebuilds its body + cores.
//!
//! Capturing lights always feeds the main lightcore counter (`Score`) and the spendable reserve
//! (`CoreReserve`). Buying a booster spends only the reserve — `Score` (the level's goal progress)
//! is untouched — and in `GameMode::Run`, `CoreReserve` is the run's persistent currency: it
//! carries over between levels instead of resetting, so boons bought early still cost the wallet
//! that funds later levels' shopping (see `gameplay::lifecycle::setup_match`).

use bevy::prelude::*;

use super::{CascadeDepth, ChainPop, CoreReserve, CoresSpent, PendingSwap, PowerCreated, SwapData};
use crate::core::locale::{Language, TrKey};
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RunState};
use crate::input::pointer::PointerInput;
use crate::state::MatchPhase;
use crate::ui::TutorialState;
use crate::visuals::assets::VisualCache;
use crate::visuals::particles::{ParticleSettings, spawn_burst};

/// The three boosters the shop sells. Costs are deliberately cheap so boosters are part of normal
/// play, not an end-game splurge.
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
            ShopItem::Swap => Some(20),
            ShopItem::Eliminate => Some(45),
            ShopItem::Upgrade => Some(90),
            ShopItem::Life => Some(80),
            ShopItem::Boon(boon) => run.boon_cost(boon),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn label(self, lang: Language) -> &'static str {
        match self {
            ShopItem::Swap => lang.tr(TrKey::ShopSwap),
            ShopItem::Eliminate => lang.tr(TrKey::ShopEliminate),
            ShopItem::Upgrade => lang.tr(TrKey::ShopUpgrade),
            ShopItem::Life => lang.tr(TrKey::ShopLife),
            ShopItem::Boon(boon) => boon.label(lang),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn status_label(self, lang: Language) -> &'static str {
        match self {
            ShopItem::Swap => lang.tr(TrKey::ShopSwapStatus),
            ShopItem::Eliminate => lang.tr(TrKey::ShopEliminateStatus),
            ShopItem::Upgrade => lang.tr(TrKey::ShopUpgradeStatus),
            ShopItem::Life => lang.tr(TrKey::ShopLifeStatus),
            ShopItem::Boon(boon) => boon.status_label(lang),
        }
    }

    pub(crate) fn is_boon(self) -> bool {
        matches!(self, ShopItem::Boon(_))
    }
}

/// In-game shop state. `armed` is the booster the player picked off the bar and is now aiming;
/// `first_pick` holds the first light chosen for a two-step Swap.
#[derive(Resource, Default)]
pub(crate) struct Shop {
    armed: Option<ShopItem>,
    first_pick: Option<Entity>,
    ignore_board_press: bool,
    pending_purchase: Option<ShopItem>,
    pub(crate) open: bool,
}

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

/// Click target for an owned special-move counter in the integrated HUD panel.
#[derive(Component, Clone, Copy)]
pub(crate) struct SpecialMoveButton(pub(crate) ShopItem);

impl Shop {
    /// Whether a booster is currently aiming — `gameplay::input::handle_input` bails when so, so
    /// the drag-swap doesn't fight the targeting click.
    pub(crate) fn is_armed(&self) -> bool {
        self.armed.is_some()
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

    pub(crate) fn active_badge_text(&self, lang: Language) -> Option<String> {
        let item = self.armed?;
        Some(match item {
            ShopItem::Swap if self.first_pick.is_some() => {
                lang.tr(TrKey::ArmedSwap1of2).to_string()
            }
            ShopItem::Swap => lang.tr(TrKey::ArmedSwap).to_string(),
            ShopItem::Eliminate => lang.tr(TrKey::ArmedEliminate).to_string(),
            ShopItem::Upgrade => lang.tr(TrKey::ArmedUpgrade).to_string(),
            ShopItem::Life => return None,
            ShopItem::Boon(_) => return None,
        })
    }
}

/// One booster button on the in-game bar (spawned by `ui::setup_ui`).
#[derive(Component, Clone, Copy)]
pub(crate) struct ShopButton(pub(crate) ShopItem);



#[derive(Component)]
pub(crate) struct ShopCard;

#[allow(dead_code)]
pub(crate) const BTN_ARMED: Color = Color::srgb(0.85, 0.65, 0.18);
pub(crate) const BTN_BORDER_BROKE: Color = Color::srgba(0.35, 0.40, 0.48, 0.18);
pub(crate) const BTN_BORDER_ARMED: Color = Color::srgba(1.0, 0.86, 0.46, 0.88);

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
        commands.entity(e).remove::<Selected>();
    }
}

fn disarm(commands: &mut Commands, shop: &mut Shop, selected: &Query<Entity, With<Selected>>) {
    shop.armed = None;
    shop.ignore_board_press = false;
    clear_pick(commands, shop, selected);
}

fn arm(shop: &mut Shop, item: ShopItem, pointer: &PointerInput) {
    shop.armed = Some(item);
    shop.ignore_board_press = pointer.just_pressed;
}

/// Purchases one special move into inventory. Targeting is deliberately separate: it starts only
/// when the player taps an owned counter in the status panel.
pub(crate) fn shop_button_system(
    mut shop: ResMut<Shop>,
    mut reserve: ResMut<CoreReserve>,
    mut spent: ResMut<CoresSpent>,
    mut run: ResMut<RunState>,
    mut inventory: ResMut<SpecialMoveInventory>,
    interactions: Query<(&Interaction, &ShopButton), Changed<Interaction>>,
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let item = btn.0;
        let Some(cost) = item.cost(&run) else {
            continue;
        };
        if item == ShopItem::Life {
            shop.pending_purchase = None;
            if reserve.0 >= cost {
                run.lives += 1;
                spend(&mut reserve, &mut spent, cost);
                shop.open = false;
            }
            continue;
        }
        if item.is_boon() {
            // Boons deliberately have no in-level purchase path. `ShopItem::Boon` remains a
            // presentation model for tooltips/reward cards, while the completion overlay owns
            // the only transaction that may call `RunState::buy`.
            continue;
        }
        if reserve.0 < cost {
            shop.pending_purchase = None;
            continue;
        }
        if shop.pending_purchase != Some(item) {
            shop.pending_purchase = Some(item);
            continue;
        }
        shop.pending_purchase = None;
        inventory.add(item);
        spend(&mut reserve, &mut spent, cost);
        shop.open = false;
    }
}

/// Arms one already-owned special move. The counter is only decremented by `shop_targeting` after
/// a valid board action, so cancelling or changing target never burns a purchase.
pub(crate) fn special_move_button_system(
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    inventory: Res<SpecialMoveInventory>,
    pointer: Res<PointerInput>,
    interactions: Query<(&Interaction, &SpecialMoveButton), Changed<Interaction>>,
    selected: Query<Entity, With<Selected>>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed || inventory.count(button.0) == 0 {
            continue;
        }
        shop.pending_purchase = None;
        if shop.armed == Some(button.0) {
            disarm(&mut commands, &mut shop, &selected);
        } else {
            if shop.is_armed() {
                disarm(&mut commands, &mut shop, &selected);
            }
            clear_pick(&mut commands, &mut shop, &selected);
            arm(&mut shop, button.0, &pointer);
        }
    }
}

/// While a booster is armed, a press on a board light applies it. Runs before `handle_input`,
/// which bails whenever `Shop::is_armed`. Cancel via the UI button (cross-platform, works on touch).
#[allow(clippy::too_many_arguments)]
pub(crate) fn shop_targeting(
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    mut inventory: ResMut<SpecialMoveInventory>,
    mut cascade: ResMut<CascadeDepth>,
    mut next_state: ResMut<NextState<MatchPhase>>,
    mut pending: ResMut<PendingSwap>,
    pointer: Res<PointerInput>,
    cache: Res<VisualCache>,
    particles: Res<ParticleSettings>,
    mut lights: Query<(Entity, &mut GridPos, &LightColor, &mut LightKind), With<Light>>,
    selected: Query<Entity, With<Selected>>,
    tutorial: Res<TutorialState>,
) {
    if tutorial.open {
        return;
    }
    let Some(item) = shop.armed else {
        return;
    };

    if !pointer.just_pressed {
        return;
    }

    if shop.ignore_board_press {
        shop.ignore_board_press = false;
        return;
    }

    let Some(world) = pointer.position_world else {
        return;
    };
    let Some(gp) = to_grid(world) else {
        return;
    };
    let Some(target) = lights
        .iter()
        .find(|(_, p, _, _)| **p == gp)
        .map(|(e, _, _, _)| e)
    else {
        return;
    };

    match item {
        ShopItem::Boon(_) | ShopItem::Life => {
            disarm(&mut commands, &mut shop, &selected);
        }
        ShopItem::Eliminate => {
            // Pop one light into the normal pipeline. `points: 0` so it grants no score and spawns
            // no score-shards (`on_chain_pop_score_light` early-returns on 0); the burst below is
            // the only flourish.
            let Ok((_, pos, color, _)) = lights.get(target) else {
                return;
            };
            cascade.0 = 1;
            let (color, w) = (*color, to_world(*pos));
            commands
                .entity(target)
                .insert(PopAnim(Timer::from_seconds(0.18, TimerMode::Once)));
            spawn_burst(
                &mut commands,
                cache.core_image.clone(),
                w,
                color.glow_color(),
                particles.pop_burst_count,
                particles.burst_radius,
            );
            commands.trigger(ChainPop {
                removed: 1,
                points: 0,
                hollow: false,
                pops: vec![(w, color, 0.0)],
                starburst_origins: Vec::new(),
                supernova_origins: Vec::new(),
            });
            inventory.consume(item);
            disarm(&mut commands, &mut shop, &selected);
            next_state.set(MatchPhase::Popping);
        }
        ShopItem::Upgrade => {
            if let Ok((_, _, _, mut kind)) = lights.get_mut(target) {
                if let Some(next) = kind.next_tier() {
                    // `rebuild_cores` reacts to the `LightKind` change (body shape + cores).
                    *kind = next;
                    commands.trigger(PowerCreated);
                    inventory.consume(item);
                    disarm(&mut commands, &mut shop, &selected);
                }
            }
        }
        ShopItem::Swap => match shop.first_pick {
            None => {
                shop.first_pick = Some(target);
                commands.entity(target).insert(Selected);
            }
            Some(first) if first == target => {
                // Click the same light again to cancel the pick.
                clear_pick(&mut commands, &mut shop, &selected);
            }
            Some(first) => {
                // Force the trade via a `free` SwapData: no move cost, no revert on no-match. The
                // existing swap pipeline (combos, cascades) takes it from here.
                let Ok((_, a_pos_ref, _, _)) = lights.get(first) else {
                    clear_pick(&mut commands, &mut shop, &selected);
                    return;
                };
                let Ok((_, b_pos_ref, _, _)) = lights.get(target) else {
                    clear_pick(&mut commands, &mut shop, &selected);
                    return;
                };
                let (a_pos, b_pos) = (*a_pos_ref, *b_pos_ref);

                if let Ok((_, mut p, _, _)) = lights.get_mut(first) {
                    *p = b_pos;
                }
                if let Ok((_, mut p, _, _)) = lights.get_mut(target) {
                    *p = a_pos;
                }
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
        },
    }
}

/// Purchase targets are intentionally borderless; affordability is communicated by the cost text
/// color in `ui::update_shop_button_texts`, without bringing back large boxed controls.
pub(crate) fn update_shop_buttons(
    mut buttons: Query<(&ShopButton, &mut BackgroundColor, &mut BorderColor), With<ShopCard>>,
) {
    for (_, mut bg, mut border) in &mut buttons {
        bg.0 = Color::NONE;
        *border = BorderColor::all(Color::NONE);
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
    use crate::visuals::breathing::BreathPhase;

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
    fn special_move_purchase_requires_a_second_confirmation_press() {
        let mut app = App::new();
        app.init_resource::<Shop>()
            .insert_resource(CoreReserve(100))
            .insert_resource(CoresSpent(0))
            .init_resource::<RunState>()
            .init_resource::<SpecialMoveInventory>()
            .add_systems(Update, shop_button_system);

        let button = app
            .world_mut()
            .spawn((Interaction::Pressed, ShopButton(ShopItem::Swap)))
            .id();

        app.update();
        assert_eq!(
            app.world().resource::<Shop>().pending_purchase_item(),
            Some(ShopItem::Swap)
        );
        assert_eq!(app.world().resource::<CoreReserve>().0, 100);
        assert_eq!(
            app.world()
                .resource::<SpecialMoveInventory>()
                .count(ShopItem::Swap),
            0
        );

        *app
            .world_mut()
            .entity_mut(button)
            .get_mut::<Interaction>()
            .expect("purchase button has Interaction") = Interaction::None;
        app.update();
        *app
            .world_mut()
            .entity_mut(button)
            .get_mut::<Interaction>()
            .expect("purchase button has Interaction") = Interaction::Pressed;
        app.update();

        assert_eq!(
            app.world().resource::<Shop>().pending_purchase_item(),
            None
        );
        assert_eq!(app.world().resource::<CoreReserve>().0, 80);
        assert_eq!(
            app.world()
                .resource::<SpecialMoveInventory>()
                .count(ShopItem::Swap),
            1
        );
    }

    #[test]
    fn upgrade_booster_system_upgrades_targeted_light() {
        let mut app = App::new();
        app.init_resource::<Shop>();
        app.init_resource::<SpecialMoveInventory>();
        app.init_resource::<CascadeDepth>();
        app.init_resource::<PendingSwap>();
        app.init_resource::<PointerInput>();
        app.insert_resource(VisualCache {
            ring_mesh: Default::default(),
            ring_mat: Default::default(),
            hollow_mesh: Default::default(),
            hollow_mat: Default::default(),
            spark_mesh: Default::default(),
            spark_mat: Default::default(),
            shadow_mesh: Default::default(),
            shadow_mat: Default::default(),
            hard_shadow_mat: Default::default(),
            blocker_mesh: Default::default(),
            blocker_mat: Default::default(),
            burst_mesh: Default::default(),
            membrane_mesh: Default::default(),
            core_image: Default::default(),
            glow_image: Default::default(),
            shard_core_image: Default::default(),
            light_core_image: Default::default(),
            unit_quad_mesh: Default::default(),
            beam_image: Default::default(),
            square_image: Default::default(),
            cross_mesh: Default::default(),
            starburst_mesh: Default::default(),
            blackhole_mesh: Default::default(),
            effect: Default::default(),
            blackhole_void_mesh: Default::default(),
            blackhole_rim_mesh: Default::default(),
            grid_cell_image: Default::default(),
        });
        app.init_resource::<ParticleSettings>();
        app.init_resource::<TutorialState>();
        app.insert_resource(NextState::<MatchPhase>::default());
        
        app.add_systems(Update, (
            shop_targeting,
            crate::visuals::core_motion::rebuild_cores,
        ).chain());
        
        let target_entity = app.world_mut().spawn((
            Light,
            GridPos { x: 2, y: 3 },
            LightColor::Red,
            LightKind::Normal,
            BreathPhase(0.0),
        )).id();
        
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
        assert_eq!(*app.world().get::<LightKind>(target_entity).unwrap(), LightKind::RayH);
        
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
        assert_eq!(*app.world().get::<LightKind>(target_entity).unwrap(), LightKind::Supernova);

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
        assert_eq!(*app.world().get::<LightKind>(target_entity).unwrap(), LightKind::Cross);

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
        assert_eq!(*app.world().get::<LightKind>(target_entity).unwrap(), LightKind::Starburst);

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
        assert_eq!(*app.world().get::<LightKind>(target_entity).unwrap(), LightKind::Blackhole);
        
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
        assert_eq!(*app.world().get::<LightKind>(target_entity).unwrap(), LightKind::Blackhole);
    }
}

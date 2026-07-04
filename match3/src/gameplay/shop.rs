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
//! (`CoreReserve`). Buying a booster spends only the reserve, so the campaign goal still measures
//! how many lightcores the player captured during the run.

use bevy::prelude::*;

use super::{
    CascadeDepth, ChainPop, CoreReserve, CoresSpent, DisplayedScore, PendingSwap, PowerCreated,
    Score, SwapData,
};
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RunState};
use crate::input::pointer::PointerInput;
use crate::state::GameState;
use crate::visuals::assets::VisualCache;
use crate::visuals::particles::{ParticleSettings, spawn_burst};

/// The three boosters the shop sells. Costs are deliberately cheap so boosters are part of normal
/// play, not an end-game splurge.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShopItem {
    Swap,
    Eliminate,
    Upgrade,
    Boon(BoonKind),
}

impl ShopItem {
    /// Bar order (left→right), also used to spawn the buttons.
    pub(crate) const ALL: [ShopItem; 8] = [
        ShopItem::Swap,
        ShopItem::Eliminate,
        ShopItem::Upgrade,
        ShopItem::Boon(BoonKind::RedValue),
        ShopItem::Boon(BoonKind::GreenReserve),
        ShopItem::Boon(BoonKind::BlueMoves),
        ShopItem::Boon(BoonKind::SparkBounty),
        ShopItem::Boon(BoonKind::PowerBounty),
    ];

    pub(crate) fn cost(self, run: &RunState) -> Option<u32> {
        match self {
            ShopItem::Swap => Some(20),
            ShopItem::Eliminate => Some(45),
            ShopItem::Upgrade => Some(90),
            ShopItem::Boon(boon) => run.boon_cost(boon),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ShopItem::Swap => "Cambiar",
            ShopItem::Eliminate => "Eliminar",
            ShopItem::Upgrade => "Subir tier",
            ShopItem::Boon(boon) => boon.label(),
        }
    }

    pub(crate) fn status_label(self) -> &'static str {
        match self {
            ShopItem::Swap => "Reposiciona 2 luces",
            ShopItem::Eliminate => "Rompe 1 luz",
            ShopItem::Upgrade => "Eleva 1 tier",
            ShopItem::Boon(boon) => boon.status_label(),
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
    pub(crate) open: bool,
}

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

    pub(crate) fn active_badge_text(&self) -> Option<String> {
        let item = self.armed?;
        Some(match item {
            ShopItem::Swap if self.first_pick.is_some() => "Cambiar activo 1/2".to_string(),
            ShopItem::Swap => "Cambiar activo".to_string(),
            ShopItem::Eliminate => "Eliminar activo".to_string(),
            ShopItem::Upgrade => "Subir tier activo".to_string(),
            ShopItem::Boon(_) => return None,
        })
    }
}

/// One booster button on the in-game bar (spawned by `ui::setup_ui`).
#[derive(Component, Clone, Copy)]
pub(crate) struct ShopButton(pub(crate) ShopItem);

/// Root of the booster bar — tagged so the HUD show/hide (`ui::HudFilter`) includes it.
#[derive(Component)]
pub(crate) struct ShopBar;

#[derive(Component)]
pub(crate) struct ShopCard;

/// Button background = neutral when affordable, dim grey when too expensive, gold when armed.
pub(crate) const BTN_IDLE: Color = Color::srgb(0.12, 0.12, 0.20);
const BTN_BROKE: Color = Color::srgb(0.06, 0.06, 0.09);
const BTN_ARMED: Color = Color::srgb(0.85, 0.65, 0.18);
pub(crate) const BTN_BORDER_IDLE: Color = Color::srgba(0.50, 0.74, 1.0, 0.20);
pub(crate) const BTN_BORDER_BROKE: Color = Color::srgba(0.35, 0.40, 0.48, 0.18);
pub(crate) const BTN_BORDER_ARMED: Color = Color::srgba(1.0, 0.86, 0.46, 0.88);

/// Spend only from the booster reserve; captured lightcores stay captured.
fn spend(
    score: &mut Score,
    displayed_score: &mut DisplayedScore,
    reserve: &mut CoreReserve,
    spent: &mut CoresSpent,
    cost: u32,
) {
    score.0 = score.0.saturating_sub(cost);
    displayed_score.0 = displayed_score.0.saturating_sub(cost);
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
    clear_pick(commands, shop, selected);
}

/// Arms / disarms a booster when its bar button is clicked. Doesn't charge — payment happens when
/// the ability is actually applied (`shop_targeting`).
pub(crate) fn shop_button_system(
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    mut reserve: ResMut<CoreReserve>,
    mut score: ResMut<Score>,
    mut displayed_score: ResMut<DisplayedScore>,
    mut spent: ResMut<CoresSpent>,
    mut run: ResMut<RunState>,
    interactions: Query<(&Interaction, &ShopButton), Changed<Interaction>>,
    selected: Query<Entity, With<Selected>>,
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let item = btn.0;
        let Some(cost) = item.cost(&run) else {
            continue;
        };
        if item.is_boon() {
            if reserve.0 >= cost
                && let ShopItem::Boon(boon) = item
                && run.buy(boon)
            {
                spend(
                    &mut score,
                    &mut displayed_score,
                    &mut reserve,
                    &mut spent,
                    cost,
                );
                shop.open = false;
            }
            continue;
        }
        if shop.armed == Some(item) {
            // Click the armed booster again to put it away.
            disarm(&mut commands, &mut shop, &selected);
        } else if reserve.0 >= cost {
            // Arm / switch boosters — drop any leftover Swap pick first.
            clear_pick(&mut commands, &mut shop, &selected);
            shop.armed = Some(item);
            shop.open = false; // Close drawer when armed!
        }
        // Can't afford → ignore (the button is shown greyed by `update_shop_buttons`).
    }
}

/// While a booster is armed, a press on a board light applies it. Runs before `handle_input`,
/// which bails whenever `Shop::is_armed`. Cancel via the UI button (cross-platform, works on touch).
#[allow(clippy::too_many_arguments)]
pub(crate) fn shop_targeting(
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    mut score: ResMut<Score>,
    mut displayed_score: ResMut<DisplayedScore>,
    mut reserve: ResMut<CoreReserve>,
    mut spent: ResMut<CoresSpent>,
    mut cascade: ResMut<CascadeDepth>,
    mut next_state: ResMut<NextState<GameState>>,
    mut pending: ResMut<PendingSwap>,
    pointer: Res<PointerInput>,
    cache: Res<VisualCache>,
    particles: Res<ParticleSettings>,
    mut lights: Query<(Entity, &mut GridPos, &LightColor, &mut LightKind), With<Light>>,
    selected: Query<Entity, With<Selected>>,
    run: Res<RunState>,
) {
    let Some(item) = shop.armed else {
        return;
    };

    if !pointer.just_pressed {
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
        ShopItem::Boon(_) => {
            disarm(&mut commands, &mut shop, &selected);
        }
        ShopItem::Eliminate => {
            // Pop one light into the normal pipeline. `points: 0` so it grants no score and spawns
            // no score-shards (`on_chain_pop_score_light` early-returns on 0); the burst below is
            // the only flourish.
            cascade.0 = 1;
            let (_, pos, color, _) = lights.get(target).unwrap();
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
                pops: vec![(w, color, 0.0)],
            });
            spend(
                &mut score,
                &mut displayed_score,
                &mut reserve,
                &mut spent,
                item.cost(&run).unwrap_or(0),
            );
            disarm(&mut commands, &mut shop, &selected);
            next_state.set(GameState::Popping);
        }
        ShopItem::Upgrade => {
            let cur = *lights.get(target).unwrap().3;
            if let Some(next) = cur.next_tier() {
                // `rebuild_cores` reacts to the `LightKind` change (body shape + cores).
                *lights.get_mut(target).unwrap().3 = next;
                commands.trigger(PowerCreated);
                spend(
                    &mut score,
                    &mut displayed_score,
                    &mut reserve,
                    &mut spent,
                    item.cost(&run).unwrap_or(0),
                );
                disarm(&mut commands, &mut shop, &selected);
            }
            // Already a Blackhole (max tier) → no charge, stays armed for another pick.
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
                let a_pos = *lights.get(first).unwrap().1;
                let b_pos = *lights.get(target).unwrap().1;
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
                spend(
                    &mut score,
                    &mut displayed_score,
                    &mut reserve,
                    &mut spent,
                    item.cost(&run).unwrap_or(0),
                );
                clear_pick(&mut commands, &mut shop, &selected);
                shop.armed = None;
                next_state.set(GameState::SwapAnimating);
            }
        },
    }
}

/// Repaints the bar each frame: armed = gold, affordable = neutral, too dear = dim grey.
pub(crate) fn update_shop_buttons(
    reserve: Res<CoreReserve>,
    run: Res<RunState>,
    shop: Res<Shop>,
    mut buttons: Query<(&ShopButton, &mut BackgroundColor, &mut BorderColor), With<ShopCard>>,
) {
    for (btn, mut bg, mut border) in &mut buttons {
        let item = btn.0;
        let cost = item.cost(&run);
        let (bg_color, border_color) = if shop.armed == Some(item) {
            (BTN_ARMED, BTN_BORDER_ARMED)
        } else if cost.is_some_and(|cost| reserve.0 >= cost) {
            (BTN_IDLE, BTN_BORDER_IDLE)
        } else {
            (BTN_BROKE, BTN_BORDER_BROKE)
        };
        bg.0 = bg_color;
        *border = BorderColor::all(border_color);
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
    shop.open = false;
}

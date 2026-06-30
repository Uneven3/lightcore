//! Blackhole mode resolution. Power lights are *tokens* (`BlackholeTier`, 2..=5 cores) that never
//! detonate: a 4+ match forges a token (`tier = pieces − 2`), and swapping two adjacent same-tier
//! tokens fuses them up a tier. Two tier-5s fused trigger the Blackhole, which swallows the board.
//! Runs alongside the Classic systems, each gated by `GameMode` so neither sees the other's path.

use bevy::prelude::*;

use crate::audio::{play, SoundAssets};
use crate::core::prelude::*;
use crate::state::GameState;
use crate::visuals::assets::VisualCache;
use crate::visuals::particles::{spawn_burst, POP_BURST_COUNT};
use super::{CascadeDepth, ChainPop, GameMode, MovesLeft, PendingSwap, Score, SwapData, SwapHappened};

/// Fired when two tier-5 tokens fuse. The board implodes and the level is won.
#[derive(Event)]
pub(crate) struct BlackholeTriggered;

/// Pops every cell in `bh.to_remove` (normal pop + score + burst + `ChainPop` for the score
/// shards) and turns each `to_upgrade` host into a `BlackholeTier` token (rendered reactively by
/// `visuals::core_motion::rebuild_blackhole_cores`). No power detonation — that's the whole point.
fn apply_blackhole_match(
    commands: &mut Commands,
    cache: &VisualCache,
    materials: &mut Assets<ColorMaterial>,
    score: &mut u32,
    cascade: u32,
    info: &EntityInfo,
    bh: &BlackholeMatch,
) {
    for &(e, tier, horizontal) in &bh.to_upgrade {
        commands.entity(e).try_insert(BlackholeTier { tier, horizontal });
    }
    let points = bh.to_remove.len() as u32 * cascade;
    *score += points;
    let mut pops: Vec<(Vec3, LightColor)> = Vec::new();
    for &e in &bh.to_remove {
        commands.entity(e).insert(PopAnim(Timer::from_seconds(0.15, TimerMode::Once)));
        if let Some(&(pos, color, _)) = info.get(&e) {
            let w = to_world(pos);
            spawn_burst(commands, cache.burst_mesh.clone(), materials, w, color.glow_color(), POP_BURST_COUNT);
            pops.push((w, color));
        }
    }
    commands.trigger(ChainPop { removed: bh.to_remove.len() as u32, points, pops });
}

fn revert_swap(
    lights: &mut Query<(Entity, &mut GridPos, &LightColor, Option<&BlackholeTier>), With<Light>>,
    swap: &SwapData,
) {
    if let Ok((_, mut gp, _, _)) = lights.get_mut(swap.a) { gp.set_if_neq(swap.a_pos); }
    if let Some(b) = swap.b {
        if let Ok((_, mut gp, _, _)) = lights.get_mut(b) { gp.set_if_neq(swap.b_pos); }
    }
}

/// Player swap in Blackhole mode. Two adjacent same-tier tokens fuse (or trigger the Blackhole at
/// tier 5); otherwise it's a normal match (which may forge a token); a swap that does neither
/// reverts. Sibling observer to the Classic `swap::on_swap_happened`, each guarded by `GameMode`.
pub(crate) fn on_swap_happened_blackhole(
    _: On<SwapHappened>,
    mut commands: Commands,
    mode: Res<GameMode>,
    mut pending: ResMut<PendingSwap>,
    mut score: ResMut<Score>,
    mut moves: ResMut<MovesLeft>,
    mut cascade: ResMut<CascadeDepth>,
    mut next_state: ResMut<NextState<GameState>>,
    cache: Res<VisualCache>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    sounds: Res<SoundAssets>,
    mut lights: Query<(Entity, &mut GridPos, &LightColor, Option<&BlackholeTier>), With<Light>>,
) {
    if *mode != GameMode::Blackhole { return; }
    let Some(swap) = pending.0.take() else { return; };
    cascade.0 = 1;

    let tier_a = lights.get(swap.a).ok().and_then(|(_, _, _, t)| t.map(|t| t.tier));

    // Fusion path: both swapped cells are tokens.
    if let Some(b) = swap.b {
        let tier_b = lights.get(b).ok().and_then(|(_, _, _, t)| t.map(|t| t.tier));
        if let (Some(ta), Some(tb)) = (tier_a, tier_b) {
            match resolve_blackhole_swap(ta, tb) {
                BlackholeSwap::Fuse(new_tier) => {
                    moves.0 = moves.0.saturating_sub(1);
                    commands.entity(swap.a).try_insert(BlackholeTier { tier: new_tier, horizontal: false });
                    if let Ok((_, gp, color, _)) = lights.get(b) {
                        let w = to_world(*gp);
                        commands.entity(b).insert(PopAnim(Timer::from_seconds(0.15, TimerMode::Once)));
                        spawn_burst(&mut commands, cache.burst_mesh.clone(), &mut materials, w, color.glow_color(), POP_BURST_COUNT);
                        score.0 += new_tier as u32;
                        commands.trigger(ChainPop { removed: 1, points: new_tier as u32, pops: vec![(w, *color)] });
                    }
                    play(&mut commands, sounds.special_created.clone());
                    next_state.set(GameState::Popping);
                    return;
                }
                BlackholeSwap::Blackhole => {
                    moves.0 = moves.0.saturating_sub(1);
                    play(&mut commands, sounds.special_created.clone());
                    commands.trigger(BlackholeTriggered);
                    return;
                }
                BlackholeSwap::Invalid => {
                    revert_swap(&mut lights, &swap);
                    play(&mut commands, sounds.swap_invalid.clone());
                    next_state.set(GameState::Playing);
                    return;
                }
            }
        }
    }

    // Normal match path (at least one cell isn't a token).
    let grid: Grid = lights.iter().map(|(e, p, c, _)| (*p, (e, *c, LightKind::Normal))).collect();
    let info: EntityInfo = lights.iter().map(|(e, p, c, _)| (e, (*p, *c, LightKind::Normal))).collect();
    let bh = scan_runs_blackhole(&grid, Some(swap.a));
    if bh.to_remove.is_empty() && bh.to_upgrade.is_empty() {
        revert_swap(&mut lights, &swap);
        play(&mut commands, sounds.swap_invalid.clone());
        next_state.set(GameState::Playing);
        return;
    }
    moves.0 = moves.0.saturating_sub(1);
    play(&mut commands, sounds.swap_valid.clone());
    play(&mut commands, sounds.match_pop.clone());
    apply_blackhole_match(&mut commands, &cache, &mut materials, &mut score.0, cascade.0, &info, &bh);
    next_state.set(GameState::Popping);
}

/// Cascade resolution in Blackhole mode (sibling to `chain::check_chain_matches`). Re-scans for
/// matches after a settle; forges tokens / pops; never shuffles (that would wipe tiers). No level
/// goal — the only win is the Blackhole (`on_blackhole_triggered`); otherwise it returns to play.
pub(crate) fn check_chain_blackhole(
    mut commands: Commands,
    mut score: ResMut<Score>,
    mut next_state: ResMut<NextState<GameState>>,
    sounds: Res<SoundAssets>,
    cache: Res<VisualCache>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut cascade: ResMut<CascadeDepth>,
    lights: Query<(Entity, &GridPos, &LightColor), With<Light>>,
) {
    cascade.0 += 1;
    let grid: Grid = lights.iter().map(|(e, p, c)| (*p, (e, *c, LightKind::Normal))).collect();
    let info: EntityInfo = lights.iter().map(|(e, p, c)| (e, (*p, *c, LightKind::Normal))).collect();

    let bh = scan_runs_blackhole(&grid, None);
    if bh.to_remove.is_empty() && bh.to_upgrade.is_empty() {
        next_state.set(GameState::Playing);
        return;
    }
    play(&mut commands, sounds.cascade.clone());
    apply_blackhole_match(&mut commands, &cache, &mut materials, &mut score.0, cascade.0, &info, &bh);
    next_state.set(GameState::Popping);
}

/// The Blackhole: every light streams into the score (reusing the `ChainPop` collection VFX) and
/// the board is cleared — the level is won. The orbit-into-the-centre implosion is a later polish.
pub(crate) fn on_blackhole_triggered(
    _: On<BlackholeTriggered>,
    mut commands: Commands,
    mut score: ResMut<Score>,
    mut next_state: ResMut<NextState<GameState>>,
    lights: Query<(Entity, &GridPos, &LightColor), With<Light>>,
) {
    let pops: Vec<(Vec3, LightColor)> = lights.iter().map(|(_, p, c)| (to_world(*p), *c)).collect();
    let removed = pops.len() as u32;
    let points = removed * 10;
    score.0 += points;
    commands.trigger(ChainPop { removed, points, pops });
    for (e, _, _) in &lights {
        commands.entity(e).despawn();
    }
    next_state.set(GameState::LevelComplete);
}

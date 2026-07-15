//! Bridges resolved power-light waves to the visual layer's events. The pure resolution in
//! `core::matching` decides *what* clears; these helpers decide *how it reads* — one unified
//! `PowerCombo` animation per combined pair, the standard flash+beam per lone power, and a single
//! board-clearing flash for a 3+ super-combo. Centralizing it here keeps the three resolution
//! sites (`swap::on_swap_happened` paths A/B and `chain::check_chain_matches` phases 1/2) in sync.

use bevy::prelude::*;
use std::collections::HashMap;

use super::popping::{accumulate_pop_delays, merge_pop_delay};
use super::{PowerBlastTrail, PowerCombo, PowerConsumed};
use crate::core::prelude::*;
use crate::visuals::RaySettings;

/// Fires the VFX for one lone (uncombined) power activation — the flash + traveling beam — and
/// stages its pop-delay ripple. The single-power path that `PowerConsumed`/`PowerBlastTrail` have
/// always driven.
pub(crate) fn trigger_single_vfx(
    commands: &mut Commands,
    activation: &PowerActivation,
    grid: &Grid,
    entity_info: &EntityInfo,
    pop_delays: &mut HashMap<Entity, f32>,
    settings: &RaySettings,
) {
    commands.trigger(PowerConsumed {
        kind: activation.kind,
        pos: activation.pos,
        color: activation.partner_color,
    });
    commands.trigger(PowerBlastTrail {
        kind: activation.kind,
        color: activation.partner_color,
        path: blast_path(activation, entity_info)
            .into_iter()
            .map(to_world)
            .collect(),
        delay_secs: 0.0,
    });
    accumulate_pop_delays(pop_delays, activation, grid, entity_info, settings);
}

/// Emits one unified `PowerCombo` for a combined pair: normalizes which position is the
/// choreography's anchor (the Starburst for star combos, the Supernova for line+supernova) and
/// resolves the target color for Starburst combos from the partner cell.
pub(crate) fn trigger_combo(
    commands: &mut Commands,
    grid: &Grid,
    entity_info: &EntityInfo,
    a: &PowerActivation,
    b: &PowerActivation,
    kind: ComboKind,
    ray: &RaySettings,
    pop_delays: &mut HashMap<Entity, f32>,
) {
    use LightKind::*;
    let star = matches!(
        kind,
        ComboKind::StarLine | ComboKind::StarSupernova | ComboKind::StarColor
    );
    let (origin, partner) = if star {
        if a.kind == Starburst { (a, b) } else { (b, a) }
    } else if kind == ComboKind::LineSupernova {
        if a.kind == Supernova { (a, b) } else { (b, a) }
    } else {
        (a, b)
    };
    let color = if star {
        grid.get(&partner.pos).map(|(_, c, _)| *c)
    } else {
        None
    };
    commands.trigger(PowerCombo {
        kind,
        a_pos: origin.pos,
        b_pos: partner.pos,
        color,
    });

    // StarLine/StarSupernova share the two-phase choreography (see
    // `trigger_star_transform_combo`): each also owns the pop delays for every cell it affects, so
    // they're handled separately from the other star combos.
    if matches!(kind, ComboKind::StarLine | ComboKind::StarSupernova) {
        let partner_kind = if kind == ComboKind::StarLine {
            partner.kind
        } else {
            Supernova
        };
        trigger_star_transform_combo(
            commands,
            grid,
            entity_info,
            origin,
            partner_kind,
            color,
            ray,
            pop_delays,
        );
        return;
    }

    // StarColor/StarStar: just the Starburst orb-seeking visual — every same-color cell (or the
    // whole board for StarStar) is cleared directly, with no intermediate tier to transform into.
    if star {
        let star_activation = PowerActivation {
            pos: origin.pos,
            kind: Starburst,
            partner_color: color,
        };
        let targets = blast_path(&star_activation, entity_info);
        commands.trigger(PowerBlastTrail {
            kind: Starburst,
            color,
            path: targets.iter().copied().map(to_world).collect(),
            delay_secs: 0.0,
        });
    }

    // DoubleLine: ambas posiciones disparan una cruz completa (H+V).
    if kind == ComboKind::DoubleLine {
        for pos in [origin.pos, partner.pos] {
            commands.trigger(PowerBlastTrail {
                kind: Cross,
                color: None,
                path: vec![to_world(pos)],
                delay_secs: 0.0,
            });
        }
    }

    // LineSupernova: el rayo partner dispara sus beams (la supernova tiene el ring en on_power_combo).
    if kind == ComboKind::LineSupernova {
        commands.trigger(PowerBlastTrail {
            kind: partner.kind,
            color: None,
            path: vec![to_world(partner.pos)],
            delay_secs: 0.0,
        });
    }
}

/// Candy-Crush-style two-phase choreography shared by `ComboKind::StarLine` and
/// `ComboKind::StarSupernova`: instead of each same-color light detonating the instant its star
/// orb arrives (a distance-staggered ripple), every same-color light first visibly *becomes* the
/// partner's tier (`PendingLightTransform`) and only once the slowest orb has landed — plus a
/// short held beat (`RaySettings::combo_hold_secs`) — do they all detonate together in one
/// synchronized wave. Also computes the pop delay for every cell the detonation touches (mirroring
/// `resolve_swap_activation`'s own reach for this pair), since that shape isn't a plain per-kind
/// `accumulate_pop_delays` case. `partner_kind` is `RayH`/`RayV`/`Cross` for `StarLine`, always
/// `Supernova` for `StarSupernova`.
fn trigger_star_transform_combo(
    commands: &mut Commands,
    grid: &Grid,
    entity_info: &EntityInfo,
    origin: &PowerActivation,
    partner_kind: LightKind,
    color: Option<LightColor>,
    ray: &RaySettings,
    pop_delays: &mut HashMap<Entity, f32>,
) {
    let star_activation = PowerActivation {
        pos: origin.pos,
        kind: LightKind::Starburst,
        partner_color: color,
    };
    let targets = blast_path(&star_activation, entity_info);
    commands.trigger(PowerBlastTrail {
        kind: LightKind::Starburst,
        color,
        path: targets.iter().copied().map(to_world).collect(),
        delay_secs: 0.0,
    });

    // The star's own cell is consumed immediately, like any lone power activation.
    if let Some(&(e, _, _)) = grid.get(&targets[0]) {
        merge_pop_delay(pop_delays, e, 0.0);
    }

    let same_color_cells = &targets[1..];
    let last_index = same_color_cells.len();
    let max_arrival = if last_index == 0 {
        0.0
    } else {
        (ray.stagger_secs * (last_index - 1) as f32).min(ray.stagger_max) + ray.trail_duration
    };
    let explode_delay = max_arrival + ray.combo_hold_secs;

    // Phase 1.5: each same-color light visibly *becomes* the partner's tier as its own orb
    // arrives — reusing `visuals::core_motion::rebuild_cores`'s existing `Changed<LightKind>`
    // reactivity (the same mechanism a real match-3 upgrade uses) instead of a bespoke overlay, so
    // it's the light's actual body mesh that changes, not a flash drawn on top of it. It keeps
    // that new look until the synchronized detonation below actually removes it.
    for (i, &pos) in same_color_cells.iter().enumerate() {
        if let Some(&(e, _, _)) = grid.get(&pos) {
            let arrival = (ray.stagger_secs * i as f32).min(ray.stagger_max) + ray.trail_duration;
            commands.entity(e).insert(PendingLightTransform {
                new_kind: partner_kind,
                timer: Timer::from_seconds(arrival, TimerMode::Once),
            });
        }
    }

    if matches!(
        partner_kind,
        LightKind::RayH | LightKind::RayV | LightKind::Cross
    ) {
        // Dedupe by swept line: several same-color cells commonly share a row or column, and each
        // would otherwise fire its own overlapping beam sweep for that exact same line — the bug
        // behind the wall of duplicate horizontal bars. Keep only one representative cell per line.
        let mut rows: HashMap<i32, GridPos> = HashMap::new();
        let mut cols: HashMap<i32, GridPos> = HashMap::new();
        for &pos in same_color_cells {
            if matches!(partner_kind, LightKind::RayH | LightKind::Cross) {
                rows.entry(pos.y).or_insert(pos);
            }
            if matches!(partner_kind, LightKind::RayV | LightKind::Cross) {
                cols.entry(pos.x).or_insert(pos);
            }
        }
        for &pos in rows.values() {
            fire_star_line_sweep(
                commands,
                pop_delays,
                grid,
                ray,
                pos,
                LightKind::RayH,
                explode_delay,
            );
        }
        for &pos in cols.values() {
            fire_star_line_sweep(
                commands,
                pop_delays,
                grid,
                ray,
                pos,
                LightKind::RayV,
                explode_delay,
            );
        }
    } else {
        // Area bursts (Supernova): each target detonates on its own — unlike duplicate full-line
        // sweeps, overlapping 3×3 bursts read fine, so no dedup is needed here.
        for (i, &pos) in same_color_cells.iter().enumerate() {
            let arrival = (ray.stagger_secs * i as f32).min(ray.stagger_max) + ray.trail_duration;
            commands.trigger(PowerBlastTrail {
                kind: partner_kind,
                color: None,
                path: vec![to_world(pos)],
                delay_secs: arrival,
            });
            accumulate_area_burst_pop_delays(pop_delays, pos, grid, ray, arrival);
        }
    }
}

/// Fires one line's synchronized beam (and its pop delays) — `axis` is always `RayH` or `RayV`,
/// never `Cross` (the caller already split a `Cross` partner into one row group and one column
/// group so each axis only fires once).
fn fire_star_line_sweep(
    commands: &mut Commands,
    pop_delays: &mut HashMap<Entity, f32>,
    grid: &Grid,
    ray: &RaySettings,
    pos: GridPos,
    axis: LightKind,
    explode_delay: f32,
) {
    commands.trigger(PowerBlastTrail {
        kind: axis,
        color: None,
        path: vec![to_world(pos)],
        delay_secs: explode_delay,
    });
    accumulate_line_sweep_pop_delays(pop_delays, pos, axis, grid, ray, explode_delay);
}

/// Every cell along `pos`'s row (if `stripe` sweeps horizontally) and/or column (vertically),
/// timed at `base_delay` (when the synchronized sweep starts) plus that ray's own travel time to
/// reach it — mirrors `resolve_swap_activation`'s `(Starburst, RayH|RayV|Cross)` arm, which sweeps
/// every same-color cell's own line, not just the swapped partner's.
fn accumulate_line_sweep_pop_delays(
    pop_delays: &mut HashMap<Entity, f32>,
    pos: GridPos,
    stripe: LightKind,
    grid: &Grid,
    ray: &RaySettings,
    base_delay: f32,
) {
    let source = to_world(pos);
    if matches!(stripe, LightKind::RayH | LightKind::Cross) {
        for x in 0..GRID_W {
            let p = GridPos { x, y: pos.y };
            if let Some(&(e, _, _)) = grid.get(&p) {
                merge_pop_delay(
                    pop_delays,
                    e,
                    base_delay + ray.pop_delay(to_world(p).distance(source)),
                );
            }
        }
    }
    if matches!(stripe, LightKind::RayV | LightKind::Cross) {
        for y in 0..GRID_H {
            let p = GridPos { x: pos.x, y };
            if let Some(&(e, _, _)) = grid.get(&p) {
                merge_pop_delay(
                    pop_delays,
                    e,
                    base_delay + ray.pop_delay(to_world(p).distance(source)),
                );
            }
        }
    }
}

/// Every cell in the 3×3 neighborhood around `pos`, timed at `base_delay` (when the synchronized
/// burst starts) plus that Supernova's own distance to reach it — mirrors
/// `resolve_swap_activation`'s `(Starburst, Supernova)` arm, which bursts a 3×3 around every
/// same-color cell, not just the swapped partner's.
fn accumulate_area_burst_pop_delays(
    pop_delays: &mut HashMap<Entity, f32>,
    pos: GridPos,
    grid: &Grid,
    ray: &RaySettings,
    base_delay: f32,
) {
    let source = to_world(pos);
    for dx in -1..=1i32 {
        for dy in -1..=1i32 {
            let p = GridPos {
                x: pos.x + dx,
                y: pos.y + dy,
            };
            if let Some(&(e, _, _)) = grid.get(&p) {
                merge_pop_delay(
                    pop_delays,
                    e,
                    base_delay + to_world(p).distance(source) / ray.speed,
                );
            }
        }
    }
}

/// A purely-cosmetic delayed `LightKind` swap on an entity already slated for removal — see
/// `trigger_star_transform_combo`'s "phase 1.5". The entity's actual removal timing comes entirely
/// from its own `PopDelay` (computed independently alongside the detonation); this only makes it
/// *look* like the partner's tier in the meantime.
#[derive(Component)]
pub(crate) struct PendingLightTransform {
    new_kind: LightKind,
    timer: Timer,
}

/// Applies `PendingLightTransform` once its timer elapses. Setting `LightKind` is enough —
/// `visuals::core_motion::rebuild_cores` reacts to the change and rebuilds the light's body mesh
/// and cores into the new kind's signature look, exactly like a real match-3 upgrade does.
pub(crate) fn tick_pending_light_transform(
    mut commands: Commands,
    mut q: Query<(Entity, &mut PendingLightTransform, &mut LightKind)>,
    time: Res<Time>,
) {
    for (e, mut pending, mut kind) in &mut q {
        if pending.timer.tick(time.delta()).is_finished() {
            *kind = pending.new_kind;
            commands.entity(e).remove::<PendingLightTransform>();
        }
    }
}

/// Drives all VFX for a resolved wave: one unified animation per combined pair, the standard
/// flash+beam per power that fired alone, and the merged pop-delay ripple for every participant.
pub(crate) fn trigger_wave_vfx(
    commands: &mut Commands,
    wave: &WaveResolution,
    grid: &Grid,
    entity_info: &EntityInfo,
    pop_delays: &mut HashMap<Entity, f32>,
    settings: &RaySettings,
) {
    for (a, b, kind) in &wave.combos {
        trigger_combo(
            commands,
            grid,
            entity_info,
            a,
            b,
            *kind,
            settings,
            pop_delays,
        );
        // StarLine/StarSupernova compute their own pop delays inside `trigger_combo` (see
        // `trigger_star_transform_combo`); the generic per-kind delays here would race them and
        // let a faster wrong answer win.
        if !matches!(kind, ComboKind::StarLine | ComboKind::StarSupernova) {
            accumulate_pop_delays(pop_delays, a, grid, entity_info, settings);
            accumulate_pop_delays(pop_delays, b, grid, entity_info, settings);
        }
    }
    for single in &wave.singles {
        trigger_single_vfx(commands, single, grid, entity_info, pop_delays, settings);
    }
}

/// One unified board-clearing animation for a 3+ power super-combo (anchored at the board centre),
/// plus the pop-delay ripple for each participating power.
pub(crate) fn trigger_super_combo_vfx(
    commands: &mut Commands,
    powers: &[PowerActivation],
    grid: &Grid,
    entity_info: &EntityInfo,
    pop_delays: &mut HashMap<Entity, f32>,
    settings: &RaySettings,
) {
    let center = GridPos {
        x: GRID_W / 2,
        y: GRID_H / 2,
    };
    commands.trigger(PowerCombo {
        kind: ComboKind::SuperCombo,
        a_pos: center,
        b_pos: center,
        color: None,
    });
    for p in powers {
        accumulate_pop_delays(pop_delays, p, grid, entity_info, settings);
    }
}

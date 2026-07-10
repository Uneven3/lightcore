//! Bridges resolved power-light waves to the visual layer's events. The pure resolution in
//! `core::matching` decides *what* clears; these helpers decide *how it reads* — one unified
//! `PowerCombo` animation per combined pair, the standard flash+beam per lone power, and a single
//! board-clearing flash for a 3+ super-combo. Centralizing it here keeps the three resolution
//! sites (`swap::on_swap_happened` paths A/B and `chain::check_chain_matches` phases 1/2) in sync.

use bevy::prelude::*;
use std::collections::HashMap;

use super::popping::accumulate_pop_delays;
use super::{PowerBlastTrail, PowerCombo, PowerConsumed};
use crate::core::grid::RaySettings;
use crate::core::prelude::*;

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

    // Star combos: disparar el trail de Starburst (orbes viajando a cada target) y además
    // el efecto del partner en cada target cuando el orbe llega (delay = tiempo de viaje).
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
        // StarLine: al llegar cada orbe, el target "se convierte" en un haz y dispara.
        // StarSupernova: al llegar cada orbe, el target explota como mini-supernova.
        if matches!(kind, ComboKind::StarLine | ComboKind::StarSupernova) {
            let partner_kind = if kind == ComboKind::StarLine {
                partner.kind
            } else {
                Supernova
            };
            for (i, &target_pos) in targets.iter().enumerate().skip(1) {
                let delay =
                    (ray.stagger_secs * (i - 1) as f32).min(ray.stagger_max) + ray.trail_duration;
                commands.trigger(PowerBlastTrail {
                    kind: partner_kind,
                    color: None,
                    path: vec![to_world(target_pos)],
                    delay_secs: delay,
                });
            }
        }
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
        trigger_combo(commands, grid, entity_info, a, b, *kind, settings);
        accumulate_pop_delays(pop_delays, a, grid, entity_info, settings);
        accumulate_pop_delays(pop_delays, b, grid, entity_info, settings);
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

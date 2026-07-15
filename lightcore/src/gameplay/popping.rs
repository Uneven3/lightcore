use bevy::prelude::*;
use std::collections::HashMap;

use super::LightPopped;
use crate::core::components::{PopAnim, PopDelay};
use crate::core::prelude::*;
use crate::state::GameState;
use crate::visuals::RaySettings;

/// Accumulates, per light hit by `activation`, the delay until the power's effect *reaches* it —
/// so lights consume as the beam touches them, not all at once. Ray/Supernova: by distance from
/// the source. Starburst: by each target's seeking-beam arrival (stagger by distance rank +
/// travel). Merged with `min` across activations, so the earliest-arriving effect wins.
/// Records `d` as entity `e`'s pop delay, keeping the smallest value seen — so when a light is
/// reachable by more than one effect (e.g. two swept lines crossing), the earliest arrival wins.
pub(super) fn merge_pop_delay(delays: &mut HashMap<Entity, f32>, e: Entity, d: f32) {
    let slot = delays.entry(e).or_insert(f32::INFINITY);
    if d < *slot {
        *slot = d;
    }
}

pub(crate) fn accumulate_pop_delays(
    delays: &mut HashMap<Entity, f32>,
    activation: &PowerActivation,
    grid: &Grid,
    entity_info: &EntityInfo,
    settings: &RaySettings,
) {
    let mut put = |e: Entity, d: f32| merge_pop_delay(delays, e, d);
    match activation.kind {
        LightKind::Normal | LightKind::Hollow => {}
        LightKind::Starburst => {
            // blast_path = [star, targets sorted by distance]; beam i arrives at stagger + travel.
            for (i, pos) in blast_path(activation, entity_info).iter().enumerate() {
                if let Some(&(e, _, _)) = grid.get(pos) {
                    let d = if i == 0 {
                        0.0
                    } else {
                        (settings.stagger_secs * (i as f32 - 1.0)).min(settings.stagger_max)
                            + settings.trail_duration
                    };
                    put(e, d);
                }
            }
        }
        // Rays y Cross usan LaserBolt. El pop se dispara con anticipación de pop_duration para
        // que las partículas aparezcan exactamente cuando el frente del bolt llega al light:
        //   delay = (d − bolt_length − speed × pop_duration).max(0) / speed
        LightKind::RayH | LightKind::RayV | LightKind::Cross => {
            let source = to_world(activation.pos);
            for pos in blast_path(activation, entity_info) {
                if let Some(&(e, _, _)) = grid.get(&pos) {
                    let d = to_world(pos).distance(source);
                    put(e, settings.pop_delay(d));
                }
            }
        }
        _ => {
            let source = to_world(activation.pos);
            for pos in blast_path(activation, entity_info) {
                if let Some(&(e, _, _)) = grid.get(&pos) {
                    put(e, to_world(pos).distance(source) / settings.speed);
                }
            }
        }
    }
}

/// Inserts the propagation delay computed by `accumulate_pop_delays` onto a popping light (if any).
pub(crate) fn apply_pop_delay(commands: &mut Commands, e: Entity, delays: &HashMap<Entity, f32>) {
    if let Some(&d) = delays.get(&e)
        && d > 0.01
    {
        commands
            .entity(e)
            .insert(PopDelay(Timer::from_seconds(d, TimerMode::Once)));
    }
}

/// Prepares a light for its pop animation: clones its shared ring material into an owned copy
/// so alpha can be faded independently per entity. Children are NOT despawned here — GlowPool
/// halos stay alive during the fade so the light doesn't look like it "turned off" before
/// the membrane dissolves. CoreMotion orbs are handled by `visuals::core_motion::despawn_cores_on_pop`.
pub(crate) fn clone_pop_material(
    mut q: Query<&mut MeshMaterial2d<ColorMaterial>, Added<PopAnim>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for mut handle in &mut q {
        if let Some(mat) = materials.get(&handle.0).cloned() {
            handle.0 = materials.add(mat);
        }
    }
}

pub(crate) fn tick_pop_anim(
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut Transform,
        &mut PopAnim,
        Option<&mut PopDelay>,
        Option<&MeshMaterial2d<ColorMaterial>>,
        Option<&LightColor>,
        Option<&LightKind>,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    time: Res<Time>,
) {
    for (e, mut t, mut anim, delay, mat_handle, color, kind) in &mut q {
        if let Some(mut d) = delay {
            if !d.0.tick(time.delta()).is_finished() {
                continue;
            }
            commands.entity(e).remove::<PopDelay>();
        }
        anim.0.tick(time.delta());
        let frac = anim.0.fraction();
        // Ring (membrane) grows very slightly as it fades — the membrane stretching before it breaks.
        t.scale = Vec3::splat(1.0 + frac * 0.1);
        if let Some(handle) = mat_handle
            && let Some(mut mat) = materials.get_mut(&handle.0)
        {
            mat.color = mat.color.with_alpha((1.0 - frac).max(0.0));
        }
        if frac >= 1.0 {
            let color = color.copied().unwrap_or(LightColor::Red);
            let kind = kind.copied().unwrap_or(LightKind::Normal);
            commands.trigger(LightPopped {
                pos: t.translation,
                color,
                kind,
            });
            commands.entity(e).try_despawn();
        }
    }
}

pub(crate) fn check_popping_done(
    q: Query<(), With<PopAnim>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if q.is_empty() {
        next_state.set(GameState::Falling);
    }
}

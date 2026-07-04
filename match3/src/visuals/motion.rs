use bevy::prelude::*;

use crate::core::prelude::*;
use crate::gameplay::falling::Dropping;
use crate::gameplay::{DragState, PendingSwap};
use crate::input::pointer::PointerInput;
use crate::state::GameState;

const BASE_FALL_SPEED: f32 = TILE * 6.0;
const FALL_ACCEL: f32 = TILE * 12.0; // per second spent continuously falling
const MAX_FALL_SPEED: f32 = TILE * 18.0;

pub(crate) fn lerp_visual_pos(
    mut commands: Commands,
    drag: Res<DragState>,
    mut q: Query<
        (
            Entity,
            &GridPos,
            &mut VisualPos,
            Option<&FallSpeed>,
            Has<Dropping>,
            Option<&mut FallMomentum>,
        ),
        (With<FallPhysics>, Without<PopAnim>),
    >,
    time: Res<Time>,
    state: Res<State<GameState>>,
    pending: Res<PendingSwap>,
) {
    let dt = time.delta_secs();
    let in_swap = *state.get() == GameState::SwapAnimating;
    let (swap_a, swap_b) = if in_swap {
        pending
            .0
            .as_ref()
            .map_or((None, None), |s| (Some(s.a), s.b))
    } else {
        (None, None)
    };
    for (entity, gpos, mut vpos, fall_speed, is_dropping, mut momentum) in &mut q {
        if drag.active
            && (Some(entity) == drag.start_entity || Some(entity) == drag.neighbor_entity)
        {
            continue;
        }
        let speed = if Some(entity) == swap_a || Some(entity) == swap_b {
            TILE * 5.0
        } else if let Some(fs) = fall_speed {
            fs.0
        } else if is_dropping {
            let elapsed = match &mut momentum {
                Some(m) => {
                    m.0 += dt;
                    m.0
                }
                None => {
                    commands.entity(entity).insert(FallMomentum(0.0));
                    0.0
                }
            };
            (BASE_FALL_SPEED + FALL_ACCEL * elapsed).min(MAX_FALL_SPEED)
        } else {
            if momentum.is_some() {
                commands.entity(entity).remove::<FallMomentum>();
            }
            TILE * 10.0
        };
        vpos.0 = vpos.0.move_towards(to_world(*gpos), speed * dt);
        if fall_speed.is_some() && vpos.0.distance(to_world(*gpos)) < 0.01 {
            commands.entity(entity).remove::<FallSpeed>();
        }
    }
}

pub(crate) fn update_drag_constrained(
    drag: Res<DragState>,
    pointer: Res<PointerInput>,
    mut lights: Query<(&GridPos, &mut VisualPos), (With<Light>, Without<Spark>)>,
    mut fallables: Query<(&GridPos, &mut VisualPos), (With<Spark>, Without<Light>)>,
) {
    if !drag.active {
        return;
    }
    let Some(dir) = drag.locked_axis else {
        return;
    };
    let Some(start_e) = drag.start_entity else {
        return;
    };

    let Some(world) = pointer.position_world else {
        return;
    };

    let proj = if dir.x != 0 {
        (world - drag.start_world).x
    } else {
        (world - drag.start_world).y
    };
    let dir_sign = (if dir.x != 0 { dir.x } else { dir.y }) as f32;
    let blocked = drag.neighbor_entity.is_none() && !drag.neighbor_is_empty;
    let max_offset = if blocked { TILE * 0.18 } else { TILE };
    let offset = (proj * dir_sign).clamp(0.0, max_offset) * dir_sign;
    let (dx, dy) = if dir.x != 0 {
        (offset, 0.0)
    } else {
        (0.0, offset)
    };

    if let Ok((gp, mut vp)) = lights.get_mut(start_e) {
        let b = to_world(*gp);
        vp.0 = Vec3::new(b.x + dx, b.y + dy, 0.5);
    } else if let Ok((gp, mut vp)) = fallables.get_mut(start_e) {
        let b = to_world(*gp);
        vp.0 = Vec3::new(b.x + dx, b.y + dy, 0.5);
    }
    if let Some(ne) = drag.neighbor_entity {
        if let Ok((gp, mut vp)) = lights.get_mut(ne) {
            let b = to_world(*gp);
            vp.0 = Vec3::new(b.x - dx, b.y - dy, 0.0);
        } else if let Ok((gp, mut vp)) = fallables.get_mut(ne) {
            let b = to_world(*gp);
            vp.0 = Vec3::new(b.x - dx, b.y - dy, 0.0);
        }
    }
}

pub(crate) fn sync_transforms(
    mut q: Query<(&VisualPos, &mut Transform), (With<FallPhysics>, Without<PopAnim>)>,
) {
    for (vpos, mut t) in &mut q {
        t.translation = vpos.0;
    }
}

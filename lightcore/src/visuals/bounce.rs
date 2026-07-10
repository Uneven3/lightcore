use bevy::prelude::*;

use crate::core::prelude::*;
use crate::gameplay::PendingSwap;

const BOUNCE_DURATION: f32 = 0.21;
const BOUNCE_SQUASH_Y: f32 = 0.78;
const BOUNCE_STRETCH_X: f32 = 1.12;

#[derive(Component)]
pub(crate) struct Settling;

#[derive(Component)]
pub(crate) struct LandBounce {
    timer: Timer,
}

pub(crate) fn detect_landing(
    mut commands: Commands,
    pending: Res<PendingSwap>,
    q: Query<(Entity, &GridPos, &VisualPos, Has<Settling>), (With<FallPhysics>, Without<PopAnim>)>,
) {
    let (a, b) = pending
        .0
        .as_ref()
        .map_or((None, None), |s| (Some(s.a), s.b));
    for (e, gp, vp, was_settling) in &q {
        if Some(e) == a || Some(e) == b {
            continue;
        }
        let unsettled = vp.0.distance(to_world(*gp)) >= TILE * 0.1;
        // `try_*` throughout: a falling/settling light can be matched and despawned before these
        // deferred commands apply, and a plain insert/remove on a despawned entity logs an error.
        if unsettled {
            if !was_settling {
                commands.entity(e).try_insert(Settling);
            }
        } else if was_settling {
            commands.entity(e).try_remove::<Settling>();
            commands.entity(e).try_insert(LandBounce {
                timer: Timer::from_seconds(BOUNCE_DURATION, TimerMode::Once),
            });
        }
    }
}

pub(crate) fn tick_land_bounce(
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut LandBounce)>,
    time: Res<Time>,
) {
    for (e, mut t, mut b) in &mut q {
        b.timer.tick(time.delta());
        let ease = 1.0 - (1.0 - b.timer.fraction()).powi(3);
        t.scale = Vec3::new(
            BOUNCE_STRETCH_X + (1.0 - BOUNCE_STRETCH_X) * ease,
            BOUNCE_SQUASH_Y + (1.0 - BOUNCE_SQUASH_Y) * ease,
            1.0,
        );
        if b.timer.is_finished() {
            t.scale = Vec3::ONE;
            commands.entity(e).try_remove::<LandBounce>();
        }
    }
}

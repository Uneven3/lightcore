use bevy::prelude::*;

use crate::core::easing::damped_squash;
use crate::core::prelude::*;
use crate::gameplay::PendingSwap;

const BOUNCE_DURATION: f32 = 0.48;
/// Peak squash/stretch deviation from `Vec3::ONE` at the instant of landing (t=0) — same damped-
/// spring shape as `gameplay::input::SelectJelly`.
const BOUNCE_AMOUNT: f32 = 0.24;
/// Number of full squash↔stretch cycles over `BOUNCE_DURATION`.
const BOUNCE_CYCLES: f32 = 3.0;

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
        let frac = b.timer.fraction();
        // Same damped-spring shape as the select-jelly punch (`gameplay::input::SelectJelly`),
        // via the shared `damped_squash` — full amplitude at the instant of impact, ringing
        // through `BOUNCE_CYCLES` squash↔stretch swings that shrink to 0.
        let squash = damped_squash(frac, BOUNCE_AMOUNT, BOUNCE_CYCLES);
        t.scale = Vec3::new(1.0 + squash, 1.0 - squash, 1.0);
        if b.timer.is_finished() {
            t.scale = Vec3::ONE;
            commands.entity(e).try_remove::<LandBounce>();
        }
    }
}

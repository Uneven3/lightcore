use std::f32::consts::TAU;

/// Damped-spring squash/stretch: a decaying cosine that starts at full amplitude (`frac == 0.0`,
/// matching an impact/grab instant) and rings down through `cycles` full swings before settling to
/// 0 by `frac == 1.0`. Returns the signed deviation to add/subtract from the two axes of a
/// `Transform::scale` (e.g. `Vec3::new(base + squash, base - squash, 1.0)`) — kept a pure `f32`
/// function (no ECS/Bevy types) so both `visuals::bounce`'s landing bounce and
/// `gameplay::input`'s select-jelly punch can share the exact formula without either module
/// depending on the other.
pub(crate) fn damped_squash(frac: f32, amount: f32, cycles: f32) -> f32 {
    let decay = (1.0 - frac).powi(2);
    let wobble = (frac * cycles * TAU).cos() * decay;
    amount * wobble
}

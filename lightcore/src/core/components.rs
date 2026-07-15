use bevy::prelude::*;

/// A light on the board: a hollow neon membrane (the ring body) wrapping a [`LightCore`].
/// The unit the player moves and matches — lining up 3+ breaks the membranes and releases
/// their cores. This is what gravity, swapping and matching all operate on.
#[derive(Component)]
pub(crate) struct Light;

#[derive(Component)]
pub(crate) struct Selected;

/// Capability marker consumed by movement systems. Obstacles such as `Stasis` remove this tag
/// instead of requiring each system to know their concrete type.
#[derive(Component)]
pub(crate) struct Movable;

/// Frozen light: it still participates in matches, but cannot be selected, swapped or moved by
/// gravity. New obstacle types can compose the capability markers below in the same way.
#[derive(Component)]
pub(crate) struct Stasis;

/// Visual cover for a stasis light. It deliberately has none of the blocking-cell capabilities.
#[derive(Component)]
pub(crate) struct StasisCover;

/// The bright core at the heart of a [`Light`] — the thing the player is actually after.
/// HDR-overexposed so the camera's Bloom pass makes it glow; child of the light entity,
/// positioned via the Transform hierarchy. When the surrounding membrane breaks, the core
/// is collected (it's what drives the score).
#[derive(Component)]
pub(crate) struct LightCore;

/// A loose spark that falls under gravity and is rescued when it reaches the bottom row (y=0).
#[derive(Component)]
pub(crate) struct Spark;

/// A lilac shadow obstacle. It blocks direct interaction and gravity while its associated
/// lightcore remains on the board underneath; it is cleared by an adjacent match.
#[derive(Component)]
pub(crate) struct Shadow;

/// Marks a cell entity as solid for gravity/refill.
#[derive(Component)]
pub(crate) struct BlocksGravity;

/// Marks a cell entity as unavailable for direct player interaction.
#[derive(Component)]
pub(crate) struct BlocksInteraction;

/// A cell obstacle damaged only by a match in one of its four orthogonal neighbours.
#[derive(Component)]
pub(crate) struct AdjacentMatchDamage;

/// A permanent missing/blocked cell used to sculpt non-rectangular boards. It also carries
/// `Shadow` so existing movement/gravity blockers see it, but clear-shadow logic ignores it.
#[derive(Component)]
pub(crate) struct Blocker;

/// Future opaque obstacle with no lightcore. It needs several orthogonally adjacent matches
/// before clearing; see `board::clear_shadow_at`.
#[derive(Component)]
pub(crate) struct DeepShadow(pub(crate) u8);

/// Backwards-compatible name while level configuration is migrated to `DeepShadow` terminology.
pub(crate) type HardShadow = DeepShadow;

/// The world-space hit-counter text child spawned under a `HardShadow` tile.
#[derive(Component)]
pub(crate) struct HardShadowLabel;

/// Non-interactive hint drawn on the board to show where ingredients are rescued.
#[derive(Component)]
pub(crate) struct IngredientExit;

/// Shared by Light + Spark for gravity/lerp. Read by both `gameplay` (apply_gravity)
/// and `visuals` (lerp_visual_pos, sync_transforms), so it lives here rather than in either.
#[derive(Component)]
pub(crate) struct FallPhysics;

/// Smoothed visual position a `gameplay`/`visuals` system eases toward the entity's logical
/// `GridPos`. Written by board spawn helpers, animated by `visuals`, read by `gameplay`.
#[derive(Component)]
pub(crate) struct VisualPos(pub(crate) Vec3);

/// Marks an entity as mid-removal-animation. Inserted by gameplay match/cascade resolution,
/// driven and filtered-on by `visuals`/`gameplay` falling systems alike.
#[derive(Component)]
pub(crate) struct PopAnim(pub(crate) Timer);

/// Optional companion to `PopAnim`: holds the light at full size until this timer elapses, then
/// the normal shrink begins. Lets a blast's pops ripple outward from its center (following the
/// traveling beam) instead of every cell collapsing at the same instant.
#[derive(Component)]
pub(crate) struct PopDelay(pub(crate) Timer);

/// One-shot override for `lerp_visual_pos`'s fall speed (units/sec), so a freshly spawned
/// light's entry takes a consistent duration regardless of how far it starts from its slot.
/// Removed by `lerp_visual_pos` itself once the entity arrives.
#[derive(Component)]
pub(crate) struct FallSpeed(pub(crate) f32);

/// Tracks how long an entity has been continuously falling under gravity (`Dropping`), so
/// `lerp_visual_pos` can accelerate it the longer it falls. Managed entirely by
/// `lerp_visual_pos` itself, reset whenever `Dropping` isn't present.
#[derive(Component)]
pub(crate) struct FallMomentum(pub(crate) f32);

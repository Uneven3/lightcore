use bevy::prelude::*;
use rand::Rng;
use std::collections::{HashMap, HashSet};

use crate::core::prelude::*;
use crate::state::{AttemptScoped, MatchScoped};

pub(crate) const HOLLOW_BASE_CHANCE: f32 = 0.025;

#[derive(Component)]
pub(crate) struct GridCellTile;

/// Narrow asset port used by board composition. Gameplay depends on this board-owned contract,
/// while the visual adapter publishes concrete handles from its larger cache.
#[derive(Resource, Clone)]
pub(crate) struct BoardVisuals {
    pub(crate) shadow_mesh: Handle<Mesh>,
    pub(crate) shadow_mat: Handle<ColorMaterial>,
    pub(crate) hard_shadow_mat: Handle<ColorMaterial>,
    pub(crate) blocker_mesh: Handle<Mesh>,
    pub(crate) blocker_mat: Handle<ColorMaterial>,
    pub(crate) spark_mesh: Handle<Mesh>,
    pub(crate) spark_mat: Handle<ColorMaterial>,
    pub(crate) core_image: Handle<Image>,
    pub(crate) glow_image: Handle<Image>,
    pub(crate) grid_cell_image: Handle<Image>,
}

#[derive(Component)]
pub(crate) struct BreathPhase(pub(crate) f32);

#[derive(Component)]
pub(crate) struct SparkNucleusPulse {
    pub(crate) base_scale: Vec3,
    pub(crate) phase: f32,
}

pub(crate) fn spawn_grid_cell(commands: &mut Commands, cache: &BoardVisuals, pos: GridPos) {
    commands.spawn((
        GridCellTile,
        Sprite {
            image: cache.grid_cell_image.clone(),
            custom_size: Some(Vec2::splat(TILE)),
            ..default()
        },
        Transform::from_translation(to_world(pos).with_z(-5.0)),
        MatchScoped,
    ));
}

pub(crate) fn random_basic_kind(rng: &mut impl Rng, hollow_chance: f32) -> LightKind {
    if rng.random::<f32>() < hollow_chance {
        LightKind::Hollow
    } else {
        LightKind::Normal
    }
}

fn generate_board_layout(
    rng: &mut impl Rng,
    hollow_chance: f32,
    weights: [f32; 5],
) -> Vec<(GridPos, LightColor, LightKind)> {
    const ALL: [LightColor; 5] = [
        LightColor::Red,
        LightColor::Green,
        LightColor::Blue,
        LightColor::Yellow,
        LightColor::Purple,
    ];
    let mut board = [[(LightColor::Red, LightKind::Normal); GRID_H as usize]; GRID_W as usize];
    for x in 0..GRID_W as usize {
        for y in 0..GRID_H as usize {
            loop {
                let mut r = rng.random_range(0.0..weights.iter().sum());
                let mut selected_idx = 0;
                for (idx, &w) in weights.iter().enumerate() {
                    if r < w {
                        selected_idx = idx;
                        break;
                    }
                    r -= w;
                }
                let color = ALL[selected_idx];
                let kind = random_basic_kind(rng, hollow_chance);
                let key = match_key(color, kind);
                let h_match = x >= 2
                    && match_key(board[x - 1][y].0, board[x - 1][y].1) == key
                    && match_key(board[x - 2][y].0, board[x - 2][y].1) == key;
                let v_match = y >= 2
                    && match_key(board[x][y - 1].0, board[x][y - 1].1) == key
                    && match_key(board[x][y - 2].0, board[x][y - 2].1) == key;
                if !h_match && !v_match {
                    board[x][y] = (color, kind);
                    break;
                }
            }
        }
    }
    (0..GRID_W)
        .flat_map(|x| {
            (0..GRID_H).map(move |y| {
                let (color, kind) = board[x as usize][y as usize];
                (GridPos { x, y }, color, kind)
            })
        })
        .collect()
}

fn board_has_valid_swap(
    board: &[(GridPos, LightColor, LightKind)],
    blocked: &HashSet<GridPos>,
) -> bool {
    let mut grid: Grid = HashMap::new();
    for (idx, (pos, color, kind)) in board.iter().enumerate() {
        if blocked.contains(pos) {
            continue;
        }
        let entity = Entity::from_raw_u32(idx as u32 + 1).unwrap();
        grid.insert(*pos, (entity, *color, *kind));
    }
    find_valid_swap(&grid, &HashSet::new()).is_some()
}

pub(crate) fn generate_board(
    rng: &mut impl Rng,
    blocked: &HashSet<GridPos>,
    hollow_chance: f32,
    weights: [f32; 5],
) -> Vec<(GridPos, LightColor, LightKind)> {
    loop {
        let board = generate_board_layout(rng, hollow_chance, weights);
        if board_has_valid_swap(&board, blocked) {
            return board;
        }
    }
}

/// Spawns a light's SIMULATION components only (`Light`, `FallPhysics`, `color`, `kind`, `pos`,
/// grid/visual position) — no `Mesh2d`/`MeshMaterial2d`. Those are attached reactively by
/// `visuals::core_motion::rebuild_cores`, which fires on `Changed<LightKind>` and therefore also
/// catches this entity's very first frame (a freshly-inserted component satisfies `Changed`), same
/// as the cores it builds alongside them. The glow halo is attached the same way, off `Added<Light>`
/// in `visuals::glow::attach_glow_pools`. This keeps board setup free of `VisualCache` — the
/// simulation layer doesn't need to know which mesh/material a light's kind maps to.
pub(crate) fn spawn_light(
    commands: &mut Commands,
    pos: GridPos,
    color: LightColor,
    kind: LightKind,
    visual_start: Vec3,
) -> Entity {
    let mut rng = rand::rng();
    let phase = rng.random_range(0.0..std::f32::consts::TAU);
    commands
        .spawn((
            Light,
            Movable,
            FallPhysics,
            color,
            kind,
            pos,
            VisualPos(visual_start),
            // Stable identity phase used only by visuals that deliberately animate, such as a
            // Hollow membrane. Normal cores and halos remain temporally stable.
            BreathPhase(phase),
            Transform::from_translation(visual_start),
            AttemptScoped,
            MatchScoped,
        ))
        .id()
}

pub(crate) fn spawn_shadow(commands: &mut Commands, cache: &BoardVisuals, pos: GridPos) {
    let world = to_world(pos);
    commands.spawn((
        Shadow,
        BlocksGravity,
        BlocksInteraction,
        AdjacentMatchDamage,
        pos,
        Mesh2d(cache.shadow_mesh.clone()),
        MeshMaterial2d(cache.hard_shadow_mat.clone()),
        Transform::from_translation(world.with_z(-0.5)),
        AttemptScoped,
        MatchScoped,
    ));
}

/// The former cyan shadow visual, now a non-blocking cover attached conceptually to a stasis
/// light. It preserves the level's appearance without turning the cell into an opaque obstacle.
pub(crate) fn spawn_stasis_cover(commands: &mut Commands, cache: &BoardVisuals, pos: GridPos) {
    let world = to_world(pos);
    commands.spawn((
        StasisCover,
        pos,
        Mesh2d(cache.shadow_mesh.clone()),
        MeshMaterial2d(cache.shadow_mat.clone()),
        Transform::from_translation(world.with_z(0.35)),
        AttemptScoped,
        MatchScoped,
    ));
}

/// Future deep shadow: an opaque cell with no lightcore that needs `hits` orthogonally adjacent
/// matches to clear — see `HardShadow` and `clear_shadow_at`.
#[allow(dead_code)]
pub(crate) fn spawn_hard_shadow(
    commands: &mut Commands,
    cache: &BoardVisuals,
    pos: GridPos,
    hits: u8,
) {
    let world = to_world(pos);
    commands
        .spawn((
            Shadow,
            DeepShadow(hits),
            BlocksGravity,
            BlocksInteraction,
            AdjacentMatchDamage,
            pos,
            Mesh2d(cache.shadow_mesh.clone()),
            MeshMaterial2d(cache.hard_shadow_mat.clone()),
            Transform::from_translation(world.with_z(-0.5)),
            AttemptScoped,
            MatchScoped,
        ))
        .with_children(|shadow| {
            shadow.spawn((
                HardShadowLabel,
                Text2d::new(hits.to_string()),
                TextFont {
                    font_size: FontSize::Px(22.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 1.0)),
                Transform::from_xyz(0.0, 0.0, 0.4),
            ));
        });
}

pub(crate) fn spawn_blocker(commands: &mut Commands, cache: &BoardVisuals, pos: GridPos) {
    let world = to_world(pos);
    commands.spawn((
        Shadow,
        Blocker,
        BlocksGravity,
        BlocksInteraction,
        pos,
        Mesh2d(cache.blocker_mesh.clone()),
        MeshMaterial2d(cache.blocker_mat.clone()),
        Transform::from_translation(world.with_z(-0.45)),
        AttemptScoped,
        MatchScoped,
    ));
}

pub(crate) fn spawn_ingredient_exits(
    commands: &mut Commands,
    cache: &BoardVisuals,
    exits: impl IntoIterator<Item = GridPos>,
) {
    for pos in exits {
        let world = to_world(pos);
        commands.spawn((
            IngredientExit,
            Sprite {
                image: cache.glow_image.clone(),
                color: Color::srgba(0.34, 1.0, 0.68, 0.24),
                custom_size: Some(Vec2::new(TILE * 0.70, TILE * 0.28)),
                ..default()
            },
            Transform::from_translation(
                (world + Vec3::new(0.0, -TILE * 0.38, -0.30)).with_z(-0.30),
            ),
            AttemptScoped,
            MatchScoped,
        ));
        commands.spawn((
            IngredientExit,
            Text2d::new("v"),
            TextFont {
                font_size: FontSize::Px(24.0),
                ..default()
            },
            TextColor(Color::srgb(0.58, 1.0, 0.74)),
            Transform::from_translation(world + Vec3::new(0.0, -TILE * 0.34, 1.10)),
            AttemptScoped,
            MatchScoped,
        ));
    }
}

pub(crate) fn spawn_sparks(
    commands: &mut Commands,
    cache: &BoardVisuals,
    positions: impl IntoIterator<Item = GridPos>,
) {
    for pos in positions {
        let world = to_world(pos);
        let spark = commands
            .spawn((
                Spark,
                FallPhysics,
                pos,
                VisualPos(world),
                Mesh2d(cache.spark_mesh.clone()),
                MeshMaterial2d(cache.spark_mat.clone()),
                Transform::from_translation(world.with_z(0.3)),
                AttemptScoped,
                MatchScoped,
            ))
            .id();
        commands.entity(spark).with_children(|spark| {
            spark.spawn((
                Sprite {
                    image: cache.glow_image.clone(),
                    color: Color::srgba(1.0, 0.42, 0.08, 0.42),
                    custom_size: Some(Vec2::splat(TILE * 0.72)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -0.1),
            ));
            spark.spawn((
                SparkNucleusPulse {
                    base_scale: Vec3::ONE,
                    phase: pos.x as f32 * 0.7,
                },
                Sprite {
                    image: cache.core_image.clone(),
                    color: Color::srgba(0.02, 0.01, 0.005, 0.96),
                    custom_size: Some(Vec2::splat(TILE * 0.42)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 0.3),
            ));
        });
    }
}

pub(crate) fn shuffle_board(
    commands: &mut Commands,
    light_entities: &[(Entity, GridPos)],
    hollow_chance: f32,
) {
    let positions: HashSet<GridPos> = light_entities.iter().map(|(_, p)| *p).collect();
    for (e, _) in light_entities {
        commands.entity(*e).try_despawn();
    }
    let mut rng = rand::rng();
    let new_board = generate_board(&mut rng, &HashSet::new(), hollow_chance, [1.0; 5]);
    for (pos, color, kind) in new_board {
        if positions.contains(&pos) {
            spawn_light(commands, pos, color, kind, to_world(pos));
        }
    }
}

/// Clears any obstacle carrying `AdjacentMatchDamage` when a match lands orthogonally adjacent.
/// `DeepShadow` chips one durability point per adjacent match.
pub(crate) fn clear_shadow_at(
    removed_positions: &HashSet<GridPos>,
    commands: &mut Commands,
    shadow_q: &mut Query<(Entity, &GridPos, Option<&mut HardShadow>), With<AdjacentMatchDamage>>,
    shadow_count: &mut u32,
) {
    for (e, gp, hard) in shadow_q.iter_mut() {
        let hit = orthogonal_neighbors(*gp)
            .iter()
            .any(|n| removed_positions.contains(n));
        if !hit {
            continue;
        }
        if let Some(mut hard) = hard
            && hard.0 > 1
        {
            hard.0 -= 1;
            continue;
        }
        commands.entity(e).try_despawn();
        *shadow_count = shadow_count.saturating_sub(1);
    }
}

/// Destroys every shadow obstacle occupying one exact cell, regardless of `DeepShadow`
/// durability. This is the semantic contract of the Eliminate special: unlike an ordinary match,
/// it targets the whole cell rather than dealing one adjacent hit.
pub(crate) fn clear_shadow_cell(
    target: GridPos,
    commands: &mut Commands,
    shadow_q: &mut Query<
        (Entity, &GridPos, Option<&mut HardShadow>),
        (With<AdjacentMatchDamage>, Without<Light>),
    >,
    shadow_count: &mut u32,
) -> u32 {
    let mut removed = 0u32;
    for (entity, position, _) in shadow_q.iter_mut() {
        if *position != target {
            continue;
        }
        commands.entity(entity).try_despawn();
        removed += 1;
    }
    *shadow_count = shadow_count.saturating_sub(removed);
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::ShadowCount;

    #[derive(Resource, Default)]
    struct Cleared(u32);

    fn clear_deep_shadow_cell(
        mut commands: Commands,
        mut shadows: Query<
            (Entity, &GridPos, Option<&mut HardShadow>),
            (With<AdjacentMatchDamage>, Without<Light>),
        >,
        mut count: ResMut<ShadowCount>,
        mut cleared: ResMut<Cleared>,
    ) {
        cleared.0 = clear_shadow_cell(
            GridPos { x: 2, y: 3 },
            &mut commands,
            &mut shadows,
            &mut count.0,
        );
    }

    #[test]
    fn eliminate_cell_bypasses_deep_shadow_durability() {
        let mut app = App::new();
        app.insert_resource(ShadowCount(1))
            .init_resource::<Cleared>()
            .add_systems(Update, clear_deep_shadow_cell);
        let shadow = app
            .world_mut()
            .spawn((GridPos { x: 2, y: 3 }, AdjacentMatchDamage, DeepShadow(3)))
            .id();

        app.update();

        assert_eq!(app.world().resource::<Cleared>().0, 1);
        assert_eq!(app.world().resource::<ShadowCount>().0, 0);
        assert!(app.world().get_entity(shadow).is_err());
    }
}

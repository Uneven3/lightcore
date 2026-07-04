use bevy::prelude::*;
use rand::Rng;
use std::collections::{HashMap, HashSet};

use crate::core::prelude::*;
use crate::visuals::assets::VisualCache;
use crate::visuals::breathing::{BreathPhase, SparkNucleusPulse};

fn generate_board_layout(rng: &mut impl Rng) -> Vec<(GridPos, LightColor)> {
    const ALL: [LightColor; 5] = [
        LightColor::Red,
        LightColor::Green,
        LightColor::Blue,
        LightColor::Yellow,
        LightColor::Purple,
    ];
    let mut board = [[LightColor::Red; GRID_H as usize]; GRID_W as usize];
    for x in 0..GRID_W as usize {
        for y in 0..GRID_H as usize {
            let mut forbidden = Vec::new();
            if x >= 2 && board[x - 1][y] == board[x - 2][y] {
                forbidden.push(board[x - 1][y]);
            }
            if y >= 2 && board[x][y - 1] == board[x][y - 2] {
                forbidden.push(board[x][y - 1]);
            }
            let allowed: Vec<LightColor> = ALL
                .iter()
                .filter(|c| !forbidden.contains(c))
                .copied()
                .collect();
            board[x][y] = allowed[rng.random_range(0..allowed.len())];
        }
    }
    (0..GRID_W)
        .flat_map(|x| (0..GRID_H).map(move |y| (GridPos { x, y }, board[x as usize][y as usize])))
        .collect()
}

fn board_has_valid_swap(board: &[(GridPos, LightColor)], blocked: &HashSet<GridPos>) -> bool {
    let mut grid: Grid = HashMap::new();
    for (idx, (pos, color)) in board.iter().enumerate() {
        if blocked.contains(pos) {
            continue;
        }
        let entity = Entity::from_raw_u32(idx as u32 + 1).unwrap();
        grid.insert(*pos, (entity, *color, LightKind::Normal));
    }
    find_valid_swap(&grid, &HashSet::new()).is_some()
}

pub(crate) fn generate_board(
    rng: &mut impl Rng,
    blocked: &HashSet<GridPos>,
) -> Vec<(GridPos, LightColor)> {
    loop {
        let board = generate_board_layout(rng);
        if board_has_valid_swap(&board, blocked) {
            return board;
        }
    }
}

pub(crate) fn spawn_light(
    commands: &mut Commands,
    cache: &VisualCache,
    pos: GridPos,
    color: LightColor,
    kind: LightKind,
    visual_start: Vec3,
) -> Entity {
    let mut rng = rand::rng();
    let phase = rng.random_range(0.0..std::f32::consts::TAU);
    // The cores (1 for a normal light, more for a power) are built reactively by
    // `visuals::core_motion::rebuild_cores` off `Changed<LightKind>`, and the glow halo by
    // `visuals::glow::attach_glow_pools` off `Added<Light>` — both keyed on this entity.
    // The ring mesh/material are shared per color (the ring is never recolored per-light).
    commands
        .spawn((
            Light,
            FallPhysics,
            color,
            kind,
            pos,
            VisualPos(visual_start),
            BreathPhase(phase), // shared by the cores and the glow halo so they breathe in lockstep
            Mesh2d(cache.light_mesh(kind, color)),
            MeshMaterial2d(cache.ring_mat(color)),
            Transform::from_translation(visual_start),
        ))
        .id()
}

pub(crate) fn spawn_shadow(commands: &mut Commands, cache: &VisualCache, pos: GridPos) {
    let world = to_world(pos);
    commands.spawn((
        Shadow,
        pos,
        Mesh2d(cache.shadow_mesh.clone()),
        MeshMaterial2d(cache.shadow_mat.clone()),
        Transform::from_translation(world.with_z(-0.5)),
    ));
}

pub(crate) fn spawn_blocker(commands: &mut Commands, cache: &VisualCache, pos: GridPos) {
    let world = to_world(pos);
    commands.spawn((
        Shadow,
        Blocker,
        pos,
        Mesh2d(cache.blocker_mesh.clone()),
        MeshMaterial2d(cache.blocker_mat.clone()),
        Transform::from_translation(world.with_z(-0.45)),
    ));
}

pub(crate) fn spawn_ingredient_exits(commands: &mut Commands, cache: &VisualCache) {
    for x in 0..GRID_W {
        let pos = GridPos { x, y: 0 };
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
        ));
    }
}

pub(crate) fn spawn_sparks(commands: &mut Commands, cache: &VisualCache, columns: &[i32]) {
    for &x in columns {
        let pos = GridPos { x, y: GRID_H - 1 };
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
                    phase: x as f32 * 0.7,
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
    cache: &VisualCache,
    light_entities: &[(Entity, GridPos)],
) {
    let positions: HashSet<GridPos> = light_entities.iter().map(|(_, p)| *p).collect();
    for (e, _) in light_entities {
        commands.entity(*e).try_despawn();
    }
    let mut rng = rand::rng();
    let new_board = generate_board(&mut rng, &HashSet::new());
    for (pos, color) in new_board {
        if positions.contains(&pos) {
            spawn_light(
                commands,
                cache,
                pos,
                color,
                LightKind::Normal,
                to_world(pos),
            );
        }
    }
}

pub(crate) fn clear_shadow_at(
    removed_positions: &HashSet<GridPos>,
    commands: &mut Commands,
    shadow_q: &Query<
        (Entity, &GridPos),
        (
            With<Shadow>,
            Without<Blocker>,
            Without<Light>,
            Without<Spark>,
        ),
    >,
    shadow_count: &mut u32,
) {
    for (e, gp) in shadow_q.iter() {
        if removed_positions.contains(gp) {
            commands.entity(e).try_despawn();
            *shadow_count = shadow_count.saturating_sub(1);
        }
    }
}

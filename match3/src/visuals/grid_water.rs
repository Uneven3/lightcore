use bevy::asset::{Handle, load_internal_asset, uuid_handle};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d};

use crate::core::prelude::*;
use crate::state::GameState;

const RIPPLE_SLOTS: usize = 8;
const GRID_MARGIN: f32 = TILE * 1.15;
const GRID_WATER_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("25461935-9fd2-42e5-ac46-1e205523db75");

pub(crate) struct GridWaterPlugin;

impl Plugin for GridWaterPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            GRID_WATER_SHADER_HANDLE,
            "../../assets/shaders/grid_water.wgsl",
            Shader::from_wgsl
        );

        app.add_plugins(Material2dPlugin::<GridWaterMaterial>::default())
            .init_resource::<GridWaterState>()
            .add_systems(Startup, spawn_grid_water)
            .add_systems(
                Update,
                (
                    update_grid_water_material,
                    apply_grid_water_visibility.run_if(resource_changed::<GridWaterSettings>),
                )
                    .chain()
                    .run_if(not(in_state(GameState::GameOver))),
            );
    }
}

#[derive(Resource)]
pub(crate) struct GridWaterSettings {
    pub(crate) enabled: bool,
}

impl Default for GridWaterSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Resource, Default)]
pub(crate) struct GridWaterState {
    ripples: [Vec4; RIPPLE_SLOTS],
}

#[derive(Resource)]
struct GridWaterMaterialHandle(Handle<GridWaterMaterial>);

#[derive(Component)]
struct GridWaterPlane;

#[derive(ShaderType, Clone, Copy)]
struct GridWaterUniform {
    time: f32,
    tile: f32,
    enabled: f32,
    shard_glow: f32,
    half_size: Vec4,
    ripples: [Vec4; RIPPLE_SLOTS],
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct GridWaterMaterial {
    #[uniform(0)]
    params: GridWaterUniform,
}

impl Material2d for GridWaterMaterial {
    fn fragment_shader() -> ShaderRef {
        GRID_WATER_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

fn board_size() -> Vec2 {
    Vec2::new(
        GRID_W as f32 * TILE + GRID_MARGIN * 2.0,
        GRID_H as f32 * TILE + GRID_MARGIN * 2.0,
    )
}

fn empty_uniform() -> GridWaterUniform {
    let size = board_size();
    GridWaterUniform {
        time: 0.0,
        tile: TILE,
        enabled: 1.0,
        shard_glow: 0.0,
        half_size: Vec4::new(size.x * 0.5, size.y * 0.5, 0.0, 0.0),
        ripples: [Vec4::ZERO; RIPPLE_SLOTS],
    }
}

fn spawn_grid_water(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GridWaterMaterial>>,
) {
    let size = board_size();
    let material = materials.add(GridWaterMaterial {
        params: empty_uniform(),
    });
    commands.insert_resource(GridWaterMaterialHandle(material.clone()));
    commands.spawn((
        GridWaterPlane,
        Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
        MeshMaterial2d(material),
        Transform::from_xyz(0.0, 0.0, -4.0),
    ));
}

fn update_grid_water_material(
    time: Res<Time>,
    settings: Res<GridWaterSettings>,
    state: Res<GridWaterState>,
    handle: Res<GridWaterMaterialHandle>,
    mut materials: ResMut<Assets<GridWaterMaterial>>,
) {
    let Some(mut material) = materials.get_mut(&handle.0) else {
        return;
    };
    material.params.time = time.elapsed_secs();
    material.params.enabled = if settings.enabled { 1.0 } else { 0.0 };
    material.params.ripples = state.ripples;
}

fn apply_grid_water_visibility(
    settings: Res<GridWaterSettings>,
    mut plane: Query<&mut Visibility, With<GridWaterPlane>>,
) {
    for mut visibility in &mut plane {
        *visibility = if settings.enabled {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

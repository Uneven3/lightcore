use bevy::prelude::*;

use crate::core::grid::RaySettings;

pub(crate) mod assets;
pub(crate) mod bounce;
pub(crate) mod breathing;
pub(crate) mod camera;
pub(crate) mod core_motion;
pub(crate) mod effects;
pub(crate) mod glow;
pub(crate) mod grid_water;
pub(crate) mod light_trail;
pub(crate) mod motion;
pub(crate) mod particles;
pub(crate) mod render_target;
pub(crate) mod score_light;
pub(crate) mod space_background;

pub(crate) struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<camera::CameraShake>()
            .init_resource::<camera::ShakeSettings>()
            .init_resource::<camera::FpsTarget>()
            .init_resource::<camera::FrameTimer>()
            .init_resource::<glow::GlowSettings>()
            .init_resource::<grid_water::GridWaterSettings>()
            .init_resource::<particles::ParticleSettings>()
            .init_resource::<render_target::RenderScale>()
            .init_resource::<RaySettings>()
            .init_resource::<score_light::ShardSettings>()
            .insert_resource(ClearColor(Color::srgb(0.012, 0.012, 0.022))) // near-black space
            .add_observer(effects::on_power_light_consumed)
            .add_observer(effects::on_power_combo)
            .add_observer(light_trail::on_power_blast_trail)
            .add_observer(camera::on_chain_pop)
            .add_observer(score_light::on_chain_pop_score_light)
            .add_observer(score_light::on_light_popped)
            .add_plugins(space_background::SpaceBackgroundPlugin)
            .add_plugins(grid_water::GridWaterPlugin)
            .add_systems(PreStartup, assets::build_cache)
            .add_systems(PreUpdate, camera::record_frame_start)
            .add_systems(Update, camera::toggle_slow_mo)
            .add_systems(Last, camera::cap_framerate)
            .add_systems(Startup, camera::setup_camera)
            .add_systems(
                Update,
                (
                    score_light::tick_score_light,
                    glow::attach_glow_pools,
                    glow::flicker,
                    core_motion::rebuild_cores,
                    core_motion::animate_cores,
                ),
            )
            .add_systems(
                Update,
                (
                    motion::lerp_visual_pos,
                    bounce::detect_landing,
                    motion::update_drag_constrained,
                    motion::sync_transforms,
                )
                    .chain()
                    .run_if(not(in_state(crate::state::GameState::GameOver))),
            )
            .add_systems(
                Update,
                (
                    effects::tick_effect_anim,
                    bounce::tick_land_bounce,
                    particles::tick_particles,
                    light_trail::tick_laser_bolt,
                    light_trail::tick_traveling_light,
                    breathing::breathe,
                    core_motion::despawn_cores_on_pop,
                )
                    .run_if(not(in_state(crate::state::GameState::GameOver))),
            )
            .add_systems(
                Update,
                camera::apply_camera_shake.run_if(not(in_state(crate::state::GameState::GameOver))),
            )
            .add_systems(
                Update,
                (
                    particles::sync_particle_mesh_settings
                        .run_if(resource_changed::<particles::ParticleSettings>),
                    // Mantiene el lienzo interno al aspecto de la ventana (y a la resolución interna
                    // elegida). Auto-gateado por un Local; solo actúa cuando algo cambia.
                    render_target::fit_canvas,
                ),
            );
    }
}

/// Drives the transient power-light blast effect: scale-lerp + alpha-fade, then despawn.
#[derive(Component)]
pub(crate) struct EffectAnim {
    pub(crate) timer: Timer,
    pub(crate) start_scale: Vec3,
    pub(crate) end_scale: Vec3,
    pub(crate) base_alpha: f32,
    /// Fraction of duration after which alpha starts fading to 0.
    pub(crate) fade_start_frac: f32,
    /// Tiempo antes de empezar la animación. `None` = inmediato.
    pub(crate) delay: Option<Timer>,
}

use bevy::prelude::*;

use crate::core::components::Light;
use crate::state::{AttemptScoped, MatchFrameSet, MatchPhase, MatchScoped, Overlay};

pub(crate) mod additive_material;
pub(crate) mod assets;
pub(crate) mod bounce;
pub(crate) mod breathing;
pub(crate) mod camera;
pub(crate) mod core_motion;
pub(crate) mod effects;
pub(crate) mod glow;
pub(crate) mod grid_water;
pub(crate) mod hard_shadow;
pub(crate) mod light_trail;
pub(crate) mod motion;
pub(crate) mod particles;
pub(crate) mod render_target;
pub(crate) mod score_light;
pub(crate) mod space_background;

pub(crate) struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<camera::FpsTarget>()
            .init_resource::<camera::FrameTimer>()
            .init_resource::<glow::GlowSettings>()
            .init_resource::<grid_water::GridWaterSettings>()
            .init_resource::<particles::ParticleSettings>()
            .init_resource::<score_light::ShardSettings>()
            .insert_resource(ClearColor(Color::srgb(0.012, 0.012, 0.022))) // near-black space
            .add_observer(effects::on_power_light_consumed)
            .add_observer(effects::on_power_combo)
            .add_observer(light_trail::on_power_blast_trail)
            .add_observer(score_light::on_capture_batch_score_light)
            .add_observer(score_light::on_score_drained)
            .add_observer(score_light::on_light_popped)
            .add_observer(particles::on_manual_light_eliminated)
            .add_observer(particles::on_light_teleported)
            .add_observer(mark_attempt_scoped::<particles::Particle>)
            .add_observer(mark_attempt_scoped::<light_trail::TravelingLight>)
            .add_observer(mark_attempt_scoped::<EffectAnim>)
            .add_observer(mark_attempt_scoped::<score_light::ScoreShard>)
            .add_observer(mark_attempt_scoped::<score_light::ScoreShardAbsorb>)
            .add_observer(mark_attempt_scoped::<score_light::ScoreShardScatter>)
            .add_observer(mark_attempt_scoped::<light_trail::LaserBolt>)
            .add_plugins(space_background::SpaceBackgroundPlugin)
            .add_plugins(grid_water::GridWaterPlugin)
            .add_plugins(additive_material::AdditiveMaterialPlugin)
            .add_systems(PreStartup, assets::build_cache)
            .add_systems(Startup, assets::publish_board_visuals)
            .add_systems(PreUpdate, camera::record_frame_start)
            .add_systems(Last, camera::cap_framerate)
            .add_systems(Startup, camera::setup_camera)
            .add_systems(
                Update,
                (
                    score_light::tick_score_light.run_if(
                        any_with_component::<score_light::ScoreShard>
                            .and_then(in_state(Overlay::None)),
                    ),
                    score_light::tick_score_shard_scatter.run_if(
                        any_with_component::<score_light::ScoreShardScatter>
                            .and_then(in_state(Overlay::None)),
                    ),
                    score_light::tick_score_shard_absorb.run_if(
                        any_with_component::<score_light::ScoreShardAbsorb>
                            .and_then(in_state(Overlay::None)),
                    ),
                    score_light::tick_score_shard_absorb_glow.run_if(
                        any_with_component::<score_light::ScoreShardAbsorbGlow>
                            .and_then(in_state(Overlay::None)),
                    ),
                    glow::attach_glow_pools.run_if(any_with_component::<Light>),
                    glow::sync_halo_settings.run_if(any_with_component::<glow::GlowPool>),
                    core_motion::rebuild_cores.run_if(any_with_component::<Light>),
                    core_motion::animate_cores
                        .run_if(any_with_component::<core_motion::CoreMotion>),
                    core_motion::animate_hollow_flow
                        .run_if(any_with_component::<core_motion::HollowFlowParticle>),
                    core_motion::animate_hollow_breath
                        .run_if(any_with_component::<core_motion::HollowBreathing>),
                ),
            )
            .add_systems(
                Update,
                (
                    motion::lerp_visual_pos.in_set(MatchFrameSet::VisualPosition),
                    bounce::detect_landing,
                )
                    .chain()
                    .run_if(
                        in_state(MatchPhase::Falling)
                            .or_else(in_state(MatchPhase::Spawning))
                            .or_else(in_state(MatchPhase::SwapAnimating)),
                    ),
            )
            .add_systems(
                Update,
                motion::update_drag_constrained
                    .run_if(crate::state::match_active),
            )
            .add_systems(
                Update,
                motion::sync_transforms
                    .after(motion::lerp_visual_pos)
                    .after(motion::update_drag_constrained)
                    .run_if(not(in_state(MatchPhase::GameOver))),
            )
            .add_systems(
                Update,
                motion::update_move_drag_preview
                    .after(motion::sync_transforms)
                    .run_if(crate::state::match_active),
            )
            .add_systems(
                Update,
                (
                    effects::tick_effect_anim.run_if(any_with_component::<EffectAnim>),
                    bounce::tick_land_bounce.run_if(any_with_component::<bounce::LandBounce>),
                    particles::tick_particles.run_if(any_with_component::<particles::Particle>),
                    light_trail::tick_laser_bolt
                        .run_if(any_with_component::<light_trail::LaserBolt>),
                    light_trail::tick_traveling_light
                        .run_if(any_with_component::<light_trail::TravelingLight>),
                    breathing::pulse_spark_nucleus
                        .run_if(any_with_component::<crate::board::SparkNucleusPulse>),
                    // A core is part of the light, so it follows the same PopAnim lifetime as its
                    // membrane. Removing it at pop start left a bright ring with an empty centre
                    // for one or more frames, which read as random flickering in long chains.
                    core_motion::fade_cores_on_pop,
                    hard_shadow::update_hard_shadow_label,
                )
                    .run_if(not(in_state(MatchPhase::GameOver))),
            )
            .add_systems(
                Update,
                (
                    particles::sync_particle_mesh_settings
                        .run_if(resource_changed::<particles::ParticleSettings>),
                    // Keeps desktop/mobile camera viewports aligned with the window. Auto-gated by
                    // a Local; it only acts when the window or device mode changes.
                    render_target::fit_canvas,
                ),
            );

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        app.add_systems(Update, camera::toggle_slow_mo);
    }
}

fn mark_attempt_scoped<T: Component>(trigger: On<Add, T>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert((AttemptScoped, MatchScoped));
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

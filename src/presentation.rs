//! Shared presentation geometry.
//!
//! The game used to let the camera, menus and HUD infer their own idea of "mobile" from a
//! platform toggle. That made them disagree after a resize or a live mode change. `GameLayout`
//! is now the single source of truth: it describes the effective viewport and the screen regions
//! reserved for gameplay and chrome. It deliberately contains no gameplay state.

use bevy::prelude::*;

use crate::core::light::LightColor;
use crate::core::locale::{Language, TrKey};
use crate::gameplay::{CollectedCores, Score};
use crate::state::{MatchPhase, Overlay, Screen};

const WIDE_MIN_WIDTH: f32 = 900.0;
const WIDE_MIN_ASPECT: f32 = 1.25;

/// Selects the physical presentation viewport, not the UI composition. `GameLayout` derives
/// compact/wide composition from the resulting available size.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum ViewportMode {
    #[default]
    Auto,
    PortraitPreview,
}

impl ViewportMode {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Auto => Self::PortraitPreview,
            Self::PortraitPreview => Self::Auto,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct PresentationSettings {
    pub(crate) viewport_mode: ViewportMode,
    pub(crate) internal_resolution: InternalResolution,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InternalResolution {
    #[default]
    Native,
    High,
    Medium,
    Low,
}

impl InternalResolution {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Native => Self::High,
            Self::High => Self::Medium,
            Self::Medium => Self::Low,
            Self::Low => Self::Native,
        }
    }

    pub(crate) fn label(self, lang: Language) -> String {
        let value = lang.tr(match self {
            Self::Native => TrKey::ResNative,
            Self::High => TrKey::ResHigh,
            Self::Medium => TrKey::ResMedium,
            Self::Low => TrKey::ResLow,
        });
        format!("{}: {}", lang.tr(TrKey::InternalResolution), value)
    }

    fn target_height(self, native: UVec2) -> u32 {
        match self {
            Self::Native => native.y,
            Self::High => 900,
            Self::Medium => 720,
            Self::Low => 540,
        }
    }

    pub(crate) fn size_for_viewport(self, viewport_size: UVec2) -> UVec2 {
        let native = viewport_size.max(UVec2::ONE);
        if self == Self::Native {
            return native;
        }

        let target_h = self.target_height(native).min(native.y).max(1);
        let target_w = ((native.x as u64 * target_h as u64 + native.y as u64 / 2) / native.y as u64)
            .max(1) as u32;
        UVec2::new(target_w, target_h)
    }
}

/// The score readout is also the world-space collector that absorbs score shards. This marker is
/// intentionally explicit: the entity is part of the score-light presentation contract, not a
/// decorative HUD label that can be moved independently from its particles.
#[derive(Component)]
pub(crate) struct ScoreCollector;

/// The objective card is also a collector, but only while the level asks for one specific color.
/// Keeping this contract explicit prevents it from becoming a decorative label that can move
/// independently from the particles which communicate the level objective.
#[derive(Component)]
pub(crate) struct ColorGoalCollector;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CollectorRoute {
    Score,
    ColorGoal,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ColorGoalTarget {
    pub(crate) color: LightColor,
    pub(crate) position: Vec3,
}

/// World-space destinations for collected-light particles. The score always exists; a color goal
/// is published only for levels whose objective consumes one particular color.
#[derive(Resource, Default)]
pub(crate) struct LightcoreCollectorTargets {
    pub(crate) score: Vec3,
    pub(crate) color_goal: Option<ColorGoalTarget>,
}

impl LightcoreCollectorTargets {
    pub(crate) fn route(&self, color: LightColor) -> (Vec3, CollectorRoute) {
        match self.color_goal {
            Some(goal) if goal.color == color => (goal.position, CollectorRoute::ColorGoal),
            _ => (self.score, CollectorRoute::Score),
        }
    }
}

/// Arrival envelope used by the native UI objective card when it absorbs a matching shard.
#[derive(Resource, Default)]
pub(crate) struct ColorGoalCollectorPulse(pub(crate) f32);

/// Animated score projection. Gameplay remains authoritative; this adapter converges toward the
/// domain score independently from particle count.
#[derive(Resource, Default)]
pub(crate) struct DisplayedScore(pub(crate) u32);

/// Arrival-driven projection of collected cores. Gameplay updates `CollectedCores` immediately,
/// while score-light particles add to this resource only when they physically reach a collector.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct DisplayedCollectedCores(pub(crate) [u32; 5]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PhysicalViewport {
    pub(crate) position: UVec2,
    pub(crate) size: UVec2,
}

/// Facts supplied by the window/platform adapter. Coordinates are logical pixels in window space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ViewportFacts {
    pub(crate) window_origin: Vec2,
    pub(crate) size: Vec2,
    pub(crate) scale_factor: f32,
    pub(crate) physical: PhysicalViewport,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LayoutClass {
    #[default]
    CompactPortrait,
    WideLandscape,
}

impl LayoutClass {
    pub(crate) fn is_compact(self) -> bool {
        self == Self::CompactPortrait
    }
}

/// Rectangles use viewport-local logical coordinates with a top-left origin, matching Bevy UI.
#[derive(Resource, Clone, Debug, PartialEq)]
pub(crate) struct GameLayout {
    pub(crate) class: LayoutClass,
    pub(crate) viewport: ViewportFacts,
    pub(crate) playfield: Rect,
    pub(crate) top_bar: Rect,
    pub(crate) bottom_dock: Option<Rect>,
    pub(crate) side_rail: Option<Rect>,
    pub(crate) score_anchor: Vec2,
}

impl Default for GameLayout {
    fn default() -> Self {
        compute_game_layout(ViewportFacts {
            size: Vec2::new(1280.0, 720.0),
            scale_factor: 1.0,
            physical: PhysicalViewport {
                position: UVec2::ZERO,
                size: UVec2::new(1280, 720),
            },
            ..default()
        })
    }
}

pub(crate) struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PresentationSettings>()
            .init_resource::<GameLayout>()
            .init_resource::<LightcoreCollectorTargets>()
            .init_resource::<ColorGoalCollectorPulse>()
            .init_resource::<DisplayedScore>()
            .init_resource::<DisplayedCollectedCores>()
            .add_systems(PreStartup, update_game_layout)
            .add_systems(PreUpdate, update_game_layout)
            .add_systems(Update, project_hud_counters.run_if(in_state(Screen::Match)));
    }
}

fn approach_counter(current: u32, target: u32, dt: f32) -> u32 {
    if current == target {
        return current;
    }
    let distance = current.abs_diff(target);
    let step = ((distance as f32 * 7.0 * dt).ceil() as u32).max(1);
    if current < target {
        current.saturating_add(step).min(target)
    } else {
        current.saturating_sub(step).max(target)
    }
}

fn project_hud_counters(
    time: Res<Time>,
    phase: Res<State<MatchPhase>>,
    overlay: Res<State<Overlay>>,
    score: Res<Score>,
    collected: Res<CollectedCores>,
    mut displayed_score: ResMut<DisplayedScore>,
    mut displayed_cores: ResMut<DisplayedCollectedCores>,
) {
    if *overlay.get() != Overlay::None {
        return;
    }
    if *phase.get() == MatchPhase::Loading {
        displayed_score.0 = score.0;
        displayed_cores.0 = collected.0;
        return;
    }
    displayed_score.0 = approach_counter(displayed_score.0, score.0, time.delta_secs());
    for (displayed, authoritative) in displayed_cores.0.iter_mut().zip(collected.0) {
        // Increases belong to particle arrival. This system only reconciles backwards on reset or
        // retry so presentation can never show more than the authoritative domain counter.
        *displayed = (*displayed).min(authoritative);
    }
}

/// Returns the effective presentation viewport. `PortraitPreview` is a development/presentation
/// aid; actual phones use `Auto` and therefore their real aspect ratio.
pub(crate) fn physical_viewport(window_size: UVec2, mode: ViewportMode) -> PhysicalViewport {
    let window_size = window_size.max(UVec2::ONE);
    if mode == ViewportMode::Auto {
        return PhysicalViewport {
            position: UVec2::ZERO,
            size: window_size,
        };
    }

    let target_aspect = 9.0 / 16.0;
    let window_aspect = window_size.x as f32 / window_size.y as f32;
    let (width, height) = if window_aspect < target_aspect {
        let width = window_size.x as f32;
        (width, width / target_aspect)
    } else {
        let height = window_size.y as f32;
        (height * target_aspect, height)
    };
    let size = UVec2::new(width.round() as u32, height.round() as u32).max(UVec2::ONE);
    PhysicalViewport {
        position: (window_size - size) / 2,
        size,
    }
}

pub(crate) fn viewport_facts(window: &Window, mode: ViewportMode) -> ViewportFacts {
    let physical = physical_viewport(
        UVec2::new(window.physical_width(), window.physical_height()),
        mode,
    );
    let scale_factor = window.scale_factor().max(f32::EPSILON);
    ViewportFacts {
        window_origin: physical.position.as_vec2() / scale_factor,
        size: physical.size.as_vec2() / scale_factor,
        scale_factor,
        physical,
    }
}

/// Pure responsive layout calculation. Camera composition and native UI consume the same result.
pub(crate) fn compute_game_layout(viewport: ViewportFacts) -> GameLayout {
    let size = viewport.size.max(Vec2::ONE);
    let aspect = size.x / size.y.max(1.0);
    let class = if size.x >= WIDE_MIN_WIDTH && aspect >= WIDE_MIN_ASPECT {
        LayoutClass::WideLandscape
    } else {
        LayoutClass::CompactPortrait
    };

    match class {
        LayoutClass::CompactPortrait => {
            let margin = 8.0;
            let top_h = 64.0;
            let dock_h = 154.0_f32.min((size.y * 0.28).max(118.0));
            let playfield = Rect::from_corners(
                Vec2::new(margin, top_h),
                Vec2::new(
                    (size.x - margin).max(margin + 1.0),
                    (size.y - dock_h).max(top_h + 1.0),
                ),
            );
            GameLayout {
                class,
                viewport,
                playfield,
                top_bar: Rect::from_corners(Vec2::ZERO, Vec2::new(size.x, top_h)),
                bottom_dock: Some(Rect::from_corners(Vec2::new(0.0, size.y - dock_h), size)),
                side_rail: None,
                score_anchor: Vec2::new(size.x * 0.5, 34.0),
            }
        }
        LayoutClass::WideLandscape => {
            let outer = 16.0;
            let top_h = 64.0;
            let rail_w = (size.x * 0.22).clamp(244.0, 320.0);
            let rail_left = size.x - outer - rail_w;
            let playfield = Rect::from_corners(
                Vec2::new(outer, top_h),
                Vec2::new(
                    (rail_left - outer).max(outer + 1.0),
                    (size.y - outer).max(top_h + 1.0),
                ),
            );
            let side_rail = Rect::from_corners(
                Vec2::new(rail_left, top_h),
                Vec2::new(size.x - outer, size.y - outer),
            );
            GameLayout {
                class,
                viewport,
                playfield,
                top_bar: Rect::from_corners(Vec2::ZERO, Vec2::new(rail_left, top_h)),
                bottom_dock: None,
                side_rail: Some(side_rail),
                // Score is a persistent landmark and the destination of capture particles. The
                // side rail adapts around it; switching compact/wide must never relocate it.
                score_anchor: Vec2::new(size.x * 0.5, 34.0),
            }
        }
    }
}

fn update_game_layout(
    window: Single<&Window>,
    settings: Res<PresentationSettings>,
    mut layout: ResMut<GameLayout>,
) {
    let next = compute_game_layout(viewport_facts(&window, settings.viewport_mode));
    if *layout != next {
        *layout = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(width: f32, height: f32) -> ViewportFacts {
        ViewportFacts {
            size: Vec2::new(width, height),
            scale_factor: 1.0,
            physical: PhysicalViewport {
                position: UVec2::ZERO,
                size: UVec2::new(width as u32, height as u32),
            },
            ..default()
        }
    }

    #[test]
    fn portrait_sizes_reserve_a_bottom_dock() {
        for (width, height) in [(360.0, 800.0), (390.0, 844.0), (600.0, 960.0)] {
            let layout = compute_game_layout(facts(width, height));
            assert_eq!(layout.class, LayoutClass::CompactPortrait);
            let dock = layout.bottom_dock.expect("compact layout needs a dock");
            assert!(layout.playfield.max.y <= dock.min.y);
            assert!(layout.playfield.width() > 0.0);
            assert!(layout.playfield.height() > 0.0);
        }
    }

    #[test]
    fn landscape_sizes_reserve_a_side_rail() {
        for (width, height) in [(1280.0, 720.0), (1920.0, 1080.0), (2560.0, 1080.0)] {
            let layout = compute_game_layout(facts(width, height));
            assert_eq!(layout.class, LayoutClass::WideLandscape);
            let rail = layout.side_rail.expect("wide layout needs a side rail");
            assert!(layout.playfield.max.x <= rail.min.x);
            assert!(layout.playfield.height() > 560.0);
            assert!(layout.score_anchor.x < rail.min.x);
            assert_eq!(layout.score_anchor.x, width * 0.5);
        }
    }

    #[test]
    fn narrow_desktop_window_uses_available_space_not_platform_name() {
        let layout = compute_game_layout(facts(720.0, 720.0));
        assert_eq!(layout.class, LayoutClass::CompactPortrait);
    }

    #[test]
    fn portrait_preview_is_centered_and_never_exceeds_window() {
        let viewport = physical_viewport(UVec2::new(1920, 1080), ViewportMode::PortraitPreview);
        assert_eq!(viewport.size.y, 1080);
        assert_eq!(viewport.size.x, 608);
        assert_eq!(viewport.position, UVec2::new(656, 0));
    }

    #[test]
    fn matching_color_routes_to_goal_collector_only() {
        let targets = LightcoreCollectorTargets {
            score: Vec3::new(1.0, 2.0, 3.0),
            color_goal: Some(ColorGoalTarget {
                color: LightColor::Red,
                position: Vec3::new(4.0, 5.0, 6.0),
            }),
        };

        assert_eq!(
            targets.route(LightColor::Red),
            (Vec3::new(4.0, 5.0, 6.0), CollectorRoute::ColorGoal)
        );
        assert_eq!(
            targets.route(LightColor::Blue),
            (Vec3::new(1.0, 2.0, 3.0), CollectorRoute::Score)
        );
    }

    #[test]
    fn levels_without_color_goal_route_every_color_to_score() {
        let targets = LightcoreCollectorTargets {
            score: Vec3::new(7.0, 8.0, 9.0),
            color_goal: None,
        };

        for color in [
            LightColor::Red,
            LightColor::Green,
            LightColor::Blue,
            LightColor::Yellow,
            LightColor::Purple,
        ] {
            assert_eq!(targets.route(color), (targets.score, CollectorRoute::Score));
        }
    }
}

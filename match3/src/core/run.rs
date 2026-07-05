use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::light::LightColor;
use super::locale::{Language, TrKey};
use super::storage::save_file_path;
use crate::gameplay::CoreReserve;

pub(crate) const RUN_LEVELS: u32 = 13;
const MAX_BOON_LEVEL: u8 = 3;
const RUN_SAVE_VERSION: &str = "lightcore-run-v2";

pub(crate) struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunState>()
            .add_systems(Startup, load_run_progress)
            .add_systems(Update, save_run_progress);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BoonKind {
    RedValue,
    GreenReserve,
    BlueMoves,
    SparkBounty,
    PowerBounty,
    HollowWard,
}

impl BoonKind {
    pub(crate) const ALL: [BoonKind; 6] = [
        BoonKind::RedValue,
        BoonKind::GreenReserve,
        BoonKind::BlueMoves,
        BoonKind::SparkBounty,
        BoonKind::PowerBounty,
        BoonKind::HollowWard,
    ];

    pub(crate) fn index(self) -> usize {
        match self {
            BoonKind::RedValue => 0,
            BoonKind::GreenReserve => 1,
            BoonKind::BlueMoves => 2,
            BoonKind::SparkBounty => 3,
            BoonKind::PowerBounty => 4,
            BoonKind::HollowWard => 5,
        }
    }

    pub(crate) fn cost(self, level: u8) -> u32 {
        let base = match self {
            BoonKind::RedValue => 70,
            BoonKind::GreenReserve => 60,
            BoonKind::BlueMoves => 80,
            BoonKind::SparkBounty => 65,
            BoonKind::PowerBounty => 75,
            BoonKind::HollowWard => 70,
        };
        base + level as u32 * 45
    }

    pub(crate) fn label(self, lang: Language) -> &'static str {
        match self {
            BoonKind::RedValue => lang.tr(TrKey::BoonRedValue),
            BoonKind::GreenReserve => lang.tr(TrKey::BoonGreenReserve),
            BoonKind::BlueMoves => lang.tr(TrKey::BoonBlueMoves),
            BoonKind::SparkBounty => lang.tr(TrKey::BoonSparkBounty),
            BoonKind::PowerBounty => lang.tr(TrKey::BoonPowerBounty),
            BoonKind::HollowWard => lang.tr(TrKey::BoonHollowWard),
        }
    }

    pub(crate) fn status_label(self, lang: Language) -> &'static str {
        match self {
            BoonKind::RedValue => lang.tr(TrKey::BoonRedValueStatus),
            BoonKind::GreenReserve => lang.tr(TrKey::BoonGreenReserveStatus),
            BoonKind::BlueMoves => lang.tr(TrKey::BoonBlueMovesStatus),
            BoonKind::SparkBounty => lang.tr(TrKey::BoonSparkBountyStatus),
            BoonKind::PowerBounty => lang.tr(TrKey::BoonPowerBountyStatus),
            BoonKind::HollowWard => lang.tr(TrKey::BoonHollowWardStatus),
        }
    }
}

#[derive(Resource, Clone)]
pub(crate) struct RunState {
    pub(crate) active: bool,
    pub(crate) seed: u64,
    pub(crate) depth: u32,
    pub(crate) lives: u32,
    boons: [u8; BoonKind::ALL.len()],
    blue_meter: u32,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            active: false,
            seed: 0xC0DE_51A7_5EED,
            depth: 1,
            lives: 2,
            boons: [0; BoonKind::ALL.len()],
            blue_meter: 0,
        }
    }
}

impl RunState {
    pub(crate) fn start_new(&mut self) {
        self.active = true;
        self.seed = rand::random();
        self.depth = 1;
        self.lives = 2;
        self.boons = [0; BoonKind::ALL.len()];
        self.blue_meter = 0;
    }

    pub(crate) fn abandon(&mut self) {
        self.active = false;
        self.depth = 1;
        self.lives = 2;
        self.boons = [0; BoonKind::ALL.len()];
        self.blue_meter = 0;
    }

    /// Enters `depth`, starting a fresh run only if there is no active one. The level map owns the
    /// explicit "Nuevo run" affordance, so clicking an older node never silently wipes progress.
    pub(crate) fn enter_depth(&mut self, depth: u32) -> bool {
        let starting_new = !self.active;
        if starting_new {
            self.start_new();
        }
        self.depth = depth.max(1);
        self.blue_meter = 0;
        starting_new
    }

    pub(crate) fn complete_depth(&mut self, depth: u32) {
        if self.active {
            self.depth = self.depth.max(depth.saturating_add(1));
            if depth >= RUN_LEVELS {
                self.active = false;
            }
        }
    }

    pub(crate) fn level(&self, boon: BoonKind) -> u8 {
        self.boons[boon.index()]
    }

    pub(crate) fn can_buy(&self, boon: BoonKind) -> bool {
        self.level(boon) < MAX_BOON_LEVEL
    }

    pub(crate) fn boon_cost(&self, boon: BoonKind) -> Option<u32> {
        self.can_buy(boon).then(|| boon.cost(self.level(boon)))
    }

    pub(crate) fn buy(&mut self, boon: BoonKind) -> bool {
        self.grant(boon)
    }

    pub(crate) fn grant(&mut self, boon: BoonKind) -> bool {
        if !self.can_buy(boon) {
            return false;
        }
        self.boons[boon.index()] += 1;
        true
    }

    pub(crate) fn reward_offer(&self, completed_depth: u32, count: usize) -> Vec<BoonKind> {
        let mut pool: Vec<_> = BoonKind::ALL
            .iter()
            .copied()
            .filter(|&boon| self.can_buy(boon))
            .collect();
        let mut offers = Vec::with_capacity(count);
        let salt = self.seed
            ^ (completed_depth as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ self.depth as u64;
        let mut rng = StdRng::seed_from_u64(salt);
        for _ in 0..count {
            if pool.is_empty() {
                break;
            }
            let idx = rng.random_range(0..pool.len());
            offers.push(pool.swap_remove(idx));
        }
        offers
    }

    pub(crate) fn score_bonus_for_color(&self, color: LightColor, count: u32) -> u32 {
        match color {
            LightColor::Red => count * self.level(BoonKind::RedValue) as u32 * 2,
            _ => 0,
        }
    }

    pub(crate) fn reserve_bonus_for_color(&self, color: LightColor, count: u32) -> u32 {
        match color {
            LightColor::Green => count * self.level(BoonKind::GreenReserve) as u32,
            _ => 0,
        }
    }

    pub(crate) fn blue_move_bonus(&mut self, blue_count: u32) -> u32 {
        let level = self.level(BoonKind::BlueMoves) as u32;
        if level == 0 {
            return 0;
        }
        self.blue_meter += blue_count;
        let threshold = 7u32.saturating_sub(level).max(3);
        let moves = self.blue_meter / threshold;
        self.blue_meter %= threshold;
        moves
    }

    pub(crate) fn spark_bonus(&self) -> u32 {
        self.level(BoonKind::SparkBounty) as u32 * 25
    }

    pub(crate) fn power_bonus(&self, created: u32) -> u32 {
        created * self.level(BoonKind::PowerBounty) as u32 * 12
    }

    pub(crate) fn hollow_spawn_chance(&self, base_chance: f32) -> f32 {
        let reduction = 0.28 * self.level(BoonKind::HollowWard) as f32;
        base_chance * (1.0 - reduction).max(0.16)
    }

    fn encode(&self, reserve: u32) -> String {
        format!(
            "{RUN_SAVE_VERSION}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            if self.active { 1 } else { 0 },
            self.seed,
            self.depth,
            self.blue_meter,
            reserve,
            self.lives,
            self.boons
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn decode(raw: &str) -> Option<(Self, u32)> {
        let mut lines = raw.lines();
        let ver = lines.next()?;
        if ver != "lightcore-run-v1" && ver != "lightcore-run-v2" {
            return None;
        }
        let active = lines.next()? == "1";
        let seed = lines.next()?.parse().ok()?;
        let depth = lines.next()?.parse::<u32>().ok()?.clamp(1, RUN_LEVELS);
        let blue_meter = lines.next()?.parse().ok()?;
        let reserve = lines.next()?.parse().ok()?;

        let (lives, raw_boons) = if ver == "lightcore-run-v2" {
            let l = lines.next()?.parse::<u32>().ok()?;
            let b = lines.next()?;
            (l, b)
        } else {
            let b = lines.next()?;
            (2, b) // fallback for v1 save
        };

        let mut boons = [0; BoonKind::ALL.len()];
        for (idx, raw_level) in raw_boons.split(',').enumerate().take(boons.len()) {
            boons[idx] = raw_level.parse::<u8>().ok()?.min(MAX_BOON_LEVEL);
        }
        Some((
            Self {
                active,
                seed,
                depth,
                lives,
                boons,
                blue_meter,
            },
            if active { reserve } else { 0 },
        ))
    }
}

fn load_run_progress(mut run: ResMut<RunState>, mut reserve: ResMut<CoreReserve>) {
    let Some((saved_run, saved_reserve)) = load_run_text().and_then(|raw| RunState::decode(&raw))
    else {
        return;
    };
    *run = saved_run;
    reserve.0 = saved_reserve;
}

fn save_run_progress(run: Res<RunState>, reserve: Res<CoreReserve>) {
    if !run.is_changed() && !reserve.is_changed() {
        return;
    }
    let reserve_value = if run.active { reserve.0 } else { 0 };
    if let Err(err) = save_run_text(&run.encode(reserve_value)) {
        bevy::log::warn!("No se pudo guardar el run: {err}");
    }
}

#[cfg(target_arch = "wasm32")]
fn load_run_text() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn save_run_text(_raw: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn load_run_text() -> Option<String> {
    std::fs::read_to_string(run_save_path()).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn save_run_text(raw: &str) -> Result<(), String> {
    let path = run_save_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(path, raw).map_err(|err| err.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn run_save_path() -> std::path::PathBuf {
    save_file_path("run.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boons_scale_until_max_level() {
        let mut run = RunState::default();
        assert_eq!(run.level(BoonKind::RedValue), 0);
        assert!(run.buy(BoonKind::RedValue));
        assert!(run.buy(BoonKind::RedValue));
        assert!(run.buy(BoonKind::RedValue));
        assert!(!run.buy(BoonKind::RedValue));
        assert_eq!(run.level(BoonKind::RedValue), 3);
        assert_eq!(run.boon_cost(BoonKind::RedValue), None);
    }

    #[test]
    fn blue_boon_converts_collected_blues_into_moves() {
        let mut run = RunState::default();
        run.buy(BoonKind::BlueMoves);

        assert_eq!(run.blue_move_bonus(5), 0);
        assert_eq!(run.blue_move_bonus(1), 1);
    }

    #[test]
    fn run_progress_round_trips_through_save_text() {
        let mut run = RunState::default();
        run.start_new();
        run.depth = 4;
        run.grant(BoonKind::RedValue);

        let (decoded, reserve) = RunState::decode(&run.encode(123)).unwrap();

        assert!(decoded.active);
        assert_eq!(decoded.depth, 4);
        assert_eq!(decoded.level(BoonKind::RedValue), 1);
        assert_eq!(reserve, 123);
    }
}

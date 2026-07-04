use bevy::prelude::*;

use super::light::LightColor;

pub(crate) const RUN_LEVELS: u32 = 9;
const MAX_BOON_LEVEL: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BoonKind {
    RedValue,
    GreenReserve,
    BlueMoves,
    SparkBounty,
    PowerBounty,
}

impl BoonKind {
    pub(crate) const ALL: [BoonKind; 5] = [
        BoonKind::RedValue,
        BoonKind::GreenReserve,
        BoonKind::BlueMoves,
        BoonKind::SparkBounty,
        BoonKind::PowerBounty,
    ];

    pub(crate) fn index(self) -> usize {
        match self {
            BoonKind::RedValue => 0,
            BoonKind::GreenReserve => 1,
            BoonKind::BlueMoves => 2,
            BoonKind::SparkBounty => 3,
            BoonKind::PowerBounty => 4,
        }
    }

    pub(crate) fn cost(self, level: u8) -> u32 {
        let base = match self {
            BoonKind::RedValue => 70,
            BoonKind::GreenReserve => 60,
            BoonKind::BlueMoves => 80,
            BoonKind::SparkBounty => 65,
            BoonKind::PowerBounty => 75,
        };
        base + level as u32 * 45
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            BoonKind::RedValue => "Rojo+",
            BoonKind::GreenReserve => "Verde+",
            BoonKind::BlueMoves => "Azul+",
            BoonKind::SparkBounty => "Chispa+",
            BoonKind::PowerBounty => "Power+",
        }
    }

    pub(crate) fn status_label(self) -> &'static str {
        match self {
            BoonKind::RedValue => "Rojos dan score",
            BoonKind::GreenReserve => "Verdes dan reserve",
            BoonKind::BlueMoves => "Azules devuelven moves",
            BoonKind::SparkBounty => "Chispas dan score",
            BoonKind::PowerBounty => "Powers dan score",
        }
    }
}

#[derive(Resource, Clone)]
pub(crate) struct RunState {
    pub(crate) active: bool,
    pub(crate) seed: u64,
    pub(crate) depth: u32,
    boons: [u8; BoonKind::ALL.len()],
    blue_meter: u32,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            active: false,
            seed: 0xC0DE_51A7_5EED,
            depth: 1,
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
        self.boons = [0; BoonKind::ALL.len()];
        self.blue_meter = 0;
    }

    pub(crate) fn enter_depth(&mut self, depth: u32) {
        if !self.active || depth <= self.depth.saturating_sub(1) {
            self.start_new();
        }
        self.depth = depth.max(1);
        self.blue_meter = 0;
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
        if !self.can_buy(boon) {
            return false;
        }
        self.boons[boon.index()] += 1;
        true
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
}

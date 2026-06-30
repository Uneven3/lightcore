use bevy::prelude::*;

pub(crate) const CAMPAIGN_LEVELS: usize = 6;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct CampaignNode {
    pub(crate) level: u32,
    pub(crate) title: &'static str,
    pub(crate) blurb: &'static str,
    pub(crate) pos: [f32; 2],
    pub(crate) accent: [f32; 3],
}

pub(crate) const CAMPAIGN_NODES: [CampaignNode; CAMPAIGN_LEVELS] = [
    CampaignNode {
        level: 1,
        title: "Orbita Base",
        blurb: "Entrada limpia. Aprende el ritmo y alcanza el score objetivo.",
        pos: [11.0, 56.0],
        accent: [0.40, 0.80, 1.25],
    },
    CampaignNode {
        level: 2,
        title: "Ruta de Chispas",
        blurb: "Abre caminos y baja ingredientes sin romper la cadencia del tablero.",
        pos: [34.0, 28.0],
        accent: [0.55, 1.00, 0.72],
    },
    CampaignNode {
        level: 3,
        title: "Nucleo Velado",
        blurb: "La presion pasa a ser espacial: importa donde cae cada explosion.",
        pos: [58.0, 63.0],
        accent: [1.05, 0.72, 1.20],
    },
    CampaignNode {
        level: 4,
        title: "Cinturon Rojo",
        blurb: "Contrarreloj. El throughput manda y el reloj no perdona.",
        pos: [82.0, 36.0],
        accent: [1.28, 0.54, 0.54],
    },
    CampaignNode {
        level: 5,
        title: "Cosecha Roja",
        blurb: "Cosecha lightcores rojos para alimentar el reactor central.",
        pos: [90.0, 50.0],
        accent: [0.92, 0.25, 0.30],
    },
    CampaignNode {
        level: 6,
        title: "Cosecha Azul",
        blurb: "Cosecha lightcores azules para estabilizar la orbita.",
        pos: [95.0, 20.0],
        accent: [0.25, 0.50, 0.95],
    },
];

#[derive(Clone, Copy, Default)]
pub(crate) struct CampaignRecord {
    pub(crate) best_score: u32,
    pub(crate) completed: bool,
}

#[derive(Resource, Clone)]
pub(crate) struct CampaignProgress {
    best_scores: [CampaignRecord; CAMPAIGN_LEVELS],
}

impl Default for CampaignProgress {
    fn default() -> Self {
        Self {
            best_scores: [CampaignRecord::default(); CAMPAIGN_LEVELS],
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct CampaignUnlockResult {
    pub(crate) new_best: bool,
    pub(crate) unlocked_next: Option<u32>,
}

impl CampaignProgress {
    pub(crate) fn best_score(&self, level: u32) -> u32 {
        level_index(level)
            .map(|idx| self.best_scores[idx].best_score)
            .unwrap_or(0)
    }

    pub(crate) fn is_unlocked(&self, level: u32) -> bool {
        let Some(idx) = level_index(level) else {
            return false;
        };
        idx == 0 || self.best_scores[idx - 1].completed
    }

    pub(crate) fn record_score(&mut self, level: u32, score: u32) -> CampaignUnlockResult {
        let Some(idx) = level_index(level) else {
            return CampaignUnlockResult::default();
        };

        let old_best = self.best_scores[idx].best_score;
        let was_completed = self.best_scores[idx].completed;
        if score > old_best {
            self.best_scores[idx].best_score = score;
        }
        self.best_scores[idx].completed = true;

        let unlocked_next = CAMPAIGN_NODES
            .get(idx + 1)
            .filter(|_| !was_completed)
            .map(|next| next.level);

        CampaignUnlockResult {
            new_best: score > old_best,
            unlocked_next,
        }
    }
}

pub(crate) fn level_index(level: u32) -> Option<usize> {
    CAMPAIGN_NODES.iter().position(|node| node.level == level)
}

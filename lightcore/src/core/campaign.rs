use bevy::prelude::*;

use super::storage;

pub(crate) const CAMPAIGN_LEVELS: usize = 13;
const SAVE_VERSION: &str = "lightcore-progress-v2";

pub(crate) struct CampaignPlugin;

impl Plugin for CampaignPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CampaignProgress>()
            .add_systems(Startup, load_campaign_progress)
            .add_systems(
                Update,
                save_campaign_progress.run_if(resource_changed::<CampaignProgress>),
            );
    }
}

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
        title: "Puntaje Basico",
        blurb: "Consigue el puntaje objetivo.",
        pos: [11.0, 56.0],
        accent: [0.40, 0.80, 1.25],
    },
    CampaignNode {
        level: 2,
        title: "Ingredientes",
        blurb: "Baja ingredientes hasta la salida.",
        pos: [34.0, 28.0],
        accent: [0.55, 1.00, 0.72],
    },
    CampaignNode {
        level: 3,
        title: "Sombras",
        blurb: "Limpia todas las sombras.",
        pos: [58.0, 63.0],
        accent: [1.05, 0.72, 1.20],
    },
    CampaignNode {
        level: 4,
        title: "Contrarreloj",
        blurb: "Consigue el puntaje antes del tiempo.",
        pos: [82.0, 36.0],
        accent: [1.28, 0.54, 0.54],
    },
    CampaignNode {
        level: 5,
        title: "Cores Rojos",
        blurb: "Recolecta cores rojos.",
        pos: [90.0, 50.0],
        accent: [0.92, 0.25, 0.30],
    },
    CampaignNode {
        level: 6,
        title: "Cores Azules",
        blurb: "Recolecta cores azules.",
        pos: [95.0, 20.0],
        accent: [0.25, 0.50, 0.95],
    },
    CampaignNode {
        level: 7,
        title: "Pocos Movimientos",
        blurb: "Consigue puntaje con movimientos limitados.",
        pos: [86.0, 70.0],
        accent: [1.08, 0.86, 0.35],
    },
    CampaignNode {
        level: 8,
        title: "Ingredientes Bloqueados",
        blurb: "Baja ingredientes en un grid con bloqueos.",
        pos: [68.0, 30.0],
        accent: [0.42, 1.05, 0.80],
    },
    CampaignNode {
        level: 9,
        title: "Grid Irregular",
        blurb: "Recolecta verdes en un grid distinto.",
        pos: [45.0, 72.0],
        accent: [0.42, 1.05, 0.48],
    },
    CampaignNode {
        level: 10,
        title: "Tempestad Azul",
        blurb: "Recolecta 10 cores azules en 1 minuto.",
        pos: [108.0, 1532.0],
        accent: [0.35, 0.72, 1.25],
    },
    CampaignNode {
        level: 11,
        title: "Campo Minado",
        blurb: "Consigue 650 puntos esquivando bloqueadores.",
        pos: [-98.0, 1760.0],
        accent: [0.95, 0.38, 1.10],
    },
    CampaignNode {
        level: 12,
        title: "Invasion de Sombras",
        blurb: "Limpia las sombras y la jalea ultra dura central.",
        pos: [88.0, 1988.0],
        accent: [1.15, 0.45, 0.95],
    },
    CampaignNode {
        level: 13,
        title: "Tormenta Solar",
        blurb: "Consigue 800 puntos en 3 minutos con chispas activas.",
        pos: [0.0, 2216.0],
        accent: [1.25, 0.88, 0.25],
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

    fn encode(&self) -> String {
        let mut out = String::from(SAVE_VERSION);
        for record in self.best_scores {
            out.push('\n');
            out.push_str(&format!(
                "{},{}",
                record.best_score,
                if record.completed { 1 } else { 0 }
            ));
        }
        out
    }

    fn decode(raw: &str) -> Option<Self> {
        let mut lines = raw.lines();
        let ver = lines.next()?;
        if ver != "lightcore-progress-v1" && ver != "lightcore-progress-v2" {
            return None;
        }
        let mut progress = Self::default();
        let max_lines = if ver == "lightcore-progress-v2" {
            13
        } else {
            9
        };
        for (idx, line) in lines.take(max_lines).enumerate() {
            if idx >= CAMPAIGN_LEVELS {
                break;
            }
            let (score, completed) = line.split_once(',')?;
            progress.best_scores[idx] = CampaignRecord {
                best_score: score.parse().ok()?,
                completed: completed == "1",
            };
        }
        Some(progress)
    }
}

pub(crate) fn level_index(level: u32) -> Option<usize> {
    CAMPAIGN_NODES.iter().position(|node| node.level == level)
}

fn load_campaign_progress(mut progress: ResMut<CampaignProgress>) {
    let Some(saved) =
        storage::load_save_file("campaign.txt").and_then(|raw| CampaignProgress::decode(&raw))
    else {
        return;
    };
    *progress = saved;
}

fn save_campaign_progress(progress: Res<CampaignProgress>) {
    if let Err(err) = storage::write_save_file("campaign.txt", &progress.encode()) {
        bevy::log::warn!("No se pudo guardar el progreso de campaña: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_progress_round_trips_through_save_text() {
        let mut progress = CampaignProgress::default();
        progress.record_score(1, 240);
        progress.record_score(2, 180);

        let decoded = CampaignProgress::decode(&progress.encode()).unwrap();

        assert_eq!(decoded.best_score(1), 240);
        assert_eq!(decoded.best_score(2), 180);
        assert!(decoded.is_unlocked(3));
        assert!(!decoded.is_unlocked(4));
    }

    #[test]
    fn campaign_progress_rejects_unknown_save_version() {
        assert!(CampaignProgress::decode("other-version\n10,1").is_none());
    }
}

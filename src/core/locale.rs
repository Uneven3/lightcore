//! Multilanguage / localisation support.
//!
//! All user-visible strings live here so they can be swapped by changing `Locale`. New languages
//! are added by extending the `Language` enum and the `tr` match arms.
//!
//! Usage pattern:
//! ```rust,ignore
//! let lang = Language::default(); // or from Res<Language>
//! Text2d::new(lang.tr(TrKey::Play))
//! ```

use bevy::prelude::*;

/// Available display languages. Extend this enum to add more.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Resource)]
pub(crate) enum Language {
    #[default]
    Spanish,
    English,
}

impl Language {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Spanish => Self::English,
            Self::English => Self::Spanish,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Spanish => "Idioma: Español",
            Self::English => "Language: English",
        }
    }
}

/// Every translated string key. Keep alphabetical within each section for maintainability.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TrKey {
    // ── Main Menu ─────────────────────────────────────────────────────────────
    Options,
    TutorialOn,
    TutorialOff,
    Quit,

    // ── Pause ─────────────────────────────────────────────────────────────────
    PauseTitle,
    Resume,
    ExitToMenu,

    // ── Options ───────────────────────────────────────────────────────────────
    OptionsTitle,
    Back,
    Fullscreen,
    GridWater,
    DeviceLabel,
    DeviceDesktop,
    DeviceMobile,

    // ── Sliders ───────────────────────────────────────────────────────────────
    SliderGlowBrightness,
    SliderGlowOuterRadius,
    SliderGlowOuterAlpha,
    SliderGlowInnerRadius,
    SliderGlowInnerAlpha,
    SliderPopBurstCount,
    SliderBurstRadius,
    SliderMembraneRadius,
    SliderTrailParticleCount,
    SliderRaySpeed,
    SliderPopDuration,
    SliderStarStagger,
    SliderBoltWidth,
    SliderTrailDuration,
    SliderShardMinSecs,
    SliderShardMaxSecs,
    SliderShardBaseSize,
    SliderShardCurve,
    SliderShardHdrBoost,
    SliderShardHold,
    SliderVolume,

    // ── HUD / in-game ─────────────────────────────────────────────────────────
    Moves,
    Lives,
    Shop,
    Cores,

    // ── Shop item labels ──────────────────────────────────────────────────────
    ShopSwap,
    ShopEliminate,
    ShopUpgrade,
    ShopLife,
    BoonRedValue,
    BoonGreenReserve,
    BoonBlueMoves,
    BoonStarBounty,
    BoonPowerBounty,
    BoonHollowWard,
    BoonRedSpawn,
    BoonGreenSpawn,
    BoonBlueSpawn,
    BoonYellowSpawn,
    BoonPurpleSpawn,

    // ── Shop item status lines ────────────────────────────────────────────────
    ShopSwapStatus,
    ShopEliminateStatus,
    ShopUpgradeStatus,
    ShopLifeStatus,
    BoonRedValueStatus,
    BoonGreenReserveStatus,
    BoonBlueMovesStatus,
    BoonStarBountyStatus,
    BoonPowerBountyStatus,
    BoonHollowWardStatus,
    BoonRedSpawnStatus,
    BoonGreenSpawnStatus,
    BoonBlueSpawnStatus,
    BoonYellowSpawnStatus,
    BoonPurpleSpawnStatus,

    // ── Active badge / armed booster ─────────────────────────────────────────
    ShopModifiers,
    ArmedSwap,
    ArmedSwap1of2,
    ArmedEliminate,
    ArmedUpgrade,
    ActiveReady,
    ActiveChooseTarget,

    // ── Shop button state lines ───────────────────────────────────────────────
    NotEnoughCores,

    // ── Goal hints ────────────────────────────────────────────────────────────
    GoalTitle,
    GoalFreePlay,
    GoalReachTarget,
    GoalRescueSparks,
    GoalClearShadows,
    GoalScoreOnClock,
    GoalCollectColor,
    GoalColorOnClock,

    // ── Stats popup ───────────────────────────────────────────────────────────
    StatsTitle,
    StatsRed,
    StatsGreen,
    StatsBlue,
    StatsYellow,
    StatsPurple,
    StatsSpecials,
    StatsMaxCombo,
    StatsChains,

    // ── Tutorial overlay ─────────────────────────────────────────────────────
    TutorialClose,

    // ── Tutorial: Score ───────────────────────────────────────────────────────
    TutorialScoreTitle,

    // ── Tutorial: Sparks ──────────────────────────────────────────────────────
    TutorialSparksTitle,

    // ── Tutorial: ClearShadow ────────────────────────────────────────────────
    TutorialShadowTitle,

    // ── Tutorial: TimedScore ─────────────────────────────────────────────────
    TutorialTimedScoreTitle,

    // ── Tutorial: CollectColor ───────────────────────────────────────────────
    TutorialCollectColorTitle,

    // ── Tutorial: TimedCollectColor ──────────────────────────────────────────
    TutorialTimedColorTitle,

    // ── Tooltip titles ────────────────────────────────────────────────────────
    TooltipSwapTitle,
    TooltipEliminateTitle,
    TooltipUpgradeTitle,
    TooltipLifeTitle,
    TooltipMovesTitle,
    TooltipCoresTitle,
    TooltipBoonRedTitle,
    TooltipBoonGreenTitle,
    TooltipBoonBlueTitle,
    TooltipBoonStarTitle,
    TooltipBoonPowerTitle,
    TooltipBoonHollowTitle,
    TooltipBoonRedSpawnTitle,
    TooltipBoonGreenSpawnTitle,
    TooltipBoonBlueSpawnTitle,
    TooltipBoonYellowSpawnTitle,
    TooltipBoonPurpleSpawnTitle,

    // ── Tooltip descriptions ──────────────────────────────────────────────────
    TooltipSwapDesc,
    TooltipEliminateDesc,
    TooltipUpgradeDesc,
    TooltipLifeDesc,
    TooltipMovesDesc,
    TooltipCoresDesc,
    TooltipBoonRedDesc,
    TooltipBoonGreenDesc,
    TooltipBoonBlueDesc,
    TooltipBoonStarDesc,
    TooltipBoonPowerDesc,
    TooltipBoonHollowDesc,
    TooltipBoonRedSpawnDesc,
    TooltipBoonGreenSpawnDesc,
    TooltipBoonBlueSpawnDesc,
    TooltipBoonYellowSpawnDesc,
    TooltipBoonPurpleSpawnDesc,

    // ── Level menu ────────────────────────────────────────────────────────────
    Play,
    Restart,

    // ── Color names ───────────────────────────────────────────────────────────
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorYellow,
    ColorPurple,

    // ── Level Complete / End screen ──────────────────────────────────────────
    LevelCompleteTitle,
    BoardConsumedTitle,
    LightcoresCaptured,
    LevelUnlocked,
    NewHighScore,
    LevelAlreadyCompleted,
    MatchSummary,
    ChooseOneModifier,
    BoonContinueInstruction,
    BoonPurchased,
    StatsSparks,
}

impl Language {
    /// Returns the translation for `key` in this language.
    pub(crate) fn tr(self, key: TrKey) -> &'static str {
        match self {
            Language::Spanish => tr_es(key),
            Language::English => tr_en(key),
        }
    }
}

fn tr_es(key: TrKey) -> &'static str {
    match key {
        // Main menu
        TrKey::Options => "Opciones",
        TrKey::TutorialOn => "Tutorial: ON",
        TrKey::TutorialOff => "Tutorial: OFF",
        TrKey::Quit => "Salir",
        // Pause
        TrKey::PauseTitle => "Pausa",
        TrKey::Resume => "Reanudar",
        TrKey::ExitToMenu => "Salir al menu",
        // Options
        TrKey::OptionsTitle => "Opciones",
        TrKey::Back => "Volver",
        TrKey::Fullscreen => "Fullscreen",
        TrKey::GridWater => "Grid agua",
        TrKey::DeviceLabel => "Dispositivo",
        TrKey::DeviceDesktop => "Escritorio",
        TrKey::DeviceMobile => "Movil",

        // Sliders
        TrKey::SliderGlowBrightness => "Glow brillo",
        TrKey::SliderGlowOuterRadius => "Glow radio ext",
        TrKey::SliderGlowOuterAlpha => "Glow alpha ext",
        TrKey::SliderGlowInnerRadius => "Glow radio int",
        TrKey::SliderGlowInnerAlpha => "Glow alpha int",
        TrKey::SliderPopBurstCount => "Part pop burst",
        TrKey::SliderBurstRadius => "Part burst radio",
        TrKey::SliderMembraneRadius => "Part membrane radio",
        TrKey::SliderTrailParticleCount => "Part trail count",
        TrKey::SliderRaySpeed => "Rayos velocidad",
        TrKey::SliderPopDuration => "Rayos pop dur",
        TrKey::SliderStarStagger => "Rayos stagger",
        TrKey::SliderBoltWidth => "Rayos bolt width",
        TrKey::SliderTrailDuration => "Rayos trail dur",
        TrKey::SliderShardMinSecs => "Fragmento min segs",
        TrKey::SliderShardMaxSecs => "Fragmento max segs",
        TrKey::SliderShardBaseSize => "Fragmento base size",
        TrKey::SliderShardCurve => "Fragmento curve",
        TrKey::SliderShardHdrBoost => "Fragmento HDR boost",
        TrKey::SliderShardHold => "Fragmento pausa",
        TrKey::SliderVolume => "Volumen",
        // HUD
        TrKey::Moves => "moves",
        TrKey::Lives => "vidas",
        TrKey::Shop => "SHOP",
        TrKey::Cores => "cores",
        // Shop item labels
        TrKey::ShopSwap => "Mover",
        TrKey::ShopEliminate => "Eliminar",
        TrKey::ShopUpgrade => "Subir tier",
        TrKey::ShopLife => "+1 Vida",
        TrKey::BoonRedValue => "Rojo+",
        TrKey::BoonGreenReserve => "Verde+",
        TrKey::BoonBlueMoves => "Azul+",
        TrKey::BoonStarBounty => "Estrella×",
        TrKey::BoonPowerBounty => "Power+",
        TrKey::BoonHollowWard => "Hollow-",
        TrKey::BoonRedSpawn => "Frec. Roja",
        TrKey::BoonGreenSpawn => "Frec. Verde",
        TrKey::BoonBlueSpawn => "Frec. Azul",
        TrKey::BoonYellowSpawn => "Frec. Amarilla",
        TrKey::BoonPurpleSpawn => "Frec. Púrpura",
        // Shop status lines
        TrKey::ShopSwapStatus => "Arrastra y teletransporta",
        TrKey::ShopEliminateStatus => "Rompe 1 luz",
        TrKey::ShopUpgradeStatus => "Eleva 1 tier",
        TrKey::ShopLifeStatus => "Compra 1 vida extra",
        TrKey::BoonRedValueStatus => "Rojos dan score",
        TrKey::BoonGreenReserveStatus => "Verdes dan reserve",
        TrKey::BoonBlueMovesStatus => "Azules devuelven moves",
        TrKey::BoonStarBountyStatus => "Estrellas dan cores extra",
        TrKey::BoonPowerBountyStatus => "Powers dan score",
        TrKey::BoonHollowWardStatus => "Menos hollows",
        TrKey::BoonRedSpawnStatus => "+ luces rojas",
        TrKey::BoonGreenSpawnStatus => "+ luces verdes",
        TrKey::BoonBlueSpawnStatus => "+ luces azules",
        TrKey::BoonYellowSpawnStatus => "+ luces amarillas",
        TrKey::BoonPurpleSpawnStatus => "+ luces púrpuras",
        // Armed badge
        TrKey::ShopModifiers => "MOVIMIENTOS ESPECIALES",
        TrKey::ArmedSwap => "Arrastra una luz",
        TrKey::ArmedSwap1of2 => "Suelta sobre otra luz",
        TrKey::ArmedEliminate => "Eliminar activo",
        TrKey::ArmedUpgrade => "Subir tier activo",
        TrKey::ActiveReady => "Activo: listo",
        TrKey::ActiveChooseTarget => "Activo: elige destino",
        // Shop state
        TrKey::NotEnoughCores => "Sin cores suficientes",
        // Goal hints
        TrKey::GoalTitle => "Objetivo del nivel",
        TrKey::GoalFreePlay => "Captura libre",
        TrKey::GoalReachTarget => "Alcanza meta",
        TrKey::GoalRescueSparks => "Rescata chispas",
        TrKey::GoalClearShadows => "Limpia sombras",
        TrKey::GoalScoreOnClock => "Score antes del reloj",
        TrKey::GoalCollectColor => "Junta este color",
        TrKey::GoalColorOnClock => "Color antes del reloj",
        // Stats
        TrKey::StatsTitle => "--- DETALLES ---",
        TrKey::StatsRed => "Rojo",
        TrKey::StatsGreen => "Verde",
        TrKey::StatsBlue => "Azul",
        TrKey::StatsYellow => "Amarillo",
        TrKey::StatsPurple => "Morado",
        TrKey::StatsSpecials => "Especiales",
        TrKey::StatsMaxCombo => "Max Combo",
        TrKey::StatsChains => "Cadenas",
        // Tutorial controls
        TrKey::TutorialClose => "Entendí",
        // Tutorial titles
        TrKey::TutorialScoreTitle => "TUTORIAL - CÓMO JUGAR",
        TrKey::TutorialSparksTitle => "TUTORIAL: RECOLECTAR CHISPAS",
        TrKey::TutorialShadowTitle => "TUTORIAL: LIMPIAR SOMBRAS",
        TrKey::TutorialTimedScoreTitle => "TUTORIAL: CONTRARRELOJ",
        TrKey::TutorialCollectColorTitle => "TUTORIAL: RECOLECTAR COLOR",
        TrKey::TutorialTimedColorTitle => "TUTORIAL: COLOR BAJO TIEMPO",
        // Tooltip titles
        TrKey::TooltipSwapTitle => "Habilidad: Mover",
        TrKey::TooltipEliminateTitle => "Habilidad: Eliminar",
        TrKey::TooltipUpgradeTitle => "Habilidad: Subir tier",
        TrKey::TooltipLifeTitle => "Artículo: +1 Vida",
        TrKey::TooltipMovesTitle => "Estado: Movimientos",
        TrKey::TooltipCoresTitle => "Reserva: Núcleos",
        TrKey::TooltipBoonRedTitle => "Boon: Rojos+",
        TrKey::TooltipBoonGreenTitle => "Boon: Verdes+",
        TrKey::TooltipBoonBlueTitle => "Boon: Azul+",
        TrKey::TooltipBoonStarTitle => "Boon: Estrella×",
        TrKey::TooltipBoonPowerTitle => "Boon: Power+",
        TrKey::TooltipBoonHollowTitle => "Boon: Hollow-",
        TrKey::TooltipBoonRedSpawnTitle => "Boon: Roja+",
        TrKey::TooltipBoonGreenSpawnTitle => "Boon: Verde+",
        TrKey::TooltipBoonBlueSpawnTitle => "Boon: Azul+",
        TrKey::TooltipBoonYellowSpawnTitle => "Boon: Amarilla+",
        TrKey::TooltipBoonPurpleSpawnTitle => "Boon: Púrpura+",
        // Tooltip descriptions
        TrKey::TooltipSwapDesc => {
            "Arrastra un light sobre otro para teletransportarlos e intercambiar sus posiciones."
        }
        TrKey::TooltipEliminateDesc => {
            "Destruye un núcleo seleccionado, activando nuevas cascadas."
        }
        TrKey::TooltipUpgradeDesc => "Sube el tier del núcleo seleccionado (ej. normal a Rayo).",
        TrKey::TooltipLifeDesc => {
            "Otorga una vida de reserva para poder reintentar si fallas un nivel."
        }
        TrKey::TooltipMovesDesc => "Movimientos restantes para completar el nivel.",
        TrKey::TooltipCoresDesc => {
            "Reserva de núcleos acumulada. Úsalos para comprar habilidades y vidas."
        }
        TrKey::TooltipBoonRedDesc => "Los núcleos rojos recolectados otorgan +25% de puntuación.",
        TrKey::TooltipBoonGreenDesc => {
            "Los núcleos verdes recolectados te dan reserva de cores para la tienda."
        }
        TrKey::TooltipBoonBlueDesc => {
            "Cada 7 cores azules recolectados te devuelven +1 movimiento extra."
        }
        TrKey::TooltipBoonStarDesc => {
            "Romper una estrella entrega el doble de lightcores por rango."
        }
        TrKey::TooltipBoonPowerDesc => "Crear núcleos especiales otorga +18 puntos por rango.",
        TrKey::TooltipBoonHollowDesc => {
            "Disminuye la probabilidad de aparición de Hollows en el tablero."
        }
        TrKey::TooltipBoonRedSpawnDesc => {
            "Incrementa la probabilidad de aparición de núcleos rojos."
        }
        TrKey::TooltipBoonGreenSpawnDesc => {
            "Incrementa la probabilidad de aparición de núcleos verdes."
        }
        TrKey::TooltipBoonBlueSpawnDesc => {
            "Incrementa la probabilidad de aparición de núcleos azules."
        }
        TrKey::TooltipBoonYellowSpawnDesc => {
            "Incrementa la probabilidad de aparición de núcleos amarillos."
        }
        TrKey::TooltipBoonPurpleSpawnDesc => {
            "Incrementa la probabilidad de aparición de núcleos púrpuras."
        }
        // Level menu
        TrKey::Play => "Jugar",
        TrKey::Restart => "Reiniciar Run",
        // Color names
        TrKey::ColorRed => "rojos",
        TrKey::ColorGreen => "verdes",
        TrKey::ColorBlue => "azules",
        TrKey::ColorYellow => "amarillos",
        TrKey::ColorPurple => "morados",

        // Level Complete / End screen
        TrKey::LevelCompleteTitle => "Nivel Completado!",
        TrKey::BoardConsumedTitle => "Tablero Consumido!",
        TrKey::LightcoresCaptured => "Lightcores capturados: {}",
        TrKey::LevelUnlocked => "Nivel {:02} desbloqueado",
        TrKey::NewHighScore => "Nuevo mejor score registrado",
        TrKey::LevelAlreadyCompleted => "Nivel ya completado",
        TrKey::MatchSummary => "Resumen de partida:",
        TrKey::ChooseOneModifier => "BOONS DISPONIBLES · compra solo al completar una etapa",
        TrKey::BoonContinueInstruction => "[Click/Tap o Espacio] para continuar",
        TrKey::BoonPurchased => "Comprado",
        TrKey::StatsSparks => "Chispas",
    }
}

fn tr_en(key: TrKey) -> &'static str {
    match key {
        // Main menu
        TrKey::Options => "Options",
        TrKey::TutorialOn => "Tutorial: ON",
        TrKey::TutorialOff => "Tutorial: OFF",
        TrKey::Quit => "Quit",
        // Pause
        TrKey::PauseTitle => "Paused",
        TrKey::Resume => "Resume",
        TrKey::ExitToMenu => "Exit to Menu",
        // Options
        TrKey::OptionsTitle => "Options",
        TrKey::Back => "Back",
        TrKey::Fullscreen => "Fullscreen",
        TrKey::GridWater => "Grid Water",
        TrKey::DeviceLabel => "Device",
        TrKey::DeviceDesktop => "Desktop",
        TrKey::DeviceMobile => "Mobile",

        // Sliders
        TrKey::SliderGlowBrightness => "Glow Brightness",
        TrKey::SliderGlowOuterRadius => "Glow Outer Radius",
        TrKey::SliderGlowOuterAlpha => "Glow Outer Alpha",
        TrKey::SliderGlowInnerRadius => "Glow Inner Radius",
        TrKey::SliderGlowInnerAlpha => "Glow Inner Alpha",
        TrKey::SliderPopBurstCount => "Particle Pop Burst",
        TrKey::SliderBurstRadius => "Particle Burst Radius",
        TrKey::SliderMembraneRadius => "Particle Membrane Radius",
        TrKey::SliderTrailParticleCount => "Particle Trail Count",
        TrKey::SliderRaySpeed => "Ray Speed",
        TrKey::SliderPopDuration => "Ray Pop Duration",
        TrKey::SliderStarStagger => "Ray Star Stagger",
        TrKey::SliderBoltWidth => "Ray Bolt Width",
        TrKey::SliderTrailDuration => "Ray Trail Duration",
        TrKey::SliderShardMinSecs => "Shard Min Secs",
        TrKey::SliderShardMaxSecs => "Shard Max Secs",
        TrKey::SliderShardBaseSize => "Shard Base Size",
        TrKey::SliderShardCurve => "Shard Curve",
        TrKey::SliderShardHdrBoost => "Shard HDR Boost",
        TrKey::SliderShardHold => "Shard Hold",
        TrKey::SliderVolume => "Volume",
        // HUD
        TrKey::Moves => "moves",
        TrKey::Lives => "lives",
        TrKey::Shop => "SHOP",
        TrKey::Cores => "cores",
        // Shop item labels
        TrKey::ShopSwap => "Move",
        TrKey::ShopEliminate => "Eliminate",
        TrKey::ShopUpgrade => "Upgrade",
        TrKey::ShopLife => "+1 Life",
        TrKey::BoonRedValue => "Red+",
        TrKey::BoonGreenReserve => "Green+",
        TrKey::BoonBlueMoves => "Blue+",
        TrKey::BoonStarBounty => "Star×",
        TrKey::BoonPowerBounty => "Power+",
        TrKey::BoonHollowWard => "Hollow-",
        TrKey::BoonRedSpawn => "Red Freq.",
        TrKey::BoonGreenSpawn => "Green Freq.",
        TrKey::BoonBlueSpawn => "Blue Freq.",
        TrKey::BoonYellowSpawn => "Yellow Freq.",
        TrKey::BoonPurpleSpawn => "Purple Freq.",
        // Shop status lines
        TrKey::ShopSwapStatus => "Drag and teleport",
        TrKey::ShopEliminateStatus => "Break 1 light",
        TrKey::ShopUpgradeStatus => "Raise 1 tier",
        TrKey::ShopLifeStatus => "Buy 1 extra life",
        TrKey::BoonRedValueStatus => "Reds give score",
        TrKey::BoonGreenReserveStatus => "Greens give reserve",
        TrKey::BoonBlueMovesStatus => "Blues return moves",
        TrKey::BoonStarBountyStatus => "Stars yield extra cores",
        TrKey::BoonPowerBountyStatus => "Powers give score",
        TrKey::BoonHollowWardStatus => "Fewer hollows",
        TrKey::BoonRedSpawnStatus => "+ red lights",
        TrKey::BoonGreenSpawnStatus => "+ green lights",
        TrKey::BoonBlueSpawnStatus => "+ blue lights",
        TrKey::BoonYellowSpawnStatus => "+ yellow lights",
        TrKey::BoonPurpleSpawnStatus => "+ purple lights",
        // Armed badge
        TrKey::ShopModifiers => "SPECIAL MOVES",
        TrKey::ArmedSwap => "Drag a light",
        TrKey::ArmedSwap1of2 => "Drop onto another light",
        TrKey::ArmedEliminate => "Eliminate active",
        TrKey::ArmedUpgrade => "Upgrade active",
        TrKey::ActiveReady => "Active: ready",
        TrKey::ActiveChooseTarget => "Active: pick target",
        // Shop state
        TrKey::NotEnoughCores => "Not enough cores",
        // Goal hints
        TrKey::GoalTitle => "Level objective",
        TrKey::GoalFreePlay => "Free play",
        TrKey::GoalReachTarget => "Reach target",
        TrKey::GoalRescueSparks => "Rescue sparks",
        TrKey::GoalClearShadows => "Clear shadows",
        TrKey::GoalScoreOnClock => "Score before time",
        TrKey::GoalCollectColor => "Collect this color",
        TrKey::GoalColorOnClock => "Color before time",
        // Stats
        TrKey::StatsTitle => "--- DETAILS ---",
        TrKey::StatsRed => "Red",
        TrKey::StatsGreen => "Green",
        TrKey::StatsBlue => "Blue",
        TrKey::StatsYellow => "Yellow",
        TrKey::StatsPurple => "Purple",
        TrKey::StatsSpecials => "Specials",
        TrKey::StatsMaxCombo => "Max Combo",
        TrKey::StatsChains => "Chains",
        // Tutorial controls
        TrKey::TutorialClose => "Got it!",
        // Tutorial titles
        TrKey::TutorialScoreTitle => "HOW TO PLAY",
        TrKey::TutorialSparksTitle => "TUTORIAL: COLLECT SPARKS",
        TrKey::TutorialShadowTitle => "TUTORIAL: CLEAR SHADOWS",
        TrKey::TutorialTimedScoreTitle => "TUTORIAL: TIMED SCORE",
        TrKey::TutorialCollectColorTitle => "TUTORIAL: COLLECT COLOR",
        TrKey::TutorialTimedColorTitle => "TUTORIAL: TIMED COLOR",
        // Tooltip titles
        TrKey::TooltipSwapTitle => "Ability: Move",
        TrKey::TooltipEliminateTitle => "Ability: Eliminate",
        TrKey::TooltipUpgradeTitle => "Ability: Upgrade",
        TrKey::TooltipLifeTitle => "Item: +1 Life",
        TrKey::TooltipMovesTitle => "Status: Moves",
        TrKey::TooltipCoresTitle => "Reserve: Cores",
        TrKey::TooltipBoonRedTitle => "Boon: Red+",
        TrKey::TooltipBoonGreenTitle => "Boon: Green+",
        TrKey::TooltipBoonBlueTitle => "Boon: Blue+",
        TrKey::TooltipBoonStarTitle => "Boon: Star×",
        TrKey::TooltipBoonPowerTitle => "Boon: Power+",
        TrKey::TooltipBoonHollowTitle => "Boon: Hollow-",
        TrKey::TooltipBoonRedSpawnTitle => "Boon: Red Spawn+",
        TrKey::TooltipBoonGreenSpawnTitle => "Boon: Green Spawn+",
        TrKey::TooltipBoonBlueSpawnTitle => "Boon: Blue Spawn+",
        TrKey::TooltipBoonYellowSpawnTitle => "Boon: Yellow Spawn+",
        TrKey::TooltipBoonPurpleSpawnTitle => "Boon: Purple Spawn+",
        // Tooltip descriptions
        TrKey::TooltipSwapDesc => {
            "Drag one light onto another to teleport them and exchange their positions."
        }
        TrKey::TooltipEliminateDesc => "Destroy a selected light, triggering new cascades.",
        TrKey::TooltipUpgradeDesc => {
            "Upgrade the tier of the selected light (e.g. normal to Lightning)."
        }
        TrKey::TooltipLifeDesc => "Grants a reserve life to retry if you fail a level.",
        TrKey::TooltipMovesDesc => "Moves remaining to complete the level.",
        TrKey::TooltipCoresDesc => {
            "Cores reserve. Use them to purchase special moves and extra lives."
        }
        TrKey::TooltipBoonRedDesc => "Collected red lights grant +25% score.",
        TrKey::TooltipBoonGreenDesc => "Collected green lights give cores for the shop.",
        TrKey::TooltipBoonBlueDesc => "Every 7 blue cores collected return +1 extra move.",
        TrKey::TooltipBoonStarDesc => "Breaking a Starburst grants double lightcores per rank.",
        TrKey::TooltipBoonPowerDesc => "Creating special lights grants +18 points per rank.",
        TrKey::TooltipBoonHollowDesc => "Reduces the chance of Hollows spawning on the board.",
        TrKey::TooltipBoonRedSpawnDesc => {
            "Increases the probability of Red lights spawning on the board."
        }
        TrKey::TooltipBoonGreenSpawnDesc => {
            "Increases the probability of Green lights spawning on the board."
        }
        TrKey::TooltipBoonBlueSpawnDesc => {
            "Increases the probability of Blue lights spawning on the board."
        }
        TrKey::TooltipBoonYellowSpawnDesc => {
            "Increases the probability of Yellow lights spawning on the board."
        }
        TrKey::TooltipBoonPurpleSpawnDesc => {
            "Increases the probability of Purple lights spawning on the board."
        }
        // Level menu
        TrKey::Play => "Play",
        TrKey::Restart => "Restart Run",
        // Color names
        TrKey::ColorRed => "red",
        TrKey::ColorGreen => "green",
        TrKey::ColorBlue => "blue",
        TrKey::ColorYellow => "yellow",
        TrKey::ColorPurple => "purple",

        // Level Complete / End screen
        TrKey::LevelCompleteTitle => "Level Completed!",
        TrKey::BoardConsumedTitle => "Board Consumed!",
        TrKey::LightcoresCaptured => "Lightcores captured: {}",
        TrKey::LevelUnlocked => "Level {:02} unlocked",
        TrKey::NewHighScore => "New high score recorded",
        TrKey::LevelAlreadyCompleted => "Level already completed",
        TrKey::MatchSummary => "Match summary:",
        TrKey::ChooseOneModifier => "AVAILABLE BOONS · buy only after completing a stage",
        TrKey::BoonContinueInstruction => "[Click/Tap or Space] to continue",
        TrKey::BoonPurchased => "Purchased",
        TrKey::StatsSparks => "Sparks",
    }
}

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
    // Tabs + option labels for the tabbed options screen.
    TabAudio,
    TabGraphics,
    TabInterface,
    ShowFps,
    Tutorial,
    AdvancedSection,
    FpsUnlimited,
    InternalResolution,
    ResNative,
    ResHigh,
    ResMedium,
    ResLow,

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
        TrKey::TabAudio => "Audio",
        TrKey::TabGraphics => "Gráficos",
        TrKey::TabInterface => "Interfaz",
        TrKey::ShowFps => "Mostrar FPS",
        TrKey::Tutorial => "Tutorial",
        TrKey::AdvancedSection => "Avanzado",
        TrKey::FpsUnlimited => "Sin límite",
        TrKey::InternalResolution => "Resolución interna",
        TrKey::ResNative => "Nativa",
        TrKey::ResHigh => "Alta",
        TrKey::ResMedium => "Media",
        TrKey::ResLow => "Baja",

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
        TrKey::ShopUpgrade => "Subir nivel",
        TrKey::ShopLife => "+1 Vida",
        TrKey::BoonRedValue => "Rojo+",
        TrKey::BoonGreenReserve => "Verde+",
        TrKey::BoonBlueMoves => "Azul+",
        TrKey::BoonStarBounty => "Estrella×",
        TrKey::BoonPowerBounty => "Especial+",
        TrKey::BoonHollowWard => "Hollow-",
        TrKey::BoonRedSpawn => "Más rojas",
        TrKey::BoonGreenSpawn => "Más verdes",
        TrKey::BoonBlueSpawn => "Más azules",
        TrKey::BoonYellowSpawn => "Más amarillas",
        TrKey::BoonPurpleSpawn => "Más moradas",
        // Shop status lines
        TrKey::ShopSwapStatus => "Arrastra y mueve",
        TrKey::ShopEliminateStatus => "Rompe 1 luz",
        TrKey::ShopUpgradeStatus => "Sube 1 nivel",
        TrKey::ShopLifeStatus => "Compra 1 vida extra",
        TrKey::BoonRedValueStatus => "Rojas dan puntos",
        TrKey::BoonGreenReserveStatus => "Verdes dan reserva",
        TrKey::BoonBlueMovesStatus => "Azules dan movimientos",
        TrKey::BoonStarBountyStatus => "Estrellas dan doble",
        TrKey::BoonPowerBountyStatus => "Especiales dan puntos",
        TrKey::BoonHollowWardStatus => "Menos Hollows",
        TrKey::BoonRedSpawnStatus => "+ luces rojas",
        TrKey::BoonGreenSpawnStatus => "+ luces verdes",
        TrKey::BoonBlueSpawnStatus => "+ luces azules",
        TrKey::BoonYellowSpawnStatus => "+ luces amarillas",
        TrKey::BoonPurpleSpawnStatus => "+ luces moradas",
        // Armed badge
        TrKey::ShopModifiers => "MOVIMIENTOS ESPECIALES",
        TrKey::ArmedSwap => "Arrastra una luz",
        TrKey::ArmedSwap1of2 => "Suelta sobre otra luz",
        TrKey::ArmedEliminate => "Eliminar activo",
        TrKey::ArmedUpgrade => "Subir nivel activo",
        TrKey::ActiveReady => "Activo: listo",
        TrKey::ActiveChooseTarget => "Activo: elige destino",
        // Shop state
        TrKey::NotEnoughCores => "Falta reserva",
        // Goal hints
        TrKey::GoalTitle => "Objetivo del nivel",
        TrKey::GoalFreePlay => "Junta las que quieras",
        TrKey::GoalReachTarget => "Consigue los puntos",
        TrKey::GoalRescueSparks => "Rescata chispas",
        TrKey::GoalClearShadows => "Limpia sombras",
        TrKey::GoalScoreOnClock => "Puntos antes de tiempo",
        TrKey::GoalCollectColor => "Junta este color",
        TrKey::GoalColorOnClock => "Color antes de tiempo",
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
        TrKey::TooltipUpgradeTitle => "Habilidad: Subir nivel",
        TrKey::TooltipLifeTitle => "Artículo: +1 Vida",
        TrKey::TooltipMovesTitle => "Estado: Movimientos",
        TrKey::TooltipCoresTitle => "Tu reserva",
        TrKey::TooltipBoonRedTitle => "Mejora: Rojas+",
        TrKey::TooltipBoonGreenTitle => "Mejora: Verdes+",
        TrKey::TooltipBoonBlueTitle => "Mejora: Azul+",
        TrKey::TooltipBoonStarTitle => "Mejora: Estrella×",
        TrKey::TooltipBoonPowerTitle => "Mejora: Especial+",
        TrKey::TooltipBoonHollowTitle => "Mejora: Hollow-",
        TrKey::TooltipBoonRedSpawnTitle => "Mejora: Más rojas",
        TrKey::TooltipBoonGreenSpawnTitle => "Mejora: Más verdes",
        TrKey::TooltipBoonBlueSpawnTitle => "Mejora: Más azules",
        TrKey::TooltipBoonYellowSpawnTitle => "Mejora: Más amarillas",
        TrKey::TooltipBoonPurpleSpawnTitle => "Mejora: Más moradas",
        // Tooltip descriptions
        TrKey::TooltipSwapDesc => {
            "Arrastra una luz sobre otra para cambiarlas de lugar."
        }
        TrKey::TooltipEliminateDesc => "Rompe la luz que toques.",
        TrKey::TooltipUpgradeDesc => "Convierte una luz normal en una especial.",
        TrKey::TooltipLifeDesc => "Te da una vida extra para reintentar si pierdes.",
        TrKey::TooltipMovesDesc => "Movimientos que te quedan.",
        TrKey::TooltipCoresDesc => {
            "Tu reserva. Gástala en la tienda en habilidades y vidas."
        }
        TrKey::TooltipBoonRedDesc => "Las luces rojas dan +25% de puntos.",
        TrKey::TooltipBoonGreenDesc => "Las luces verdes te dan reserva para la tienda.",
        TrKey::TooltipBoonBlueDesc => "Cada 7 luces azules te dan 1 movimiento.",
        TrKey::TooltipBoonStarDesc => "Romper una estrella da el doble de luces.",
        TrKey::TooltipBoonPowerDesc => "Crear luces especiales da +18 puntos.",
        TrKey::TooltipBoonHollowDesc => "Aparecen menos Hollows.",
        TrKey::TooltipBoonRedSpawnDesc => "Aparecen más luces rojas.",
        TrKey::TooltipBoonGreenSpawnDesc => "Aparecen más luces verdes.",
        TrKey::TooltipBoonBlueSpawnDesc => "Aparecen más luces azules.",
        TrKey::TooltipBoonYellowSpawnDesc => "Aparecen más luces amarillas.",
        TrKey::TooltipBoonPurpleSpawnDesc => "Aparecen más luces moradas.",
        // Level menu
        TrKey::Play => "Jugar",
        TrKey::Restart => "Reiniciar Run",
        // Color names
        TrKey::ColorRed => "rojas",
        TrKey::ColorGreen => "verdes",
        TrKey::ColorBlue => "azules",
        TrKey::ColorYellow => "amarillas",
        TrKey::ColorPurple => "moradas",

        // Level Complete / End screen
        TrKey::LevelCompleteTitle => "Nivel Completado!",
        TrKey::BoardConsumedTitle => "Tablero Consumido!",
        TrKey::LightcoresCaptured => "Luces capturadas: {}",
        TrKey::LevelUnlocked => "Nivel {:02} desbloqueado",
        TrKey::NewHighScore => "¡Nuevo récord!",
        TrKey::LevelAlreadyCompleted => "Nivel ya completado",
        TrKey::MatchSummary => "Resumen de partida:",
        TrKey::ChooseOneModifier => "MEJORAS · compra solo al terminar una etapa",
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
        TrKey::TabAudio => "Audio",
        TrKey::TabGraphics => "Graphics",
        TrKey::TabInterface => "Interface",
        TrKey::ShowFps => "Show FPS",
        TrKey::Tutorial => "Tutorial",
        TrKey::AdvancedSection => "Advanced",
        TrKey::FpsUnlimited => "Unlimited",
        TrKey::InternalResolution => "Internal resolution",
        TrKey::ResNative => "Native",
        TrKey::ResHigh => "High",
        TrKey::ResMedium => "Medium",
        TrKey::ResLow => "Low",

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
        TrKey::BoonPowerBounty => "Special+",
        TrKey::BoonHollowWard => "Hollow-",
        TrKey::BoonRedSpawn => "More reds",
        TrKey::BoonGreenSpawn => "More greens",
        TrKey::BoonBlueSpawn => "More blues",
        TrKey::BoonYellowSpawn => "More yellows",
        TrKey::BoonPurpleSpawn => "More purples",
        // Shop status lines
        TrKey::ShopSwapStatus => "Drag and move",
        TrKey::ShopEliminateStatus => "Break 1 light",
        TrKey::ShopUpgradeStatus => "Level up 1",
        TrKey::ShopLifeStatus => "Buy 1 extra life",
        TrKey::BoonRedValueStatus => "Reds give points",
        TrKey::BoonGreenReserveStatus => "Greens give reserve",
        TrKey::BoonBlueMovesStatus => "Blues give moves",
        TrKey::BoonStarBountyStatus => "Stars give double",
        TrKey::BoonPowerBountyStatus => "Specials give points",
        TrKey::BoonHollowWardStatus => "Fewer Hollows",
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
        TrKey::NotEnoughCores => "Not enough reserve",
        // Goal hints
        TrKey::GoalTitle => "Level objective",
        TrKey::GoalFreePlay => "Grab any you like",
        TrKey::GoalReachTarget => "Reach the points",
        TrKey::GoalRescueSparks => "Rescue sparks",
        TrKey::GoalClearShadows => "Clear shadows",
        TrKey::GoalScoreOnClock => "Points before time",
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
        TrKey::TooltipCoresTitle => "Your reserve",
        TrKey::TooltipBoonRedTitle => "Upgrade: Red+",
        TrKey::TooltipBoonGreenTitle => "Upgrade: Green+",
        TrKey::TooltipBoonBlueTitle => "Upgrade: Blue+",
        TrKey::TooltipBoonStarTitle => "Upgrade: Star×",
        TrKey::TooltipBoonPowerTitle => "Upgrade: Special+",
        TrKey::TooltipBoonHollowTitle => "Upgrade: Hollow-",
        TrKey::TooltipBoonRedSpawnTitle => "Upgrade: More reds",
        TrKey::TooltipBoonGreenSpawnTitle => "Upgrade: More greens",
        TrKey::TooltipBoonBlueSpawnTitle => "Upgrade: More blues",
        TrKey::TooltipBoonYellowSpawnTitle => "Upgrade: More yellows",
        TrKey::TooltipBoonPurpleSpawnTitle => "Upgrade: More purples",
        // Tooltip descriptions
        TrKey::TooltipSwapDesc => "Drag one light onto another to swap them.",
        TrKey::TooltipEliminateDesc => "Break the light you tap.",
        TrKey::TooltipUpgradeDesc => "Turns a normal light into a special one.",
        TrKey::TooltipLifeDesc => "Gives an extra life to retry if you lose.",
        TrKey::TooltipMovesDesc => "Moves you have left.",
        TrKey::TooltipCoresDesc => {
            "Your reserve. Spend it in the shop on abilities and lives."
        }
        TrKey::TooltipBoonRedDesc => "Red lights give +25% points.",
        TrKey::TooltipBoonGreenDesc => "Green lights give reserve for the shop.",
        TrKey::TooltipBoonBlueDesc => "Every 7 blue lights give 1 move.",
        TrKey::TooltipBoonStarDesc => "Breaking a star gives double lights.",
        TrKey::TooltipBoonPowerDesc => "Making special lights gives +18 points.",
        TrKey::TooltipBoonHollowDesc => "Fewer Hollows appear.",
        TrKey::TooltipBoonRedSpawnDesc => "More red lights appear.",
        TrKey::TooltipBoonGreenSpawnDesc => "More green lights appear.",
        TrKey::TooltipBoonBlueSpawnDesc => "More blue lights appear.",
        TrKey::TooltipBoonYellowSpawnDesc => "More yellow lights appear.",
        TrKey::TooltipBoonPurpleSpawnDesc => "More purple lights appear.",
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
        TrKey::LightcoresCaptured => "Lights captured: {}",
        TrKey::LevelUnlocked => "Level {:02} unlocked",
        TrKey::NewHighScore => "New record!",
        TrKey::LevelAlreadyCompleted => "Level already completed",
        TrKey::MatchSummary => "Match summary:",
        TrKey::ChooseOneModifier => "UPGRADES · buy only after finishing a stage",
        TrKey::BoonContinueInstruction => "[Click/Tap or Space] to continue",
        TrKey::BoonPurchased => "Purchased",
        TrKey::StatsSparks => "Sparks",
    }
}

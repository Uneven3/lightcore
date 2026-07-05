use bevy::diagnostic::{
    DiagnosticPath, DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
    SystemInformationDiagnosticsPlugin,
};
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};

use crate::state::GameState;
use crate::visuals::particles::Particle;

/// Refresh the readout a few times a second — fast enough to be live, slow enough that the overlay's
/// own text relayout doesn't muddy the very numbers we're trying to measure.
const REFRESH_HZ: f32 = 5.0;

/// On-screen performance overlay: FPS / frame time, entity & draw-proxy counts, live asset counts
/// (the heart of our batching problem — how many distinct `Mesh`/`ColorMaterial` exist), particle
/// load, and process CPU/RAM. Toggle with F3. Measure first, optimize second.
pub(crate) struct DebugOverlayPlugin;

#[derive(Component)]
struct DebugText;

#[derive(Resource)]
struct RefreshTimer(Timer);

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
        ));
        // OJO: el sysinfo (CPU/mem del overlay) refresca leyendo /proc en el hilo principal y provoca
        // hitches periódicos de ~50 ms (medido: tira el suelo de 41 fps a 17 fps). Por eso es opt-in:
        // solo se carga con MATCH3_SYSINFO=1 cuando de verdad quieras ver CPU/mem. En uso normal NO va.
        if std::env::var("MATCH3_SYSINFO").is_ok() {
            app.add_plugins(SystemInformationDiagnosticsPlugin);
        }
        // Diagnóstico de rendimiento: con MATCH3_LOGFPS=1 vuelca por terminal, cada segundo, un
        // resumen RICO (FPS medio/mín, frametime medio/peor, el GameState del peor frame y los PICOS
        // de conteos: entidades/sprites/mesh2d/partículas + assets material/mesh/imagen). Pensado para
        // jugar 30 s y leer el log: distingue FUGA de entidades (sube monótono) de PICO transitorio
        // (un mal frame puntual) de coste SOSTENIDO. NEUTRAL: no asume un culpable, deja que los
        // números decidan. No se activa en uso normal.
        if std::env::var("MATCH3_LOGFPS").is_ok() {
            app.insert_resource(PerfLog::default())
                .add_systems(Update, perf_log);
        }
        app.insert_resource(RefreshTimer(Timer::from_seconds(
            1.0 / REFRESH_HZ,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, setup_overlay)
        .add_systems(Update, (toggle_overlay, toggle_vsync, update_overlay));
    }
}

/// F4 alterna el vsync (AutoVsync ↔ AutoNoVsync). Es el test decisivo de rendimiento: si al quitar
/// el vsync los FPS se disparan, el cuello de botella era la presentación (vsync/compositor/ruta
/// PRIME entre GPUs), no el coste real de render. Si se quedan igual, el coste es GPU de verdad.
fn toggle_vsync(keys: Res<ButtonInput<KeyCode>>, window: Single<&mut Window, With<PrimaryWindow>>) {
    if keys.just_pressed(KeyCode::F4) {
        let mut window = window.into_inner();
        window.present_mode = match window.present_mode {
            PresentMode::AutoNoVsync => PresentMode::AutoVsync,
            _ => PresentMode::AutoNoVsync,
        };
    }
}

fn setup_overlay(mut commands: Commands) {
    commands.spawn((
        DebugText,
        Visibility::Hidden,
        Text::new("perf: …"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgb(0.4, 1.0, 0.5)), // bright green, readable over the dark board
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
    ));
}

fn toggle_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut vis: Single<&mut Visibility, With<DebugText>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        **vis = match **vis {
            Visibility::Hidden => Visibility::Visible,
            _ => Visibility::Hidden,
        };
    }
}

#[expect(clippy::too_many_arguments)]
fn update_overlay(
    time: Res<Time>,
    mut refresh: ResMut<RefreshTimer>,
    diagnostics: Res<DiagnosticsStore>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<ColorMaterial>>,
    mesh2d: Query<(), With<Mesh2d>>,
    particles: Query<(), With<Particle>>,
    window: Single<&Window, With<PrimaryWindow>>,
    text: Single<(&mut Text, &Visibility), With<DebugText>>,
) {
    let (mut text, visibility) = text.into_inner();
    if *visibility == Visibility::Hidden {
        return;
    }
    if !refresh.0.tick(time.delta()).just_finished() {
        return;
    }

    let read = |path: DiagnosticPath| {
        diagnostics
            .get(&path)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0)
    };
    let fps = read(FrameTimeDiagnosticsPlugin::FPS);
    let frame_ms = read(FrameTimeDiagnosticsPlugin::FRAME_TIME);
    let entities = read(EntityCountDiagnosticsPlugin::ENTITY_COUNT);

    // cpu/mem solo existen si SystemInformationDiagnosticsPlugin está cargado (opt-in, MATCH3_SYSINFO=1,
    // porque su refresco de /proc causa hitches). Si no está, lo mostramos como "off" en vez de un 0 engañoso.
    let proc = match diagnostics.get(&SystemInformationDiagnosticsPlugin::PROCESS_CPU_USAGE) {
        Some(_) => {
            let cpu = read(SystemInformationDiagnosticsPlugin::PROCESS_CPU_USAGE);
            let mem_mb = read(SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE) * 1024.0; // GiB → MiB
            format!("cpu {cpu:4.1}%  mem {mem_mb:5.0} MB")
        }
        None => "cpu/mem off (MATCH3_SYSINFO=1)".to_string(),
    };

    // Resolución FÍSICA de la VENTANA; el "x{sf}" es el factor HiDPI.
    let (pw, ph) = (window.physical_width(), window.physical_height());
    let sf = window.resolution.scale_factor();

    let vsync = match window.present_mode {
        PresentMode::AutoNoVsync | PresentMode::Immediate => "OFF",
        PresentMode::Mailbox => "mailbox",
        _ => "on",
    };

    **text = format!(
        "FPS {fps:5.1}  ({frame_ms:5.2} ms)\n\
         ventana {pw}x{ph}  (x{sf:.2})\n\
         entities {entities:5.0}   mesh2d {mesh2d:4}\n\
         assets: mesh {amesh:4}  material {amat:4}\n\
         particles {parts:4}\n\
         proc: {proc}\n\
         present {vsync}   [F3] overlay  [F4] vsync",
        mesh2d = mesh2d.count(),
        amesh = meshes.len(),
        amat = materials.len(),
        parts = particles.count(),
    );
}

/// Estado acumulado del volcado por consola (`MATCH3_LOGFPS=1`). Acumula una ventana de 1 s de
/// frametimes y los PICOS de conteos vistos en ella, para imprimir una línea por segundo.
#[derive(Resource)]
struct PerfLog {
    window: Timer,
    frames: u32,
    ms_accum: f32,
    ms_max: f32,
    /// Frames de la ventana que pasaron de SPIKE_MS — distingue UN pico periódico (spikes≈1) de
    /// jitter sostenido (spikes alto). Decisivo para saber si "los fps bailan" es un stall puntual.
    spikes: u32,
    /// GameState en el peor frame de la ventana — para correlacionar el pico con la fase de juego.
    worst_state: Option<GameState>,
    ent_max: usize,
    sprite_max: usize,
    mesh2d_max: usize,
    part_max: usize,
    mat_max: usize,
}

impl Default for PerfLog {
    fn default() -> Self {
        Self {
            window: Timer::from_seconds(1.0, TimerMode::Repeating),
            frames: 0,
            ms_accum: 0.0,
            ms_max: 0.0,
            spikes: 0,
            worst_state: None,
            ent_max: 0,
            sprite_max: 0,
            mesh2d_max: 0,
            part_max: 0,
            mat_max: 0,
        }
    }
}

/// Volcado RICO por terminal: una línea por segundo con FPS medio/mín, frametime medio/peor, el
/// GameState del peor frame y los picos de conteos. Solo activo con `MATCH3_LOGFPS=1`.
#[expect(clippy::too_many_arguments)]
fn perf_log(
    time: Res<Time>,
    mut log: ResMut<PerfLog>,
    state: Res<State<GameState>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<ColorMaterial>>,
    images: Res<Assets<Image>>,
    entities: Query<()>,
    sprites: Query<(), With<Sprite>>,
    mesh2d: Query<(), With<Mesh2d>>,
    particles: Query<(), With<Particle>>,
) {
    /// Umbral de "frame malo": > ~33 ms es perder por debajo de 30 fps en ese frame, lo que el ojo
    /// percibe como tirón.
    const SPIKE_MS: f32 = 33.0;

    let dt_ms = time.delta_secs() * 1000.0;
    log.frames += 1;
    log.ms_accum += dt_ms;
    if dt_ms > SPIKE_MS {
        log.spikes += 1;
    }
    if dt_ms > log.ms_max {
        log.ms_max = dt_ms;
        log.worst_state = Some(state.get().clone());
    }
    // Picos de conteos dentro de la ventana (no el valor del último frame): así un burst breve no se
    // nos escapa entre dos muestreos.
    log.ent_max = log.ent_max.max(entities.count());
    log.sprite_max = log.sprite_max.max(sprites.count());
    log.mesh2d_max = log.mesh2d_max.max(mesh2d.count());
    log.part_max = log.part_max.max(particles.count());
    log.mat_max = log.mat_max.max(materials.len());

    if !log.window.tick(time.delta()).just_finished() {
        return;
    }

    let avg_ms = log.ms_accum / log.frames.max(1) as f32;
    let avg_fps = 1000.0 / avg_ms.max(0.0001);
    let min_fps = 1000.0 / log.ms_max.max(0.0001);
    info!(
        "PERF fps~{avg_fps:3.0} (min {min_fps:3.0})  ms~{avg_ms:4.1} peor {peor:4.1} spikes {spikes:2}/{frames} @{worst:?}  \
         ent {ent:4}  spr {spr:4}  mesh2d {m2d:4}  part {part:4}  | assets mat {mat:4} mesh {mesh:3} img {img:3}",
        peor = log.ms_max,
        spikes = log.spikes,
        frames = log.frames,
        worst = log.worst_state,
        ent = log.ent_max,
        spr = log.sprite_max,
        m2d = log.mesh2d_max,
        part = log.part_max,
        mat = log.mat_max,
        mesh = meshes.len(),
        img = images.len(),
    );

    log.frames = 0;
    log.ms_accum = 0.0;
    log.ms_max = 0.0;
    log.spikes = 0;
    log.worst_state = None;
    log.ent_max = 0;
    log.sprite_max = 0;
    log.mesh2d_max = 0;
    log.part_max = 0;
    log.mat_max = 0;
}

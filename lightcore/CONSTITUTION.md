# Constitution: cómo se organiza este proyecto

Este documento explica el *por qué* detrás de la estructura del código, no el *qué* (eso lo explica el código mismo). Sirve como referencia para decisiones futuras: cuando agregues una feature nueva y no sepas dónde ponerla, volvé a este documento antes de improvisar.

Para el *estado* de features concretas (qué funciona hoy vs. qué falta pulir, con su historial de intentos/decisiones), ver `ROADMAP.md`.

## 1. El patrón Plugin

Bevy organiza absolutamente todo — desde el renderer hasta tu propio juego — como **Plugins**: unidades que registran systems, resources y events en el `App`. La consecuencia práctica para este proyecto:

- `main.rs` queda **mínimo**: `App::new().add_plugins((DefaultPlugins, GamePlugin)).run()`. Nada de lógica de juego ahí.
- La lógica real vive en `lib.rs` + módulos. Esto no es solo prolijidad: separar el bootstrap de la lógica permite, más adelante, escribir tests que levanten un `App` con `MinimalPlugins` (sin renderer, sin ventana) en vez de `DefaultPlugins` — algo que sería imposible si todo viviera amontonado en `main()`.
- Cada feature cohesiva (audio, UI, el pipeline de juego, los efectos visuales) es su propio `struct XPlugin` con `impl Plugin for XPlugin { fn build(&self, app: &mut App) { ... } }`. `GamePlugin` no contiene lógica propia — solo compone los Plugins de feature: `app.add_plugins((AudioPlugin, GameplayPlugin, VisualsPlugin, UiPlugin))`.

## 2. No todo necesita ser un Plugin

Un Plugin sirve para una sola cosa: **registrar algo en el `App`** (`add_systems`, `add_observer`, `add_event`, `init_resource`, `insert_resource`). Si un módulo no hace nada de eso —porque es lógica pura sobre datos planos, sin `Query`/`Res`/`Commands`— no necesita ser un Plugin, es un módulo común de Rust.

Ejemplo real de este proyecto: `core::matching::scan_runs` no toca el `App` para nada — recibe un `HashMap` y devuelve qué entidades deberían eliminarse. Es una función pura, vive en un módulo plano (`core/matching.rs`), y la llaman los systems que sí están registrados (`gameplay::swap::on_swap_happened`, `gameplay::chain::check_chain_matches`).

**Regla práctica:** si la pregunta es "¿esto registra algo en el `App`?" y la respuesta es no, es un módulo plano, no un Plugin.

## 3. Events para desacoplar — con cautela

Cuando dos partes del juego necesitan comunicarse sin conocerse directamente, se usa un Event + Observer en vez de una llamada de función directa. Ejemplo real: cuando se resuelve un match o una power light se activa, el código de `gameplay` dispara `commands.trigger(ChainPop { removed })` / `commands.trigger(PowerConsumed { kind, pos, color })` / `commands.trigger(PowerCombo { kind, .. })` — y `gameplay` nunca se entera de que del otro lado hay un flash, un beam viajando o un combo entero animándose. Eso vive en `visuals` (`effects.rs`, `light_trail.rs`), suscripto vía `add_observer`.

Esto es deliberado: permite que `gameplay` (las reglas del juego) y `visuals` (cómo se ve/siente) crezcan en direcciones distintas sin pisarse. Cuando agregues animaciones, luces o VFX nuevos, en general solo tocarás `visuals` — no `gameplay`.

**Pero esto no es gratis.** No se usa Event+Observer para cualquier comunicación entre sistemas — solo cuando hay una razón real para desacoplar (como evitar que un módulo dependa del otro, o esquivar el límite de 16 parámetros de Bevy). Para una mutación local simple (ej. `visuals::bounce::tick_land_bounce` escribiendo directamente su propio componente `LandBounce`, o `gameplay::vfx::tick_pending_light_transform` escribiendo `LightKind` en el lugar), una asignación directa alcanza. Reservá Events para fronteras reales entre Plugins, no para todo.

## 4. SOLID / DRY / KISS traducidos a ECS

| Principio | Cómo se traduce acá |
|---|---|
| **S**ingle Responsibility | Un Plugin = una preocupación (`AudioPlugin` no sabe de cámaras; `GameplayPlugin` no sabe de meshes). Un system = una cosa puntual. |
| **O**pen/Closed | Agregar una feature nueva = agregar un Plugin nuevo a la lista de `GamePlugin`, sin tocar los Plugins existentes. Así se diseñó `visuals/` separado de `gameplay/`: el espacio para crecer (animación, luces, VFX) ya existe sin tocar las reglas del juego. |
| **I**nterface Segregation | Cuando un system necesitaría demasiados parámetros, se agrupan en un `#[derive(SystemParam)]` (`PowerComboParams`, `ResetParams`) en vez de seguir sumando parámetros sueltos. |
| **D**ependency Inversion | Los systems no se llaman entre sí directamente — se comunican vía Events/Resources compartidos (ver sección 3). |
| **L**iskov | No traduce bien — no hay herencia de clases en ECS. No forzarlo. |
| **D**on't **R**epeat Yourself | Con cautela: dos cosas que se ven parecidas en ECS (dos Events similares, dos systems con queries parecidas) a veces son intencionalmente explícitas para que se entienda qué dispara qué. No fusionar solo por las dudas. |
| **K**eep **I**t **S**imple | Usar la herramienta más simple que resuelva el problema real que tenés ahora — no la que podrías necesitar después. Un Event+Observer para una mutación local simple es sobre-ingeniería. |

## 5. Ports-and-adapters vs. ECS

La motivación de fondo de ports-and-adapters (hexagonal) — mantener la lógica central ignorante de los detalles volátiles (audio, render, UI) — es real y aplica acá también. La diferencia es el mecanismo: en vez de definir traits/interfaces e inyectar implementaciones concretas, ECS logra el mismo desacople con **Events + Components + Resources como frontera**. `gameplay` dispara un Event sin saber quién (si alguien) lo escucha; `visuals` se suscribe sin que `gameplay` necesite saber que existe. Mismo objetivo (núcleo ignorante de lo volátil), mecanismo distinto (datos/eventos en vez de traits inyectados).

## 6. Convención de visibilidad

Este es un solo crate binario (no una librería publicada), así que no hay consumidores externos reales. Por eso:

- Todo es `pub(crate)` por defecto. `pub` (visible fuera del crate) se reserva únicamente para lo que `main.rs` mismo consume — en la práctica, solo `GamePlugin`.
- Un módulo `prelude` se justifica solo donde de verdad ahorra imports repetidos: `core::prelude` (porque `core/` lo usa literalmente todo lo demás). No se crean preludes para `gameplay`, `visuals`, `ui`, `audio`, `board` — cada uno lo consumen 1-2 módulos como mucho, así que un `use` explícito es más claro que un prelude.

## 7. Mapa de módulos

```
src/
  main.rs       // bootstrap: App::new().add_plugins((DefaultPlugins, GamePlugin)).run()
  lib.rs        // mod core; mod board; mod audio; mod state; mod gameplay; mod visuals; mod ui; ... + GamePlugin
  state.rs      // GameState — la máquina de estados central del juego
  core/         // lógica pura, cero parámetros de Bevy salvo donde el propio dato es un Resource/Component.
                //   NO es un Plugin.
    grid.rs       // GridPos, to_world, to_grid, GRID_W/GRID_H/TILE, RaySettings
    components.rs // Light, Selected, SpecialMarker, Spark, Shadow (marcadores cruzados por varios módulos)
    light.rs      // LightColor, LightKind
    level.rs      // LevelGoal, LevelConfig, make_level, MOVES
    matching.rs   // Grid, EntityInfo, MatchResult, scan_runs, resolve_swap_activation, find_valid_swap...
    campaign.rs   // CampaignProgress (desbloqueo de niveles) + su persistencia
    run.rs        // RunState/CoreReserve (progreso de un run) + su persistencia
    storage.rs    // load_save_file/write_save_file — backend nativo (fs) vs WASM (localStorage)
    locale.rs     // Language, TrKey — localización es→en
    easing.rs     // damped_squash y otras funciones de easing puras, sin dependencias
  board/        // helpers de spawn/generación de tablero. NO es un Plugin (sin systems propios).
  audio/        // AudioPlugin — síntesis de sonido, SoundAssets, reproducción, volumen.
  input/        // InputPlugin — capa de input agnóstica de dispositivo (teclado/gamepad/mouse → InputActions).
  platform/     // PlatformPlugin — detección de plataforma/perf tier, ícono de ventana en desktop.
  menu/         // MenuPlugin — MainMenu → LevelMenu (selector unificado de mapas) → Options/Pause,
                //   cada pantalla es su propio Plugin sub-agregado.
  gameplay/     // GameplayPlugin — TODO el pipeline swap→pop→fall→spawn→chain, como un solo Plugin
                //   (las fases comparten demasiados Resources entre sí para separarlas con beneficio real;
                //   el GameState ya es el seam que las ordena).
    swap.rs, chain.rs, falling.rs, spawning.rs, popping.rs  // las fases del pipeline
    vfx.rs        // traduce una resolución de `core::matching` a Events para `visuals` (PowerCombo, etc.)
    rewards.rs    // economía compartida (score/reserve/stats) entre el swap directo y las cascadas
    lifecycle.rs  // setup/reset/restart de partida, niveles de debug (`DEBUG_SCENARIOS`)
  visuals/      // VisualsPlugin — posición/animación visual, efectos de power lights.
                //   Separado de gameplay a propósito: es el lugar donde entran animación/luces/VFX futuros
                //   sin tocar las reglas del juego.
  ui/           // UiPlugin — HUD (score, moves, nivel, objetivo, volumen).
  debug/        // DebugOverlayPlugin — overlay de rendimiento (F3): FPS, entidades, assets, memoria.
  embedded/     // rutas de assets embebidos en el binario (ej. el logo de Bevy del watermark).
```

Cuando agregues algo nuevo, preguntate: ¿es lógica pura sin Bevy? → `core/` o un módulo plano nuevo. ¿Registra systems/events/resources? → un Plugin nuevo, agregado a la lista de `GamePlugin`. ¿Es sobre cómo se ve/siente el juego sin cambiar sus reglas? → probablemente `visuals/`.

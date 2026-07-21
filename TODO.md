# 📋 Lightcore — Lista de Tareas para Pulido del Juego (TODO)

Esta lista realiza un seguimiento del progreso para estabilizar, mejorar y pulir el juego con miras a su publicación y uso educativo.

---

## 📝 Estado de la sesión — 2026-07-19 (release build: Linux/Windows/wasm/Android para itch.io)

Contexto: primer intento de compilar los 4 targets en release para subir a itch.io, usando el
workspace común en `uneven/` (`build.build-dir` compartido en `.cargo/config.toml` del root para
no recompilar Bevy por proyecto, `target-dir = "target"` local en `lightcore/.cargo/config.toml`
para que los binarios finales queden en el proyecto).

- [x] **`run.sh` apuntaba a un binario que no existe**
  - `[[bin]] name = "lightcore_bin"` en `Cargo.toml`, pero `run.sh` hacía `exec ./lightcore`. El
    launcher de Linux fallaba al primer doble clic. Arreglado a `exec ./lightcore_bin`.
  - *Archivos:* `run.sh`.

- [x] **Binarios de release sin `strip` — 122MB (Linux) / 108MB (Windows)**
  - `[profile.release]` en el `Cargo.toml` del workspace (`uneven/Cargo.toml`, compartido por
    todos los juegos) no tenía `strip`. Agregado `strip = true` ahí — baja Linux a 83MB y Windows a
    68MB. También se subió `[profile.wasm-release].strip` de `"debuginfo"` a `true`.
  - **Regla para cualquier juego nuevo en este workspace:** setear `strip` en el profile de release
    antes del primer build real, no esperar a que se note el tamaño después de compilar Bevy entero
    (~14 min por plataforma).
  - *Archivos:* `../Cargo.toml` (root del workspace, fuera de este repo).

- [x] **`trunk build --release` no usa el profile `wasm-release`**
  - El `[profile.wasm-release]` (opt-level `"s"`, `strip`) está definido pero no hay `Trunk.toml`
    que lo seleccione. Trunk sí soporta pasarlo por flag: hay que compilar wasm con
    `trunk build --cargo-profile wasm-release -d dist` (no alcanza con `--release`, que usa el
    profile `release` a secas). Sin esto el `.wasm` sale sin el `opt-level="s"` pensado para web.

- [x] **`cargo apk build --release` fallaba con `Error: No such file or directory (os error 2)`**
  - Causa raíz (dos bugs distintos de `cargo-apk`/`ndk-build` 0.10.0, ninguno de nuestro código):
    1. `ndk-build::dylibs::get_libs_search_paths` asume que `build/` (outputs de build scripts)
       vive bajo `<target-dir>/<triple>/<profile>/build`. Con `build.build-dir` compartido, esa
       carpeta en realidad queda bajo el `build-dir` separado (`uneven/.cargo/shared-build/...`),
       así que el `read_dir()` de cargo-apk falla con ENOENT.
    2. `cargo_subcommand::Subcommand::target_dir()` (que usa `cargo-apk` internamente) **no lee
       `.cargo/config.toml`** — solo respeta la env var `CARGO_TARGET_DIR` o cae al default
       `<workspace_root>/target`. Como `lightcore/.cargo/config.toml` fija `target-dir = "target"`
       (relativo al proyecto), cargo-apk terminaba buscando el `.so` compilado en
       `uneven/target/aarch64-linux-android/release/` en vez de
       `uneven/lightcore/target/aarch64-linux-android/release/` (donde realmente quedó).
  - Diagnóstico: se instrumentó una copia local de `cargo-apk`/`ndk-build` (`cargo install --path
    ... --debug --root <scratch>`) con `eprintln!` en cada paso, porque el binario de crates.io no
    imprime contexto útil en el error (ni siquiera con `RUST_BACKTRACE=full`).
  - **Fix: invocar el build de Android con ambas env vars apuntando al target-dir local del
    proyecto**, para que coincida con lo que `cargo-apk` espera:
    ```sh
    cd lightcore
    CARGO_TARGET_DIR="$(pwd)/target" CARGO_BUILD_BUILD_DIR="$(pwd)/target" cargo apk build --release
    ```
    Nota: esto unifica target-dir y build-dir *solo para esa invocación* (pierde el cache
    compartido de Bevy para esa build puntual), pero es la única forma de que cargo-apk encuentre
    los artefactos. No tiene arreglo de config permanente sin parchear `cargo-apk`.
  - *Archivos:* ninguno del repo — es una incompatibilidad entre el workspace compartido y la
    herramienta `cargo-apk`, documentada acá para no tener que re-diagnosticarla.

- [x] **`[package.metadata.android] strip = "strip"` — value inválido, casi lleva a un fix
  equivocado**
  - `strip` en `[package.metadata.android]` es un enum (`default` | `strip` | `split`), no el
    nombre de un binario. `"strip"` ya era válido — el error de arriba no tenía nada que ver con
    este campo (se investigó y descartó antes de encontrar la causa real).

- [x] **Build de Android terminado y firmado con éxito** — `target/release/apk/lightcore.apk`
  (23.5MB, verificado con `apksigner verify`: firmado con esquema v3).
  - `cargo apk build --release` (con las dos env vars de arriba) itera sobre *todos* los artifacts
    del crate (`cmd.artifacts()` = lib + bin). El `.apk` de la lib (`cdylib`, el que importa) se
    genera y firma bien, pero después el comando panickea intentando armar un APK también para el
    binario de escritorio `lightcore_bin` (`Bin is not compatible with Cdylib`,
    `cargo-subcommand-0.12.0/src/artifact.rs:51`). El `.apk` ya quedó completo en disco antes del
    panic (la firma es una llamada bloqueante, corre antes de pasar al siguiente artifact), así que
    es inofensivo — pero ensucia el output y hace pensar que falló todo.
  - **Para evitarlo:** agregar `--lib` al comando, que le dice a cargo-apk que solo construya el
    artifact de la librería:
    ```sh
    CARGO_TARGET_DIR="$(pwd)/target" CARGO_BUILD_BUILD_DIR="$(pwd)/target" cargo apk build --release --lib
    ```

---

## 📝 Estado de la sesión — 2026-07-18 (build Android)

- [x] **Reserva protegida, boons responsivos y economía reajustada**
  - `RunState` es ahora el dueño canónico de la reserva, junto a vidas y boons. Partidas Classic/sandbox/debug usan un saldo local y no pueden sobrescribirla; abandonar, terminar o reiniciar el run elimina su billetera.
  - La suma usa saturación en el límite numérico de `u32`, evitando cualquier wrap a un valor pequeño incluso con cantidades extremas o modificadas.
  - Los saves nativos se escriben a un temporal y se reemplazan atómicamente, evitando que un cierre/crash durante la escritura convierta el run guardado en un archivo parcial.
  - La bandeja de boons dispone de un área exclusiva a la derecha del botón de pausa y envuelve sus iconos en varias filas cuando se acumulan.
  - Vida y habilidades especiales cuestan ×10: `800c`, `200c`, `450c` y `900c`.
  - *Archivos:* `core/run.rs`, `gameplay/lifecycle.rs`, `gameplay/shop.rs`, `ui/mod.rs`.

- [x] **Primer build/deploy funcional en Android (Nokia 5.1 Plus)**
  - `Cargo.toml` no tenía `assets = "assets"` bajo `[package.metadata.android]` → cargo-apk nunca empaquetaba `assets/` en el APK, dejando iconos/fuentes/tutorial rotos en silencio. Agregado.
  - Faltaba el entry point `android_main` en el crate `cdylib`: `#[bevy_main] fn main()` vivía implícito solo en `main.rs` (target `bin`, no empaquetado por cargo-apk). Agregado a `lib.rs`.
  - Firmado temporal de release con el `debug.keystore` del sistema (`[package.metadata.android.signing.release]`) — **no sirve para Play Store**, falta generar un keystore dedicado antes de publicar.
  - *Archivos:* `Cargo.toml`, `src/lib.rs`.

- [x] **Panel de estado (moves/vidas/reserva/especiales) invadía el grid en portrait**
  - Causa 1: `toggle_hud_descriptions_on_hover` solo alternaba `Visibility`, no `Display` — en `bevy_ui` un nodo `Visibility::Hidden` sigue reservando espacio de layout (taffy no mira `Visibility`). Arreglado seteando también `Node.display`.
  - Causa 2: sin tamaño compacto para `DeviceMode::Mobile` (icon/padding/gap fijos independiente del modo). Agregado `compact` en `ui/mod.rs::setup_ui`.
  - Resultado verificado en dispositivo: el panel ya no se solapa con el grid.
  - *Archivos:* `ui/mod.rs`.

- [x] **Boon se vendía sin querer al tocar para leerlo (touch no tiene hover)**
  - Se separó "leer" (tap en el ícono expande la tarjeta con descripción, `BoonPeekButton` + `PeekedBoon`) de "vender" (botón `BoonSellButton` nuevo, solo visible expandido, con la misma confirmación de doble-tap de antes).
  - *Archivos:* `ui/mod.rs`.

- [x] **Texto "Tutorial: ON/OFF" del overlay en partida no aparecía**
  - `update_tutorial_overlay_toggle_text` estaba gateado por `resource_changed::<UserSettings>`, que no disparaba a tiempo tras el spawn inicial (`Startup`). Se quitó el gate (costo trivial).
  - *Archivos:* `ui/mod.rs`.

- [x] **Redistribuir el HUD y separar compra/uso de especiales**
  - Objetivo, movimientos y cores quedaron como indicadores borderless sobre los controles inferiores.
  - Vida, Swap, Eliminar y Mejorar están en una fila borderless con iconos grandes; `+coste` compra y tocar el icono usa una copia existente.
  - Las compras de especiales requieren segundo toque de confirmación (`OK`); comprar vida sigue siendo directo.
  - El HUD inferior deja una franja exclusiva para los datos de versión/FPS del build.
  - *Archivos:* `ui/mod.rs`, `gameplay/shop.rs`.

- [x] **Tooltips legibles en desktop y touch**
  - Centrados, con wrapping y ancho limitado; en touch persisten tres segundos después del toque.
  - El objetivo se identifica explícitamente como "Objetivo del nivel".
  - Los tooltips de vida y botones de compra fueron verificados en el Nokia.
  - Los boons compactos son borderless; su detalle usa un panel oscuro y la venta mantiene doble confirmación.
  - *Archivos:* `ui/mod.rs`, `core/locale.rs`.

- [x] Version bump: `0.1.0` → `0.1.1`.

---

## 📝 Estado de la sesión — 2026-07-15

- [x] **Boons como economía de run**
  - Los boons se ofrecen y compran solamente tras completar una etapa; ya no se conceden gratis.
  - Se pueden vender durante la etapa por el coste íntegro del último rango comprado, con confirmación de dos toques para evitar ventas accidentales.
  - `Chispa+` fue reemplazado por `Estrella×`: los Starburst entregan lightcores y shards extra. `Rojo+` ahora aumenta tanto el valor (+25% por rango) como los shards visuales rojos.
  - *Archivos:* `core/run.rs`, `gameplay/lifecycle.rs`, `gameplay/rewards.rs`, `visuals/score_light.rs`, `ui/mod.rs`.

- [x] **Inventario de movimientos especiales**
  - La tienda compra `SWP`, `POP` y `UP` como inventario. Los contadores del HUD arman una copia y solo la consumen tras ejecutar una acción válida.
  - *Archivos:* `gameplay/shop.rs`, `gameplay/mod.rs`, `ui/mod.rs`.

- [/] **Panel integrado de estado/economía**
  - El HUD agrupa moves, vidas, cores/tienda y contadores de especiales; los boons se muestran independientemente en la esquina inferior derecha.
  - **Pendiente crítico para mañana:** el panel superior y/o su desplegable de movimientos especiales bloquea/invade el área de juego. Rediseñar su anclaje, dimensiones y comportamiento responsive antes de seguir puliendo su estética.
  - *Archivos:* `ui/mod.rs`.

- [x] **VFX de captura y Supernova**
  - Refinadas las trayectorias de shards por color, brillo HDR, pausa/velocidad variables y la expulsión continua de shards de Supernova antes de que el score los absorba.
  - El refill ahora cae como gotera rápida, evitando el efecto de persiana.
  - *Archivos:* `visuals/score_light.rs`, `gameplay/spawning.rs`, `visuals/particles.rs`.

---

## 🎨 1. VFX e Interacciones entre Power Lights

- [x] **Animación Unificada para Doble Supernova (Supernova × Supernova)**
  - *Descripción:* Actualmente, si dos supernovas estallan juntas, reproducen dos animaciones solapadas. Deberían tener una única animación de onda expansiva de $5 \times 5$ espectacular.
  - *Archivos clave:* [vfx.rs](file:///home/francisco/Programming/uneven/lightcore/src/gameplay/vfx.rs) y [assets.rs](file:///home/francisco/Programming/uneven/lightcore/src/visuals/assets.rs).

- [x] **Animación Especial de Shuriken (Starburst) × Supernova**
  - *Descripción:* Cuando un Starburst (shuriken) se junta con una Supernova, el haz de luz que busca y golpea a cada objetivo de ese color debe provocar una explosión de supernova (área $3 \times 3$) al impactar.
  - *Archivos clave:* [vfx.rs](file:///home/francisco/Programming/uneven/lightcore/src/gameplay/vfx.rs) (ver `trigger_wave_vfx` o `trigger_combo`).

---

## 🎇 2. Trayectorias y Comportamientos de Partículas por Color

Queremos que recolectar diferentes formas/colores de luces tenga físicas visuales distintas cuando viajan hacia el Score:

- [x] **Triángulos (Green / Verde): Trayectoria de Rayo Rápido**
  - *Descripción:* Viajan de forma directa e inmediata en línea recta y a alta velocidad hacia el marcador de puntos.
  - *Archivos clave:* [particles.rs](file:///home/francisco/Programming/uneven/lightcore/src/visuals/particles.rs).

- [x] **Cuadrados (Yellow / Amarillo o Purple / Púrpura): Vuelo Flotante**
  - *Descripción:* Al ser recolectados, revolotean en círculos u ondas sobre el tablero antes de acelerar e irse al marcador de puntos.
  - *Archivos clave:* [particles.rs](file:///home/francisco/Programming/uneven/lightcore/src/visuals/particles.rs).

---

## ⚖️ 3. Nuevos Boons de Probabilidad de Color en la Tienda

- [x] **Boons de Frecuencia de Color en la Tienda (Color Spawn Boons)**
  - *Descripción:* Agregar cartas/boons a la tienda (como `+Rojo`, `+Azul`, `+Verde`, `+Amarillo`, `+Púrpura`) que alteren las probabilidades de aparición de luces de colores específicos en el tablero, facilitando niveles que requieran recolectar ciertos colores.
  - *Archivos clave:* [run.rs](file:///home/francisco/Programming/uneven/lightcore/src/core/run.rs) (añadir a `BoonKind` y calcular pesos en `RunState::color_weights`), [spawning.rs](file:///home/francisco/Programming/uneven/lightcore/src/gameplay/spawning.rs) (usar los pesos al reponer el board), [shop.rs](file:///home/francisco/Programming/uneven/lightcore/src/gameplay/shop.rs) y [locale.rs](file:///home/francisco/Programming/uneven/lightcore/src/core/locale.rs) (para traducciones de las cartas).

---

## 💀 4. Penalización de Hollows (Feedback Negativo Claro)

Consumir un Hollow (vacío) vacía el score actual a 0. Esto debe sentirse pesado y dramático para alertar al jugador.

- [x] **Animación Más Lenta/Dramática de Drenado**
  - *Descripción:* Hacer que el drenado de puntos en pantalla y el desvanecimiento de la pieza hollow sea más pausado y tenga una distorsión visual.
  - *Archivos clave:* [popping.rs](file:///home/francisco/Programming/uneven/lightcore/src/gameplay/popping.rs) y [rewards.rs](file:///home/francisco/Programming/uneven/lightcore/src/gameplay/rewards.rs).

- [x] **Efecto de Sonido Triste y Largo**
  - *Descripción:* Reproducir un efecto de sonido descendente, apagado y más largo al consumir un Hollow para dar feedback auditivo negativo.
  - *Archivos clave:* [audio/mod.rs](file:///home/francisco/Programming/uneven/lightcore/src/audio/mod.rs).

---

## 🎛️ 5. Rediseño Neon e Iconos de la UI

- [/] **Diseño Estilo Neón**
  - *Descripción:* Aplicar sombreados con resplandor (glow), bordes brillantes y fuentes tipo retro-futurista/cyberpunk en los botones de menús y HUD. (En progreso: bordes, colores, animaciones hover y `BorderRadius` compatible con Bevy 0.19 ya implementados).
  - *Archivos clave:* [ui/mod.rs](file:///home/francisco/Programming/uneven/lightcore/src/ui/mod.rs) y [menu/mod.rs](file:///home/francisco/Programming/uneven/lightcore/src/menu/mod.rs).

- [/] **Sustituir Texto por Iconos**
  - *Descripción:* Minimizar el uso de texto en pantalla en favor de iconos estilizados (por ejemplo, para botones de configuración, tienda, volver al menú, etc.). (En progreso: iconos vectoriales propios de play, configuración, salida y volver integrados en MainMenu, PauseMenu y botones de volver; faltan los iconos del HUD/tienda).
  - *Archivos clave:* [ui/mod.rs](file:///home/francisco/Programming/uneven/lightcore/src/ui/mod.rs) y [menu/main_menu.rs](file:///home/francisco/Programming/uneven/lightcore/src/menu/main_menu.rs).

---

## 📱 6. UI Multiplataforma y Responsiva

- [/] **Adaptar Layout para Desktop y Dispositivos Móviles (Android Portrait)**
  - *Descripción:* Asegurar que la relación de aspecto del tablero, los botones de la tienda inferior y el HUD superior no se solapen y se reacomoden perfectamente en pantallas verticales de móvil sin salirse del área segura. (En progreso: implementado reescalado dinámico y compacto de la tienda según el modo Mobile/Desktop).
  - *Archivos clave:* [ui/mod.rs](file:///home/francisco/Programming/uneven/lightcore/src/ui/mod.rs) y [platform.rs](file:///home/francisco/Programming/uneven/lightcore/src/platform.rs).

---

## 🧪 7. Playtest e Iteración Continua

- [ ] Realizar playtest para comprobar las mejoras.

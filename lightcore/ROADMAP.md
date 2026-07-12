# Roadmap: estado actual vs. estado deseado

Este documento es distinto a `CONSTITUTION.md` (que explica el *por qué* de la arquitectura). Acá se documenta el *gap*: qué funciona hoy mecánicamente vs. qué falta para que se sienta terminado. Sirve para retomar trabajo de una sesión a otra sin perder el hilo de qué es "funciona pero es provisorio" vs. "está bien así".

Convención: ✅ terminado · 🚧 funciona pero necesita más trabajo · ❌ no empezado.

## 1. Interacciones entre power lights (combos)

**🚧 Funcionan todas, pero con calidad dispareja entre combos.**

### Estado actual

Los 9 `ComboKind` (`DoubleLine`, `LineSupernova`, `DoubleSupernova`, `StarLine`, `StarSupernova`, `StarStar`, `StarColor`, `Blackhole`, `SuperCombo`) están todos implementados y son jugables — cada uno tiene su efecto de juego correcto (`core::matching::resolve_swap_activation`) y su propio sonido sintetizado (`audio::on_power_combo`).

Pero solo dos de ellos tienen la coreografía "en dos fases" que se sintió bien en las pruebas (`gameplay::vfx::trigger_star_transform_combo`):

- **`StarLine` y `StarSupernova`**: la estrella busca cada light del color objetivo, cada uno se *transforma visualmente* al tier del partner al llegar su orbe (`PendingLightTransform`, reutilizando el rebuild real de `visuals::core_motion`), y recién cuando llega el orbe más lento — más un pequeño respiro (`RaySettings::combo_hold_secs`) — todos detonan juntos en una ola sincronizada.
- **El resto** (`DoubleLine`, `LineSupernova`, `DoubleSupernova`, `StarColor`, `StarStar`, `Blackhole`, `SuperCombo`) siguen con el modelo viejo: el efecto dispara casi instantáneamente (`delay_secs: 0.0` en la mayoría de sus `PowerBlastTrail`), sin transformación previa ni pausa dramática.

### Estado deseado

Extender el mismo tratamiento de dos fases al resto de combos donde tenga sentido — no es un copy-paste directo, cada uno necesita pensar qué es "la transformación" y qué es "la detonación sincronizada" para su propia forma:

- `DoubleLine`/`LineSupernova`/`DoubleSupernova` no tienen un paso de "la estrella busca targets", así que el equivalente sería otra cosa (¿una pausa antes de que las dos líneas/supernovas disparen a la vez, en vez de simultáneo instantáneo?) — hay que pensarlo, no asumir que el patrón de `trigger_star_transform_combo` aplica tal cual.
- `Blackhole`/`SuperCombo` ya tienen su propia coreografía dedicada (`spawn_blackhole_effect`, el shockwave dorado) — probablemente no necesitan el patrón de dos fases, pero valdría la pena revisar si el timing actual (instantáneo) se siente débil comparado con `StarLine`.

## 2. Sonido de combos — desincronizado con la nueva coreografía

**🚧 Bug de diseño conocido, no arreglado todavía.**

### Estado actual

`audio::on_power_combo` reproduce el sonido del combo apenas se dispara el evento `PowerCombo` — es decir, en el instante en que el jugador hace el swap, con `delay_secs: 0.0` siempre. Para `StarLine`/`StarSupernova` esto quedó desincronizado: el sonido suena de entrada, pero el pago visual (la detonación sincronizada) puede tardar hasta ~1 segundo (orbe más lento + `combo_hold_secs`).

### Estado deseado

El sonido del combo debería sonar en el momento de la detonación (`explode_delay`), no en el momento del swap — quizás con un sonido más chico/sutil en el instante del swap (la estrella "cargando") y el sonido principal del combo sincronizado con la explosión. Esto requiere que `trigger_star_transform_combo` le pase el `explode_delay` a algo que dispare el sonido con ese retraso (hoy `on_power_combo` no tiene forma de saber ese delay, porque lee directo del evento `PowerCombo` que se dispara sin retraso).

## 3. Feedback de cámara ("pausa" / cámara lenta en combos)

**❌ Probado dos veces, revertido las dos.**

### Historial (para no repetir el mismo camino sin darse cuenta)

1. Durante la auditoría (`/goal`) se agregó un sistema completo de camera shake (trauma-based, disparado por `PowerConsumed`/`PowerCombo`). El usuario lo probó y decidió explícitamente que no encaja con el género ("el screenshake no es bueno para este tipo de juegos") — se revirtió por completo, sin dejar rastros en el código.
2. Al construir la coreografía de `StarLine`, se agregó un hit-stop más acotado (`ComboTimeWarp`: dip a 0.15x por 0.05s + recuperación suave en 0.3s, escalando `Time<Virtual>`) scopeado solo a `StarLine`. Generó bugs difíciles de diagnosticar sin verificación visual directa en este entorno (sandbox sin captura de pantalla confiable) — se retiró para aislar y resolver primero el bug de la transformación visual (que resultó ser autónomo del time-warp). No se reintentó después.

### Estado deseado

El usuario sigue queriendo "una pequeña pausa de cámara, o una pequeña cámara lenta" para las interacciones entre power lights grandes — pero **no** screenshake (eso ya fue descartado). Si se retoma:
- Aislar bien de la lógica de timing del combo (`Time<Virtual>` afecta *todo*, incluyendo los propios delays que recién se armaron para `StarLine` — fácil generar loops de retroalimentación confusos si no se separa con cuidado, como pasó la primera vez).
- Probablemente conviene verificarlo con el usuario jugando en vivo antes de darlo por bueno, dado que este entorno no puede confirmarlo visualmente por sí solo.

## 4. Juice general (fuera de combos)

**✅ Lo que se hizo, se mantiene.**

- Jelly effect al seleccionar una luz (`gameplay::input::SelectJelly`, 3 rebotes decrecientes) y al aterrizar (`visuals::bounce::LandBounce`) — confirmados y en uso.
- Screenshake para animaciones grandes — descartado explícitamente (ver sección 3). No reabrir sin que el usuario lo pida de nuevo.

## 5. Niveles de debug — cobertura de variantes

**🚧 Cubre los 9 `ComboKind`, pero no todas las variantes internas de cada uno.**

### Estado actual

`gameplay::lifecycle::DEBUG_SCENARIOS` tiene 9 escenarios — uno por `ComboKind`, más uno extra para la variante "shuriken" (`Cross`) de `StarLine`. Pero `DoubleLine` y `LineSupernova` también aceptan `Cross` como partner (no solo `RayH`/`RayV`) y no tienen su propio nodo de debug para esa variante.

### Estado deseado

Si al iterar sobre la sección 1 (DoubleLine/LineSupernova con Cross) hace falta verlo aislado, agregar los nodos de debug correspondientes siguiendo el mismo patrón (`MenuEntryKind::Debug(n)`, un nodo más en la rama de debug del mapa de niveles).

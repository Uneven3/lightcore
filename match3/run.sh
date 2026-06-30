#!/usr/bin/env bash
# Lanzador del juego en Linux. Se para en su propia carpeta para que Bevy encuentre ./assets,
# así da igual desde dónde lo ejecutes (o si le das doble clic).
cd "$(dirname "$0")" || exit 1
exec ./match3 "$@"

// En release de Windows, oculta la ventana de consola que de otro modo aparece detrás del juego.
// En debug se deja para seguir viendo los logs por stdout.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lightcore::run_game();
}

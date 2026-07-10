# Lightcore

**Lightcore** es un dinámico juego de puzzle estilo Match-3 desarrollado en Rust utilizando el motor de videojuegos [Bevy](https://bevyengine.org/). El proyecto está optimizado y diseñado para ejecutarse de forma fluida tanto en computadoras de escritorio como en la web (WASM) y en dispositivos móviles (Android).

---

## 🚀 Características Principales

*   **Motor Bevy (v0.19):** Utiliza una arquitectura moderna basada en ECS (Entity Component System) que garantiza un excelente rendimiento.
*   **Efectos Visuales Premium (VFX):** Sistema de iluminación dinámica para gemas especiales (*light/power candies*), animación de rebote (*bouncing*), efectos de estela de luz y un grid con simulaciones de fluidos/agua en shaders (WGSL).
*   **Optimización de Rendimiento:** Integración de modos de presentación ajustables (Mailbox en escritorio para evitar saltos bruscos de FPS en arquitecturas híbridas y AutoVsync en móviles) y deshabilitación selectiva de plugins pesados no requeridos.
*   **Soporte Multiplataforma:**
    *   **Escritorio:** Windows, macOS y Linux.
    *   **Web:** Compilación optimizada a WebAssembly (WASM).
    *   **Móvil:** Integración nativa con Android (orientación vertical por defecto).
*   **Localización integrada:** Traducción dinámica entre español e inglés.

---

## 🛠️ Requisitos de Compilación

Asegúrate de tener instalado:
*   [Rust](https://www.rust-lang.org/) (Edición 2024 o superior).
*   Las dependencias de desarrollo del sistema necesarias para Bevy. En Linux (Ubuntu/Debian), puedes instalarlas con:
    ```bash
    sudo apt-get install g++ pkg-config libx11-dev libasound2-dev libudev-dev
    ```
    *Para más detalles sobre dependencias de Bevy en otros sistemas operativos, consulta la [guía oficial de Bevy](https://bevyengine.org/learn/book/getting-started/setup/).*

---

## 💻 Instrucciones de Compilación y Ejecución

### 1. Ejecución en Escritorio (Desktop)
Para compilar y correr el juego de forma local:
```bash
cargo run
```

Para desarrollo incremental más rápido (compila Bevy como biblioteca dinámica):
```bash
cargo run --features dev
```

Para compilar la versión optimizada de producción:
```bash
cargo run --release
```

### 2. Ejecución en Web (WASM)
El proyecto utiliza [Trunk](https://trunkrs.dev/) para empaquetar la aplicación web.

Para instalar Trunk y el target de compilación WebAssembly:
```bash
cargo install --locked trunk
rustup target add wasm32-unknown-unknown
```

Para arrancar el servidor de desarrollo local con recarga en vivo:
```bash
cd lightcore
trunk serve
```
El servidor web estará disponible por defecto en `http://localhost:8080`.

Para generar la compilación de producción:
```bash
cd lightcore
trunk build --release
```
Los archivos de distribución generados se ubicarán en el directorio `/lightcore/dist/`.

### 3. Ejecución en Android
El empaquetado para Android está configurado mediante `cargo-apk`.

Requisitos previos:
*   Tener instalado el SDK y NDK de Android.
*   Tener configurado el target de Rust correspondiente:
    ```bash
    rustup target add aarch64-linux-android
    ```

Para instalar la herramienta de compilación:
```bash
cargo install cargo-apk
```

Para compilar y correr la aplicación en un dispositivo Android conectado o emulador:
```bash
cargo apk run
```

---

## ⚖️ Licencia

Este proyecto está licenciado bajo la **GNU General Public License v3.0 (GPLv3)**. Consulta el archivo [LICENSE](LICENSE) para ver el texto completo de la licencia.

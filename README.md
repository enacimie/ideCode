# ideCode

[![verify](https://github.com/enacimie/ideCode/actions/workflows/verify.yml/badge.svg)](https://github.com/enacimie/ideCode/actions/workflows/verify.yml)
[![release](https://github.com/enacimie/ideCode/actions/workflows/release.yml/badge.svg)](https://github.com/enacimie/ideCode/actions/workflows/release.yml)

Mini IDE modular para aprender a programar, pensado para el aula. Todo el análisis
y la generación de diagramas ocurren en **Rust**; la interfaz es una capa fina de
**React** sobre **Tauri 2**.

## Características

- Editor con resaltado de sintaxis (CodeMirror 6) para **Java**, **Python** y
  **Kotlin**.
- **Java**: compilación real con `javac`, diagnósticos con archivo y línea,
  ejecución con `java -cp` y argumentos.
- **Python**: comprobación de sintaxis de todos los archivos de una pasada
  (sin generar `__pycache__` en tu proyecto) y ejecución con `python3`,
  `PYTHONPATH` apuntando a la raíz y argumentos.
- **Kotlin**: compilación real con `kotlinc` y ejecución con `java -cp` más la
  biblioteca estándar, detectando la clase fachada (`Main.kt` → `MainKt`).
- **Diagrama de clases automático y completo**, generado en Mermaid a partir del
  código fuente: herencia, interfaces, asociaciones con multiplicidad (`1`,
  `0..1`, `0..*`, agregación), dependencias de uso, clases anidadas, enums,
  records y, en Python, módulos con funciones libres, clases abstractas
  (`ABC`/`@abstractmethod`), `@staticmethod`, `async` y visibilidad por
  convención (`_x`, `__x`), y en Kotlin `data class` (`<<record>>`), `object`
  (miembros estáticos), `companion object`, `suspend`, propiedades `val`/`var`
  del constructor primario y funciones de nivel superior en un nodo
  `<<module>>`.
- Diagrama **clicable**: pulsa una clase, atributo o método para abrirlo en el
  editor en su línea.
- Ejecución en streaming (salida y errores en vivo) con límite de tiempo y
  limpieza del grupo de procesos completo al vencer.
- Ejemplos integrados que se descubren solos: cualquier carpeta bajo
  `examples/` aparece en el menú **Archivo ▸ Ejemplos**.
- Confirmación antes de cerrar si hay cambios sin guardar; `Ctrl+S` global.

## Requisitos

Para usar la aplicación (no para desarrollarla):

- **Java**: un JDK 17 o superior en el `PATH`, o `JAVA_HOME` definido.
- **Python**: Python 3.9+ en el `PATH` (`python3` o `python`), u `PYTHON_HOME`.
- **Kotlin**: `kotlinc` 1.5.30+ en el `PATH`, o `KOTLIN_HOME` definido
  (recomendado con SDKMAN: `sdk install kotlin`). Las versiones antiguas
  (p. ej. la 1.3 de algunos repositorios) fallan con JDK 17+; la aplicación
  lo detecta y lo explica en el panel de salida. Además necesita un JDK.

Para desarrollarla:

- [Node.js](https://nodejs.org) 20+ y [pnpm](https://pnpm.io) 10.
- [Rust](https://rustup.rs) estable.
- Linux: dependencias de Tauri (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`,
  `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`…).
  En Windows y macOS basta con el toolchain de Tauri:
  <https://tauri.app/start/prerequisites/>.

## Desarrollo

```sh
pnpm install          # dependencias del frontend
pnpm tauri dev        # aplicación de escritorio en caliente
pnpm dev              # solo la demo web (sin compilación/ejecución/diagramas)
pnpm verify           # lint + formato + typecheck + build + vitest + fmt/clippy/test de Rust
```

## Empaquetado

```sh
pnpm tauri build      # genera .deb/.rpm/.AppImage en Linux; .msi/.exe en Windows; .app/.dmg en macOS
```

La versión del paquete proviene únicamente de `src-tauri/Cargo.toml`.
Los binarios de AppImage descargan herramientas de linuxdeploy durante la
construcción (requiere red).

La CI (`release.yml`) construye los artefactos de Linux dentro de un
contenedor **Ubuntu 22.04** para que el suelo de glibc sea 2.35: los
`.deb`, `.rpm` y `.AppImage` funcionan en Ubuntu 22.04+, Debian 12+ y
Fedora 36+ (si se construyeran en el runner `ubuntu-latest`, exigirían la
glibc del Ubuntu más nuevo y no arrancarían en distribuciones anteriores).

## Arquitectura

```
src-tauri/src/
  core/            núcleo agnóstico del lenguaje
    model.rs       IR: ClassModel, MethodModel, FieldModel, FunctionModel…
    adapter.rs     trait LanguageAdapter + BuildResult/RunSpec/Diagnostic
    registry.rs    selección de adaptador por contenido o extensión
    diagram.rs     generador Mermaid (puro, sin listas de tipos)
    project.rs     carga de fuentes ignorando directorios generados
    tools.rs       descubrimiento portable de ejecutables (PATH/PATHEXT/HOME)
  adapters/
    java.rs        javac/java + tree-sitter-java
    python.rs      py_compile/python3 + tree-sitter-python
    kotlin.rs      kotlinc/java + tree-sitter-kotlin-ng
  runner.rs        ejecución en streaming, timeout y muerte del grupo
  lib.rs           comandos Tauri, ejemplos embebidos, estado del proyecto
src/
  backend/         Backend único con implementaciones Tauri y web
  components/      Toolbar (menú Archivo), FileTree, CodeEditor, OutputPanel,
                   DiagramPanel (Mermaid clicable), Menu, CloseDialog
  languages/       registro de lenguajes del editor (extensiones CodeMirror)
examples/          proyectos de ejemplo embebidos en el binario (include_dir)
```

Cómo añadir un lenguaje: implementa `LanguageAdapter` en `src-tauri/src/adapters/`,
regístralo en `core/registry.rs` y añade su extensión de CodeMirror en
`src/languages/index.ts`. El diagrama, el registro, los ejemplos y la UI se
adaptan solos.

## Límites conocidos

- No hay borrado ni renombrado de archivos desde la UI.
- El estado (último proyecto, argumentos) no persiste entre sesiones.
- En macOS, `Cmd+Q` no pasa por la confirmación de cambios sin guardar
  (el botón de cerrar de la ventana sí).
- Los diagramas no muestran dependencias hacia tipos fuera del proyecto
  (salvo superclases/interfaces, que aparecen como cajas externas).
- En Windows, el timeout mata al proceso directo, no a todo su árbol
  (en Unix muere el grupo completo).
- En Kotlin, el punto de entrada debe ser un `fun main()` de nivel superior
  (no dentro de un `object` o clase con `@JvmStatic`).

## Licencia

AGPL-3.0-or-later. Véase [LICENSE](LICENSE).

Copyright © Eduardo Nacimiento-García y colaboradores de ideCode.

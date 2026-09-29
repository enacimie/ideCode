mod adapters;
mod core;
mod runner;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use include_dir::{include_dir, Dir};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::core::adapter::BuildResult;
use crate::core::diagram;
use crate::core::generator::{self, GeneratedFile};
use crate::core::model::ClassModel;
use crate::core::project::Project;
use crate::core::registry;
use crate::runner::{RunOutcome, Stream};

const RUN_TIMEOUT: Duration = Duration::from_secs(30);

static EXAMPLES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../examples");

#[derive(Default)]
struct ProjectState(Mutex<Option<PathBuf>>);

#[derive(Serialize)]
struct SourceEntry {
    path: String,
    relative: String,
    name: String,
    content: String,
}

#[derive(Serialize)]
struct ProjectSnapshot {
    root: String,
    language: String,
    files: Vec<SourceEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LanguageInfo {
    id: String,
    name: String,
    extensions: Vec<String>,
    compile_label: String,
    entry_label: String,
}

#[derive(Serialize)]
struct ExampleInfo {
    id: String,
    name: String,
    files: usize,
}

#[derive(Serialize, Clone)]
struct RunChunk {
    stream: String,
    line: String,
}

#[derive(Serialize)]
struct DiagramResult {
    language: String,
    mermaid: String,
    classes: Vec<ClassModel>,
}

#[derive(Serialize)]
struct WriteOutcome {
    written: Vec<SourceEntry>,
    conflicts: Vec<String>,
}

fn current_root(state: &State<'_, ProjectState>) -> Result<PathBuf, String> {
    state
        .0
        .lock()
        .map_err(|_| "El estado del proyecto está bloqueado.".to_string())?
        .clone()
        .ok_or_else(|| "Abre una carpeta de proyecto primero.".to_string())
}

fn snapshot(root: &Path) -> Result<ProjectSnapshot, String> {
    let adapter = registry::for_project(root)
        .ok_or_else(|| "No hay adaptador para este proyecto.".to_string())?;
    let project = Project::load(root, adapter.extensions()).map_err(|error| error.to_string())?;
    let files = project
        .sources
        .iter()
        .map(|source| SourceEntry {
            path: source.path.to_string_lossy().into_owned(),
            relative: source.relative.clone(),
            name: Path::new(&source.relative)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| source.relative.clone()),
            content: source.content.clone(),
        })
        .collect();

    Ok(ProjectSnapshot {
        root: project.root.to_string_lossy().into_owned(),
        language: adapter.id().to_string(),
        files,
    })
}

fn resolve(root: &Path, path: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let candidate = PathBuf::from(path);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };

    let canonical = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| error.to_string())?
    } else {
        let parent = candidate
            .parent()
            .ok_or_else(|| "Ruta inválida.".to_string())?;
        let parent = parent.canonicalize().map_err(|error| error.to_string())?;
        let name = candidate
            .file_name()
            .ok_or_else(|| "Ruta inválida.".to_string())?;
        parent.join(name)
    };

    if !canonical.starts_with(&root) {
        return Err("La ruta está fuera del proyecto abierto.".to_string());
    }
    Ok(canonical)
}

fn validate_new_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Escribe un nombre de archivo.".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err("El nombre no puede contener rutas.".to_string());
    }
    Ok(trimmed.to_string())
}

fn example_name(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => {
                    let mut word = first.to_uppercase().collect::<String>();
                    word.push_str(characters.as_str());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_source_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    registry::all().iter().any(|adapter| {
        adapter
            .extensions()
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(extension))
    })
}

fn count_files(dir: &Dir<'_>) -> usize {
    dir.files()
        .filter(|file| is_source_file(file.path()))
        .count()
        + dir.dirs().map(|child| count_files(child)).sum::<usize>()
}

fn examples() -> Vec<ExampleInfo> {
    let mut found: Vec<ExampleInfo> = EXAMPLES
        .dirs()
        .filter_map(|dir| {
            let id = dir.path().file_name()?.to_string_lossy().into_owned();
            let files = count_files(dir);
            if files == 0 {
                return None;
            }
            Some(ExampleInfo {
                name: example_name(&id),
                id,
                files,
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn write_example(example: &Dir<'_>, root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    write_example_dir(example, example.path(), root)
}

fn write_example_dir(dir: &Dir<'_>, prefix: &Path, root: &Path) -> Result<(), String> {
    for file in dir.files() {
        let relative = file
            .path()
            .strip_prefix(prefix)
            .map_err(|_| "Ruta de ejemplo inválida.".to_string())?;
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if path.exists() {
            continue;
        }
        let contents = file.contents_utf8().ok_or_else(|| {
            format!(
                "El ejemplo «{}» no es texto UTF-8.",
                file.path().to_string_lossy()
            )
        })?;
        std::fs::write(&path, contents).map_err(|error| error.to_string())?;
    }
    for child in dir.dirs() {
        write_example_dir(child, prefix, root)?;
    }
    Ok(())
}

fn example_relative_root(id: &str) -> PathBuf {
    Path::new("ejemplos")
        .join(env!("CARGO_PKG_VERSION"))
        .join(id)
}

fn valid_example_id(id: &str) -> bool {
    !id.is_empty() && !id.contains('/') && !id.contains('\\') && !id.contains("..")
}

fn example_root(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if !valid_example_id(id) {
        return Err("Nombre de ejemplo inválido.".to_string());
    }
    let example = EXAMPLES
        .get_dir(id)
        .ok_or_else(|| format!("No existe el ejemplo «{id}»."))?;
    let base = app
        .path()
        .app_local_data_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|error| error.to_string())?;
    let root = base.join(example_relative_root(id));
    write_example(example, &root)?;
    Ok(root)
}

#[tauri::command]
fn list_languages() -> Vec<LanguageInfo> {
    registry::all()
        .iter()
        .map(|adapter| LanguageInfo {
            id: adapter.id().to_string(),
            name: adapter.display_name().to_string(),
            extensions: adapter
                .extensions()
                .iter()
                .map(|extension| extension.to_string())
                .collect(),
            compile_label: adapter.compile_label().to_string(),
            entry_label: adapter.entry_label().to_string(),
        })
        .collect()
}

#[tauri::command]
async fn select_project(
    app: AppHandle,
    state: State<'_, ProjectState>,
) -> Result<Option<ProjectSnapshot>, String> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let root = folder.into_path().map_err(|error| error.to_string())?;
    *state
        .0
        .lock()
        .map_err(|_| "El estado del proyecto está bloqueado.".to_string())? = Some(root.clone());
    Ok(Some(snapshot(&root)?))
}

#[tauri::command]
fn current_project(state: State<'_, ProjectState>) -> Result<Option<ProjectSnapshot>, String> {
    match state
        .0
        .lock()
        .map_err(|_| "El estado del proyecto está bloqueado.".to_string())?
        .clone()
    {
        Some(root) => Ok(Some(snapshot(&root)?)),
        None => Ok(None),
    }
}

#[tauri::command]
fn list_examples() -> Vec<ExampleInfo> {
    examples()
}

#[tauri::command]
async fn load_example(
    app: AppHandle,
    state: State<'_, ProjectState>,
    example: String,
) -> Result<ProjectSnapshot, String> {
    let root = example_root(&app, &example)?;
    *state
        .0
        .lock()
        .map_err(|_| "El estado del proyecto está bloqueado.".to_string())? = Some(root.clone());
    snapshot(&root)
}

#[tauri::command]
fn read_source(state: State<'_, ProjectState>, path: String) -> Result<String, String> {
    let root = current_root(&state)?;
    let resolved = resolve(&root, &path)?;
    std::fs::read_to_string(resolved).map_err(|error| error.to_string())
}

#[tauri::command]
fn write_source(
    state: State<'_, ProjectState>,
    path: String,
    content: String,
) -> Result<(), String> {
    let root = current_root(&state)?;
    let resolved = resolve(&root, &path)?;
    std::fs::write(resolved, content).map_err(|error| error.to_string())
}

fn plan_new_source(root: &Path, name: &str) -> Result<(PathBuf, String, String), String> {
    let safe_name = validate_new_name(name)?;
    let adapter = match registry::detected(root) {
        Some(detected) => detected,
        None => registry::for_extension(&safe_name).ok_or_else(|| {
            let extensions: Vec<&str> = registry::all()
                .iter()
                .flat_map(|adapter| adapter.extensions().iter().copied())
                .collect();
            format!(
                "Extensión no reconocida en «{safe_name}». En una carpeta vacía empieza por un archivo {}.",
                extensions
                    .iter()
                    .map(|extension| format!(".{extension}"))
                    .collect::<Vec<_>>()
                    .join(" o ")
            )
        })?,
    };
    if !adapter.accepts(&safe_name) {
        return Err(format!(
            "Para {} el nombre debe terminar en .{}.",
            adapter.display_name(),
            adapter.extensions().join(" o .")
        ));
    }
    adapter.validate_new_file(&safe_name)?;
    let template = adapter.new_source(&safe_name);
    Ok((root.join(&safe_name), safe_name, template))
}

#[tauri::command]
fn create_source(state: State<'_, ProjectState>, name: String) -> Result<SourceEntry, String> {
    let root = current_root(&state)?;
    let (path, safe_name, template) = plan_new_source(&root, &name)?;
    if path.exists() {
        return Err(format!("Ya existe «{safe_name}»."));
    }
    std::fs::write(&path, template).map_err(|error| error.to_string())?;
    let content = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    Ok(SourceEntry {
        path: path.to_string_lossy().into_owned(),
        relative: safe_name.clone(),
        name: safe_name,
        content,
    })
}

#[tauri::command]
async fn compile_project(state: State<'_, ProjectState>) -> Result<BuildResult, String> {
    let root = current_root(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = registry::for_project(&root)
            .ok_or_else(|| "No hay adaptador para este proyecto.".to_string())?;
        let project =
            Project::load(&root, adapter.extensions()).map_err(|error| error.to_string())?;
        adapter.compile(&project).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn class_diagram(state: State<'_, ProjectState>) -> Result<DiagramResult, String> {
    let root = current_root(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = registry::for_project(&root)
            .ok_or_else(|| "No hay adaptador para este proyecto.".to_string())?;
        let project =
            Project::load(&root, adapter.extensions()).map_err(|error| error.to_string())?;
        let classes = adapter
            .analyze(&project)
            .map_err(|error| error.to_string())?;
        let mermaid = diagram::to_mermaid(&classes);
        Ok::<DiagramResult, String>(DiagramResult {
            language: adapter.id().to_string(),
            mermaid,
            classes,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn run_project(
    app: AppHandle,
    state: State<'_, ProjectState>,
    entry: Option<String>,
    args: Vec<String>,
) -> Result<RunOutcome, String> {
    let root = current_root(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = registry::for_project(&root)
            .ok_or_else(|| "No hay adaptador para este proyecto.".to_string())?;
        let project =
            Project::load(&root, adapter.extensions()).map_err(|error| error.to_string())?;
        let spec = adapter
            .run_spec(&project, entry.as_deref(), &args)
            .map_err(|error| error.to_string())?;

        let emitter = app.clone();
        runner::run_streaming(&spec, Some(RUN_TIMEOUT), move |stream, line| {
            let payload = RunChunk {
                stream: match stream {
                    Stream::Stdout => "stdout".to_string(),
                    Stream::Stderr => "stderr".to_string(),
                },
                line,
            };
            let _ = emitter.emit("run://output", payload);
        })
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn generate_code(classes: Vec<ClassModel>, language: String) -> Result<Vec<GeneratedFile>, String> {
    generator::generate(&classes, &language)
}

fn plan_generated<'a>(
    root: &Path,
    files: &'a [GeneratedFile],
) -> Result<Vec<(PathBuf, &'a GeneratedFile)>, String> {
    files
        .iter()
        .map(|file| {
            if file.relative.is_empty()
                || file.relative.contains('/')
                || file.relative.contains('\\')
                || file.relative.contains("..")
            {
                return Err(format!("Nombre de archivo inválido: «{}».", file.relative));
            }
            let resolved = resolve(root, &file.relative)?;
            Ok((resolved, file))
        })
        .collect()
}

#[tauri::command]
fn write_generated_files(
    state: State<'_, ProjectState>,
    files: Vec<GeneratedFile>,
    overwrite: bool,
) -> Result<WriteOutcome, String> {
    let root = current_root(&state)?;
    let targets = plan_generated(&root, &files)?;
    let conflicts: Vec<String> = targets
        .iter()
        .filter(|(path, _)| path.exists())
        .map(|(_, file)| file.relative.clone())
        .collect();
    if !conflicts.is_empty() && !overwrite {
        return Ok(WriteOutcome {
            written: Vec::new(),
            conflicts,
        });
    }

    let mut written = Vec::new();
    for (path, file) in targets {
        std::fs::write(&path, &file.content).map_err(|error| error.to_string())?;
        let content = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let name = Path::new(&file.relative)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| file.relative.clone());
        written.push(SourceEntry {
            path: path.to_string_lossy().into_owned(),
            relative: file.relative.clone(),
            name,
            content,
        });
    }
    Ok(WriteOutcome {
        written,
        conflicts: Vec::new(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ProjectState::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.emit("app://close-requested", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_languages,
            list_examples,
            select_project,
            current_project,
            load_example,
            read_source,
            write_source,
            create_source,
            compile_project,
            class_diagram,
            generate_code,
            write_generated_files,
            run_project
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    static NESTED_FIXTURE: Dir<'_> =
        include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/ejemplo_anidado");

    #[test]
    fn writes_nested_examples_completely() {
        let root = std::env::temp_dir().join(format!("idecode-anidado-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_example(&NESTED_FIXTURE, &root).unwrap();

        assert!(root.join("app.py").is_file(), "falta app.py");
        assert!(
            root.join("pkg").join("util.py").is_file(),
            "falta pkg/util.py"
        );
        assert!(
            root.join("datos").join("notas.txt").is_file(),
            "falta datos/notas.txt"
        );
        let content = std::fs::read_to_string(root.join("pkg").join("util.py")).unwrap();
        assert!(content.contains("def doble"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_embedded_example_lands_flat_in_the_project_root() {
        let root = std::env::temp_dir().join(format!("idecode-plano-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let example = EXAMPLES.get_dir("zoologico").expect("ejemplo zoologico");
        write_example(example, &root).unwrap();

        assert!(root.join("Main.java").is_file());
        assert!(
            !root.join("zoologico").exists(),
            "el ejemplo no debe anidarse en una subcarpeta extra"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plans_sources_by_extension_in_empty_folders() {
        let root = std::env::temp_dir().join(format!("idecode-vacia-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let (path, name, content) = plan_new_source(&root, "modulo.py").unwrap();
        assert_eq!(name, "modulo.py");
        assert_eq!(path, root.join("modulo.py"));
        assert_eq!(content, "def modulo():\n    pass\n");

        let (_, _, java) = plan_new_source(&root, "Clase.java").unwrap();
        assert_eq!(java, "public class Clase {\n}\n");

        let (_, _, kotlin) = plan_new_source(&root, "Clase.kt").unwrap();
        assert_eq!(kotlin, "class Clase {\n}\n");

        let error = plan_new_source(&root, "notas.txt").unwrap_err();
        assert!(error.contains(".java"), "mensaje: {error}");
        assert!(error.contains(".py"), "mensaje: {error}");
        assert!(error.contains(".kt"), "mensaje: {error}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_the_project_language_when_sources_exist() {
        let root = std::env::temp_dir().join(format!("idecode-mezcla-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("A.java"), "class A {}").unwrap();

        assert!(plan_new_source(&root, "Otra.java").is_ok());
        let error = plan_new_source(&root, "modulo.py").unwrap_err();
        assert!(error.contains("Para Java"), "mensaje: {error}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_new_names_with_paths() {
        assert!(validate_new_name("").is_err());
        assert!(validate_new_name("../fuera.java").is_err());
        assert!(validate_new_name("sub/Clase.java").is_err());
        assert_eq!(validate_new_name("  Clase.java ").unwrap(), "Clase.java");
    }

    #[test]
    fn serializes_language_info_in_camel_case() {
        let info = LanguageInfo {
            id: "python".into(),
            name: "Python".into(),
            extensions: vec!["py".into()],
            compile_label: "Comprobar sintaxis".into(),
            entry_label: "Módulo principal".into(),
        };
        let value = serde_json::to_value(&info).unwrap();
        let object = value.as_object().unwrap();
        assert!(object.contains_key("compileLabel"), "claves: {object:?}");
        assert!(object.contains_key("entryLabel"), "claves: {object:?}");
        assert!(!object.contains_key("compile_label"));
        assert!(!object.contains_key("entry_label"));
        assert_eq!(object["compileLabel"], "Comprobar sintaxis");
        assert_eq!(object["entryLabel"], "Módulo principal");
    }

    #[test]
    fn every_adapter_describes_its_own_labels() {
        for adapter in registry::all() {
            assert!(!adapter.compile_label().trim().is_empty());
            assert!(!adapter.entry_label().trim().is_empty());
        }
        let java = registry::all()
            .iter()
            .find(|adapter| adapter.id() == "java")
            .expect("adaptador java");
        assert_eq!(java.compile_label(), "Compilar");
        assert_eq!(java.entry_label(), "Clase principal");
        let python = registry::all()
            .iter()
            .find(|adapter| adapter.id() == "python")
            .expect("adaptador python");
        assert_eq!(python.compile_label(), "Comprobar sintaxis");
        assert_eq!(python.entry_label(), "Módulo principal");
    }

    #[test]
    fn refuses_paths_outside_the_project() {
        let root = std::env::temp_dir().join(format!("idecode-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let inside = root.join("Clase.java");
        std::fs::write(&inside, "class Clase {}").unwrap();

        let resolved = resolve(&root, inside.to_str().unwrap()).unwrap();
        assert!(resolved.starts_with(root.canonicalize().unwrap()));
        assert!(resolve(&root, "/etc/passwd").is_err());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plans_new_sources_through_the_adapter() {
        let root = std::env::temp_dir().join(format!("idecode-nueva-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Existente.java"), "class Existente {}").unwrap();

        let (path, name, content) = plan_new_source(&root, "Perro.java").unwrap();
        assert_eq!(name, "Perro.java");
        assert_eq!(path, root.join("Perro.java"));
        assert_eq!(content, "public class Perro {\n}\n");

        assert!(plan_new_source(&root, "notas.txt").is_err());
        assert!(plan_new_source(&root, "../fuera.java").is_err());
        assert!(plan_new_source(&root, "Mi Clase.java").is_err());
        assert!(plan_new_source(&root, "1Clase.java").is_err());
        assert!(plan_new_source(&root, "Clase_1$.java").is_ok());
        assert!(
            plan_new_source(&root, "modulo.py").is_err(),
            "un proyecto Java no admite ficheros sueltos de Python"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_folder_accepts_any_supported_language() {
        let root =
            std::env::temp_dir().join(format!("idecode-vacia-lenguaje-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let (_, name, content) = plan_new_source(&root, "modulo.py").unwrap();
        assert_eq!(name, "modulo.py");
        assert_eq!(content, "def modulo():\n    pass\n");

        let (_, name, content) = plan_new_source(&root, "Clase.java").unwrap();
        assert_eq!(name, "Clase.java");
        assert_eq!(content, "public class Clase {\n}\n");

        let error = plan_new_source(&root, "notas.txt").unwrap_err();
        assert!(error.contains(".java"), "mensaje: {error}");
        assert!(error.contains(".py"), "mensaje: {error}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolves_adapters_by_extension() {
        assert_eq!(registry::for_extension("Main.java").unwrap().id(), "java");
        assert_eq!(registry::for_extension("main.PY").unwrap().id(), "python");
        assert!(registry::for_extension("notas.txt").is_none());
    }

    #[test]
    fn writes_nested_examples_without_an_extra_folder() {
        use include_dir::include_dir;

        static FIXTURE: Dir<'_> =
            include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/ejemplo_anidado");

        let root =
            std::env::temp_dir().join(format!("idecode-anidado-plano-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_example(&FIXTURE, &root).unwrap();

        assert_eq!(
            std::fs::read_to_string(root.join("app.py")).unwrap(),
            "from pkg.util import doble\n\nprint(doble(21))\n"
        );
        assert!(root.join("pkg/util.py").is_file());
        assert!(root.join("datos/notas.txt").is_file());
        assert!(
            !root.join("ejemplo_anidado").exists(),
            "el ejemplo no debe anidarse bajo su propio nombre"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rewrites_missing_files_but_preserves_edits() {
        use include_dir::include_dir;

        static FIXTURE: Dir<'_> =
            include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/ejemplo_anidado");

        let root = std::env::temp_dir().join(format!("idecode-editado-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_example(&FIXTURE, &root).unwrap();

        std::fs::write(root.join("app.py"), "# edición del alumno\n").unwrap();
        std::fs::remove_file(root.join("pkg/util.py")).unwrap();
        write_example(&FIXTURE, &root).unwrap();

        assert_eq!(
            std::fs::read_to_string(root.join("app.py")).unwrap(),
            "# edición del alumno\n",
            "las ediciones del alumno se conservan"
        );
        assert!(
            root.join("pkg/util.py").is_file(),
            "el fichero borrado se restaura"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_example_lives_in_a_version_scoped_folder() {
        let relative = example_relative_root("zoologico");
        let parts: Vec<String> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(parts, ["ejemplos", env!("CARGO_PKG_VERSION"), "zoologico"]);
    }

    #[test]
    fn lists_every_embedded_example() {
        let found = examples();
        assert!(!found.is_empty(), "no se encontró ningún ejemplo");

        let zoologico = found
            .iter()
            .find(|example| example.id == "zoologico")
            .expect("falta el ejemplo zoologico");
        assert_eq!(zoologico.name, "Zoologico");
        assert_eq!(zoologico.files, 6);

        let python = found
            .iter()
            .find(|example| example.id == "zoologico_python")
            .expect("falta el ejemplo zoologico_python");
        assert_eq!(python.name, "Zoologico Python");
        assert_eq!(python.files, 6);

        assert!(found.windows(2).all(|pair| pair[0].name <= pair[1].name));
    }

    #[test]
    fn a_python_example_compiles_runs_and_draws() {
        use crate::adapters::python::PythonAdapter;
        use crate::core::adapter::LanguageAdapter;
        use crate::runner::{self, Stream};
        use std::time::Duration;

        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-ejemplo-py-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let example = EXAMPLES
            .get_dir("zoologico_python")
            .expect("ejemplo zoologico_python");
        write_example(example, &root).unwrap();

        let adapter = PythonAdapter::new();
        let project = Project::load(&root, adapter.extensions()).unwrap();
        assert_eq!(project.sources.len(), count_files(example));

        let build = adapter.compile(&project).unwrap();
        assert!(
            build.success,
            "el ejemplo no compila: {:?}",
            build.diagnostics
        );

        let spec = adapter
            .run_spec(&project, None, &["hola".to_string()])
            .unwrap();
        let mut lines = Vec::new();
        let outcome =
            runner::run_streaming(&spec, Some(Duration::from_secs(20)), |stream, line| {
                if stream == Stream::Stdout {
                    lines.push(line);
                }
            })
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0), "spec: {spec:?}");
        assert!(
            lines.iter().any(|line| line.contains("Dra. Ruiz")),
            "salida capturada: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Argumentos recibidos: hola")),
            "salida capturada: {lines:?}"
        );

        let classes = adapter.analyze(&project).unwrap();
        assert!(classes.iter().all(|class| class.line > 0));
        let mermaid = diagram::to_mermaid(&classes);
        assert!(mermaid.contains("Animal <|-- Perro"));
        assert!(mermaid.contains("Animal <|-- Gato"));
        assert!(mermaid.contains("Cuidador \"1\" o-- \"0..*\" Animal : animales"));
        assert!(mermaid.contains("Perro \"1\" --> \"1\" Cuidador : cuidador"));
        assert!(mermaid.contains("<<module>>"));
        assert!(mermaid.contains("<<abstract>>"), "Animal hereda de ABC");
        assert!(!mermaid.contains("ABC <|--"), "ABC no se dibuja");
        assert!(
            mermaid.contains("str sonido()*"),
            "método abstracto marcado"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn derives_readable_names_from_folder_names() {
        assert_eq!(example_name("zoologico"), "Zoologico");
        assert_eq!(example_name("mi-primer-programa"), "Mi Primer Programa");
        assert_eq!(example_name("listas_enlazadas"), "Listas Enlazadas");
        assert_eq!(example_name(""), "");
    }

    #[test]
    fn refuses_unknown_or_traversing_example_ids() {
        assert!(valid_example_id("zoologico"));
        assert!(valid_example_id("mi-primer-programa"));
        assert!(!valid_example_id(""));
        assert!(!valid_example_id("../fuera"));
        assert!(!valid_example_id("sub/ejemplo"));
        assert!(!valid_example_id("sub\\ejemplo"));
    }

    #[test]
    fn the_example_compiles_runs_and_draws_when_a_jdk_is_available() {
        use crate::adapters::java::JavaAdapter;
        use crate::core::adapter::LanguageAdapter;
        use crate::runner::{self, Stream};
        use std::time::Duration;

        if std::process::Command::new("javac")
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-ejemplo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let example = EXAMPLES.get_dir("zoologico").expect("ejemplo zoologico");
        write_example(example, &root).unwrap();

        let adapter = JavaAdapter::new();
        let project = Project::load(&root, adapter.extensions()).unwrap();
        assert_eq!(project.sources.len(), example.files().count());

        let build = adapter.compile(&project).unwrap();
        assert!(
            build.success,
            "el ejemplo no compila: {:?}",
            build.diagnostics
        );

        let spec = adapter
            .run_spec(&project, None, &["hola".to_string()])
            .unwrap();
        let mut lines = Vec::new();
        let outcome =
            runner::run_streaming(&spec, Some(Duration::from_secs(20)), |stream, line| {
                if stream == Stream::Stdout {
                    lines.push(line);
                }
            })
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0), "spec: {spec:?}");
        assert!(
            lines.iter().any(|line| line.contains("Dra. Ruiz")),
            "salida capturada: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Argumentos recibidos: hola")),
            "salida capturada: {lines:?}"
        );

        let classes = adapter.analyze(&project).unwrap();
        assert!(classes.iter().all(|class| class.line > 0));
        let mermaid = diagram::to_mermaid(&classes);
        assert!(mermaid.contains("<<abstract>>"));
        assert!(mermaid.contains("Animal <|-- Perro"));
        assert!(mermaid.contains("Mascota <|.. Gato"));
        assert!(mermaid.contains("Veterinario \"1\" o-- \"0..*\" Animal : pacientes"));
        assert!(mermaid.contains("Main ..> Perro"));
        assert!(mermaid.contains("Main ..> Mascota"));
        assert!(mermaid.contains("Main ..> Veterinario"));

        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod generated_paths_tests {
    use super::*;

    fn file(relative: &str) -> GeneratedFile {
        GeneratedFile {
            relative: relative.to_string(),
            content: String::new(),
        }
    }

    #[test]
    fn plan_generated_rejects_unsafe_names() {
        let root = std::env::temp_dir();
        assert!(plan_generated(&root, &[file("../escape.java")]).is_err());
        assert!(plan_generated(&root, &[file("sub/Perro.java")]).is_err());
        assert!(plan_generated(&root, &[file("..")]).is_err());
        assert!(plan_generated(&root, &[file("")]).is_err());

        let files = [file("Perro.java")];
        let targets = plan_generated(&root, &files).unwrap();
        assert_eq!(targets.len(), 1);
        assert!(targets[0].0.ends_with("Perro.java"));
    }
}

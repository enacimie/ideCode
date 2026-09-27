use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::adapter::AdapterError;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".idecode",
    "target",
    "build",
    "out",
    "node_modules",
    "dist",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub relative: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    pub sources: Vec<SourceFile>,
}

impl Project {
    pub fn load(root: &Path, extensions: &[&str]) -> Result<Self, AdapterError> {
        let root = root.canonicalize().map_err(|error| {
            AdapterError::Invalid(format!("No se pudo abrir la carpeta del proyecto: {error}"))
        })?;

        let mut sources = Vec::new();
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|entry| entry.depth() == 0 || !is_ignored_dir(entry))
        {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !extensions.iter().any(|ext| has_extension(path, ext)) {
                continue;
            }
            let Ok(content) = fs::read_to_string(path) else {
                continue;
            };
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            sources.push(SourceFile {
                path: path.to_path_buf(),
                relative,
                content,
            });
        }

        sources.sort_by(|a, b| a.relative.cmp(&b.relative));
        Ok(Project { root, sources })
    }
}

fn is_ignored_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.') || IGNORED_DIRS.contains(&name))
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "class A {}").unwrap();
    }

    #[test]
    fn loads_sources_even_when_the_root_has_a_reserved_name() {
        let base = std::env::temp_dir().join(format!("idecode-project-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        for name in ["out", "build", "target", "dist", ".oculto"] {
            let root = base.join(name);
            write(&root, "A.java");
            let project = Project::load(&root, &["java"]).unwrap();
            assert_eq!(project.sources.len(), 1, "falló con la carpeta «{name}»");
        }

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn skips_generated_and_hidden_directories_below_the_root() {
        let root = std::env::temp_dir().join(format!("idecode-skip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write(&root, "A.java");
        write(&root, "src/B.java");
        write(&root, "target/C.java");
        write(&root, "node_modules/D.java");
        write(&root, ".git/E.java");

        let project = Project::load(&root, &["java"]).unwrap();
        let relatives: Vec<&str> = project
            .sources
            .iter()
            .map(|source| source.relative.as_str())
            .collect();
        assert_eq!(relatives, ["A.java", "src/B.java"]);

        let _ = fs::remove_dir_all(&root);
    }
}

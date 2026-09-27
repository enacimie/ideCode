use std::path::Path;
use std::sync::OnceLock;

use walkdir::WalkDir;

use super::adapter::LanguageAdapter;
use crate::adapters::java::JavaAdapter;
use crate::adapters::kotlin::KotlinAdapter;
use crate::adapters::python::PythonAdapter;

static ADAPTERS: OnceLock<Vec<Box<dyn LanguageAdapter>>> = OnceLock::new();

fn adapters() -> &'static [Box<dyn LanguageAdapter>] {
    ADAPTERS.get_or_init(|| {
        vec![
            Box::new(JavaAdapter::new()),
            Box::new(PythonAdapter::new()),
            Box::new(KotlinAdapter::new()),
        ]
    })
}

pub fn all() -> &'static [Box<dyn LanguageAdapter>] {
    adapters()
}

pub fn for_extension(name: &str) -> Option<&'static dyn LanguageAdapter> {
    adapters()
        .iter()
        .map(AsRef::as_ref)
        .find(|adapter| adapter.accepts(name))
}

pub fn detected(root: &Path) -> Option<&'static dyn LanguageAdapter> {
    let mut best: Option<(&'static dyn LanguageAdapter, usize)> = None;
    for adapter in adapters() {
        let count = count_source_files(root, adapter.extensions());
        if count > 0 && best.is_none_or(|(_, previous)| count > previous) {
            best = Some((adapter.as_ref(), count));
        }
    }
    best.map(|(adapter, _)| adapter)
}

pub fn for_project(root: &Path) -> Option<&'static dyn LanguageAdapter> {
    detected(root).or_else(|| {
        adapters()
            .iter()
            .find(|adapter| adapter.id() == "java")
            .map(AsRef::as_ref)
    })
}

fn count_source_files(root: &Path, extensions: &[&str]) -> usize {
    WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| {
                    extensions
                        .iter()
                        .any(|extension| extension.eq_ignore_ascii_case(value))
                })
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_adapters_by_file_extension() {
        assert_eq!(
            for_extension("Main.java").map(|adapter| adapter.id()),
            Some("java")
        );
        assert_eq!(
            for_extension("app.PY").map(|adapter| adapter.id()),
            Some("python")
        );
        assert_eq!(
            for_extension("App.kt").map(|adapter| adapter.id()),
            Some("kotlin")
        );
        assert_eq!(for_extension("script.rb").map(|adapter| adapter.id()), None);
        assert_eq!(
            for_extension("sinextension").map(|adapter| adapter.id()),
            None
        );
    }

    #[test]
    fn detects_nothing_in_an_empty_folder_but_for_project_falls_back_to_java() {
        let root = std::env::temp_dir().join(format!("idecode-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        assert!(detected(&root).is_none());
        assert_eq!(for_project(&root).map(|adapter| adapter.id()), Some("java"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_the_predominant_language() {
        let root =
            std::env::temp_dir().join(format!("idecode-registry-mix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.py"), "x = 1\n").unwrap();
        std::fs::write(root.join("b.py"), "y = 2\n").unwrap();
        std::fs::write(root.join("C.java"), "class C {}").unwrap();

        assert_eq!(detected(&root).map(|adapter| adapter.id()), Some("python"));

        let _ = std::fs::remove_dir_all(&root);
    }
}

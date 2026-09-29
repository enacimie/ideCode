//! Helpers compartidos para diagnósticos y colecciones de los adaptadores.
//!
//! `shorten` presenta las rutas de los diagnósticos de forma relativa al
//! proyecto; `sorted_unique` ordena y deduplica listas como los `uses`.

use std::path::Path;

/// Ruta relativa al proyecto para mostrar en un diagnóstico.
///
/// Devuelve `None` si alguna ruta no existe o queda fuera del proyecto;
/// los adaptadores usan entonces la ruta original como reserva.
pub fn shorten(path: &str, root: &Path) -> Option<String> {
    let canonical = Path::new(path).canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    canonical
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

/// Ordena y deduplica una lista, descartando cadenas vacías.
pub fn sorted_unique(values: Vec<String>) -> Vec<String> {
    let mut collected: Vec<String> = values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect();
    collected.sort();
    collected.dedup();
    collected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_dedups_and_drops_empty_values() {
        assert_eq!(
            sorted_unique(vec![
                "b".to_string(),
                String::new(),
                "a".to_string(),
                "b".to_string(),
            ]),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(sorted_unique(Vec::new()).is_empty());
    }

    #[test]
    fn shortens_paths_inside_the_project_root() {
        let root = std::env::temp_dir().join(format!("idecode-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        let file = root.join("src").join("Main.java");
        std::fs::write(&file, "class Main {}").unwrap();

        assert_eq!(
            shorten(file.to_str().unwrap(), &root).as_deref(),
            Some("src/Main.java")
        );
        assert!(shorten("/no/existe.java", &root).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }
}

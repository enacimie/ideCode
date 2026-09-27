use std::env;
use std::path::PathBuf;

pub fn executable_extensions() -> Vec<String> {
    if cfg!(windows) {
        let raw = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
        let mut extensions: Vec<String> = raw
            .split(';')
            .map(|extension| extension.trim().to_string())
            .filter(|extension| !extension.is_empty())
            .collect();
        if !extensions
            .iter()
            .any(|extension| extension.eq_ignore_ascii_case(".exe"))
        {
            extensions.push(".EXE".to_string());
        }
        extensions.push(String::new());
        extensions
    } else {
        vec![String::new()]
    }
}

pub fn path_dirs() -> Vec<PathBuf> {
    env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect()
}

pub fn find_first(names: &[&str], dirs: &[PathBuf], extensions: &[String]) -> Option<PathBuf> {
    for name in names {
        for dir in dirs {
            for extension in extensions {
                let candidate = dir.join(format!("{name}{extension}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

pub fn find_program(names: &[&str], extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let extensions = executable_extensions();
    let mut dirs = extra_dirs.to_vec();
    dirs.extend(path_dirs());
    find_first(names, &dirs, &extensions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("idecode-tools-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn prefers_the_first_name_that_exists() {
        let root = temp_root("orden");
        fs::write(root.join("python"), "").unwrap();

        let found = find_first(
            &["python3", "python"],
            std::slice::from_ref(&root),
            &["".to_string()],
        );
        assert_eq!(found, Some(root.join("python")));

        fs::write(root.join("python3"), "").unwrap();
        let found = find_first(
            &["python3", "python"],
            std::slice::from_ref(&root),
            &["".to_string()],
        );
        assert_eq!(found, Some(root.join("python3")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolves_windows_extensions() {
        let root = temp_root("windows");
        fs::write(root.join("python.exe"), "").unwrap();
        fs::write(root.join("javac.bat"), "").unwrap();

        let extensions: Vec<String> = if cfg!(windows) {
            vec![".EXE".to_string(), ".BAT".to_string(), String::new()]
        } else {
            vec![".exe".to_string(), ".bat".to_string(), String::new()]
        };

        let found = find_first(&["python"], std::slice::from_ref(&root), &extensions);
        assert_eq!(found, Some(root.join("python.exe")));

        let found = find_first(&["javac"], std::slice::from_ref(&root), &extensions);
        assert_eq!(found, Some(root.join("javac.bat")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn searches_directories_in_order() {
        let root = temp_root("dirs");
        let first = root.join("primero");
        let second = root.join("segundo");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("java"), "").unwrap();

        assert_eq!(
            find_first(
                &["java"],
                &[first.clone(), second.clone()],
                &["".to_string()]
            ),
            Some(second.join("java"))
        );
        fs::write(first.join("java"), "").unwrap();
        assert_eq!(
            find_first(
                &["java"],
                &[first.clone(), second.clone()],
                &["".to_string()]
            ),
            Some(first.join("java"))
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_directories_and_missing_names() {
        let root = temp_root("faltan");
        fs::create_dir_all(root.join("python3")).unwrap();
        assert_eq!(
            find_first(&["python3"], std::slice::from_ref(&root), &["".to_string()]),
            None,
            "un directorio no cuenta como ejecutable"
        );
        assert_eq!(
            find_first(&["ruby"], std::slice::from_ref(&root), &["".to_string()]),
            None
        );

        let _ = fs::remove_dir_all(&root);
    }
}

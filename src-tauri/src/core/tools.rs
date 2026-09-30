use std::env;
use std::path::{Path, PathBuf};

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
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
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

    fn make_executable(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
        #[cfg(not(unix))]
        {
            let _ = path;
        }
    }

    fn write_program(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "").unwrap();
        make_executable(&path);
        path
    }

    #[test]
    fn prefers_the_first_name_that_exists() {
        let root = temp_root("orden");
        write_program(&root, "python");

        let found = find_first(
            &["python3", "python"],
            std::slice::from_ref(&root),
            &["".to_string()],
        );
        assert_eq!(found, Some(root.join("python")));

        write_program(&root, "python3");
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
        write_program(&root, "python.exe");
        write_program(&root, "javac.bat");

        let extensions: Vec<String> = vec![".exe".to_string(), ".bat".to_string(), String::new()];

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
        write_program(&second, "java");

        assert_eq!(
            find_first(
                &["java"],
                &[first.clone(), second.clone()],
                &["".to_string()]
            ),
            Some(second.join("java"))
        );
        write_program(&first, "java");
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

    #[cfg(unix)]
    #[test]
    fn ignores_files_without_execute_permission() {
        let root = temp_root("permisos");
        let path = root.join("python3");
        fs::write(&path, "").unwrap();

        assert_eq!(
            find_first(&["python3"], std::slice::from_ref(&root), &["".to_string()]),
            None,
            "un fichero sin permiso de ejecución no debe usarse"
        );

        make_executable(&path);
        assert_eq!(
            find_first(&["python3"], std::slice::from_ref(&root), &["".to_string()]),
            Some(path)
        );

        let _ = fs::remove_dir_all(&root);
    }
}

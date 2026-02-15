use std::env;
use std::path::{Path, PathBuf};

pub fn get_base_dir() -> PathBuf {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if has_config_candidate(&exe_dir) {
        return exe_dir;
    }

    if let Ok(cwd) = env::current_dir() {
        if has_config_candidate(&cwd) {
            return cwd;
        }
    }

    exe_dir
}

pub fn resolve_config_path(raw: Option<String>, base_dir: &Path) -> PathBuf {
    if let Some(path) = raw {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return path;
        }
        if let Ok(cwd) = env::current_dir() {
            return cwd.join(path);
        }
        return path;
    }

    let candidates = [
        base_dir.join("config.txt"),
        base_dir.join("config").join("config.txt"),
    ];
    for path in candidates {
        if path.exists() {
            return path;
        }
    }

    base_dir.join("config.txt")
}

fn has_config_candidate(base_dir: &Path) -> bool {
    base_dir.join("config.txt").exists() || base_dir.join("config").join("config.txt").exists()
}

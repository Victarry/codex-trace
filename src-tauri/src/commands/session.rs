use std::sync::Arc;
use std::{
    fs,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, State};

use crate::parser::session::parse_session;
use crate::state::AppState;
use crate::watcher::start_session_watcher;

pub const NO_SESSION_PATH_PROVIDED: &str = "no session path provided";

const SESSION_FILE_NOT_FOUND: &str = "session file does not exist";

pub fn load_session_from_path(path: &str) -> Result<crate::parser::session::CodexSession, String> {
    if path.is_empty() {
        return Err(NO_SESSION_PATH_PROVIDED.to_string());
    }
    let p = std::path::Path::new(path);
    parse_session(p)
}

#[tauri::command]
pub async fn load_session(path: String) -> Result<crate::parser::session::CodexSession, String> {
    load_session_from_path(&path)
}

/// Delete a session rollout from the configured local or remote sessions tree.
/// The caller must provide the source directory so deletion can be constrained
/// to the same tree that produced the picker entry.
pub fn delete_session_from_path(sessions_dir: &str, path: &str) -> Result<(), String> {
    if sessions_dir.is_empty() || path.is_empty() {
        return Err(NO_SESSION_PATH_PROVIDED.to_string());
    }
    let sessions_remote = crate::parser::remote::is_remote_spec(sessions_dir);
    let path_remote = crate::parser::remote::is_remote_spec(path);
    if sessions_remote != path_remote {
        return Err(
            "session source and session path must use the same connection type".to_string(),
        );
    }
    if sessions_remote {
        return crate::parser::remote::delete_remote_session(sessions_dir, path);
    }
    delete_local_session(sessions_dir, path)
}

fn delete_local_session(sessions_dir: &str, path: &str) -> Result<(), String> {
    let root =
        fs::canonicalize(sessions_dir).map_err(|e| format!("resolve sessions directory: {e}"))?;
    if !root.is_dir() {
        return Err("configured sessions path is not a directory".to_string());
    }

    let variants = local_rollout_variants(Path::new(path))?;
    let mut existing = Vec::new();
    for candidate in variants {
        if !candidate.exists() {
            continue;
        }
        let canonical =
            fs::canonicalize(&candidate).map_err(|e| format!("resolve session file: {e}"))?;
        if !canonical.starts_with(&root) {
            return Err("session path is outside the configured sessions directory".to_string());
        }
        if !canonical.is_file() {
            return Err("session path is not a file".to_string());
        }
        existing.push(candidate);
    }

    if existing.is_empty() {
        return Err(SESSION_FILE_NOT_FOUND.to_string());
    }
    for candidate in existing {
        fs::remove_file(&candidate).map_err(|e| format!("delete session file: {e}"))?;
    }
    Ok(())
}

fn local_rollout_variants(path: &Path) -> Result<[PathBuf; 2], String> {
    if !path.is_absolute() {
        return Err("session path must be absolute".to_string());
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "session path has no file name".to_string())?;
    if !name.starts_with("rollout-") {
        return Err("session path must point to a rollout file".to_string());
    }
    if let Some(plain) = name.strip_suffix(".jsonl.zst") {
        return Ok([
            path.with_file_name(format!("{plain}.jsonl")),
            path.to_path_buf(),
        ]);
    }
    if name.ends_with(".jsonl") {
        return Ok([
            path.to_path_buf(),
            path.with_file_name(format!("{name}.zst")),
        ]);
    }
    Err("session path must end in .jsonl or .jsonl.zst".to_string())
}

#[tauri::command]
pub async fn watch_session(
    path: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let session = load_session_from_path(&path)?;
    state.stop_session_watcher()?;
    state.set_watched_ongoing(path.clone(), session.is_ongoing);
    let handle = start_session_watcher(path, state.inner().clone(), Some(app));
    state.set_session_watcher(handle)
}

#[tauri::command]
pub async fn delete_session(
    sessions_dir: String,
    path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    delete_session_from_path(&sessions_dir, &path)?;
    state.stop_session_watcher()?;
    state.clear_watched_ongoing();
    state.invalidate_sessions_cache();
    Ok(())
}

#[tauri::command]
pub async fn unwatch_session(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.clear_watched_ongoing();
    state.stop_session_watcher()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_session_from_path_rejects_empty_path() {
        let result = load_session_from_path("");

        assert_eq!(result.unwrap_err(), "no session path provided");
    }

    #[test]
    fn deletes_local_rollout_and_compressed_sibling_inside_sessions_dir() {
        let root = tempfile::tempdir().unwrap();
        let day = root.path().join("2026/09/07");
        fs::create_dir_all(&day).unwrap();
        let plain = day.join("rollout-example.jsonl");
        let compressed = day.join("rollout-example.jsonl.zst");
        fs::write(&plain, "plain").unwrap();
        fs::write(&compressed, "compressed").unwrap();

        delete_session_from_path(root.path().to_str().unwrap(), plain.to_str().unwrap()).unwrap();
        assert!(!plain.exists());
        assert!(!compressed.exists());
    }

    #[test]
    fn rejects_local_rollout_outside_sessions_dir() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("rollout-outside.jsonl");
        fs::write(&path, "outside").unwrap();

        let error = delete_session_from_path(root.path().to_str().unwrap(), path.to_str().unwrap())
            .unwrap_err();
        assert!(error.contains("outside") || error.contains("does not exist"));
        assert!(path.exists());
    }
}

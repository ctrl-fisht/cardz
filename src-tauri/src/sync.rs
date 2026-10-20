use crate::commands::AppState;
use crate::vault::now_ms;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const IGNORE_BLOCK: &str = "\n# Cardz\n.cardz/\n*.db\n*.db-shm\n*.db-wal\n*.crdz\n";
const AUTO_DEBOUNCE_MS: i64 = 1_000;
const AUTO_RETRY_MS: i64 = 60_000;

static GIT_AVAILABLE: OnceLock<bool> = OnceLock::new();
static CONFIG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    #[serde(default)]
    pub remote: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default)]
    pub auto_sync: bool,
    #[serde(default)]
    pub last_sync: Option<i64>,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Serialize, Deserialize, Default)]
struct SyncConfigFile {
    #[serde(default)]
    vaults: HashMap<String, SyncConfig>,
}

#[derive(Default)]
pub struct SyncRuntime {
    pub busy: bool,
    pub last_error: Option<String>,
    pub vault_key: String,
    pub auto_signature: String,
    pub auto_since: i64,
    pub retry_after: i64,
    pub startup_pull_done: bool,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub configured: bool,
    pub git_available: bool,
    pub repo: bool,
    pub remote: Option<String>,
    pub branch: String,
    pub auto_sync: bool,
    pub dirty: bool,
    pub ahead: i64,
    pub behind: i64,
    pub conflicts: Vec<String>,
    pub last_sync: Option<i64>,
    pub error: Option<String>,
    pub syncing: bool,
}

#[derive(Clone, Copy)]
enum SyncMode {
    Full,
    Pull,
}

fn git_available() -> bool {
    *GIT_AVAILABLE.get_or_init(|| {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    })
}

fn run_git_raw(
    root: &Path,
    args: &[&str],
    allow_prompt: bool,
) -> Result<(bool, String, String), String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(args);
    if !allow_prompt {
        command.env("GIT_TERMINAL_PROMPT", "0");
        command.env("GCM_INTERACTIVE", "Never");
    }
    let output = command
        .output()
        .map_err(|err| format!("Не удалось запустить git: {}", err))?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

fn run_git(root: &Path, args: &[&str], allow_prompt: bool) -> Result<String, String> {
    let (ok, stdout, stderr) = run_git_raw(root, args, allow_prompt)?;
    if ok {
        Ok(stdout)
    } else {
        let message = format!("{}{}", stdout, stderr);
        let message = message.trim();
        Err(if message.is_empty() {
            "git завершился с ошибкой".to_string()
        } else {
            message.to_string()
        })
    }
}

fn run_git_ok(root: &Path, args: &[&str]) -> Option<String> {
    run_git_raw(root, args, false)
        .ok()
        .filter(|(ok, _, _)| *ok)
        .map(|(_, stdout, _)| stdout)
}

fn has_repo(root: &Path) -> bool {
    root.join(".git").exists()
}

fn root_key(root: &Path) -> String {
    root.canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("Не удалось найти папку конфигурации: {}", err))?;
    Ok(dir.join("sync.json"))
}

fn read_config_file(app: &AppHandle) -> SyncConfigFile {
    let Ok(path) = config_path(app) else {
        return SyncConfigFile::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn load_config(app: &AppHandle, root: &Path) -> Option<SyncConfig> {
    read_config_file(app).vaults.get(&root_key(root)).cloned()
}

fn save_config(app: &AppHandle, root: &Path, config: &SyncConfig) -> Result<(), String> {
    let _guard = CONFIG_LOCK
        .lock()
        .map_err(|_| "Ошибка блокировки конфигурации".to_string())?;
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = read_config_file(app);
    file.vaults.insert(root_key(root), config.clone());
    fs::write(
        path,
        serde_json::to_string_pretty(&file).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

fn remove_config(app: &AppHandle, root: &Path) -> Result<(), String> {
    let _guard = CONFIG_LOCK
        .lock()
        .map_err(|_| "Ошибка блокировки конфигурации".to_string())?;
    let path = config_path(app)?;
    let mut file = read_config_file(app);
    file.vaults.remove(&root_key(root));
    fs::write(
        path,
        serde_json::to_string_pretty(&file).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

fn ensure_gitignore(root: &Path) -> Result<(), String> {
    let path = root.join(".gitignore");
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    if content.contains(".cardz/") {
        return Ok(());
    }
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(IGNORE_BLOCK);
    fs::write(path, content).map_err(|err| err.to_string())
}

fn ensure_identity(root: &Path) {
    let has_email = run_git_ok(root, &["config", "user.email"])
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if has_email {
        return;
    }
    let _ = run_git(root, &["config", "user.name", "Cardz"], true);
    let _ = run_git(root, &["config", "user.email", "cardz@localhost"], true);
}

fn ensure_repo(root: &Path, branch: &str) -> Result<(), String> {
    if !has_repo(root) {
        run_git(root, &["init", "-b", branch], true)?;
    }
    ensure_gitignore(root)?;
    ensure_identity(root);
    if run_git_ok(root, &["rev-parse", "--verify", "HEAD"]).is_none() {
        let _ = run_git(root, &["checkout", "-B", branch], true);
        ensure_identity(root);
    }
    Ok(())
}

fn set_remote(root: &Path, remote: &str) -> Result<(), String> {
    if run_git_ok(root, &["remote", "get-url", "origin"]).is_some() {
        run_git(root, &["remote", "set-url", "origin", remote], true)?;
    } else {
        run_git(root, &["remote", "add", "origin", remote], true)?;
    }
    Ok(())
}

fn commit_all(root: &Path, message: &str, allow_prompt: bool) -> Result<(), String> {
    let dirty = !run_git_ok(root, &["status", "--porcelain"])
        .unwrap_or_default()
        .trim()
        .is_empty();
    if !dirty {
        return Ok(());
    }
    run_git(root, &["add", "-A"], allow_prompt)?;
    let (ok, stdout, stderr) = run_git_raw(root, &["commit", "-m", message], allow_prompt)?;
    if !ok {
        let text = format!("{}{}", stdout, stderr).to_lowercase();
        if text.contains("nothing to commit") {
            return Ok(());
        }
        return Err(format!("{}{}", stdout, stderr).trim().to_string());
    }
    Ok(())
}

fn ensure_initial_commit(root: &Path, allow_prompt: bool) -> Result<(), String> {
    if run_git_ok(root, &["rev-parse", "--verify", "HEAD"]).is_some() {
        return commit_all(root, "cardz: local changes", allow_prompt);
    }
    run_git(root, &["add", "-A"], allow_prompt)?;
    let (ok, stdout, stderr) = run_git_raw(
        root,
        &["commit", "--allow-empty", "-m", "cardz: initial commit"],
        allow_prompt,
    )?;
    if !ok {
        return Err(format!("{}{}", stdout, stderr).trim().to_string());
    }
    Ok(())
}

fn fetch_origin(root: &Path, allow_prompt: bool) -> Result<(), String> {
    run_git(root, &["fetch", "origin", "--prune"], allow_prompt).map(|_| ())
}

fn remote_ref_exists(root: &Path, branch: &str) -> bool {
    run_git_ok(
        root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/remotes/origin/{}", branch),
        ],
    )
    .is_some()
}

fn merge_remote(
    root: &Path,
    branch: &str,
    allow_prompt: bool,
    allow_unrelated: bool,
) -> Result<bool, String> {
    if !remote_ref_exists(root, branch) {
        return Ok(false);
    }
    let reference = format!("origin/{}", branch);
    let mut args = vec!["merge", "--no-edit"];
    if allow_unrelated {
        args.push("--allow-unrelated-histories");
    }
    args.push(&reference);
    let (ok, stdout, stderr) = run_git_raw(root, &args, allow_prompt)?;
    if !ok {
        if !conflict_paths(root).is_empty() {
            return Ok(true);
        }
        return Err(format!("{}{}", stdout, stderr).trim().to_string());
    }
    Ok(false)
}

fn push_origin(root: &Path, branch: &str, allow_prompt: bool) -> Result<(), String> {
    run_git(
        root,
        &["push", "-u", "origin", &format!("HEAD:{}", branch)],
        allow_prompt,
    )
    .map(|_| ())
}

fn conflict_paths(root: &Path) -> Vec<String> {
    run_git_ok(root, &["diff", "--name-only", "--diff-filter=U"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn ahead_behind(root: &Path, branch: &str) -> (i64, i64) {
    if run_git_ok(root, &["rev-parse", "--verify", "HEAD"]).is_none() {
        return (0, 0);
    }
    let upstream = format!("origin/{}", branch);
    if run_git_ok(root, &["rev-parse", "--verify", &upstream]).is_none() {
        return (0, 0);
    }
    let output = run_git_ok(
        root,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{}", upstream),
        ],
    )
    .unwrap_or_default();
    let mut parts = output.split_whitespace();
    let ahead = parts.next().and_then(|value| value.parse().ok()).unwrap_or(0);
    let behind = parts.next().and_then(|value| value.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

fn status_for(app: &AppHandle, root: &Path, state: &AppState) -> SyncStatus {
    let config = load_config(app, root);
    let (busy, last_error) = state
        .sync
        .lock()
        .map(|runtime| (runtime.busy, runtime.last_error.clone()))
        .unwrap_or((false, None));
    let git = git_available();
    let repo = has_repo(root);
    let branch = config
        .as_ref()
        .map(|value| value.branch.clone())
        .unwrap_or_else(|| "main".to_string());

    let mut status = SyncStatus {
        configured: config.is_some(),
        git_available: git,
        repo,
        remote: config.as_ref().map(|value| value.remote.clone()),
        branch: branch.clone(),
        auto_sync: config.as_ref().map(|value| value.auto_sync).unwrap_or(false),
        last_sync: config.as_ref().and_then(|value| value.last_sync),
        syncing: busy,
        ..SyncStatus::default()
    };

    if git && repo {
        status.dirty = !run_git_ok(root, &["status", "--porcelain"])
            .unwrap_or_default()
            .trim()
            .is_empty();
        status.conflicts = conflict_paths(root);
        let (ahead, behind) = ahead_behind(root, &branch);
        status.ahead = ahead;
        status.behind = behind;
    }

    status.error = if !git {
        Some("git не найден. Установите Git и перезапустите приложение.".to_string())
    } else if let Some(error) = last_error {
        Some(error)
    } else if status.configured && !repo {
        Some("Репозиторий ещё не инициализирован".to_string())
    } else {
        None
    };
    status
}

fn current_root(state: &AppState) -> Result<PathBuf, String> {
    state
        .vault
        .lock()
        .map_err(|_| "Внутренняя ошибка состояния".to_string())?
        .as_ref()
        .map(|vault| vault.root.clone())
        .ok_or_else(|| "Хранилище не открыто".to_string())
}

fn start_busy(state: &AppState) -> Result<(), String> {
    let mut runtime = state
        .sync
        .lock()
        .map_err(|_| "Внутренняя ошибка синхронизации".to_string())?;
    if runtime.busy {
        return Err("Синхронизация уже выполняется".to_string());
    }
    runtime.busy = true;
    runtime.last_error = None;
    Ok(())
}

fn finish_busy(state: &AppState, error: Option<String>) {
    if let Ok(mut runtime) = state.sync.lock() {
        runtime.busy = false;
        runtime.last_error = error;
        if runtime.last_error.is_none() {
            runtime.auto_signature.clear();
            runtime.auto_since = 0;
            runtime.retry_after = 0;
        } else {
            runtime.retry_after = now_ms() + AUTO_RETRY_MS;
        }
    }
}

fn emit_status(app: &AppHandle, status: &SyncStatus) {
    let _ = app.emit("sync://status", status.clone());
}

fn after_repo_change(app: &AppHandle, state: &AppState, root: &Path) {
    if !conflict_paths(root).is_empty() {
        return;
    }
    if let Ok(mut guard) = state.vault.lock() {
        if let Some(vault) = guard.as_mut() {
            let _ = vault.sync();
        }
    }
    let _ = app.emit("sync://updated", ());
}

fn validate_remote(remote: &str) -> Result<(), String> {
    let trimmed = remote.trim();
    if trimmed.is_empty() {
        return Err("Укажите URL репозитория".to_string());
    }
    if trimmed.chars().any(|ch| ch.is_whitespace()) {
        return Err("URL репозитория не должен содержать пробелы".to_string());
    }
    Ok(())
}

fn validate_branch(branch: &str) -> Result<(), String> {
    let trimmed = branch.trim();
    if trimmed.is_empty() {
        return Err("Укажите ветку".to_string());
    }
    if trimmed.starts_with('-')
        || trimmed.contains("..")
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
    {
        return Err("Недопустимое имя ветки".to_string());
    }
    Ok(())
}

fn perform_sync(
    app: &AppHandle,
    root: &Path,
    mode: SyncMode,
    allow_prompt: bool,
) -> Result<bool, String> {
    let mut config = load_config(app, root)
        .ok_or_else(|| "Синхронизация не настроена".to_string())?;
    if !git_available() {
        return Err("git не найден. Установите Git и перезапустите приложение.".to_string());
    }
    ensure_repo(root, &config.branch)?;

    if !conflict_paths(root).is_empty() {
        return Err("Сначала разрешите конфликты".to_string());
    }

    commit_all(root, "cardz: sync local changes", allow_prompt)?;
    fetch_origin(root, allow_prompt)?;

    let had_conflicts = merge_remote(root, &config.branch, allow_prompt, false)?;
    if had_conflicts {
        return Ok(true);
    }

    if matches!(mode, SyncMode::Full) {
        push_origin(root, &config.branch, allow_prompt)?;
    }

    config.last_sync = Some(now_ms());
    save_config(app, root, &config)?;
    Ok(false)
}

#[tauri::command]
pub fn sync_status(app: AppHandle, state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let root = current_root(&state)?;
    Ok(status_for(&app, &root, &state))
}

#[tauri::command]
pub fn sync_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: String,
    branch: String,
    auto_sync: bool,
) -> Result<SyncStatus, String> {
    let root = current_root(&state)?;
    let remote = remote.trim().to_string();
    let branch = branch.trim().to_string();
    validate_remote(&remote)?;
    validate_branch(&branch)?;
    start_busy(&state)?;

    let result = (|| -> Result<(), String> {
        ensure_repo(&root, &branch)?;
        set_remote(&root, &remote)?;
        ensure_initial_commit(&root, true)?;
        fetch_origin(&root, true)?;
        let conflicts = merge_remote(&root, &branch, true, true)?;

        let mut config = SyncConfig {
            remote: remote.clone(),
            branch: branch.clone(),
            auto_sync,
            last_sync: None,
        };
        save_config(&app, &root, &config)?;
        if conflicts {
            return Ok(());
        }
        push_origin(&root, &branch, true)?;
        config.last_sync = Some(now_ms());
        save_config(&app, &root, &config)?;
        Ok(())
    })();

    finish_busy(&state, result.as_ref().err().cloned());
    if result.is_ok() {
        after_repo_change(&app, &state, &root);
    }
    let status = status_for(&app, &root, &state);
    emit_status(&app, &status);
    result.map(|_| status)
}

fn manual_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: SyncMode,
) -> Result<SyncStatus, String> {
    let root = current_root(&state)?;
    start_busy(&state)?;
    let result = perform_sync(&app, &root, mode, true);
    let error = result.as_ref().err().cloned();
    finish_busy(&state, error.clone());

    if result.is_ok() {
        after_repo_change(&app, &state, &root);
    }
    let status = status_for(&app, &root, &state);
    emit_status(&app, &status);
    match result {
        Ok(true) => Ok(status),
        Ok(false) => Ok(status),
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub fn sync_now(app: AppHandle, state: State<'_, AppState>) -> Result<SyncStatus, String> {
    manual_sync(app, state, SyncMode::Full)
}

#[tauri::command]
pub fn sync_pull(app: AppHandle, state: State<'_, AppState>) -> Result<SyncStatus, String> {
    manual_sync(app, state, SyncMode::Pull)
}

#[tauri::command]
pub fn sync_disconnect(app: AppHandle, state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let root = current_root(&state)?;
    if let Ok(mut runtime) = state.sync.lock() {
        if runtime.busy {
            return Err("Дождитесь окончания синхронизации".to_string());
        }
        runtime.last_error = None;
        runtime.auto_signature.clear();
        runtime.auto_since = 0;
        runtime.retry_after = 0;
        runtime.startup_pull_done = false;
    }
    remove_config(&app, &root)?;
    let status = status_for(&app, &root, &state);
    emit_status(&app, &status);
    Ok(status)
}

#[tauri::command]
pub fn sync_resolve(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    side: String,
) -> Result<SyncStatus, String> {
    let root = current_root(&state)?;
    start_busy(&state)?;

    let result = (|| -> Result<(), String> {
        let conflicts = conflict_paths(&root);
        if !conflicts.iter().any(|item| item == &path) {
            return Err("Файл не найден среди конфликтов".to_string());
        }
        let flag = match side.as_str() {
            "local" | "ours" => "--ours",
            "remote" | "theirs" => "--theirs",
            _ => return Err("Неизвестная сторона конфликта".to_string()),
        };
        run_git(&root, &["checkout", flag, "--", &path], true)?;
        run_git(&root, &["add", "--", &path], true)?;
        if conflict_paths(&root).is_empty() {
            let (ok, stdout, stderr) =
                run_git_raw(&root, &["commit", "--no-edit", "-m", "cardz: resolve conflicts"], true)?;
            if !ok {
                let text = format!("{}{}", stdout, stderr).to_lowercase();
                if !text.contains("nothing to commit") {
                    return Err(format!("{}{}", stdout, stderr).trim().to_string());
                }
            }
        }
        Ok(())
    })();

    finish_busy(&state, result.as_ref().err().cloned());
    if result.is_ok() && conflict_paths(&root).is_empty() {
        after_repo_change(&app, &state, &root);
    }
    let status = status_for(&app, &root, &state);
    emit_status(&app, &status);
    result.map(|_| status)
}

#[tauri::command]
pub fn sync_abort(app: AppHandle, state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let root = current_root(&state)?;
    start_busy(&state)?;
    let result = run_git(&root, &["merge", "--abort"], true).map(|_| ());
    let error = result.as_ref().err().cloned();
    finish_busy(&state, error.clone());
    if result.is_ok() {
        after_repo_change(&app, &state, &root);
    }
    let status = status_for(&app, &root, &state);
    emit_status(&app, &status);
    result.map(|_| status)
}

pub fn spawn_auto_sync(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        loop {
            auto_tick(&app);
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn auto_tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(root) = state
        .vault
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|vault| vault.root.clone()))
    else {
        return;
    };
    let Some(config) = load_config(app, &root) else {
        return;
    };
    if !config.auto_sync || !git_available() || !has_repo(&root) {
        return;
    }
    if !conflict_paths(&root).is_empty() {
        return;
    }

    let now = now_ms();
    let key = root_key(&root);
    let need_startup;
    {
        let Ok(mut runtime) = state.sync.lock() else {
            return;
        };
        if runtime.vault_key != key {
            runtime.vault_key = key;
            runtime.startup_pull_done = false;
            runtime.auto_signature.clear();
            runtime.auto_since = 0;
            runtime.retry_after = 0;
            runtime.last_error = None;
        }
        if runtime.busy || now < runtime.retry_after {
            return;
        }
        need_startup = !runtime.startup_pull_done;
        if need_startup {
            runtime.startup_pull_done = true;
        }
    }

    if need_startup {
        run_auto(app, &state, &root, SyncMode::Full);
        return;
    }

    let status_text = run_git_ok(&root, &["status", "--porcelain"]).unwrap_or_default();
    let (ahead, _) = ahead_behind(&root, &config.branch);
    let signature = if status_text.trim().is_empty() {
        if ahead > 0 {
            "ahead".to_string()
        } else {
            String::new()
        }
    } else {
        status_text
    };
    if signature.is_empty() {
        if let Ok(mut runtime) = state.sync.lock() {
            runtime.auto_signature.clear();
            runtime.auto_since = 0;
        }
        return;
    }

    {
        let Ok(mut runtime) = state.sync.lock() else {
            return;
        };
        if runtime.busy || now < runtime.retry_after {
            return;
        }
        if runtime.auto_signature != signature {
            runtime.auto_signature = signature;
            runtime.auto_since = now;
            return;
        }
        if now - runtime.auto_since < AUTO_DEBOUNCE_MS {
            return;
        }
    }

    run_auto(app, &state, &root, SyncMode::Full);
}

fn run_auto(app: &AppHandle, state: &State<AppState>, root: &Path, mode: SyncMode) {
    if start_busy(state).is_err() {
        return;
    }
    let result = perform_sync(app, root, mode, false);
    let error = result.as_ref().err().cloned();
    finish_busy(state, error.clone());
    if result.is_ok() {
        after_repo_change(app, state, root);
    } else if let Ok(mut runtime) = state.sync.lock() {
        runtime.retry_after = now_ms() + AUTO_RETRY_MS;
    }
    let status = status_for(app, root, state);
    emit_status(app, &status);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cardz-sync-{}-{}", name, stamp));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn remote_url(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn git_roundtrip_and_conflict_resolution() {
        assert!(git_available(), "git is required for this test");
        let base = temp_root("roundtrip");
        let remote = base.join("remote.git");
        fs::create_dir_all(&remote).unwrap();
        run_git(&remote, &["init", "--bare", "-b", "main"], true).unwrap();

        let repo_a = base.join("a");
        fs::create_dir_all(&repo_a).unwrap();
        ensure_repo(&repo_a, "main").unwrap();
        fs::write(repo_a.join("deck.txt"), "one").unwrap();
        set_remote(&repo_a, &remote_url(&remote)).unwrap();
        ensure_initial_commit(&repo_a, true).unwrap();
        push_origin(&repo_a, "main", true).unwrap();

        let repo_b = base.join("b");
        fs::create_dir_all(&repo_b).unwrap();
        ensure_repo(&repo_b, "main").unwrap();
        fs::write(repo_b.join("other.txt"), "b").unwrap();
        set_remote(&repo_b, &remote_url(&remote)).unwrap();
        ensure_initial_commit(&repo_b, true).unwrap();
        fetch_origin(&repo_b, true).unwrap();
        assert!(!merge_remote(&repo_b, "main", true, true).unwrap());
        push_origin(&repo_b, "main", true).unwrap();

        fetch_origin(&repo_a, true).unwrap();
        assert!(!merge_remote(&repo_a, "main", true, false).unwrap());
        assert!(repo_a.join("other.txt").is_file());

        fs::write(repo_a.join("deck.txt"), "from-a").unwrap();
        commit_all(&repo_a, "a", true).unwrap();
        push_origin(&repo_a, "main", true).unwrap();

        fs::write(repo_b.join("deck.txt"), "from-b").unwrap();
        commit_all(&repo_b, "b", true).unwrap();
        fetch_origin(&repo_b, true).unwrap();
        assert!(merge_remote(&repo_b, "main", true, false).unwrap());
        assert!(conflict_paths(&repo_b).contains(&"deck.txt".to_string()));

        run_git(&repo_b, &["checkout", "--ours", "--", "deck.txt"], true).unwrap();
        run_git(&repo_b, &["add", "deck.txt"], true).unwrap();
        assert!(conflict_paths(&repo_b).is_empty());
        run_git(
            &repo_b,
            &["commit", "--no-edit", "-m", "resolve"],
            true,
        )
        .unwrap();
        push_origin(&repo_b, "main", true).unwrap();

        fetch_origin(&repo_a, true).unwrap();
        assert!(!merge_remote(&repo_a, "main", true, false).unwrap());
        assert_eq!(fs::read_to_string(repo_a.join("deck.txt")).unwrap(), "from-b");

        let _ = fs::remove_dir_all(base);
    }
}

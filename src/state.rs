use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub type State = Map<String, Value>;

fn os_user() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_default()
}

pub fn get_state_file(agent: &str, run_id: &str) -> PathBuf {
    let user    = os_user();
    let agent_s = agent.replace(std::path::MAIN_SEPARATOR, "_");
    let run_s   = run_id.replace(std::path::MAIN_SEPARATOR, "_");
    let filename = format!("notilens_{}_{}_{}.json", user, agent_s, run_s);
    env::temp_dir().join(filename)
}

pub fn get_pointer_file(agent: &str, label: &str) -> PathBuf {
    let user       = os_user();
    let safe_label = label.replace('/', "_").replace('\\', "_");
    let filename   = format!("notilens_{}_{}_{}.ptr", user, agent, safe_label);
    env::temp_dir().join(filename)
}

pub fn read_state(path: &PathBuf) -> State {
    let data = match fs::read_to_string(path) {
        Ok(d) => d,
        Err(_) => return Map::new(),
    };
    serde_json::from_str::<Map<String, Value>>(&data).unwrap_or_default()
}

pub fn write_state(path: &PathBuf, state: &State) {
    if let Ok(data) = serde_json::to_string_pretty(state) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, &data).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
            }
            let _ = fs::rename(&tmp, path);
        }
    }
}

pub fn update_state(path: &PathBuf, updates: State) {
    let mut s = read_state(path);
    for (k, v) in updates {
        s.insert(k, v);
    }
    write_state(path, &s);
}

pub fn delete_state(path: &PathBuf) {
    let _ = fs::remove_file(path);
}

pub fn read_pointer(agent: &str, label: &str) -> String {
    let path = get_pointer_file(agent, label);
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn write_pointer(agent: &str, label: &str, run_id: &str) {
    let path = get_pointer_file(agent, label);
    let _ = fs::write(path, run_id);
}

pub fn delete_pointer(agent: &str, label: &str) {
    let _ = fs::remove_file(get_pointer_file(agent, label));
}

pub fn cleanup_stale_state(agent: &str, state_ttl_seconds: u64) {
    let user   = os_user();
    let tmp    = env::temp_dir();
    let prefix = format!("notilens_{}_{}_", user, agent);
    let cutoff = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .saturating_sub(state_ttl_seconds);

    let entries = match fs::read_dir(&tmp) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&prefix) {
            continue;
        }
        if !name.ends_with(".json") && !name.ends_with(".ptr") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(UNIX_EPOCH) {
                    if dur.as_secs() < cutoff {
                        let _ = fs::remove_file(tmp.join(&name));
                    }
                }
            }
        }
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn to_f64(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

pub fn to_i64(v: Option<&Value>) -> i64 {
    to_f64(v) as i64
}

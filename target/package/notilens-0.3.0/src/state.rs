use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub type State = Map<String, Value>;

pub fn get_state_file(agent: &str, task_id: &str) -> PathBuf {
    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_default();
    let agent_s  = agent.replace(std::path::MAIN_SEPARATOR, "_");
    let task_s   = task_id.replace(std::path::MAIN_SEPARATOR, "_");
    let filename = format!("notilens_{}_{}_{}.json", user, agent_s, task_s);
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
        let _ = fs::write(path, data);
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

pub fn calc_duration(path: &PathBuf) -> i64 {
    let s = read_state(path);
    let start = to_i64(s.get("start_time"));
    if start == 0 {
        return 0;
    }
    now_ms() - start
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

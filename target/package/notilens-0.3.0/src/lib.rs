pub mod config;
pub mod notify;
pub mod state;

pub use config::{get_agent, list_agents, remove_agent, save_agent, AgentConfig};
pub use notify::VERSION;
pub use state::{
    calc_duration, delete_state, get_state_file, now_ms, read_state, update_state, write_state,
};

use serde_json::{Map, Value};
use std::collections::HashMap;

/// Main NotiLens SDK client.
pub struct NotiLens {
    agent: String,
    token: String,
    secret: String,
    metrics: HashMap<String, Value>,
}

/// Credentials passed directly to [`NotiLens::init`].
pub struct Options {
    pub token: String,
    pub secret: String,
}

/// Options for [`NotiLens::emit`].
pub struct EmitOptions {
    pub meta: HashMap<String, Value>,
}

impl NotiLens {
    /// Create a NotiLens client.
    /// Credentials resolved: Options → env vars → saved CLI config.
    pub fn init(agent: &str, opts: Option<Options>) -> Result<Self, String> {
        let mut token = String::new();
        let mut secret = String::new();

        if let Some(o) = opts {
            token = o.token;
            secret = o.secret;
        }
        if token.is_empty() {
            token = std::env::var("NOTILENS_TOKEN").unwrap_or_default();
        }
        if secret.is_empty() {
            secret = std::env::var("NOTILENS_SECRET").unwrap_or_default();
        }
        if token.is_empty() || secret.is_empty() {
            if let Some(conf) = get_agent(agent) {
                if token.is_empty() {
                    token = conf.token;
                }
                if secret.is_empty() {
                    secret = conf.secret;
                }
            }
        }
        if token.is_empty() || secret.is_empty() {
            return Err(format!(
                "NotiLens: token and secret are required. Pass them directly, \
                set NOTILENS_TOKEN/NOTILENS_SECRET env vars, or run: \
                notilens init --agent {} --token TOKEN --secret SECRET",
                agent
            ));
        }
        Ok(NotiLens {
            agent: agent.to_string(),
            token,
            secret,
            metrics: HashMap::new(),
        })
    }

    // ── Metrics ───────────────────────────────────────────────────────────────

    /// Set a metric. Numeric values accumulate; strings are replaced.
    pub fn metric(&mut self, key: &str, value: impl Into<Value>) -> &mut Self {
        let v = value.into();
        if let Some(fv) = as_f64(&v) {
            if let Some(existing) = self.metrics.get(key) {
                if let Some(fe) = as_f64(existing) {
                    self.metrics.insert(key.to_string(), Value::from(fe + fv));
                    return self;
                }
            }
        }
        self.metrics.insert(key.to_string(), v);
        self
    }

    /// Reset one metric by key, or all metrics if no key given.
    pub fn reset_metrics(&mut self, key: Option<&str>) -> &mut Self {
        match key {
            Some(k) => { self.metrics.remove(k); }
            None => self.metrics.clear(),
        }
        self
    }

    // ── Task lifecycle ────────────────────────────────────────────────────────

    pub fn task_start(&self, task_id: Option<&str>) -> String {
        let id = task_id
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("task_{}", now_ms()));

        let sf = get_state_file(&self.agent, &id);
        let mut s = Map::new();
        s.insert("agent".into(),       Value::from(self.agent.as_str()));
        s.insert("task".into(),        Value::from(id.as_str()));
        s.insert("start_time".into(),  Value::from(now_ms()));
        s.insert("retry_count".into(), Value::from(0));
        s.insert("loop_count".into(),  Value::from(0));
        write_state(&sf, &s);
        self.send("task.started", "Task started", &id, None);
        id
    }

    pub fn task_progress(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let dur = calc_duration(&sf);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(dur));
        update_state(&sf, u);
        self.send("task.progress", message, task_id, None);
    }

    pub fn task_loop(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let s = read_state(&sf);
        let count = state::to_i64(s.get("loop_count")) + 1;
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        u.insert("loop_count".into(),  Value::from(count));
        update_state(&sf, u);
        self.send("task.loop", message, task_id, None);
    }

    pub fn task_retry(&self, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let s = read_state(&sf);
        let count = state::to_i64(s.get("retry_count")) + 1;
        let mut u = Map::new();
        u.insert("duration_ms".into(),  Value::from(calc_duration(&sf)));
        u.insert("retry_count".into(),  Value::from(count));
        update_state(&sf, u);
        self.send("task.retry", "Retrying task", task_id, None);
    }

    pub fn task_error(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        u.insert("last_error".into(),  Value::from(message));
        update_state(&sf, u);
        self.send("task.error", message, task_id, None);
    }

    pub fn task_complete(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        update_state(&sf, u);
        self.send("task.completed", message, task_id, None);
        delete_state(&sf);
    }

    pub fn task_fail(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        update_state(&sf, u);
        self.send("task.failed", message, task_id, None);
        delete_state(&sf);
    }

    pub fn task_timeout(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        update_state(&sf, u);
        self.send("task.timeout", message, task_id, None);
        delete_state(&sf);
    }

    pub fn task_cancel(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        update_state(&sf, u);
        self.send("task.cancelled", message, task_id, None);
        delete_state(&sf);
    }

    pub fn task_stop(&self, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        update_state(&sf, u);
        self.send("task.stopped", "Task stopped", task_id, None);
    }

    pub fn task_terminate(&self, message: &str, task_id: &str) {
        let sf = get_state_file(&self.agent, task_id);
        let mut u = Map::new();
        u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
        update_state(&sf, u);
        self.send("task.terminated", message, task_id, None);
        delete_state(&sf);
    }

    // ── Input events ──────────────────────────────────────────────────────────

    pub fn input_required(&self, message: &str, task_id: &str) {
        self.send("input.required", message, task_id, None);
    }

    pub fn input_approved(&self, message: &str, task_id: &str) {
        self.send("input.approved", message, task_id, None);
    }

    pub fn input_rejected(&self, message: &str, task_id: &str) {
        self.send("input.rejected", message, task_id, None);
    }

    // ── Output events ─────────────────────────────────────────────────────────

    pub fn output_generated(&self, message: &str, task_id: &str) {
        self.send("output.generated", message, task_id, None);
    }

    pub fn output_failed(&self, message: &str, task_id: &str) {
        self.send("output.failed", message, task_id, None);
    }

    // ── Generic emit ──────────────────────────────────────────────────────────

    pub fn emit(&self, event: &str, message: &str, opts: Option<EmitOptions>) {
        let meta = opts.map(|o| o.meta);
        self.send(event, message, "", meta);
    }

    // ── Internal send ─────────────────────────────────────────────────────────

    fn send(
        &self,
        event: &str,
        message: &str,
        task_id: &str,
        extra: Option<HashMap<String, Value>>,
    ) {
        let title = if task_id.is_empty() {
            format!("{} | {}", self.agent, event)
        } else {
            format!("{} | {} | {}", self.agent, task_id, event)
        };

        let mut duration = 0i64;
        let mut retry_count = 0i64;
        let mut loop_count = 0i64;

        if !task_id.is_empty() {
            let sf = get_state_file(&self.agent, task_id);
            let s = read_state(&sf);
            duration    = state::to_i64(s.get("duration_ms"));
            retry_count = state::to_i64(s.get("retry_count"));
            loop_count  = state::to_i64(s.get("loop_count"));
        }

        let mut meta: HashMap<String, Value> = HashMap::new();
        meta.insert("agent".into(), Value::from(self.agent.as_str()));
        if duration    > 0 { meta.insert("duration_ms".into(),  Value::from(duration));    }
        if retry_count > 0 { meta.insert("retry_count".into(),  Value::from(retry_count)); }
        if loop_count  > 0 { meta.insert("loop_count".into(),   Value::from(loop_count));  }
        for (k, v) in &self.metrics {
            meta.insert(k.clone(), v.clone());
        }
        if let Some(extra_meta) = extra {
            for (k, v) in extra_meta {
                meta.insert(k, v);
            }
        }

        // Strip reserved URL/tag fields
        let image_url    = pop_string(&mut meta, "image_url");
        let open_url     = pop_string(&mut meta, "open_url");
        let download_url = pop_string(&mut meta, "download_url");
        let tags         = pop_string(&mut meta, "tags");

        let is_actionable = notify::is_actionable_default(event);
        let is_actionable = if let Some(Value::Bool(b)) = meta.remove("is_actionable") {
            b
        } else {
            is_actionable
        };

        let ts = now_ms() as f64 / 1000.0;

        let mut payload: HashMap<String, Value> = HashMap::new();
        payload.insert("event".into(),         Value::from(event));
        payload.insert("title".into(),         Value::from(title.as_str()));
        payload.insert("message".into(),       Value::from(message));
        payload.insert("type".into(),          Value::from(notify::get_event_type(event)));
        payload.insert("agent".into(),         Value::from(self.agent.as_str()));
        payload.insert("task_id".into(),       Value::from(task_id));
        payload.insert("is_actionable".into(), Value::from(is_actionable));
        payload.insert("image_url".into(),     Value::from(image_url.as_str()));
        payload.insert("open_url".into(),      Value::from(open_url.as_str()));
        payload.insert("download_url".into(),  Value::from(download_url.as_str()));
        payload.insert("tags".into(),          Value::from(tags.as_str()));
        payload.insert("ts".into(),            Value::from(ts));
        payload.insert("meta".into(),          serde_json::to_value(&meta).unwrap_or_default());

        // Silent fail
        let _ = notify::send_http(&self.token, &self.secret, &payload);
    }
}

fn pop_string(meta: &mut HashMap<String, Value>, key: &str) -> String {
    if let Some(Value::String(s)) = meta.remove(key) {
        s
    } else {
        String::new()
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

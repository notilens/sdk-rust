pub mod config;
pub mod notify;
pub mod state;

pub use config::{get_agent, list_agents, remove_agent, save_agent, AgentConfig};
pub use notify::VERSION;
pub use state::{
    cleanup_stale_state, delete_pointer, delete_state, get_pointer_file, get_state_file,
    now_ms, read_pointer, read_state, update_state, write_pointer, write_state,
};

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::mpsc::{self, SyncSender};

/// Main NotiLens SDK client.
pub struct NotiLens {
    agent:     String,
    token:     String,
    secret:    String,
    state_ttl: u64,
    metrics:   HashMap<String, Value>,
    sender:    SyncSender<HashMap<String, Value>>,
}

/// Options for [`NotiLens::init`].
pub struct Options {
    pub token:     String,
    pub secret:    String,
    pub state_ttl: u64, // orphaned state TTL in seconds (default: 86400)
}

/// Options for [`NotiLens::track`] and [`Run::track`].
pub struct TrackOptions {
    pub meta: HashMap<String, Value>,
}

impl NotiLens {
    /// Create a NotiLens client.
    /// Credentials resolved: Options → env vars → saved CLI config.
    pub fn init(agent: &str, opts: Option<Options>) -> Result<Self, String> {
        let mut token     = String::new();
        let mut secret    = String::new();
        let mut state_ttl = 86400u64;

        if let Some(o) = opts {
            token  = o.token;
            secret = o.secret;
            if o.state_ttl > 0 { state_ttl = o.state_ttl; }
        }
        if token.is_empty() {
            token = std::env::var("NOTILENS_TOKEN").unwrap_or_default();
        }
        if secret.is_empty() {
            secret = std::env::var("NOTILENS_SECRET").unwrap_or_default();
        }
        if token.is_empty() || secret.is_empty() {
            if let Some(conf) = get_agent(agent) {
                if token.is_empty()  { token  = conf.token; }
                if secret.is_empty() { secret = conf.secret; }
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
        cleanup_stale_state(agent, state_ttl);

        // Single background worker — one OS thread shared across all sends
        let (tx, rx) = mpsc::sync_channel::<HashMap<String, Value>>(256);
        let token_w  = token.clone();
        let secret_w = secret.clone();
        std::thread::spawn(move || {
            while let Ok(payload) = rx.recv() {
                let _ = notify::send_http(&token_w, &secret_w, &payload);
            }
        });

        Ok(NotiLens {
            agent: agent.to_string(),
            token,
            secret,
            state_ttl,
            metrics: HashMap::new(),
            sender: tx,
        })
    }

    // ── Task factory ──────────────────────────────────────────────────────────

    /// Create a new [`Run`] for the given label.
    /// Each call generates a unique run_id — concurrent runs never conflict.
    pub fn task(&self, label: &str) -> Run {
        cleanup_stale_state(&self.agent, self.state_ttl);
        let run_id = gen_run_id();
        let sf     = get_state_file(&self.agent, &run_id);
        Run {
            agent:   self,
            label:   label.to_string(),
            run_id,
            sf,
            metrics: HashMap::new(),
        }
    }

    // ── Agent-level metrics ───────────────────────────────────────────────────

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
            None    => self.metrics.clear(),
        }
        self
    }

    // ── Generic track ─────────────────────────────────────────────────────────

    /// Send a free-form agent-level event.
    pub fn track(&self, event: &str, message: &str, opts: Option<TrackOptions>) {
        let meta = opts.map(|o| o.meta);
        self.send_payload(event, message, "", "", "", &self.metrics, meta);
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    pub fn send_payload(
        &self,
        event:       &str,
        message:     &str,
        run_id:      &str,
        label:       &str,
        state_file:  &str,
        run_metrics: &HashMap<String, Value>,
        extra:       Option<HashMap<String, Value>>,
    ) {
        let title = if label.is_empty() {
            format!("{} | {}", self.agent, event)
        } else {
            format!("{} | {} | {}", self.agent, label, event)
        };

        let mut meta: HashMap<String, Value> = HashMap::new();
        meta.insert("agent".into(), Value::from(self.agent.as_str()));
        if !run_id.is_empty() {
            meta.insert("run_id".into(), Value::from(run_id));
        }
        if !label.is_empty() {
            meta.insert("task".into(), Value::from(label));
        }

        if !state_file.is_empty() {
            let sf = std::path::PathBuf::from(state_file);
            let s  = read_state(&sf);
            let now = now_ms();
            let start_time  = state::to_i64(s.get("start_time"));
            let queued_at   = state::to_i64(s.get("queued_at"));
            let mut pause_total = state::to_i64(s.get("pause_total_ms"));
            let mut wait_total  = state::to_i64(s.get("wait_total_ms"));
            if let Some(v) = s.get("paused_at") { let v = state::to_i64(Some(v)); if v > 0 { pause_total += now - v; } }
            if let Some(v) = s.get("wait_at")   { let v = state::to_i64(Some(v)); if v > 0 { wait_total  += now - v; } }
            let total_ms  = if start_time > 0 { now - start_time } else { 0 };
            let queue_ms  = if start_time > 0 && queued_at > 0 { start_time - queued_at } else { 0 };
            let active_ms = std::cmp::max(0, total_ms - pause_total - wait_total);

            if total_ms   > 0 { meta.insert("total_duration_ms".into(), Value::from(total_ms));   }
            if queue_ms   > 0 { meta.insert("queue_ms".into(),          Value::from(queue_ms));   }
            if pause_total > 0 { meta.insert("pause_ms".into(),         Value::from(pause_total)); }
            if wait_total  > 0 { meta.insert("wait_ms".into(),          Value::from(wait_total));  }
            if active_ms  > 0 { meta.insert("active_ms".into(),         Value::from(active_ms));  }
            let rc = state::to_i64(s.get("retry_count")); if rc > 0 { meta.insert("retry_count".into(), Value::from(rc)); }
            let lc = state::to_i64(s.get("loop_count"));  if lc > 0 { meta.insert("loop_count".into(),  Value::from(lc)); }
            let ec = state::to_i64(s.get("error_count")); if ec > 0 { meta.insert("error_count".into(), Value::from(ec)); }
            let pc = state::to_i64(s.get("pause_count")); if pc > 0 { meta.insert("pause_count".into(), Value::from(pc)); }
            let wc = state::to_i64(s.get("wait_count"));  if wc > 0 { meta.insert("wait_count".into(),  Value::from(wc)); }
            if let Some(Value::Object(metrics)) = s.get("metrics") {
                for (k, v) in metrics {
                    meta.insert(k.clone(), v.clone());
                }
            }
        }

        for (k, v) in run_metrics {
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
        payload.insert("task_id".into(),       Value::from(label));
        payload.insert("is_actionable".into(), Value::from(is_actionable));
        payload.insert("image_url".into(),     Value::from(image_url.as_str()));
        payload.insert("open_url".into(),      Value::from(open_url.as_str()));
        payload.insert("download_url".into(),  Value::from(download_url.as_str()));
        payload.insert("tags".into(),          Value::from(tags.as_str()));
        payload.insert("ts".into(),            Value::from(ts));
        payload.insert("meta".into(),          serde_json::to_value(&meta).unwrap_or_default());

        // Fire-and-forget — push to background worker queue, never blocks the caller
        let _ = self.sender.try_send(payload);
    }
}

// ── Run ───────────────────────────────────────────────────────────────────────

/// An isolated execution context for a single task invocation.
pub struct Run<'a> {
    agent:   &'a NotiLens,
    label:   String,
    pub run_id: String,
    sf:      std::path::PathBuf,
    metrics: HashMap<String, Value>,
}

impl<'a> Run<'a> {
    // ── Metrics ───────────────────────────────────────────────────────────────

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

    pub fn reset_metrics(&mut self, key: Option<&str>) -> &mut Self {
        match key {
            Some(k) => { self.metrics.remove(k); }
            None    => self.metrics.clear(),
        }
        self
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    pub fn queue(&mut self) -> &mut Self {
        let mut s = Map::new();
        s.insert("agent".into(),          Value::from(self.agent.agent.as_str()));
        s.insert("task".into(),           Value::from(self.label.as_str()));
        s.insert("run_id".into(),         Value::from(self.run_id.as_str()));
        s.insert("queued_at".into(),      Value::from(now_ms()));
        s.insert("retry_count".into(),    Value::from(0));
        s.insert("loop_count".into(),     Value::from(0));
        s.insert("error_count".into(),    Value::from(0));
        s.insert("pause_count".into(),    Value::from(0));
        s.insert("wait_count".into(),     Value::from(0));
        s.insert("pause_total_ms".into(), Value::from(0));
        s.insert("wait_total_ms".into(),  Value::from(0));
        write_state(&self.sf, &s);
        self.send("task.queued", "Task queued", None);
        self
    }

    pub fn start(&mut self) -> &mut Self {
        let now      = now_ms();
        let existing = read_state(&self.sf);
        if !existing.is_empty() {
            let mut u = Map::new();
            u.insert("start_time".into(), Value::from(now));
            update_state(&self.sf, u);
        } else {
            let mut s = Map::new();
            s.insert("agent".into(),          Value::from(self.agent.agent.as_str()));
            s.insert("task".into(),           Value::from(self.label.as_str()));
            s.insert("run_id".into(),         Value::from(self.run_id.as_str()));
            s.insert("start_time".into(),     Value::from(now));
            s.insert("retry_count".into(),    Value::from(0));
            s.insert("loop_count".into(),     Value::from(0));
            s.insert("error_count".into(),    Value::from(0));
            s.insert("pause_count".into(),    Value::from(0));
            s.insert("wait_count".into(),     Value::from(0));
            s.insert("pause_total_ms".into(), Value::from(0));
            s.insert("wait_total_ms".into(),  Value::from(0));
            write_state(&self.sf, &s);
        }
        self.send("task.started", "Task started", None);
        self
    }

    pub fn progress(&self, message: &str) { self.send("task.progress", message, None); }

    pub fn loop_step(&self, message: &str) {
        let s = read_state(&self.sf);
        let count = state::to_i64(s.get("loop_count")) + 1;
        let mut u = Map::new();
        u.insert("loop_count".into(), Value::from(count));
        update_state(&self.sf, u);
        self.send("task.loop", message, None);
    }

    pub fn retry(&self) {
        let s = read_state(&self.sf);
        let count = state::to_i64(s.get("retry_count")) + 1;
        let mut u = Map::new();
        u.insert("retry_count".into(), Value::from(count));
        update_state(&self.sf, u);
        self.send("task.retry", "Retrying task", None);
    }

    pub fn pause(&self, message: &str) {
        let s = read_state(&self.sf);
        let mut u = Map::new();
        u.insert("paused_at".into(),   Value::from(now_ms()));
        u.insert("pause_count".into(), Value::from(state::to_i64(s.get("pause_count")) + 1));
        update_state(&self.sf, u);
        self.send("task.paused", message, None);
    }

    pub fn resume(&self, message: &str) {
        let s   = read_state(&self.sf);
        let now = now_ms();
        let mut u = Map::new();
        let paused_at = state::to_i64(s.get("paused_at"));
        if paused_at > 0 {
            u.insert("pause_total_ms".into(), Value::from(state::to_i64(s.get("pause_total_ms")) + (now - paused_at)));
            u.insert("paused_at".into(), Value::Null);
        }
        let wait_at = state::to_i64(s.get("wait_at"));
        if wait_at > 0 {
            u.insert("wait_total_ms".into(), Value::from(state::to_i64(s.get("wait_total_ms")) + (now - wait_at)));
            u.insert("wait_at".into(), Value::Null);
        }
        if !u.is_empty() { update_state(&self.sf, u); }
        self.send("task.resumed", message, None);
    }

    pub fn wait(&self, message: &str) {
        let s = read_state(&self.sf);
        let mut u = Map::new();
        u.insert("wait_at".into(),    Value::from(now_ms()));
        u.insert("wait_count".into(), Value::from(state::to_i64(s.get("wait_count")) + 1));
        update_state(&self.sf, u);
        self.send("task.waiting", message, None);
    }

    pub fn stop(&self) { self.send("task.stopped", "Task stopped", None); }

    pub fn error(&self, message: &str) {
        let s = read_state(&self.sf);
        let mut u = Map::new();
        u.insert("last_error".into(),  Value::from(message));
        u.insert("error_count".into(), Value::from(state::to_i64(s.get("error_count")) + 1));
        update_state(&self.sf, u);
        self.send("task.error", message, None);
    }

    pub fn complete(&self, message: &str) {
        self.send("task.completed", message, None);
        self.terminal();
    }

    pub fn fail(&self, message: &str) {
        self.send("task.failed", message, None);
        self.terminal();
    }

    pub fn timeout(&self, message: &str) {
        self.send("task.timeout", message, None);
        self.terminal();
    }

    pub fn cancel(&self, message: &str) {
        self.send("task.cancelled", message, None);
        self.terminal();
    }

    pub fn terminate(&self, message: &str) {
        self.send("task.terminated", message, None);
        self.terminal();
    }

    // ── Input / Output ────────────────────────────────────────────────────────

    pub fn input_required(&self, message: &str)   { self.send("input.required",  message, None); }
    pub fn input_approved(&self, message: &str)   { self.send("input.approved",  message, None); }
    pub fn input_rejected(&self, message: &str)   { self.send("input.rejected",  message, None); }
    pub fn output_generated(&self, message: &str) { self.send("output.generated", message, None); }
    pub fn output_failed(&self, message: &str)    { self.send("output.failed",   message, None); }

    pub fn track(&self, event: &str, message: &str, opts: Option<TrackOptions>) {
        let meta = opts.map(|o| o.meta);
        self.send(event, message, meta);
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    fn send(&self, event: &str, message: &str, extra: Option<HashMap<String, Value>>) {
        let sf_str = self.sf.to_string_lossy().to_string();
        self.agent.send_payload(
            event,
            message,
            &self.run_id,
            &self.label,
            &sf_str,
            &self.metrics,
            extra,
        );
    }

    fn terminal(&self) {
        delete_state(&self.sf);
        delete_pointer(&self.agent.agent, &self.label);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

fn gen_run_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Use a simple pseudo-random suffix from process ID and time
    let suffix = (ms ^ (std::process::id() as u128 * 0x9e3779b97f4a7c15)) & 0xFFFF_FFFF;
    format!("run_{}_{:08x}", ms, suffix)
}

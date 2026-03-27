use notilens::{
    delete_pointer, delete_state, get_agent, get_state_file, list_agents, now_ms,
    read_pointer, read_state, remove_agent, save_agent, update_state, write_pointer,
    write_state, VERSION,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }
    let command = &args[1];
    let rest = &args[2..];

    match command.as_str() {
        "init" => run_init(rest),

        "agents" => {
            let agents = list_agents();
            if agents.is_empty() {
                println!("No agents configured.");
            } else {
                for a in agents {
                    println!("  {}", a);
                }
            }
        }

        "remove-agent" => {
            if rest.is_empty() {
                eprintln!("Usage: notilens remove-agent <agent>");
                std::process::exit(1);
            }
            if remove_agent(&rest[0]) {
                println!("✔ Agent '{}' removed", rest[0]);
            } else {
                eprintln!("Agent '{}' not found", rest[0]);
            }
        }

        "task.queue" => {
            let flags = parse_flags(rest);
            let run_id = gen_run_id();
            let sf = get_state_file(&flags.agent, &run_id);
            let mut s = Map::new();
            s.insert("agent".into(),          Value::from(flags.agent.as_str()));
            s.insert("task".into(),           Value::from(flags.task_label.as_str()));
            s.insert("run_id".into(),         Value::from(run_id.as_str()));
            s.insert("queued_at".into(),      Value::from(now_ms()));
            s.insert("retry_count".into(),    Value::from(0));
            s.insert("loop_count".into(),     Value::from(0));
            s.insert("error_count".into(),    Value::from(0));
            s.insert("pause_count".into(),    Value::from(0));
            s.insert("wait_count".into(),     Value::from(0));
            s.insert("pause_total_ms".into(), Value::from(0));
            s.insert("wait_total_ms".into(),  Value::from(0));
            write_state(&sf, &s);
            write_pointer(&flags.agent, &flags.task_label, &run_id);
            send_notify("task.queued", "Task queued", &flags, &run_id);
            println!("{}", run_id);
        }

        "task.start" => {
            let flags = parse_flags(rest);
            // Reuse run_id from a prior task.queue if available
            let run_id = {
                let existing = read_pointer(&flags.agent, &flags.task_label);
                if existing.is_empty() { gen_run_id() } else { existing }
            };
            let sf = get_state_file(&flags.agent, &run_id);
            let existing = read_state(&sf);
            if !existing.is_empty() {
                let mut u = Map::new();
                u.insert("start_time".into(), Value::from(now_ms()));
                update_state(&sf, u);
            } else {
                let mut s = Map::new();
                s.insert("agent".into(),          Value::from(flags.agent.as_str()));
                s.insert("task".into(),           Value::from(flags.task_label.as_str()));
                s.insert("run_id".into(),         Value::from(run_id.as_str()));
                s.insert("start_time".into(),     Value::from(now_ms()));
                s.insert("retry_count".into(),    Value::from(0));
                s.insert("loop_count".into(),     Value::from(0));
                s.insert("error_count".into(),    Value::from(0));
                s.insert("pause_count".into(),    Value::from(0));
                s.insert("wait_count".into(),     Value::from(0));
                s.insert("pause_total_ms".into(), Value::from(0));
                s.insert("wait_total_ms".into(),  Value::from(0));
                write_state(&sf, &s);
            }
            write_pointer(&flags.agent, &flags.task_label, &run_id);
            send_notify("task.started", "Task started", &flags, &run_id);
            println!("{}", run_id);
        }

        "task.progress" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("task.progress", msg, &flags, &run_id);
        }

        "task.loop" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            let s = read_state(&sf);
            let count = notilens::state::to_i64(s.get("loop_count")) + 1;
            let mut u = Map::new();
            u.insert("loop_count".into(), Value::from(count));
            update_state(&sf, u);
            send_notify("task.loop", msg, &flags, &run_id);
        }

        "task.retry" => {
            let flags = parse_flags(rest);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            let s = read_state(&sf);
            let count = notilens::state::to_i64(s.get("retry_count")) + 1;
            let mut u = Map::new();
            u.insert("retry_count".into(), Value::from(count));
            update_state(&sf, u);
            send_notify("task.retry", "Retrying task", &flags, &run_id);
        }

        "task.stop" => {
            let flags = parse_flags(rest);
            let run_id = resolve_run_id(&flags);
            send_notify("task.stopped", "Task stopped", &flags, &run_id);
        }

        "task.pause" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            let s = read_state(&sf);
            let mut u = Map::new();
            u.insert("paused_at".into(),   Value::from(now_ms()));
            u.insert("pause_count".into(), Value::from(notilens::state::to_i64(s.get("pause_count")) + 1));
            update_state(&sf, u);
            send_notify("task.paused", msg, &flags, &run_id);
        }

        "task.resume" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            let s = read_state(&sf);
            let now = now_ms();
            let mut u = Map::new();
            let paused_at = notilens::state::to_i64(s.get("paused_at"));
            if paused_at > 0 {
                u.insert("pause_total_ms".into(), Value::from(notilens::state::to_i64(s.get("pause_total_ms")) + (now - paused_at)));
                u.insert("paused_at".into(), Value::Null);
            }
            let wait_at = notilens::state::to_i64(s.get("wait_at"));
            if wait_at > 0 {
                u.insert("wait_total_ms".into(), Value::from(notilens::state::to_i64(s.get("wait_total_ms")) + (now - wait_at)));
                u.insert("wait_at".into(), Value::Null);
            }
            if !u.is_empty() { update_state(&sf, u); }
            send_notify("task.resumed", msg, &flags, &run_id);
        }

        "task.wait" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            let s = read_state(&sf);
            let mut u = Map::new();
            u.insert("wait_at".into(),    Value::from(now_ms()));
            u.insert("wait_count".into(), Value::from(notilens::state::to_i64(s.get("wait_count")) + 1));
            update_state(&sf, u);
            send_notify("task.waiting", msg, &flags, &run_id);
        }

        "task.error" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            let s = read_state(&sf);
            let mut u = Map::new();
            u.insert("last_error".into(),  Value::from(msg));
            u.insert("error_count".into(), Value::from(notilens::state::to_i64(s.get("error_count")) + 1));
            update_state(&sf, u);
            send_notify("task.error", msg, &flags, &run_id);
            eprintln!("❌ Error: {}", msg);
        }

        "task.fail" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("task.failed", msg, &flags, &run_id);
            delete_state(&get_state_file(&flags.agent, &run_id));
            delete_pointer(&flags.agent, &flags.task_label);
        }

        "task.timeout" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("task.timeout", msg, &flags, &run_id);
            delete_state(&get_state_file(&flags.agent, &run_id));
            delete_pointer(&flags.agent, &flags.task_label);
        }

        "task.cancel" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("task.cancelled", msg, &flags, &run_id);
            delete_state(&get_state_file(&flags.agent, &run_id));
            delete_pointer(&flags.agent, &flags.task_label);
        }

        "task.terminate" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("task.terminated", msg, &flags, &run_id);
            delete_state(&get_state_file(&flags.agent, &run_id));
            delete_pointer(&flags.agent, &flags.task_label);
        }

        "task.complete" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("task.completed", msg, &flags, &run_id);
            delete_state(&get_state_file(&flags.agent, &run_id));
            delete_pointer(&flags.agent, &flags.task_label);
        }

        "metric" => {
            let (pos, rest2) = positional_args(rest);
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            let s = read_state(&sf);
            let mut metrics: HashMap<String, Value> = match s.get("metrics") {
                Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => HashMap::new(),
            };
            for kv in &pos {
                if let Some(eq) = kv.find('=') {
                    let k = &kv[..eq];
                    let v = &kv[eq + 1..];
                    if let Ok(fv) = v.parse::<f64>() {
                        let existing = metrics.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                        metrics.insert(k.to_string(), Value::from(existing + fv));
                    } else {
                        metrics.insert(k.to_string(), Value::from(v));
                    }
                }
            }
            let mut u = Map::new();
            u.insert("metrics".into(), serde_json::to_value(&metrics).unwrap_or_default());
            update_state(&sf, u);
            let parts: Vec<String> = metrics.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
            println!("📊 Metrics: {}", parts.join(", "));
        }

        "metric.reset" => {
            let (pos, rest2) = positional_args(rest);
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            let sf = get_state_file(&flags.agent, &run_id);
            if let Some(key) = pos.first() {
                let s = read_state(&sf);
                let mut metrics: HashMap<String, Value> = match s.get("metrics") {
                    Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    _ => HashMap::new(),
                };
                metrics.remove(key.as_str());
                let mut u = Map::new();
                u.insert("metrics".into(), serde_json::to_value(&metrics).unwrap_or_default());
                update_state(&sf, u);
                println!("📊 Metric '{}' reset", key);
            } else {
                let mut u = Map::new();
                u.insert("metrics".into(), Value::Object(Map::new()));
                update_state(&sf, u);
                println!("📊 All metrics reset");
            }
        }

        "output.generate" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("output.generated", msg, &flags, &run_id);
        }

        "output.fail" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("output.failed", msg, &flags, &run_id);
        }

        "input.required" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("input.required", msg, &flags, &run_id);
        }

        "input.approve" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("input.approved", msg, &flags, &run_id);
        }

        "input.reject" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let run_id = resolve_run_id(&flags);
            send_notify("input.rejected", msg, &flags, &run_id);
        }

        "track" => {
            if rest.len() < 2 {
                eprintln!("Usage: notilens track <event> <message> --agent <agent>");
                std::process::exit(1);
            }
            let event = &rest[0];
            let msg   = &rest[1];
            let flags = parse_flags(&rest[2..]);
            // track is agent-level; use pointer if available but don't error if absent
            let run_id = read_pointer(&flags.agent, &flags.task_label);
            send_notify(event, msg, &flags, &run_id);
            println!("📡 Tracked: {}", event);
        }

        "version" => println!("NotiLens v{}", VERSION),

        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}

// ── Flag parsing ──────────────────────────────────────────────────────────────

struct Flags {
    agent:         String,
    task_label:    String,
    typ:           String,
    meta:          HashMap<String, String>,
    image_url:     String,
    open_url:      String,
    download_url:  String,
    tags:          String,
    is_actionable: String,
}

fn positional_args(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut pos = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            break;
        }
        pos.push(args[i].clone());
        i += 1;
    }
    (pos, args[i..].to_vec())
}

fn parse_flags(args: &[String]) -> Flags {
    let mut f = Flags {
        agent: String::new(),
        task_label: String::new(),
        typ: String::new(),
        meta: HashMap::new(),
        image_url: String::new(),
        open_url: String::new(),
        download_url: String::new(),
        tags: String::new(),
        is_actionable: String::new(),
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent"         => { if i + 1 < args.len() { f.agent        = args[i+1].clone(); i += 1; } }
            "--task"          => { if i + 1 < args.len() { f.task_label   = args[i+1].clone(); i += 1; } }
            "--type"          => { if i + 1 < args.len() { f.typ          = args[i+1].clone(); i += 1; } }
            "--image_url"     => { if i + 1 < args.len() { f.image_url    = args[i+1].clone(); i += 1; } }
            "--open_url"      => { if i + 1 < args.len() { f.open_url     = args[i+1].clone(); i += 1; } }
            "--download_url"  => { if i + 1 < args.len() { f.download_url = args[i+1].clone(); i += 1; } }
            "--tags"          => { if i + 1 < args.len() { f.tags         = args[i+1].clone(); i += 1; } }
            "--is_actionable" => { if i + 1 < args.len() { f.is_actionable= args[i+1].clone(); i += 1; } }
            "--meta" => {
                if i + 1 < args.len() {
                    let kv = &args[i + 1];
                    if let Some(eq) = kv.find('=') {
                        f.meta.insert(kv[..eq].to_string(), kv[eq+1..].to_string());
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if f.agent.is_empty() {
        eprintln!("❌ --agent is required");
        std::process::exit(1);
    }
    f
}

fn resolve_run_id(f: &Flags) -> String {
    if f.task_label.is_empty() {
        eprintln!("❌ --task is required");
        std::process::exit(1);
    }
    let run_id = read_pointer(&f.agent, &f.task_label);
    if run_id.is_empty() {
        eprintln!(
            "❌ No active run for task '{}' on agent '{}'. Run task.start first.",
            f.task_label, f.agent
        );
        std::process::exit(1);
    }
    run_id
}

// ── Core send ─────────────────────────────────────────────────────────────────

fn get_event_type(event: &str) -> &'static str {
    notilens::notify::get_event_type(event)
}

fn is_actionable_default(event: &str) -> bool {
    notilens::notify::is_actionable_default(event)
}

fn send_notify(event: &str, message: &str, f: &Flags, run_id: &str) {
    let conf = match get_agent(&f.agent) {
        Some(c) if !c.token.is_empty() && !c.secret.is_empty() => c,
        _ => {
            eprintln!(
                "❌ Agent '{}' not configured. Run: notilens init --agent {} --token TOKEN --secret SECRET",
                f.agent, f.agent
            );
            std::process::exit(1);
        }
    };

    let sf    = get_state_file(&f.agent, run_id);
    let state = read_state(&sf);

    let mut meta: HashMap<String, Value> = HashMap::new();
    meta.insert("agent".into(), Value::from(f.agent.as_str()));
    if !run_id.is_empty() {
        meta.insert("run_id".into(), Value::from(run_id));
    }
    if !f.task_label.is_empty() {
        meta.insert("task".into(), Value::from(f.task_label.as_str()));
        let now         = now_ms();
        let start_time  = notilens::state::to_i64(state.get("start_time"));
        let queued_at   = notilens::state::to_i64(state.get("queued_at"));
        let mut pause_total = notilens::state::to_i64(state.get("pause_total_ms"));
        let mut wait_total  = notilens::state::to_i64(state.get("wait_total_ms"));
        if let Some(v) = state.get("paused_at") { let v = notilens::state::to_i64(Some(v)); if v > 0 { pause_total += now - v; } }
        if let Some(v) = state.get("wait_at")   { let v = notilens::state::to_i64(Some(v)); if v > 0 { wait_total  += now - v; } }
        let total_ms  = if start_time > 0 { now - start_time } else { 0 };
        let queue_ms  = if start_time > 0 && queued_at > 0 { start_time - queued_at } else { 0 };
        let active_ms = std::cmp::max(0, total_ms - pause_total - wait_total);

        if total_ms   > 0 { meta.insert("total_duration_ms".into(), Value::from(total_ms));   }
        if queue_ms   > 0 { meta.insert("queue_ms".into(),          Value::from(queue_ms));   }
        if pause_total > 0 { meta.insert("pause_ms".into(),         Value::from(pause_total)); }
        if wait_total  > 0 { meta.insert("wait_ms".into(),          Value::from(wait_total));  }
        if active_ms  > 0 { meta.insert("active_ms".into(),         Value::from(active_ms));  }
        let rc = notilens::state::to_i64(state.get("retry_count")); if rc > 0 { meta.insert("retry_count".into(), Value::from(rc)); }
        let lc = notilens::state::to_i64(state.get("loop_count"));  if lc > 0 { meta.insert("loop_count".into(),  Value::from(lc)); }
        let ec = notilens::state::to_i64(state.get("error_count")); if ec > 0 { meta.insert("error_count".into(), Value::from(ec)); }
        let pc = notilens::state::to_i64(state.get("pause_count")); if pc > 0 { meta.insert("pause_count".into(), Value::from(pc)); }
        let wc = notilens::state::to_i64(state.get("wait_count"));  if wc > 0 { meta.insert("wait_count".into(),  Value::from(wc)); }
    }

    if let Some(Value::Object(metrics)) = state.get("metrics") {
        for (k, v) in metrics {
            meta.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &f.meta {
        meta.insert(k.clone(), Value::from(v.as_str()));
    }

    let title = if f.task_label.is_empty() {
        format!("{} | {}", f.agent, event)
    } else {
        format!("{} | {} | {}", f.agent, f.task_label, event)
    };

    let ev_type = match f.typ.as_str() {
        "info" | "success" | "warning" | "urgent" => f.typ.as_str(),
        _ => get_event_type(event),
    };

    let is_actionable = if f.is_actionable == "true" {
        true
    } else if f.is_actionable == "false" {
        false
    } else {
        is_actionable_default(event)
    };

    let ts = now_ms() as f64 / 1000.0;

    let mut payload: HashMap<String, Value> = HashMap::new();
    payload.insert("event".into(),         Value::from(event));
    payload.insert("title".into(),         Value::from(title.as_str()));
    payload.insert("message".into(),       Value::from(message));
    payload.insert("type".into(),          Value::from(ev_type));
    payload.insert("agent".into(),         Value::from(f.agent.as_str()));
    payload.insert("task_id".into(),       Value::from(f.task_label.as_str()));
    payload.insert("is_actionable".into(), Value::from(is_actionable));
    payload.insert("image_url".into(),     Value::from(f.image_url.as_str()));
    payload.insert("open_url".into(),      Value::from(f.open_url.as_str()));
    payload.insert("download_url".into(),  Value::from(f.download_url.as_str()));
    payload.insert("tags".into(),          Value::from(f.tags.as_str()));
    payload.insert("ts".into(),            Value::from(ts));
    payload.insert("meta".into(),          serde_json::to_value(&meta).unwrap_or_default());

    let _ = notilens::notify::send_http(&conf.token, &conf.secret, &payload);
    std::thread::sleep(Duration::from_millis(300));
}

// ── Usage ─────────────────────────────────────────────────────────────────────

fn print_usage() {
    print!(r#"Usage:
  notilens init --agent <name> --token <token> --secret <secret>
  notilens agents
  notilens remove-agent <agent>

Task Lifecycle:
  notilens task.queue           --agent <agent> --task <label>
  notilens task.start           --agent <agent> --task <label>
  notilens task.progress  "msg" --agent <agent> --task <label>
  notilens task.loop      "msg" --agent <agent> --task <label>
  notilens task.retry           --agent <agent> --task <label>
  notilens task.stop            --agent <agent> --task <label>
  notilens task.pause     "msg" --agent <agent> --task <label>
  notilens task.resume    "msg" --agent <agent> --task <label>
  notilens task.wait      "msg" --agent <agent> --task <label>
  notilens task.error     "msg" --agent <agent> --task <label>
  notilens task.fail      "msg" --agent <agent> --task <label>
  notilens task.timeout   "msg" --agent <agent> --task <label>
  notilens task.cancel    "msg" --agent <agent> --task <label>
  notilens task.terminate "msg" --agent <agent> --task <label>
  notilens task.complete  "msg" --agent <agent> --task <label>

Output / Input:
  notilens output.generate "msg" --agent <agent> --task <label>
  notilens output.fail     "msg" --agent <agent> --task <label>
  notilens input.required  "msg" --agent <agent> --task <label>
  notilens input.approve   "msg" --agent <agent> --task <label>
  notilens input.reject    "msg" --agent <agent> --task <label>

Metrics:
  notilens metric       tokens=512 cost=0.003 --agent <agent> --task <label>
  notilens metric.reset tokens               --agent <agent> --task <label>
  notilens metric.reset                      --agent <agent> --task <label>

Generic:
  notilens track <event> "msg" --agent <agent>

Options:
  --agent <name>
  --task <label>
  --type success|warning|urgent|info
  --meta key=value   (repeatable)
  --image_url <url>
  --open_url <url>
  --download_url <url>
  --tags "tag1,tag2"
  --is_actionable true|false

Other:
  notilens version
"#);
}

// ── init command ──────────────────────────────────────────────────────────────

fn run_init(args: &[String]) {
    let mut agent  = String::new();
    let mut token  = String::new();
    let mut secret = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent"  => { if i + 1 < args.len() { agent  = args[i+1].clone(); i += 1; } }
            "--token"  => { if i + 1 < args.len() { token  = args[i+1].clone(); i += 1; } }
            "--secret" => { if i + 1 < args.len() { secret = args[i+1].clone(); i += 1; } }
            _ => {}
        }
        i += 1;
    }
    if agent.is_empty() || token.is_empty() || secret.is_empty() {
        eprintln!("Usage: notilens init --agent <name> --token <token> --secret <secret>");
        std::process::exit(1);
    }
    if let Err(e) = save_agent(&agent, &token, &secret) {
        eprintln!("Error saving agent: {}", e);
        std::process::exit(1);
    }
    println!("✔ Agent '{}' saved", agent);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn gen_run_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let suffix = (ms ^ (std::process::id() as u128 * 0x9e3779b97f4a7c15)) & 0xFFFF_FFFF;
    format!("run_{}_{:08x}", ms, suffix)
}

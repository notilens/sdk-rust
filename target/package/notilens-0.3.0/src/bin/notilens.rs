use notilens::{
    calc_duration, delete_state, get_agent, get_state_file, list_agents, now_ms, read_state,
    remove_agent, save_agent, update_state, write_state, VERSION,
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

        "task.start" => {
            let flags = parse_flags(rest);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut s = Map::new();
            s.insert("agent".into(),       Value::from(flags.agent.as_str()));
            s.insert("task".into(),        Value::from(flags.task_id.as_str()));
            s.insert("start_time".into(),  Value::from(now_ms()));
            s.insert("retry_count".into(), Value::from(0));
            s.insert("loop_count".into(),  Value::from(0));
            write_state(&sf, &s);
            send_notify("task.started", "Task started", &flags);
            println!("▶  Started: {} | {}", flags.agent, flags.task_id);
        }

        "task.progress" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            update_state(&sf, u);
            send_notify("task.progress", msg, &flags);
            println!("⏳ Progress: {} | {}", flags.agent, flags.task_id);
        }

        "task.loop" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let s = read_state(&sf);
            let count = notilens::state::to_i64(s.get("loop_count")) + 1;
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            u.insert("loop_count".into(),  Value::from(count));
            update_state(&sf, u);
            send_notify("task.loop", msg, &flags);
            println!("🔄 Loop ({}): {} | {}", count, flags.agent, flags.task_id);
        }

        "task.retry" => {
            let flags = parse_flags(rest);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let s = read_state(&sf);
            let count = notilens::state::to_i64(s.get("retry_count")) + 1;
            let mut u = Map::new();
            u.insert("duration_ms".into(),  Value::from(calc_duration(&sf)));
            u.insert("retry_count".into(),  Value::from(count));
            update_state(&sf, u);
            send_notify("task.retry", "Retrying task", &flags);
            println!("🔁 Retry: {} | {}", flags.agent, flags.task_id);
        }

        "task.stop" => {
            let flags = parse_flags(rest);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let dur = calc_duration(&sf);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(dur));
            update_state(&sf, u);
            send_notify("task.stopped", "Task stopped", &flags);
            println!("⏹  Stopped: {} | {} ({} ms)", flags.agent, flags.task_id, dur);
        }

        "task.error" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            u.insert("last_error".into(),  Value::from(msg));
            update_state(&sf, u);
            send_notify("task.error", msg, &flags);
            eprintln!("❌ Error: {}", msg);
        }

        "task.fail" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            update_state(&sf, u);
            send_notify("task.failed", msg, &flags);
            delete_state(&sf);
            println!("💥 Failed: {} | {}", flags.agent, flags.task_id);
        }

        "task.timeout" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            update_state(&sf, u);
            send_notify("task.timeout", msg, &flags);
            delete_state(&sf);
            println!("⏰ Timeout: {} | {}", flags.agent, flags.task_id);
        }

        "task.cancel" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            update_state(&sf, u);
            send_notify("task.cancelled", msg, &flags);
            delete_state(&sf);
            println!("🚫 Cancelled: {} | {}", flags.agent, flags.task_id);
        }

        "task.terminate" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            update_state(&sf, u);
            send_notify("task.terminated", msg, &flags);
            delete_state(&sf);
            println!("⚠  Terminated: {} | {}", flags.agent, flags.task_id);
        }

        "task.complete" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            update_state(&sf, u);
            send_notify("task.completed", msg, &flags);
            delete_state(&sf);
            println!("✅ Completed: {} | {}", flags.agent, flags.task_id);
        }

        "metric" => {
            let (pos, rest2) = positional_args(rest);
            let flags = parse_flags(&rest2);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let s = read_state(&sf);
            let mut metrics: HashMap<String, Value> = match s.get("metrics") {
                Some(Value::Object(m)) => {
                    m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                }
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
            let sf = get_state_file(&flags.agent, &flags.task_id);
            if let Some(key) = pos.first() {
                let s = read_state(&sf);
                let mut metrics: HashMap<String, Value> = match s.get("metrics") {
                    Some(Value::Object(m)) => {
                        m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                    }
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
            send_notify("output.generated", msg, &flags);
        }

        "output.fail" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            send_notify("output.failed", msg, &flags);
        }

        "input.required" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            send_notify("input.required", msg, &flags);
        }

        "input.approve" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            send_notify("input.approved", msg, &flags);
        }

        "input.reject" => {
            let (pos, rest2) = positional_args(rest);
            let msg = pos.first().map(|s| s.as_str()).unwrap_or("");
            let flags = parse_flags(&rest2);
            send_notify("input.rejected", msg, &flags);
        }

        "emit" => {
            if rest.len() < 2 {
                eprintln!("Usage: notilens emit <event> <message> --agent <agent>");
                std::process::exit(1);
            }
            let event = &rest[0];
            let msg   = &rest[1];
            let flags = parse_flags(&rest[2..]);
            let sf = get_state_file(&flags.agent, &flags.task_id);
            let mut u = Map::new();
            u.insert("duration_ms".into(), Value::from(calc_duration(&sf)));
            update_state(&sf, u);
            send_notify(event, msg, &flags);
            println!("📡 Event emitted: {}", event);
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
    task_id:       String,
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
        task_id: String::new(),
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
            "--agent"        => { if i + 1 < args.len() { f.agent        = args[i+1].clone(); i += 1; } }
            "--task"         => { if i + 1 < args.len() { f.task_id      = args[i+1].clone(); i += 1; } }
            "--type"         => { if i + 1 < args.len() { f.typ          = args[i+1].clone(); i += 1; } }
            "--image_url"    => { if i + 1 < args.len() { f.image_url    = args[i+1].clone(); i += 1; } }
            "--open_url"     => { if i + 1 < args.len() { f.open_url     = args[i+1].clone(); i += 1; } }
            "--download_url" => { if i + 1 < args.len() { f.download_url = args[i+1].clone(); i += 1; } }
            "--tags"         => { if i + 1 < args.len() { f.tags         = args[i+1].clone(); i += 1; } }
            "--is_actionable"=> { if i + 1 < args.len() { f.is_actionable= args[i+1].clone(); i += 1; } }
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
    if f.task_id.is_empty() {
        f.task_id = format!("task_{}", now_ms());
    }
    f
}

// ── Core send ─────────────────────────────────────────────────────────────────

fn get_event_type(event: &str) -> &'static str {
    notilens::notify::get_event_type(event)
}

fn is_actionable_default(event: &str) -> bool {
    notilens::notify::is_actionable_default(event)
}

fn send_notify(event: &str, message: &str, f: &Flags) {
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

    let sf = get_state_file(&f.agent, &f.task_id);
    let state = read_state(&sf);

    let mut meta: HashMap<String, Value> = HashMap::new();
    meta.insert("agent".into(), Value::from(f.agent.as_str()));

    let dur = notilens::state::to_i64(state.get("duration_ms"));
    let rc  = notilens::state::to_i64(state.get("retry_count"));
    let lc  = notilens::state::to_i64(state.get("loop_count"));
    if dur > 0 { meta.insert("duration_ms".into(),  Value::from(dur)); }
    if rc  > 0 { meta.insert("retry_count".into(),  Value::from(rc));  }
    if lc  > 0 { meta.insert("loop_count".into(),   Value::from(lc));  }

    if let Some(Value::Object(metrics)) = state.get("metrics") {
        for (k, v) in metrics {
            meta.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &f.meta {
        meta.insert(k.clone(), Value::from(v.as_str()));
    }

    let title = format!("{} | {} | {}", f.agent, f.task_id, event);

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
    payload.insert("task_id".into(),       Value::from(f.task_id.as_str()));
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

// ── Init command ──────────────────────────────────────────────────────────────

fn run_init(args: &[String]) {
    let mut agent = String::new();
    let mut token = String::new();
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

// ── Usage ─────────────────────────────────────────────────────────────────────

fn print_usage() {
    print!(
        r#"Usage:
  notilens init --agent <name> --token <token> --secret <secret>
  notilens agents
  notilens remove-agent <agent>

Task Lifecycle:
  notilens task.start     --agent <agent> [--task <id>]
  notilens task.progress  "msg" --agent <agent> [--task <id>]
  notilens task.loop      "msg" --agent <agent> [--task <id>]
  notilens task.retry           --agent <agent> [--task <id>]
  notilens task.stop            --agent <agent> [--task <id>]
  notilens task.error     "msg" --agent <agent> [--task <id>]
  notilens task.fail      "msg" --agent <agent> [--task <id>]
  notilens task.timeout   "msg" --agent <agent> [--task <id>]
  notilens task.cancel    "msg" --agent <agent> [--task <id>]
  notilens task.terminate "msg" --agent <agent> [--task <id>]
  notilens task.complete  "msg" --agent <agent> [--task <id>]

Output / Input:
  notilens output.generate "msg" --agent <agent> [--task <id>]
  notilens output.fail     "msg" --agent <agent> [--task <id>]
  notilens input.required  "msg" --agent <agent> [--task <id>]
  notilens input.approve   "msg" --agent <agent> [--task <id>]
  notilens input.reject    "msg" --agent <agent> [--task <id>]

Metrics:
  notilens metric       tokens=512 cost=0.003 --agent <agent> --task <id>
  notilens metric.reset tokens               --agent <agent> --task <id>
  notilens metric.reset                      --agent <agent> --task <id>

Generic:
  notilens emit <event> "msg" --agent <agent>

Options:
  --agent <name>
  --task <id>
  --type success|warning|urgent|info
  --meta key=value   (repeatable)
  --image_url <url>
  --open_url <url>
  --download_url <url>
  --tags "tag1,tag2"
  --is_actionable true|false

Other:
  notilens version
"#
    );
}

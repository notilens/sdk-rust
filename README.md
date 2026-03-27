# NotiLens Rust SDK

Rust SDK and CLI for [NotiLens](https://notilens.com) — task lifecycle notifications for AI agents, background jobs, and any Rust application.

## Installation

Add to `Cargo.toml`:

```toml
[dependencies]
notilens = "0.4"
```

## Quick Start

```rust
use notilens::NotiLens;

let nl = NotiLens::init("my-agent", None)?;

let mut run = nl.task("report");
run.start();
run.progress("Processing...");
run.complete("Done!");
```

## Credentials

Resolved in order:
1. `Options { token, secret, .. }` passed to `init()`
2. `NOTILENS_TOKEN` / `NOTILENS_SECRET` env vars
3. Saved CLI config (`notilens init --agent ...`)

```rust
use notilens::{NotiLens, Options};

let nl = NotiLens::init("my-agent", Some(Options {
    token:     "your-token".into(),
    secret:    "your-secret".into(),
    state_ttl: 86400, // optional — orphaned state TTL in seconds (default: 86400)
}))?;
```

## SDK Reference

### Task Lifecycle

`nl.task(label)` creates a `Run` — an isolated execution context. Multiple concurrent runs of the same label never conflict.

```rust
let mut run = nl.task("email");   // create a run for the "email" task
run.queue();                      // optional — pre-start signal
run.start();                      // begin the run

run.progress("Fetching data...");
run.loop_step("Processing item 42");
run.retry();
run.pause("Waiting for rate limit");
run.resume("Resuming work");
run.wait("Waiting for tool response");
run.stop();
run.error("Quota exceeded");      // non-fatal, run continues

// Terminal — pick one
run.complete("All done!");
run.fail("Unrecoverable error");
run.timeout("Timed out after 5m");
run.cancel("Cancelled by user");
run.terminate("Force-killed");
```

### Output & Input Events

```rust
run.output_generated("Report ready");
run.output_failed("Rendering failed");

run.input_required("Approve deployment?");
run.input_approved("Approved");
run.input_rejected("Rejected");
```

### Metrics

Numeric values accumulate; strings are replaced.

```rust
run.metric("tokens", 512);
run.metric("tokens", 128);           // now 640
run.metric("cost", 0.003f64);

run.reset_metrics(Some("tokens"));   // reset one key
run.reset_metrics(None);             // reset all
```

### Automatic Timing

NotiLens automatically tracks task timing. These fields are included in every notification's `meta` payload when non-zero:

| Field | Description |
|-------|-------------|
| `total_duration_ms` | Wall-clock time since `start` |
| `queue_ms` | Time between `queue` and `start` |
| `pause_ms` | Cumulative time spent paused |
| `wait_ms` | Cumulative time spent waiting |
| `active_ms` | Active time (`total − pause − wait`) |

### Generic Events

```rust
use notilens::TrackOptions;
use std::collections::HashMap;

nl.track("custom.event", "Something happened", None);
run.track("custom.event", "Run-level event", None);

let mut meta = HashMap::new();
meta.insert("key".into(), serde_json::json!("value"));
run.track("custom.event", "With meta", Some(TrackOptions { meta }));
```

### Full Example

```rust
use notilens::NotiLens;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nl  = NotiLens::init("summarizer", None)?;
    let mut run = nl.task("report");
    run.start();

    match process() {
        Ok(tokens) => {
            run.metric("tokens", tokens);
            run.output_generated("Summary ready");
            run.complete("All done!");
        }
        Err(e) => run.fail(&e.to_string()),
    }
    Ok(())
}
```

## CLI

### Install

```bash
cargo install notilens
```

### Configure

```bash
notilens init --agent my-agent --token TOKEN --secret SECRET
notilens agents
notilens remove-agent my-agent
```

### Commands

`--task` is a semantic label (e.g. `email`, `report`). Each `task.start` creates an isolated run internally — concurrent executions of the same label never conflict.

```bash
# Task lifecycle
notilens task.queue                      --agent my-agent --task email
notilens task.start                      --agent my-agent --task email
notilens task.progress  "Fetching data"  --agent my-agent --task email
notilens task.loop      "Item 5/100"     --agent my-agent --task email
notilens task.retry                      --agent my-agent --task email
notilens task.pause     "Rate limited"   --agent my-agent --task email
notilens task.resume    "Resuming"       --agent my-agent --task email
notilens task.wait      "Awaiting tool"  --agent my-agent --task email
notilens task.stop                       --agent my-agent --task email
notilens task.error     "Quota hit"      --agent my-agent --task email
notilens task.fail      "Fatal error"    --agent my-agent --task email
notilens task.timeout   "Timed out"      --agent my-agent --task email
notilens task.cancel    "Cancelled"      --agent my-agent --task email
notilens task.terminate "Force stop"     --agent my-agent --task email
notilens task.complete  "Done!"          --agent my-agent --task email

# Output / Input
notilens output.generate "Report ready"  --agent my-agent --task email
notilens output.fail     "Render failed" --agent my-agent --task email
notilens input.required  "Approve?"      --agent my-agent --task email
notilens input.approve   "Approved"      --agent my-agent --task email
notilens input.reject    "Rejected"      --agent my-agent --task email

# Metrics (accumulated per run)
notilens metric       tokens=512 cost=0.003 --agent my-agent --task email
notilens metric.reset tokens               --agent my-agent --task email
notilens metric.reset                      --agent my-agent --task email

# Generic
notilens track my.event "Something happened" --agent my-agent

# Version
notilens version
```

`task.start` prints the internal `run_id` to stdout.

## Requirements

- Rust 1.70+
- Dependencies: `serde`, `serde_json`, `ureq` (all lightweight)

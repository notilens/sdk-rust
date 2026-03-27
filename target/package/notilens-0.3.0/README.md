# NotiLens Rust SDK

Rust SDK and CLI for [NotiLens](https://notilens.com) — task lifecycle notifications for AI agents, background jobs, and any Rust application.

## Installation

Add to `Cargo.toml`:

```toml
[dependencies]
notilens = "0.3"
```

## Quick Start

```rust
use notilens::NotiLens;

let mut nl = NotiLens::init("my-agent", None)?;

let task_id = nl.task_start(None);
nl.task_progress("Processing...", &task_id);
nl.task_complete("Done!", &task_id);
```

## Credentials

Resolved in order:
1. `Options { token, secret }` passed to `init()`
2. `NOTILENS_TOKEN` / `NOTILENS_SECRET` env vars
3. Saved CLI config (`notilens init --agent ...`)

```rust
use notilens::{NotiLens, Options};

let nl = NotiLens::init("my-agent", Some(Options {
    token: "your-token".into(),
    secret: "your-secret".into(),
}))?;
```

## SDK Reference

### Task Lifecycle

```rust
let task_id = nl.task_start(None);                          // auto-generated ID
let task_id = nl.task_start(Some("my-task-123"));           // custom ID

nl.task_progress("Fetching data...", &task_id);
nl.task_loop("Processing item 42", &task_id);
nl.task_retry(&task_id);
nl.task_stop(&task_id);
nl.task_error("Quota exceeded", &task_id);                  // non-fatal
nl.task_complete("All done!", &task_id);                    // terminal
nl.task_fail("Unrecoverable error", &task_id);              // terminal
nl.task_timeout("Timed out after 5m", &task_id);            // terminal
nl.task_cancel("Cancelled by user", &task_id);              // terminal
nl.task_terminate("Force-killed", &task_id);                // terminal
```

### Output & Input Events

```rust
nl.output_generated("Report ready", &task_id);
nl.output_failed("Rendering failed", &task_id);

nl.input_required("Approve deployment?", &task_id);
nl.input_approved("Approved", &task_id);
nl.input_rejected("Rejected", &task_id);
```

### Metrics

Numeric values accumulate; strings are replaced.

```rust
nl.metric("tokens", 512);
nl.metric("tokens", 128);    // now 640

nl.reset_metrics(Some("tokens"));   // reset one key
nl.reset_metrics(None);             // reset all
```

### Generic Events

```rust
use notilens::EmitOptions;
use std::collections::HashMap;

nl.emit("custom.event", "Something happened", None);

let mut meta = HashMap::new();
meta.insert("key".into(), serde_json::json!("value"));
nl.emit("custom.event", "With meta", Some(EmitOptions { meta }));
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

```bash
# Task lifecycle
notilens task.start     --agent my-agent --task job-123
notilens task.progress  "Fetching data" --agent my-agent --task job-123
notilens task.loop      "Item 5/100"    --agent my-agent --task job-123
notilens task.retry                     --agent my-agent --task job-123
notilens task.stop                      --agent my-agent --task job-123
notilens task.error     "Quota hit"     --agent my-agent --task job-123
notilens task.fail      "Fatal error"   --agent my-agent --task job-123
notilens task.timeout   "Timed out"     --agent my-agent --task job-123
notilens task.cancel    "Cancelled"     --agent my-agent --task job-123
notilens task.terminate "Force stop"    --agent my-agent --task job-123
notilens task.complete  "Done!"         --agent my-agent --task job-123

# Output / Input
notilens output.generate "Report ready"  --agent my-agent --task job-123
notilens output.fail     "Render failed" --agent my-agent --task job-123
notilens input.required  "Approve?"      --agent my-agent --task job-123
notilens input.approve   "Approved"      --agent my-agent --task job-123
notilens input.reject    "Rejected"      --agent my-agent --task job-123

# Metrics
notilens metric       tokens=512 cost=0.003  --agent my-agent --task job-123
notilens metric.reset tokens                 --agent my-agent --task job-123
notilens metric.reset                        --agent my-agent --task job-123

# Generic
notilens emit my.event "Something happened"  --agent my-agent

# Version
notilens version
```

## Requirements

- Rust 1.70+
- Dependencies: `serde`, `serde_json`, `ureq` (all lightweight)

use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

pub const VERSION: &str = "0.3.0";
const WEBHOOK_URL: &str = "https://hook.notilens.com/webhook/{}/send";

pub fn get_event_type(event: &str) -> &'static str {
    match event {
        "task.completed" | "output.generated" | "input.approved" => "success",
        "task.failed" | "task.timeout" | "task.error" | "task.terminated" | "output.failed" => "urgent",
        "task.retry" | "task.cancelled" | "input.required" | "input.rejected" => "warning",
        _ => "info",
    }
}

pub fn is_actionable_default(event: &str) -> bool {
    matches!(
        event,
        "task.error"
            | "task.failed"
            | "task.timeout"
            | "task.retry"
            | "task.loop"
            | "output.failed"
            | "input.required"
            | "input.rejected"
    )
}

pub fn send_http(
    token: &str,
    secret: &str,
    payload: &HashMap<String, Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = WEBHOOK_URL.replace("{}", token);
    let body = serde_json::to_string(payload)?;

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();

    agent
        .post(&url)
        .set("Content-Type", "application/json")
        .set("X-NOTILENS-KEY", secret)
        .set("User-Agent", &format!("NotiLens-SDK/{}", VERSION))
        .send_string(&body)?;

    Ok(())
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub token: String,
    pub secret: String,
}

fn config_path() -> PathBuf {
    let home = dirs_next();
    home.join(".notilens_config.json")
}

fn dirs_next() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn load_config() -> HashMap<String, AgentConfig> {
    let path = config_path();
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

fn save_config(cfg: &HashMap<String, AgentConfig>) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();
    let data = serde_json::to_string_pretty(cfg)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn save_agent(agent: &str, token: &str, secret: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = load_config();
    cfg.insert(
        agent.to_string(),
        AgentConfig {
            token: token.to_string(),
            secret: secret.to_string(),
        },
    );
    save_config(&cfg)
}

pub fn get_agent(agent: &str) -> Option<AgentConfig> {
    load_config().remove(agent)
}

pub fn remove_agent(agent: &str) -> bool {
    let mut cfg = load_config();
    if cfg.remove(agent).is_none() {
        return false;
    }
    let _ = save_config(&cfg);
    true
}

pub fn list_agents() -> Vec<String> {
    load_config().into_keys().collect()
}

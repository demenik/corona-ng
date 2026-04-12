use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub trait PersistentStore<T> {
    fn load(&self) -> Result<T, String>;
    fn save(&self, data: &T) -> Result<(), String>;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

pub struct SecureCredentialStore;

impl SecureCredentialStore {
    pub fn new() -> Self {
        Self
    }

    fn get_entry() -> Result<Entry, String> {
        Entry::new("corona-ng", "default_user").map_err(|e| e.to_string())
    }
}

impl Default for SecureCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentStore<Credentials> for SecureCredentialStore {
    fn load(&self) -> Result<Credentials, String> {
        let entry = Self::get_entry()?;
        let secret = entry.get_password().map_err(|e| e.to_string())?;
        serde_json::from_str(&secret).map_err(|e| e.to_string())
    }

    fn save(&self, data: &Credentials) -> Result<(), String> {
        let entry = Self::get_entry()?;
        let secret = serde_json::to_string(data).map_err(|e| e.to_string())?;
        entry.set_password(&secret).map_err(|e| e.to_string())
    }
}

pub struct JsonScheduleStore {
    path: PathBuf,
}

impl JsonScheduleStore {
    pub fn new() -> Self {
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".config");
        path.push("corona-ng");
        let _ = fs::create_dir_all(&path);
        path.push("schedules.json");
        Self { path }
    }
}

impl Default for JsonScheduleStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentStore<HashMap<String, Vec<String>>> for JsonScheduleStore {
    fn load(&self) -> Result<HashMap<String, Vec<String>>, String> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    fn save(&self, data: &HashMap<String, Vec<String>>) -> Result<(), String> {
        let content = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
        fs::write(&self.path, content).map_err(|e| e.to_string())
    }
}

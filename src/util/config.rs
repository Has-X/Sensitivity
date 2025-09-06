// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    pub override_md5: Option<String>,
}

fn config_path() -> PathBuf {
    if let Some(dir) = dirs_next::config_dir() {
        return dir.join("sensitivity").join("state.json");
    }
    // Fallback to current directory
    PathBuf::from(".sensitivity_state.json")
}

pub fn load_state() -> State {
    let path = config_path();
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(state) = serde_json::from_slice::<State>(&bytes) {
            return state;
        }
    }
    State::default()
}

pub fn save_state(state: &State) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).ok(); }
    let bytes = serde_json::to_vec_pretty(state)?;
    fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))
}


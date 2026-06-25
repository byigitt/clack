//! Serde structs mirroring thock's `config.json` (we reuse its files verbatim).

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct PackConfig {
    pub metadata: Metadata,
    pub sounds: HashMap<String, KeySounds>,
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub name: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(rename = "supportsKeyUp", default)]
    pub supports_key_up: bool,
}

fn default_category() -> String {
    "keyboard".to_string()
}

#[derive(Debug, Deserialize)]
pub struct KeySounds {
    #[serde(default)]
    pub down: Vec<String>,
    #[serde(default)]
    pub up: Vec<String>,
}

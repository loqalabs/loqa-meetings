use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub service: ServiceConfig,
    pub audio: AudioConfig,
    pub obsidian: ObsidianConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub http: HttpConfig,
}

#[derive(Debug, Deserialize)]
pub struct HttpConfig {
    pub bind: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct AudioConfig {
    pub recordings_path: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Deserialize)]
pub struct ObsidianConfig {
    pub vault_path: String,
    pub meetings_folder: String,
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::with_name(path))
            .build()?;

        Ok(settings.try_deserialize()?)
    }
}

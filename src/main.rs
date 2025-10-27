use anyhow::Result;
use loqa_meetings::{AudioFile, Config};
use tracing::info;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cfg = Config::load("config/loqa-meetings")?;

    info!("Loqa Meetings v0.1.0");
    info!("Loaded config: {}", cfg.service.name);
    info!("HTTP server will bind to {}:{}", cfg.service.http.bind, cfg.service.http.port);
    info!("Obsidian vault: {}", cfg.obsidian.vault_path);
    info!("Week 1: Audio file processing");

    // Test with a fixture audio file if it exists
    let fixture_path = "tests/fixtures/sample-meeting.wav";
    if std::path::Path::new(fixture_path).exists() {
        let audio = AudioFile::open(fixture_path)?;

        info!("Successfully loaded audio file!");
        info!("Duration: {:.1} seconds", audio.duration_seconds);
        info!("Sample rate: {} Hz", audio.sample_rate);
        info!("Channels: {}", audio.channels);
    } else {
        info!("No test fixture found at {}", fixture_path);
        info!("To test audio reading, place a .wav file at: {}", fixture_path);
    }

    Ok(())
}

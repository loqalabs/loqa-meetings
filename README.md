# Loqa Meetings

AI-powered meeting transcription service with Obsidian integration.

**Status**: Week 1 of 10-week MVP

## What It Does

- 🎙️ Record meetings (live or from files)
- 📝 Transcribe with Whisper (incremental, during meeting)
- 🤖 AI-generated summaries
- 👥 Speaker diarization
- 📓 Obsidian integration
- 🔒 100% local, privacy-first

## Architecture

- **Language**: Rust
- **STT/LLM**: Provided by [loqa-core](https://github.com/loqalabs/loqa-core) (Go services via NATS)

See [architecture docs](https://github.com/loqalabs/loqa-meta) for details.

## Development Status

Week 1: Project setup + audio file processing

## License

MIT (Open Core)

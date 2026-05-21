# aztui-builder

`aztui-builder` is a production-oriented LangGraph multi-agent system that plans, generates, tests, and iterates on Rust + Ratatui applications. The first target profile is `aztui`: an Azure resource browser with k9s-like navigation and workflow fidelity.

## What this repository contains

- A supervisor-driven LangGraph workflow.
- Eight specialist agents for architecture, Azure integration, TUI engineering, UX, data layer, feature delivery, validation, and polish.
- Sandboxed workspace tools for reading, writing, and listing files.
- Cargo execution tools for `check`, `test`, `clippy`, `build`, and `run`.
- Git commit tooling behind human-in-the-loop approval.
- SQLite checkpoint persistence so interrupted runs can be resumed.
- LangSmith tracing support through environment variables.

## Layout

- `src/aztui_builder/` — application package.
- `config/` — prompt templates and sample app specifications.
- `tests/` — unit and smoke tests.
- `workspaces/` — generated target application workspaces.

## Quick start

1. Install dependencies with `uv sync`.
2. Copy `.env.example` to `.env` and set your credentials.
3. Run `uv run aztui-builder run --spec-file config/aztui_app_spec.md --thread-id aztui-dev`.
4. If the graph interrupts for approval, resume with `uv run aztui-builder resume --thread-id aztui-dev --resume-json '{"approved": true}'`.

## Environment

The runtime supports:

- `ANTHROPIC_API_KEY`
- `LANGSMITH_TRACING`
- `LANGSMITH_API_KEY`
- `LANGSMITH_ENDPOINT`
- `LANGSMITH_WORKSPACE_ID`
- `AZTUI_BUILDER_WORKSPACE_ROOT`
- `AZTUI_BUILDER_CHECKPOINT_DB`
- `TAVILY_API_KEY` for live web search when configured

## Notes

- The generated Rust app does not have to exist yet. This repository provides the orchestration system that will create and evolve it.
- Checkpointing is thread-based. Reuse the same `thread_id` to resume an interrupted run.

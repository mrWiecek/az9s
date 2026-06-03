from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

import typer
from dotenv import load_dotenv
from langgraph.types import Command
from rich.console import Console
from rich.json import JSON
from rich.panel import Panel

from .graph import build_graph
from .state import RuntimeSettings, build_initial_state

app = typer.Typer(no_args_is_help=True)
console = Console()


def _project_root() -> Path:
    return Path(__file__).resolve().parents[2]


def load_settings(
    *,
    app_name: str,
    workspace_subdir: str,
    max_iterations: int,
    allow_git_commits: bool,
    use_memory_checkpointer: bool,
) -> RuntimeSettings:
    app_name = str(app_name).strip()
    workspace_subdir = str(workspace_subdir).strip()
    if app_name.startswith("app="):
        app_name = app_name.removeprefix("app=")
    if workspace_subdir.startswith("workspace="):
        workspace_subdir = workspace_subdir.removeprefix("workspace=")

    project_root = _project_root()
    workspace_root_env = os.getenv("AZTUI_BUILDER_WORKSPACE_ROOT", "src/apps")
    checkpoint_db_env = os.getenv("AZTUI_BUILDER_CHECKPOINT_DB", ".aztui-builder/checkpoints.sqlite")
    workspace_root = (project_root / workspace_root_env / workspace_subdir).resolve()
    workspace_root.mkdir(parents=True, exist_ok=True)
    return RuntimeSettings(
        project_root=project_root,
        workspace_root=workspace_root,
        checkpoint_db=(project_root / checkpoint_db_env).resolve(),
        prompt_dir=(project_root / "config").resolve(),
        app_name=app_name,
        max_iterations=max_iterations,
        allow_git_commits=allow_git_commits,
        use_memory_checkpointer=use_memory_checkpointer,
        enable_web_search=bool(os.getenv("TAVILY_API_KEY")),
    )


def _graph_config(thread_id: str) -> dict[str, Any]:
    return {"configurable": {"thread_id": thread_id}}


def _render_stream(graph, payload: Any, *, thread_id: str) -> None:
    config = _graph_config(thread_id)
    for chunk in graph.stream(
        payload,
        config=config,
        stream_mode=["updates", "values"],
        subgraphs=True,
        version="v2",
    ):
        if chunk["type"] == "updates":
            node_name = next(iter(chunk["data"].keys()))
            console.print(f"[bold cyan]node[/bold cyan] {node_name}")
        if chunk["type"] == "values" and chunk.get("interrupts"):
            interrupt_payload = chunk["interrupts"][0].value
            console.print(Panel.fit(JSON.from_data(interrupt_payload), title="Pending HITL interrupt"))
            return
    snapshot = graph.get_state(config)
    console.print(Panel.fit(JSON.from_data(snapshot.values), title="Final state"))


@app.command()
def run(
    spec_file: Path = typer.Option(..., exists=True, readable=True, help="Path to the app specification markdown file."),
    thread_id: str = typer.Option(..., help="Persistent thread identifier used for checkpointed runs."),
    app_name: str = typer.Option("aztui", help="Target application name."),
    workspace_subdir: str = typer.Option("aztui", help="Directory under src/apps/ that will hold the generated app."),
    max_iterations: int = typer.Option(18, min=1, help="Maximum specialist iterations before the graph stops."),
    allow_git_commits: bool = typer.Option(False, help="Allow git commits after HITL approval."),
    use_memory_checkpointer: bool = typer.Option(False, help="Use in-memory checkpointing instead of SQLite."),
) -> None:
    """Start a new build run from an application specification."""
    load_dotenv()
    settings = load_settings(
        app_name=app_name,
        workspace_subdir=workspace_subdir,
        max_iterations=max_iterations,
        allow_git_commits=allow_git_commits,
        use_memory_checkpointer=use_memory_checkpointer,
    )
    graph = build_graph(settings)
    app_spec = spec_file.read_text(encoding="utf-8")
    initial_state = build_initial_state(
        app_spec=app_spec,
        thread_id=thread_id,
        settings=settings,
        working_directory=settings.workspace_root,
    )
    _render_stream(graph, initial_state, thread_id=thread_id)


@app.command()
def resume(
    thread_id: str = typer.Option(..., help="Persistent thread identifier to resume."),
    resume_json: str = typer.Option(..., help="JSON payload returned to the pending interrupt."),
    app_name: str = typer.Option("aztui", help="Target application name."),
    workspace_subdir: str = typer.Option("aztui", help="Directory under src/apps/ that holds the generated app."),
    allow_git_commits: bool = typer.Option(False, help="Allow git commits after HITL approval."),
    use_memory_checkpointer: bool = typer.Option(False, help="Use in-memory checkpointing instead of SQLite."),
) -> None:
    """Resume a previously interrupted run."""
    load_dotenv()
    settings = load_settings(
        app_name=app_name,
        workspace_subdir=workspace_subdir,
        max_iterations=18,
        allow_git_commits=allow_git_commits,
        use_memory_checkpointer=use_memory_checkpointer,
    )
    graph = build_graph(settings)
    payload = json.loads(resume_json)
    _render_stream(graph, Command(resume=payload), thread_id=thread_id)


@app.command("show-state")
def show_state(
    thread_id: str = typer.Option(..., help="Persistent thread identifier to inspect."),
    app_name: str = typer.Option("aztui", help="Target application name."),
    workspace_subdir: str = typer.Option("aztui", help="Directory under src/apps/."),
    use_memory_checkpointer: bool = typer.Option(False, help="Use in-memory checkpointing instead of SQLite."),
) -> None:
    """Display the latest checkpointed graph state."""
    load_dotenv()
    settings = load_settings(
        app_name=app_name,
        workspace_subdir=workspace_subdir,
        max_iterations=18,
        allow_git_commits=False,
        use_memory_checkpointer=use_memory_checkpointer,
    )
    graph = build_graph(settings)
    snapshot = graph.get_state(_graph_config(thread_id))
    console.print(JSON.from_data(snapshot.values))


if __name__ == "__main__":
    app()

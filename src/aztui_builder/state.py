from __future__ import annotations

import operator
from pathlib import Path
from typing import Any, Literal

from pydantic import BaseModel, Field
from typing_extensions import Annotated, TypedDict

AgentName = Literal[
    "project_architect",
    "authentication_azure_specialist",
    "core_tui_engineer",
    "ui_ux_navigation_specialist",
    "data_layer_engineer",
    "feature_implementer",
    "tester_validator",
    "reviewer_polisher",
]

SupervisorTarget = AgentName | Literal["human_checkpoint", "END"]

SPECIALIST_AGENTS: tuple[AgentName, ...] = (
    "project_architect",
    "authentication_azure_specialist",
    "core_tui_engineer",
    "ui_ux_navigation_specialist",
    "data_layer_engineer",
    "feature_implementer",
    "tester_validator",
    "reviewer_polisher",
)


class RuntimeSettings(BaseModel):
    project_root: Path
    workspace_root: Path
    checkpoint_db: Path
    prompt_dir: Path
    app_name: str = "aztui"
    model_provider: str = "anthropic"
    supervisor_model: str = "claude-opus-4-1"
    specialist_model: str = "claude-sonnet-4-5"
    temperature: float = 0.0
    max_iterations: int = 18
    allow_git_commits: bool = False
    enable_web_search: bool = True
    use_memory_checkpointer: bool = False

    model_config = {"arbitrary_types_allowed": True}


class SupervisorDecision(BaseModel):
    next_agent: SupervisorTarget = Field(description="Next agent to run or END")
    rationale: str = Field(description="Reason for the routing choice")
    requires_human: bool = Field(default=False)
    checkpoint_reason: str | None = Field(default=None)
    target_file: str | None = Field(default=None)
    iteration_goal: str = Field(description="What the next step should accomplish")


class SpecialistResult(BaseModel):
    summary: str
    plan_delta: list[str] = Field(default_factory=list)
    current_file: str | None = None
    generated_files: list[str] = Field(default_factory=list)
    errors: list[str] = Field(default_factory=list)
    validation_results: dict[str, Any] = Field(default_factory=dict)
    next_recommendation: str | None = None


class BuildState(TypedDict, total=False):
    messages: Annotated[list[dict[str, Any]], operator.add]
    logs: Annotated[list[str], operator.add]
    plan: list[str]
    app_spec: str
    app_name: str
    active_agent: str
    current_file: str | None
    working_directory: str
    generated_files: Annotated[list[str], operator.add]
    validation_results: dict[str, Any]
    errors: Annotated[list[str], operator.add]
    pending_actions: list[str]
    interrupt_payload: dict[str, Any] | None
    human_response: dict[str, Any] | str | bool | None
    iteration_count: int
    max_iterations: int
    thread_id: str
    last_decision: dict[str, Any]
    final_report: str | None
    app_blueprint: dict[str, Any]


def state_message(role: str, content: str, name: str | None = None) -> dict[str, Any]:
    message: dict[str, Any] = {"role": role, "content": content}
    if name:
        message["name"] = name
    return message


def build_initial_state(
    *,
    app_spec: str,
    thread_id: str,
    settings: RuntimeSettings,
    working_directory: Path,
) -> BuildState:
    initial_plan = [
        "Clarify architecture and execution plan for the target Rust application.",
        "Create or update Rust workspace structure and dependencies.",
        "Implement features iteratively with cargo validation loops.",
        "Review, polish, and summarize the resulting application state.",
    ]
    return BuildState(
        messages=[
            state_message(
                "user",
                app_spec,
            )
        ],
        logs=["Initialized build state."],
        plan=initial_plan,
        app_spec=app_spec,
        app_name=settings.app_name,
        active_agent="supervisor",
        current_file=None,
        working_directory=str(working_directory),
        generated_files=[],
        validation_results={},
        errors=[],
        pending_actions=[],
        interrupt_payload=None,
        human_response=None,
        iteration_count=0,
        max_iterations=settings.max_iterations,
        thread_id=thread_id,
        last_decision={},
        final_report=None,
        app_blueprint={"target_stack": ["rust", "ratatui", "crossterm", "sqlite"]},
    )


def summarize_state_for_model(state: BuildState) -> str:
    recent_messages = state.get("messages", [])[-6:]
    message_lines = []
    for item in recent_messages:
        role = item.get("name") or item.get("role", "unknown")
        content = str(item.get("content", "")).strip()
        message_lines.append(f"- {role}: {content[:600]}")
    plan_lines = "\n".join(f"- {entry}" for entry in state.get("plan", []))
    error_lines = "\n".join(f"- {entry}" for entry in state.get("errors", [])[-8:]) or "- none"
    generated_lines = "\n".join(f"- {entry}" for entry in state.get("generated_files", [])[-10:]) or "- none"
    return (
        f"App name: {state.get('app_name', 'unknown')}\n"
        f"Working directory: {state.get('working_directory', '')}\n"
        f"Iteration: {state.get('iteration_count', 0)} / {state.get('max_iterations', 0)}\n"
        f"Current file: {state.get('current_file') or 'none'}\n"
        f"Plan:\n{plan_lines or '- none'}\n\n"
        f"Generated files:\n{generated_lines}\n\n"
        f"Errors:\n{error_lines}\n\n"
        f"Recent messages:\n" + "\n".join(message_lines)
    )

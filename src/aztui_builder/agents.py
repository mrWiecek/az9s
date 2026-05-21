from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path
from typing import Any

from langchain.chat_models import init_chat_model
from langchain_core.messages import HumanMessage
from langchain_core.prompts import ChatPromptTemplate
from langgraph.prebuilt import create_react_agent
from pydantic import ValidationError

from .state import (
    AgentName,
    BuildState,
    RuntimeSettings,
    SPECIALIST_AGENTS,
    SpecialistResult,
    SupervisorDecision,
    summarize_state_for_model,
)


PROMPT_FILES: dict[str, str] = {
    "supervisor": "supervisor.prompt.md",
    "project_architect": "project_architect.prompt.md",
    "authentication_azure_specialist": "authentication_azure_specialist.prompt.md",
    "core_tui_engineer": "core_tui_engineer.prompt.md",
    "ui_ux_navigation_specialist": "ui_ux_navigation_specialist.prompt.md",
    "data_layer_engineer": "data_layer_engineer.prompt.md",
    "feature_implementer": "feature_implementer.prompt.md",
    "tester_validator": "tester_validator.prompt.md",
    "reviewer_polisher": "reviewer_polisher.prompt.md",
    "human_checkpoint": "human_checkpoint.prompt.md",
}


def load_prompt(prompt_dir: Path, prompt_name: str) -> str:
    prompt_path = prompt_dir / PROMPT_FILES[prompt_name]
    return prompt_path.read_text(encoding="utf-8")


def create_chat_model(settings: RuntimeSettings, model_name: str):
    return init_chat_model(
        model=model_name,
        model_provider=settings.model_provider,
        temperature=settings.temperature,
    )


@lru_cache(maxsize=16)
def build_supervisor_chain_cached(
    prompt_dir: str,
    provider: str,
    model_name: str,
    temperature: float,
) -> Any:
    settings = RuntimeSettings(
        project_root=Path("."),
        workspace_root=Path("."),
        checkpoint_db=Path("checkpoints.sqlite"),
        prompt_dir=Path(prompt_dir),
        model_provider=provider,
        supervisor_model=model_name,
        specialist_model=model_name,
        temperature=temperature,
    )
    prompt = ChatPromptTemplate.from_messages(
        [
            ("system", load_prompt(Path(prompt_dir), "supervisor")),
            (
                "human",
                "Application specification:\n{app_spec}\n\nState summary:\n{state_summary}\n\nAvailable agents:\n{available_agents}",
            ),
        ]
    )
    model = create_chat_model(settings, model_name).with_structured_output(SupervisorDecision)
    return prompt | model


def build_supervisor_chain(settings: RuntimeSettings) -> Any:
    return build_supervisor_chain_cached(
        str(settings.prompt_dir),
        settings.model_provider,
        settings.supervisor_model,
        settings.temperature,
    )


def build_specialist_agent(role: AgentName, settings: RuntimeSettings, tools: list[Any]):
    model = create_chat_model(settings, settings.specialist_model)
    prompt = load_prompt(settings.prompt_dir, role)
    return create_react_agent(model=model, tools=tools, prompt=prompt)


def run_supervisor(state: BuildState, settings: RuntimeSettings) -> SupervisorDecision:
    chain = build_supervisor_chain(settings)
    return chain.invoke(
        {
            "app_spec": state.get("app_spec", ""),
            "state_summary": summarize_state_for_model(state),
            "available_agents": ", ".join(SPECIALIST_AGENTS),
        }
    )


def build_specialist_request(role: AgentName, state: BuildState) -> str:
    return (
        f"You are executing the role: {role}.\n"
        f"Target application: {state.get('app_name', 'unknown')}\n"
        f"Working directory: {state.get('working_directory', '')}\n\n"
        f"Application specification:\n{state.get('app_spec', '')}\n\n"
        f"Current shared state summary:\n{summarize_state_for_model(state)}\n\n"
        "Use tools when you need repository context or when you need to write code. "
        "Your final answer must be valid JSON that matches this schema: "
        '{"summary": str, "plan_delta": [str], "current_file": str | null, "generated_files": [str], '
        '"errors": [str], "validation_results": {str: any}, "next_recommendation": str | null}'
    )


def parse_specialist_result(raw_text: str) -> SpecialistResult:
    text = raw_text.strip()
    if text.startswith("```"):
        lines = text.splitlines()
        if len(lines) >= 3:
            text = "\n".join(lines[1:-1]).strip()
    try:
        return SpecialistResult.model_validate(json.loads(text))
    except (json.JSONDecodeError, ValidationError):
        return SpecialistResult(summary=raw_text.strip() or "No summary returned.")


def run_specialist(
    role: AgentName,
    state: BuildState,
    settings: RuntimeSettings,
    tools: list[Any],
) -> SpecialistResult:
    agent = build_specialist_agent(role, settings, tools)
    response = agent.invoke(
        {
            "messages": [HumanMessage(content=build_specialist_request(role, state))],
        }
    )
    messages = response.get("messages", [])
    if not messages:
        return SpecialistResult(summary=f"{role} produced no output.")
    final_message = messages[-1]
    content = getattr(final_message, "content", final_message)
    if isinstance(content, list):
        content = "\n".join(str(item) for item in content)
    return parse_specialist_result(str(content))

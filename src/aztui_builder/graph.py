from __future__ import annotations

import sqlite3
from functools import partial
from typing import Any

from langgraph.checkpoint.memory import InMemorySaver
from langgraph.graph import END, START, StateGraph
from langgraph.types import interrupt

from .agents import run_specialist, run_supervisor
from .state import AgentName, BuildState, RuntimeSettings, SPECIALIST_AGENTS, state_message
from .tools import build_toolkit

try:
    from langgraph.checkpoint.sqlite import SqliteSaver
except ImportError:  # pragma: no cover
    SqliteSaver = None


def _make_checkpointer(settings: RuntimeSettings):
    if settings.use_memory_checkpointer or SqliteSaver is None:
        return InMemorySaver()
    settings.checkpoint_db.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(settings.checkpoint_db, check_same_thread=False)
    saver = SqliteSaver(connection)
    setup = getattr(saver, "setup", None)
    if callable(setup):
        setup()
    return saver


def supervisor_node(state: BuildState, settings: RuntimeSettings) -> BuildState:
    decision = run_supervisor(state, settings)
    updates: BuildState = {
        "active_agent": decision.next_agent,
        "last_decision": decision.model_dump(),
        "interrupt_payload": None,
        "logs": [f"Supervisor selected {decision.next_agent}: {decision.rationale}"],
    }
    if decision.target_file:
        updates["current_file"] = decision.target_file
    if decision.requires_human:
        updates["interrupt_payload"] = {
            "question": decision.checkpoint_reason or "Human approval required.",
            "decision": decision.model_dump(),
        }
        updates["active_agent"] = "human_checkpoint"
    if decision.next_agent == "END":
        updates["final_report"] = decision.rationale
    return updates


def human_checkpoint_node(state: BuildState) -> BuildState:
    payload = state.get("interrupt_payload") or {
        "question": "Approve the next step?",
        "decision": state.get("last_decision", {}),
    }
    response = interrupt(payload)
    approved = response if isinstance(response, bool) else bool(getattr(response, "get", lambda *_args, **_kwargs: False)("approved", False))
    if isinstance(response, dict):
        approved = response.get("approved", approved)
    logs = [f"Human checkpoint response received: {response}"]
    updates: BuildState = {"human_response": response, "logs": logs, "interrupt_payload": None}
    if approved:
        next_agent = state.get("last_decision", {}).get("next_agent", "project_architect")
        updates["active_agent"] = next_agent
    else:
        updates["active_agent"] = "supervisor"
        updates["errors"] = ["Human reviewer rejected the proposed action."]
    return updates


def specialist_node(role: AgentName, state: BuildState, settings: RuntimeSettings, tools: list[Any]) -> BuildState:
    result = run_specialist(role=role, state=state, settings=settings, tools=tools)
    plan = list(state.get("plan", []))
    for item in result.plan_delta:
        if item not in plan:
            plan.append(item)
    validation_results = dict(state.get("validation_results", {}))
    validation_results.update(result.validation_results)
    updates: BuildState = {
        "messages": [state_message("assistant", result.summary, name=role)],
        "logs": [f"{role} completed."],
        "plan": plan,
        "generated_files": result.generated_files,
        "errors": result.errors,
        "validation_results": validation_results,
        "current_file": result.current_file or state.get("current_file"),
        "active_agent": "supervisor",
        "iteration_count": state.get("iteration_count", 0) + 1,
    }
    return updates


def route_from_supervisor(state: BuildState) -> str:
    target = state.get("active_agent", "supervisor")
    if target == "human_checkpoint":
        return "human_checkpoint"
    if target == "END":
        return END
    if target in SPECIALIST_AGENTS:
        return target
    return END


def route_after_specialist(state: BuildState) -> str:
    if state.get("iteration_count", 0) >= state.get("max_iterations", 0):
        return END
    if state.get("errors"):
        return "supervisor"
    return "supervisor"


def route_after_human(state: BuildState) -> str:
    target = state.get("active_agent", "supervisor")
    if target in SPECIALIST_AGENTS:
        return target
    return "supervisor"


def build_graph(settings: RuntimeSettings):
    tools = build_toolkit(settings)
    builder = StateGraph(BuildState)
    builder.add_node("supervisor", partial(supervisor_node, settings=settings))
    builder.add_node("human_checkpoint", human_checkpoint_node)
    for role in SPECIALIST_AGENTS:
        builder.add_node(role, partial(specialist_node, role, settings=settings, tools=tools))
    builder.add_edge(START, "supervisor")
    builder.add_conditional_edges(
        "supervisor",
        route_from_supervisor,
        ["human_checkpoint", *SPECIALIST_AGENTS, END],
    )
    builder.add_conditional_edges(
        "human_checkpoint",
        route_after_human,
        ["supervisor", *SPECIALIST_AGENTS],
    )
    for role in SPECIALIST_AGENTS:
        builder.add_conditional_edges(role, route_after_specialist, ["supervisor", END])
    checkpointer = _make_checkpointer(settings)
    return builder.compile(checkpointer=checkpointer)

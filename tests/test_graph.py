from pathlib import Path

from langgraph.types import Command

from aztui_builder import graph as graph_module
from aztui_builder.state import RuntimeSettings, SpecialistResult, SupervisorDecision, build_initial_state


def test_graph_interrupts_and_resumes(monkeypatch, tmp_path: Path) -> None:
    prompt_dir = tmp_path / "config"
    prompt_dir.mkdir(parents=True)
    for name in [
        "supervisor.prompt.md",
        "project_architect.prompt.md",
        "authentication_azure_specialist.prompt.md",
        "core_tui_engineer.prompt.md",
        "ui_ux_navigation_specialist.prompt.md",
        "data_layer_engineer.prompt.md",
        "feature_implementer.prompt.md",
        "tester_validator.prompt.md",
        "reviewer_polisher.prompt.md",
        "human_checkpoint.prompt.md",
    ]:
        (prompt_dir / name).write_text("prompt", encoding="utf-8")

    settings = RuntimeSettings(
        project_root=tmp_path,
        workspace_root=tmp_path / "workspaces" / "aztui",
        checkpoint_db=tmp_path / ".aztui-builder" / "checkpoints.sqlite",
        prompt_dir=prompt_dir,
        use_memory_checkpointer=True,
        max_iterations=4,
    )
    settings.workspace_root.mkdir(parents=True)

    def fake_supervisor(state, settings):
        if state.get("iteration_count", 0) == 0 and not state.get("human_response"):
            return SupervisorDecision(
                next_agent="feature_implementer",
                rationale="Need human approval before implementation.",
                requires_human=True,
                checkpoint_reason="Approve implementation phase?",
                iteration_goal="Start implementation",
            )
        if state.get("iteration_count", 0) == 0:
            return SupervisorDecision(
                next_agent="feature_implementer",
                rationale="Approval granted; proceed.",
                requires_human=False,
                iteration_goal="Implement feature",
            )
        return SupervisorDecision(
            next_agent="END",
            rationale="Implementation step completed for this smoke test.",
            requires_human=False,
            iteration_goal="Stop",
        )

    def fake_specialist(role, state, settings, tools):
        return SpecialistResult(
            summary=f"{role} created a first draft.",
            generated_files=["src/main.rs"],
            current_file="src/main.rs",
            validation_results={"smoke": "ok"},
        )

    monkeypatch.setattr(graph_module, "run_supervisor", fake_supervisor)
    monkeypatch.setattr(graph_module, "run_specialist", fake_specialist)

    graph = graph_module.build_graph(settings)
    initial_state = build_initial_state(
        app_spec="# spec",
        thread_id="thread-graph",
        settings=settings,
        working_directory=settings.workspace_root,
    )
    config = {"configurable": {"thread_id": "thread-graph"}}

    interrupted = graph.invoke(initial_state, config=config, version="v2")
    assert interrupted.interrupts
    assert interrupted.interrupts[0].value["question"] == "Approve implementation phase?"

    resumed = graph.invoke(Command(resume={"approved": True}), config=config, version="v2")
    assert resumed.value["current_file"] == "src/main.rs"
    assert resumed.value["generated_files"] == ["src/main.rs"]
    assert resumed.value["validation_results"]["smoke"] == "ok"

from pathlib import Path

from aztui_builder.state import RuntimeSettings, build_initial_state, summarize_state_for_model


def test_build_initial_state_sets_expected_fields(tmp_path: Path) -> None:
    settings = RuntimeSettings(
        project_root=tmp_path,
        workspace_root=tmp_path / "workspaces" / "aztui",
        checkpoint_db=tmp_path / ".aztui-builder" / "checkpoints.sqlite",
        prompt_dir=tmp_path / "config",
    )
    settings.workspace_root.mkdir(parents=True)
    state = build_initial_state(
        app_spec="# spec",
        thread_id="thread-1",
        settings=settings,
        working_directory=settings.workspace_root,
    )

    assert state["thread_id"] == "thread-1"
    assert state["app_spec"] == "# spec"
    assert state["working_directory"] == str(settings.workspace_root)
    assert state["iteration_count"] == 0
    summary = summarize_state_for_model(state)
    assert "App name: aztui" in summary
    assert "Iteration: 0 / 18" in summary

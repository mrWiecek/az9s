from pathlib import Path

import pytest

from aztui_builder.state import RuntimeSettings
from aztui_builder.tools import WorkspaceSandbox


@pytest.fixture()
def sandbox(tmp_path: Path) -> WorkspaceSandbox:
    workspace_root = tmp_path / "workspaces" / "aztui"
    workspace_root.mkdir(parents=True)
    settings = RuntimeSettings(
        project_root=tmp_path,
        workspace_root=workspace_root,
        checkpoint_db=tmp_path / ".aztui-builder" / "checkpoints.sqlite",
        prompt_dir=tmp_path / "config",
    )
    return WorkspaceSandbox(settings=settings)


def test_workspace_sandbox_reads_and_writes_inside_workspace(sandbox: WorkspaceSandbox) -> None:
    write_result = sandbox.write_file("src/main.rs", "fn main() {}\n")
    assert write_result["error"] is None

    read_result = sandbox.read_file("src/main.rs")
    assert read_result["content"] == "fn main() {}"

    listing = sandbox.list_dir("src")
    assert listing["entries"][0]["name"] == "main.rs"


def test_workspace_sandbox_rejects_escape(sandbox: WorkspaceSandbox) -> None:
    with pytest.raises(ValueError):
        sandbox.resolve_path("../outside.txt")

from __future__ import annotations

import json
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal
from urllib import error, request

from langchain_core.tools import tool
from langsmith import traceable
from langgraph.types import interrupt

from .state import RuntimeSettings

CargoCommand = Literal["check", "test", "clippy", "build", "run"]


@dataclass(slots=True)
class WorkspaceSandbox:
    settings: RuntimeSettings

    def resolve_path(self, relative_path: str) -> Path:
        candidate = (self.settings.workspace_root / relative_path).resolve()
        if self.settings.workspace_root.resolve() in candidate.parents or candidate == self.settings.workspace_root.resolve():
            return candidate
        raise ValueError(f"Path escapes project sandbox: {relative_path}")

    def list_dir(self, relative_path: str = ".") -> dict[str, Any]:
        path = self.resolve_path(relative_path)
        if not path.exists():
            return {"path": relative_path, "entries": [], "error": "Path does not exist"}
        if not path.is_dir():
            return {"path": relative_path, "entries": [], "error": "Path is not a directory"}
        entries = []
        for child in sorted(path.iterdir()):
            entries.append(
                {
                    "name": child.name,
                    "path": str(child.relative_to(self.settings.workspace_root)),
                    "is_dir": child.is_dir(),
                }
            )
        return {"path": relative_path, "entries": entries, "error": None}

    def read_file(self, relative_path: str, start_line: int = 1, end_line: int | None = None) -> dict[str, Any]:
        path = self.resolve_path(relative_path)
        if not path.exists() or not path.is_file():
            return {"path": relative_path, "content": "", "error": "File does not exist"}
        lines = path.read_text(encoding="utf-8").splitlines()
        if start_line < 1:
            raise ValueError("start_line must be >= 1")
        stop = end_line or len(lines)
        selected = lines[start_line - 1 : stop]
        return {
            "path": relative_path,
            "start_line": start_line,
            "end_line": stop,
            "content": "\n".join(selected),
            "error": None,
        }

    def write_file(self, relative_path: str, content: str, append: bool = False) -> dict[str, Any]:
        path = self.resolve_path(relative_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        mode = "a" if append else "w"
        with path.open(mode, encoding="utf-8") as handle:
            handle.write(content)
        return {
            "path": relative_path,
            "bytes_written": len(content.encode("utf-8")),
            "append": append,
            "error": None,
        }

    def run_cargo(
        self,
        command: CargoCommand,
        working_dir: str,
        extra_args: list[str] | None = None,
        timeout_seconds: int = 600,
    ) -> dict[str, Any]:
        cwd = self.resolve_path(working_dir)
        full_command = ["cargo", command]
        if command == "clippy":
            full_command.extend(["--all-targets", "--all-features", "--", "-D", "warnings"])
        elif command == "test":
            full_command.append("--all-features")
        if extra_args:
            full_command.extend(extra_args)
        completed = subprocess.run(
            full_command,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
            check=False,
        )
        return {
            "command": full_command,
            "working_dir": str(cwd.relative_to(self.settings.workspace_root)),
            "exit_code": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
            "success": completed.returncode == 0,
        }

    def git_commit(
        self,
        message: str,
        working_dir: str,
        add_all: bool = False,
        paths: list[str] | None = None,
    ) -> dict[str, Any]:
        if not self.settings.allow_git_commits:
            return {
                "committed": False,
                "error": "Git commits are disabled by configuration.",
            }
        cwd = self.resolve_path(working_dir)
        approval = interrupt(
            {
                "type": "git_commit",
                "message": message,
                "working_dir": str(cwd.relative_to(self.settings.workspace_root)),
                "paths": paths or [],
                "question": "Approve git commit?",
            }
        )
        if isinstance(approval, dict) and not approval.get("approved", False):
            return {"committed": False, "error": "Commit rejected by human reviewer."}
        if approval is False:
            return {"committed": False, "error": "Commit rejected by human reviewer."}
        if add_all:
            subprocess.run(["git", "add", "-A"], cwd=cwd, check=False, capture_output=True, text=True)
        elif paths:
            subprocess.run(["git", "add", *paths], cwd=cwd, check=False, capture_output=True, text=True)
        completed = subprocess.run(
            ["git", "commit", "-m", message],
            cwd=cwd,
            capture_output=True,
            text=True,
            check=False,
        )
        return {
            "committed": completed.returncode == 0,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
            "exit_code": completed.returncode,
        }


def _local_search(settings: RuntimeSettings, query: str, max_results: int) -> list[dict[str, Any]]:
    terms = [token for token in query.lower().split() if token]
    results: list[dict[str, Any]] = []
    search_roots = [settings.prompt_dir, settings.project_root / "README.md"]
    files: list[Path] = []
    for root in search_roots:
        if root.is_file():
            files.append(root)
            continue
        if root.exists():
            files.extend(path for path in root.rglob("*") if path.is_file())
    for path in files:
        text = path.read_text(encoding="utf-8", errors="ignore")
        haystack = text.lower()
        score = sum(haystack.count(term) for term in terms)
        if score == 0:
            continue
        snippet = text[:800]
        results.append(
            {
                "source": str(path.relative_to(settings.project_root)),
                "score": score,
                "snippet": snippet,
            }
        )
    return sorted(results, key=lambda item: item["score"], reverse=True)[:max_results]


def _tavily_search(query: str, max_results: int) -> list[dict[str, Any]]:
    api_key = os.getenv("TAVILY_API_KEY")
    if not api_key:
        return []
    payload = json.dumps(
        {
            "api_key": api_key,
            "query": query,
            "max_results": max_results,
            "search_depth": "advanced",
        }
    ).encode("utf-8")
    req = request.Request(
        "https://api.tavily.com/search",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with request.urlopen(req, timeout=20) as response:
            body = json.loads(response.read().decode("utf-8"))
    except (TimeoutError, error.URLError, error.HTTPError, json.JSONDecodeError):
        return []
    return body.get("results", [])[:max_results]


def build_toolkit(settings: RuntimeSettings) -> list[Any]:
    sandbox = WorkspaceSandbox(settings=settings)

    @tool
    @traceable(run_type="tool", name="list_dir")
    def list_dir(relative_path: str = ".") -> dict[str, Any]:
        """List files and directories within the allowed project sandbox."""
        return sandbox.list_dir(relative_path)

    @tool
    @traceable(run_type="tool", name="read_file")
    def read_file(relative_path: str, start_line: int = 1, end_line: int | None = None) -> dict[str, Any]:
        """Read a file from the allowed project sandbox."""
        return sandbox.read_file(relative_path, start_line=start_line, end_line=end_line)

    @tool
    @traceable(run_type="tool", name="write_file")
    def write_file(relative_path: str, content: str, append: bool = False) -> dict[str, Any]:
        """Write a file inside the allowed project sandbox."""
        return sandbox.write_file(relative_path, content, append=append)

    @tool
    @traceable(run_type="tool", name="run_cargo")
    def run_cargo(
        command: CargoCommand,
        working_dir: str = ".",
        extra_args: list[str] | None = None,
        timeout_seconds: int = 600,
    ) -> dict[str, Any]:
        """Run cargo commands inside a Rust workspace."""
        return sandbox.run_cargo(
            command=command,
            working_dir=working_dir,
            extra_args=extra_args,
            timeout_seconds=timeout_seconds,
        )

    @tool
    @traceable(run_type="tool", name="git_commit")
    def git_commit(
        message: str,
        working_dir: str = ".",
        add_all: bool = False,
        paths: list[str] | None = None,
    ) -> dict[str, Any]:
        """Commit changes in git after a human approval interrupt."""
        return sandbox.git_commit(message=message, working_dir=working_dir, add_all=add_all, paths=paths)

    @tool
    @traceable(run_type="tool", name="docs_search")
    def docs_search(query: str, max_results: int = 5) -> dict[str, Any]:
        """Search local prompts/docs and optionally Tavily for Ratatui, Azure, and LangGraph references."""
        local_results = _local_search(settings, query=query, max_results=max_results)
        remote_results = _tavily_search(query=query, max_results=max_results) if settings.enable_web_search else []
        return {
            "query": query,
            "local_results": local_results,
            "remote_results": remote_results,
        }

    return [list_dir, read_file, write_file, run_cargo, git_commit, docs_search]

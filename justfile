set shell := ["bash", "-cu"]

default:
    @just help

help:
    @echo "Available recipes:"
    @echo "  just setup"
    @echo "  just run                         # uses config/aztui_app_spec.md and thread aztui-dev"
    @echo "  just run spec=config/aztui_app_spec.md thread=my-thread"
    @echo "  just run-no-trace"
    @echo "  just run-short                    # low-iteration + no-trace safety run"
    @echo "  just resume payload='{\"approved\": true}'"
    @echo "  just resume-approve"
    @echo "  just resume-reject"
    @echo "  just state"
    @echo "  just test"
    @echo "  just clean"
    @echo "  just stop                         # kill lingering local orchestrator runs"
    @echo "  just env-check"

# Install and sync dependencies from uv.lock.
setup:
    uv sync

# Start a LangGraph run with tracing configuration from .env.
run spec="config/aztui_app_spec.md" thread="aztui-dev" app="aztui" workspace="aztui" max_iterations="18":
    @spec="{{spec}}"; thread="{{thread}}"; app="{{app}}"; workspace="{{workspace}}"; max_iterations="{{max_iterations}}"; \
    if [[ "$spec" == spec=* ]]; then spec="${spec#spec=}"; fi; \
    if [[ "$spec" == thread=* ]]; then thread="${spec#thread=}"; spec="config/aztui_app_spec.md"; fi; \
    if [[ "$thread" == thread=* ]]; then thread="${thread#thread=}"; fi; \
    if [[ "$thread" == max_iterations=* ]]; then max_iterations="${thread#max_iterations=}"; thread="aztui-dev"; fi; \
    if [[ "$app" == app=* ]]; then app="${app#app=}"; fi; \
    if [[ "$workspace" == workspace=* ]]; then workspace="${workspace#workspace=}"; fi; \
    if [[ "$max_iterations" == max_iterations=* ]]; then max_iterations="${max_iterations#max_iterations=}"; fi; \
    uv run aztui-builder run \
      --spec-file "$spec" \
      --thread-id "$thread" \
      --app-name "$app" \
      --workspace-subdir "$workspace" \
      --max-iterations "$max_iterations"

# Start a LangGraph run with LangSmith tracing disabled for local debugging.
run-no-trace spec="config/aztui_app_spec.md" thread="aztui-dev" app="aztui" workspace="aztui" max_iterations="18":
    @spec="{{spec}}"; thread="{{thread}}"; app="{{app}}"; workspace="{{workspace}}"; max_iterations="{{max_iterations}}"; \
    if [[ "$spec" == spec=* ]]; then spec="${spec#spec=}"; fi; \
    if [[ "$spec" == thread=* ]]; then thread="${spec#thread=}"; spec="config/aztui_app_spec.md"; fi; \
    if [[ "$thread" == thread=* ]]; then thread="${thread#thread=}"; fi; \
    if [[ "$thread" == max_iterations=* ]]; then max_iterations="${thread#max_iterations=}"; thread="aztui-dev"; fi; \
    if [[ "$app" == app=* ]]; then app="${app#app=}"; fi; \
    if [[ "$workspace" == workspace=* ]]; then workspace="${workspace#workspace=}"; fi; \
    if [[ "$max_iterations" == max_iterations=* ]]; then max_iterations="${max_iterations#max_iterations=}"; fi; \
    LANGSMITH_TRACING=false uv run aztui-builder run \
      --spec-file "$spec" \
      --thread-id "$thread" \
      --app-name "$app" \
      --workspace-subdir "$workspace" \
      --max-iterations "$max_iterations"

# Safe default loop when hitting model rate limits.
run-short thread="aztui-dev" max_iterations="4":
    just run-no-trace thread="{{thread}}" max_iterations="{{max_iterations}}"

# Resume a paused run from a human-in-the-loop interrupt.
resume thread="aztui-dev" payload='{"approved": true}' app="aztui" workspace="aztui":
    @thread="{{thread}}"; payload='{{payload}}'; app="{{app}}"; workspace="{{workspace}}"; \
    if [[ "$thread" == thread=* ]]; then thread="${thread#thread=}"; fi; \
    if [[ "$thread" == payload=* ]]; then payload="${thread#payload=}"; thread="aztui-dev"; fi; \
    if [[ "$payload" == payload=* ]]; then payload="${payload#payload=}"; fi; \
    if [[ "$app" == app=* ]]; then app="${app#app=}"; fi; \
    if [[ "$workspace" == workspace=* ]]; then workspace="${workspace#workspace=}"; fi; \
    uv run aztui-builder resume \
      --thread-id "$thread" \
      --resume-json "$payload" \
      --app-name "$app" \
      --workspace-subdir "$workspace"

resume-approve thread="aztui-dev" app="aztui" workspace="aztui":
    @thread="{{thread}}"; app="{{app}}"; workspace="{{workspace}}"; \
    if [[ "$thread" == thread=* ]]; then thread="${thread#thread=}"; fi; \
    if [[ "$app" == app=* ]]; then app="${app#app=}"; fi; \
    if [[ "$workspace" == workspace=* ]]; then workspace="${workspace#workspace=}"; fi; \
    just resume thread="$thread" payload='{"approved": true}' app="$app" workspace="$workspace"

resume-reject thread="aztui-dev" app="aztui" workspace="aztui":
    @thread="{{thread}}"; app="{{app}}"; workspace="{{workspace}}"; \
    if [[ "$thread" == thread=* ]]; then thread="${thread#thread=}"; fi; \
    if [[ "$app" == app=* ]]; then app="${app#app=}"; fi; \
    if [[ "$workspace" == workspace=* ]]; then workspace="${workspace#workspace=}"; fi; \
    just resume thread="$thread" payload='{"approved": false}' app="$app" workspace="$workspace"

# Show latest checkpointed state for a thread.
state thread="aztui-dev" app="aztui" workspace="aztui":
    @thread="{{thread}}"; app="{{app}}"; workspace="{{workspace}}"; \
    if [[ "$thread" == thread=* ]]; then thread="${thread#thread=}"; fi; \
    if [[ "$app" == app=* ]]; then app="${app#app=}"; fi; \
    if [[ "$workspace" == workspace=* ]]; then workspace="${workspace#workspace=}"; fi; \
    uv run aztui-builder show-state \
      --thread-id "$thread" \
      --app-name "$app" \
      --workspace-subdir "$workspace"

test:
    uv run pytest -q

# Remove local Python and generation caches.
clean:
    rm -rf .pytest_cache .aztui-builder
    find . -type d -name '__pycache__' -prune -exec rm -rf {} +

# Kill any lingering local run processes.
stop:
    @pkill -f 'aztui-builder run' || true
    @echo "Stopped lingering aztui-builder run processes (if any)."

# Print whether key env vars are configured without revealing secrets.
env-check:
    @set -a; [ -f .env ] && . ./.env; set +a; \
    for key in ANTHROPIC_API_KEY LANGSMITH_TRACING LANGSMITH_API_KEY LANGSMITH_ENDPOINT LANGSMITH_WORKSPACE_ID LANGSMITH_PROJECT; do \
      if [ -n "${!key:-}" ]; then \
        echo "$key=SET"; \
      else \
        echo "$key=UNSET"; \
      fi; \
    done
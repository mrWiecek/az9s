# az9s Roadmap

az9s is a fast, keyboard-first terminal tool for **platform engineers** who operate Azure,
in the spirit of k9s: browse everything, find things instantly, act on them safely, and
see policy compliance without leaving the terminal.

> Status: **pre-release / development**. Nothing has been published yet.
> Milestones are sized in weeks for a single maintainer and are estimates, not promises.

## Guiding principles

1. **Speed is the feature.** First frame in under 200 ms (from cache), every keypress
   renders in under 16 ms, and the UI never blocks on the network.
2. **Breadth first, depth where it matters.** Every Azure resource gets a useful generic
   view (properties, tags, raw JSON). A small set of resource types that platform
   engineers touch daily get rich views and actions.
3. **Safe by default.** Actions are enabled, but every mutating action needs
   confirmation. Destructive actions need you to type the resource name. A
   `--readonly` flag or config setting disables all actions.
4. **Bring your own login.** Authentication goes through the Azure CLI (`az login`). az9s
   stores no credentials.
5. **Open source, cross-platform.** It must work on Linux, macOS and Windows, and CI
   proves that. Licensed under Apache-2.0.
6. **Management groups are first-class.** Platform engineers work at management-group
   scope. The scope tree (tenant root → management groups → subscriptions) is the
   top of navigation, and Policy is viewed per scope.

## Milestones at a glance

| # | Milestone | Size | Version | Outcome |
|---|---|---|---|---|
| M0 | Foundation & cleanup | 1–2 wk | — | One name, flat repo, honest docs, CI green |
| M1 | Speed | 2–3 wk | **v0.1.0** (first public release) | Instant UI, Resource Graph backend |
| M2 | Inspect | 2 wk | v0.2.0 | Real detail views, palette, config |
| M3 | Policy | 3 wk | v0.3.0 | Azure Policy at management-group scope: assignments, compliance, exemptions |
| M4 | Operate | 3 wk | v0.4.0 | Safe action framework + first actions |
| M5 | Rich views & polish | ongoing | v0.5+ | AKS, Key Vault, logs, themes, packaging |

---

## M0 — Foundation & cleanup (1–2 weeks)

Goal: a repository a stranger can clone, understand and contribute to in 10 minutes.

- [x] **One name: az9s.** Rename the binary (`aztui` → `az9s`), crates (`az9s`,
      `az9s-core`, `az9s-azure`, `az9s-tui`), cache dir, log targets, and the
      `repository` URL in `Cargo.toml`.
- [x] **Flatten the repo.** Move `src/apps/aztui/*` to the repo root
      (`Cargo.toml` + `crates/`) and update the `justfile`.
- [x] **Docs cleanup.** Deleted the old docs folder: about 80 archived iteration
      reports and fix scripts, plus about 8k lines of outdated design docs. Git
      history keeps them. Replaced with `README.md`, `CONTRIBUTING.md` and
      `docs/ARCHITECTURE.md`.
- [x] **License.** Apache-2.0 only. `LICENSE` and `Cargo.toml` now agree.
- [x] **Formatting & lint.** Run `cargo fmt --all` once and add `rustfmt.toml`.
      Keep clippy clean with `-D warnings`.
- [x] **CI (GitHub Actions).** Run fmt, clippy and test on Linux, macOS and Windows
      for every PR.
- [x] **Small correctness fixes.**
  - Replace the hand-written `dirs` stub with the `directories` crate (the stub
    breaks on Windows).
  - Remove the dead `Ctrl+P` "command palette" toggle until the palette exists.
  - Remove the unused SDK client built in `list_subscriptions`.
- [x] Add a PR template, an issue template and `CODEOWNERS`.

**Done when:** `just ci` passes locally and in GitHub Actions, the README is accurate,
and no file or crate says "aztui".

Remaining in M0: confirm the first CI run is green on all three OSes, and verify or
raise `rust-version` (currently `1.75`; not checked against the dependency tree).

## M1 — Speed (2–3 weeks) → v0.1.0

Goal: fix the main complaint that az9s is slow, **at the scale of the target user:
400+ subscriptions** (and potentially 100k+ resources) under a management-group
hierarchy.

Root causes found in the current code:

| Cause | Where | Effect |
|---|---|---|
| Input loop reads **one** key per 250 ms tick | `crates/az9s-tui/src/event.rs` | Navigation lags; holding `j` crawls |
| Azure calls are `await`ed inside the key handler | `crates/az9s-tui/src/app.rs` | UI freezes, spinner never draws |
| Opening a resource group refetches **every resource in the subscription** | `handle_resource_group_select` | Seconds per click on large subs |
| "All subscriptions" views loop over subscriptions **sequentially** | `load_all_*` | `:res all` = 400+ sequential ARM list calls, which takes minutes |
| Every frame re-filters (lowercasing every field) and builds a `ListItem` for **every** row, not just the visible ones | `crates/az9s-tui/src/ui/mod.rs`, `state.rs` | Each frame costs O(total rows); at 100k rows every keypress stalls |
| Startup blocks on `az account list` before the first frame | `App::new` | Slow launch |
| Cache is written but only read as a fallback | `crates/az9s-core` | Data is fetched again on every navigation |
| **Bug:** opening a resource group shows every resource in the subscription; the view is never filtered to the RG | `handle_resource_group_select` | Wrong results, and slow too |

Work items, in order:

- [ ] **Measure first.** Add `tracing` spans with durations around every Azure call,
      cache read and frame render, plus a `--timings` summary on exit. Record
      before and after numbers against a 400+ subscription tenant in the PR
      description.
- [ ] **Event loop rewrite.** Block on input with a timeout, drain all pending
      events, and redraw on change (not on a fixed tick).
- [ ] **Async architecture.** Move all I/O into background tasks that report
      results to the UI over a channel (an action/message pattern). The UI thread
      only renders and updates state. Cancel stale requests when the user moves on.
- [ ] **Azure Resource Graph as the read backend.** One KQL query lists resource
      groups and resources across all subscriptions, with server-side filtering
      (by RG, type, location or tag) and paging. The same backend serves Policy
      later (`policyresources` table).
  - **Scope by management group**, not by passing 400+ subscription IDs. Fall back
    to batched subscription lists only when no management group is readable.
  - **Never pull the whole tenant into memory by default.** Fetch counts first
    (`summarize count()`), then page rows in (`$top` / `$skipToken`) as the user
    scrolls.
  - **Filtering:** filter locally while the full result set is in memory. Above a
    threshold, push `/` filters into the KQL `where` clause (debounced).
  - **Respect Resource Graph throttling.** Read the per-user quota headers, back
    off, and never fire one query per subscription.
  - Trade-off: Resource Graph indexes changes with a small delay. After an action,
    refresh the single resource through ARM `GET`.
- [ ] **Virtualized rendering.** Build widgets only for the visible rows. Precompute
      lowercase search keys once when data arrives, not on every frame.
- [ ] **Fix the RG view bug.** Resources shown under a resource group are filtered to
      that group (server-side through Resource Graph).
- [ ] **Cache-first (stale-while-revalidate).** Render cached data immediately,
      refresh in the background, and show data age in the status bar.
- [ ] **Scope-aware queries.** Resource Graph queries can target a management group
      as well as a list of subscriptions.
- [ ] **Shared clients.** Create one HTTP client and one credential at startup and
      cache the token.
- [ ] **Performance budget in CI.** Add benchmarks for rendering and filtering
      100k-row lists with synthetic data (no Azure needed), plus a startup-time
      check.
- [ ] **Release pipeline.** Publish binaries to GitHub Releases (e.g. `cargo-dist`)
      and support `cargo install az9s`.

**Done when** (measured against a 400+ subscription tenant):

| Scenario | Target |
|---|---|
| Launch with a warm cache → first usable frame | < 200 ms |
| Keypress → rendered frame (scroll or filter), 100k rows loaded | < 16 ms |
| `:rgs all` across 400+ subscriptions, cold | < 3 s |
| `:res all`, first page visible, cold | < 3 s |
| Open a resource group (≤ 1k resources), cold | < 1 s |
| Any network call | never blocks input; a spinner is always visible |

## M2 — Inspect (2 weeks) → v0.2.0

- [ ] Generic resource detail view with three tabs: Overview, Tags, JSON (from
      Resource Graph or ARM).
- [ ] `y` copies the resource ID, `o` opens the resource in the Azure Portal.
- [ ] Filters: `:res type=Microsoft.Compute/virtualMachines`, `loc=westeurope`,
      `tag=env:prod`, and a resource-type summary view.
- [ ] A real command palette with fuzzy search over commands and resources, and
      autocomplete for `:` commands.
- [ ] Config file `~/.config/az9s/config.toml` for default subscription, refresh
      interval, theme, and read-only mode.
- [ ] **Scope tree:** tenant root → management groups → subscriptions → resource
      groups, with `:mg` to jump. This becomes the top-level navigation (needed by M3).
- [ ] `:ctx` to switch tenant.

## M3 — Policy (3 weeks) → v0.3.0

- [ ] Browse policy assignments **per scope, starting at management groups**, with
      inherited assignments shown at child scopes. Browse definitions and
      initiatives (set definitions).
- [ ] Compliance summary per assignment and per scope (Resource Graph
      `policyresources`).
- [ ] Drill down: assignment → non-compliant resources → resource detail. Also the
      reverse: resource → its policy states.
- [ ] Compliance roll-up across the management-group tree: which subscriptions and
      resource groups have the most non-compliant resources.
- [ ] List exemptions (with expiry) and remediation tasks, read-only.
- Policy *actions* (trigger a compliance scan, create or extend exemptions, start
  remediation) come in M4 on top of the action framework.

## M4 — Operate (3 weeks) → v0.4.0

- [ ] **Action framework.**
  - Confirmation dialog for every action. Typed-name confirmation for destructive
    ones.
  - `--readonly` flag.
  - A preview that shows the equivalent `az` command.
  - A local audit log of executed actions.
  - Status-bar tracking of long-running operations.
- [ ] **First actions:**
  - VM start, stop, deallocate and restart
  - App Service and Function App restart
  - Edit tags
  - Delete a resource or resource group
- [ ] **Policy actions:** trigger a compliance scan, create or extend an exemption,
      start remediation.
- [ ] **AKS basics:** clusters, node pools, `get-credentials`, and handing off to k9s.

## M5 — Rich views & polish (ongoing) → v0.5+

- Rich views for AKS, Key Vault (metadata only; secret values only on explicit
  request), Storage, VMs, and App Service log stream.
- Activity log per resource or resource group.
- Custom keybindings and themes.
- Packaging for Homebrew, winget, Scoop and AUR.

---

## Decisions

| Decision | Choice | Status |
|---|---|---|
| Name | az9s (binary, crates, data dir) | **Decided** |
| Who it's for | Platform engineers; open source | **Decided** |
| Product shape | Operations tool (not read-only browser) | **Decided** |
| Generic vs rich views | Generic for all types (M2). Rich views for AKS, VM, App Service/Functions, Key Vault, plus Policy | **Decided** |
| Policy scope | Management groups first-class; Policy (M3) before Operate (M4) | **Decided** |
| Auth | Azure CLI credential (`az login`) | **Decided** |
| Actions default | Enabled with confirmation; `--readonly` available | **Decided** |
| License | Apache-2.0 | **Decided** |
| Read backend | Azure Resource Graph for lists, ARM for single-resource reads and writes | Proposed (validate in M1) |
| Multi-tenant | Switch tenants with `:ctx`; no parallel multi-tenant view | Proposed |

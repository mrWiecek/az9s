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
3. **Safe by default.** Every mutating action needs confirmation. Destructive actions
   need you to type the resource name. A `--readonly` mode disables all actions.
4. **Bring your own login.** Authentication goes through the Azure CLI (`az login`). az9s
   stores no credentials.
5. **Open source, cross-platform.** It must work on Linux, macOS and Windows, and CI
   proves that.

## Milestones at a glance

| # | Milestone | Size | Version | Outcome |
|---|---|---|---|---|
| M0 | Foundation & cleanup | 1–2 wk | — | One name, flat repo, honest docs, CI green |
| M1 | Speed | 2–3 wk | **v0.1.0** (first public release) | Instant UI, Resource Graph backend |
| M2 | Inspect | 2 wk | v0.2.0 | Real detail views, palette, config |
| M3 | Operate | 3 wk | v0.3.0 | Safe action framework + first actions |
| M4 | Policy | 3 wk | v0.4.0 | Azure Policy assignments, compliance, exemptions |
| M5 | Rich views & polish | ongoing | v0.5+ | AKS, Key Vault, logs, themes, packaging |

---

## M0 — Foundation & cleanup (1–2 weeks)

Goal: a repository a stranger can clone, understand and contribute to in 10 minutes.

- [ ] **One name: az9s.** Rename the binary (`aztui` → `az9s`), crates (`az9s`,
      `az9s-core`, `az9s-azure`, `az9s-tui`), cache dir, log targets, and the
      `repository` URL in `Cargo.toml`.
- [ ] **Flatten the repo.** Move `src/apps/aztui/*` to the repo root
      (`Cargo.toml` + `crates/`) and update the `justfile`.
- [ ] **Docs cleanup.** Delete `docs/archive/` (about 80 iteration reports and fix
      scripts; git history keeps them). Merge the remaining docs into a few files:
      `ARCHITECTURE.md`, `AUTHENTICATION.md`, `KEYBINDINGS.md`, `CONTRIBUTING.md`.
- [ ] **License.** Pick one licensing model and make `LICENSE*` files and
      `Cargo.toml` agree.
- [ ] **Formatting & lint.** Run `cargo fmt --all` once and add `rustfmt.toml`.
      Keep clippy clean with `-D warnings`.
- [ ] **CI (GitHub Actions).** Run fmt, clippy and test on Linux, macOS and Windows
      for every PR.
- [ ] **Small correctness fixes.**
  - Replace the hand-written `dirs` stub with the `directories` crate (the stub
    breaks on Windows).
  - Remove the dead `Ctrl+P` "command palette" toggle until the palette exists.
  - Remove the unused SDK client built in `list_subscriptions`.
- [ ] Add a PR template, an issue template and `CODEOWNERS`.

**Done when:** `just ci` passes locally and in GitHub Actions, the README is accurate,
and no file or crate says "aztui".

## M1 — Speed (2–3 weeks) → v0.1.0

Goal: fix the main complaint that az9s is slow. Root causes found in the current code:

| Cause | Where | Effect |
|---|---|---|
| Input loop reads **one** key per 250 ms tick | `aztui-tui/src/event.rs` | Navigation lags; holding `j` crawls |
| Azure calls are `await`ed inside the key handler | `aztui-tui/src/app.rs` | UI freezes, spinner never draws |
| Opening a resource group refetches **every resource in the subscription** | `handle_resource_group_select` | Seconds per click on large subs |
| "All subscriptions" views loop over subscriptions **sequentially** | `load_all_*` | Time grows with subscription count |
| Startup blocks on `az account list` before the first frame | `App::new` | Slow launch |
| Cache is written but only read as a fallback | `aztui-core` | Data is fetched again on every navigation |

Work items:

- [ ] **Event loop rewrite.** Block on input with a timeout, drain all pending
      events, and redraw on change (not on a fixed tick).
- [ ] **Async architecture.** Move all I/O into background tasks that report
      results to the UI over a channel (an action/message pattern). The UI thread
      only renders and updates state. Cancel stale requests when the user moves on.
- [ ] **Azure Resource Graph as the read backend.** One KQL query lists resource
      groups and resources across *all* subscriptions, with server-side filtering
      (by RG, type, location or tag) and paging. The same backend serves Policy
      later (`policyresources` table).
      - Trade-off: Resource Graph indexes changes with a small delay. After an
        action, refresh the single resource through ARM `GET`.
- [ ] **Cache-first (stale-while-revalidate).** Render cached data immediately,
      refresh in the background, and show data age in the status bar.
- [ ] **Shared clients.** Create one HTTP client and one credential at startup and
      cache the token.
- [ ] **Performance budget in CI.** Add a benchmark for list rendering and filtering
      with 10k resources, plus a startup-time check.
- [ ] **Release pipeline.** Publish binaries to GitHub Releases (e.g. `cargo-dist`)
      and support `cargo install az9s`.

**Done when:** a cold start shows cached data in under 200 ms, `:res all` across 20+
subscriptions returns in roughly one Resource Graph round-trip, and scrolling 10k rows
is smooth.

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
- [ ] `:ctx` to switch tenant or subscription. A management-group tree (needed by M4).

## M3 — Operate (3 weeks) → v0.3.0

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
- [ ] **AKS basics:** clusters, node pools, `get-credentials`, and handing off to k9s.

## M4 — Policy (3 weeks) → v0.4.0

- [ ] Browse policy definitions, initiatives (set definitions) and assignments by
      scope (management group, subscription, resource group).
- [ ] Compliance summary per assignment and per scope (Resource Graph
      `policyresources`).
- [ ] Drill down: assignment → non-compliant resources → resource detail. Also the
      reverse: resource → its policy states.
- [ ] List exemptions. Action: trigger an on-demand compliance scan.
- [ ] Later: create or extend exemptions and view remediation tasks.

## M5 — Rich views & polish (ongoing) → v0.5+

- Rich views for AKS, Key Vault (metadata only; secret values only on explicit
  request), Storage, VMs, and App Service log stream.
- Activity log per resource or resource group.
- Custom keybindings and themes.
- Packaging for Homebrew, winget, Scoop and AUR.

---

## Open decisions

| Decision | Recommendation | Status |
|---|---|---|
| Generic views vs rich per-type views | Generic for all types (M2), rich for AKS, VM, App Service/Functions, Key Vault, plus Policy (M3–M5) | Proposed |
| Read backend | Azure Resource Graph for lists, ARM for single-resource reads and writes | Proposed |
| Auth | Azure CLI credential only | **Decided** |
| Actions default | Enabled with confirmation; `--readonly` flag available | Proposed |
| License | MIT OR Apache-2.0 (Rust ecosystem convention) vs MIT only | Open |
| Multi-tenant support | Switch tenants with `:ctx`; no parallel multi-tenant view | Proposed |

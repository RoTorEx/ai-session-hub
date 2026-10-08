# FleetScope

Personal macOS Rust CLI/TUI for finding already running interactive Codex and
Claude agents in existing iTerm2 panes and returning to the exact pane. Users
keep normal launch commands. FleetScope never types into panes or starts,
resumes, stops, or changes agent processes.

## Sessions and navigation

Live includes working, waiting for input and waiting for approval. CPU activity
is not liveness. Shells after exit, noninteractive commands and helper processes
are excluded. Independent instances in one directory remain independent.
Process identity is PID plus microsecond start time; terminal identity is pane
UUID. TTY, stdin attachment and ancestry establish linkage. Titles and project
paths never authorize Jump. Every Jump rereads identity/linkage, selects the
existing window/tab/pane and reads back the selected UUID. Ambiguity blocks it.
Groups use Git worktree roots, otherwise actual working directories. Unknown
paths do not exclude live sessions. Details show provider, directory, process
identity, TTY and location. Unknown context/conversation IDs remain unavailable.
The MVP reports Live; it does not read conversations or terminal screen text.

## Application lifecycle

Scan at launch and while open; no monitoring or resident workers after exit.
Agents are independent. Jump leaves the TUI open in its original pane. English
UI, Russian documentation. Tree and details scroll independently. Refresh keeps
selection by identity, filter and expanded groups where possible. Failed scans
are distinct from empty lists; previous results are stale.

## Defaults and delivery

| Value | Unit | Meaning / reason | Source / impact / checks |
|---|---|---|---|
| 2 | seconds | Default refresh, fresh overview without daemon | Accepted plan; positive configurable value, config/reconciliation tests |
| 0.1.0 | version | Intended first release | Accepted plan, release/version checks |
| 0.0.0 | version | Development base | Enables neighboring increasing-version release flow |

Commands follow neighbors: help, version, update, setup-config, gtd themes/theme.
Themes: classic (default), forest, sunset, ocean, midnight. XDG config convention
and FLEETSCOPE_CONFIG override. Install/update preserve existing configuration.
Binary: ~/.x-cli-fleet-scope/bin. Build: ~/construction_side/fleet-scope.
Public release target: RoTorEx/fleet-scope, Apple Silicon macOS only. Release
preparation and push are separate; real release needs explicit approval.

## Non-goals

No tasks, Inbox, conversation history/search, AI classification, notifications,
daemon, LaunchAgent, mandatory hooks, account or AI Fleet integration. No SSH
agents, tmux, additional providers or terminals in v0.1.

## Code map

Process discovery/iTerm2 integration own identity/linking; the model owns
session grouping/reconciliation; CLI/TUI share that model and Jump. Delivery
belongs to installer, updater and release scripts.

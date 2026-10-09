# FleetScope

Personal macOS Rust CLI/TUI for finding already running interactive Codex and
Claude agents in existing iTerm2 panes and returning to the exact pane. Users
keep normal launch commands. FleetScope never types into panes or starts,
resumes, stops, or changes agent processes.

## Sessions and navigation

Live includes working, waiting for input and waiting for approval. CPU activity
is not liveness. Shells after exit, noninteractive commands and helpers are
excluded. Independent instances in one directory remain independent.
Identity is PID plus microsecond start time; terminal identity is pane UUID.
TTY, stdin attachment and ancestry establish linkage. Every Jump rereads the
live process identity and lineage, resolves the current pane by UUID, checks its
TTY and root PID, selects it and reads back the selected UUID. Cached container
indexes only speed UUID resolution; a moved tab uses fresh UUID lookup. Titles,
paths and terminal contents never authorize Jump. Ambiguity blocks linking.

Groups use Git worktree roots, otherwise actual working directories. Headings
are visual separators: keyboard and mouse select only sessions. Groups stay
expanded. The session list is the entry point; PID, counts, arrows and duplicate
project/provider suffixes do not clutter each entry. Refresh preserves selection
by identity and independent details scroll. Unknown paths do not exclude agents.

## Details and content

Details show status, compact terminal/session metadata, latest request/response
previews and recent history. Labels and section headings use gtd accents; values
use normal text. Classic selection uses dark text on blue for contrast with the
user's iTerm palette. Version stays visible in the header in all normal states.
Duplicate project/cwd paths are shown once; Git root appears only when different.
Technical IDs are hidden behind m. Tab toggles focus; mouse wheel scrolls the
pane under the pointer. A scrollbar and explicit keyboard hint reveal scrolling.
Long messages use four-line/280-character previews; e expands all messages.

Content is read only from the selected, revalidated iTerm2 pane's terminal
scrollback; no archived conversation is guessed by path/title or recency. Codex
and Claude prompt/response markers provide observed text blocks. The unfinished
bottom prompt is excluded because it can be a draft/queued message. Terminal
state markers provide Working, Waiting for input or Needs approval when known;
unrecognized output stays Live with activity unknown. These observations are
format-dependent, may be incomplete and are not authoritative provider events.
No conversation ID is invented. Capture output retains the last 65,536 characters,
up to 32 blocks, each at most 8,192 characters. iTerm2 may supply more scrollback
before the retained tail is taken. Nothing is persisted or uploaded.

## Application lifecycle and synchronization

Scan at launch and while open; no workers/monitoring after exit. Jump leaves the
TUI open in its original pane. English UI/Russian docs. Separate worker lanes keep
Jump independent of inventory/content requests. Two task-owned JXA helpers reuse
API bindings while the TUI is open, have five-second reply deadlines and are
killed/reaped on exit or helper timeout. They never launch iTerm2.

Lightweight native inventory detects exits/starts; only new agent identities,
manual refresh or the metadata timer require a fresh pane map. Closed agents are
removed without re-reading all tabs. Git roots are cached until metadata refresh.
Selected content is read independently, debounced while navigating. Unchanged
snapshots do not redraw. Routine refresh has no transient Live/Updating indicator.
Failed scans are distinct from an empty list; prior results are marked stale.

## Defaults and delivery

| Value | Unit | Meaning / reason | Source / impact / checks |
|---|---|---|---|
| 2 | seconds | Native inventory; prompt opening/exit detection at low cost | User correction + measured benchmark; positive config, reconciliation/identity checks |
| 5 | seconds | Selected content/status; lower Apple Event/terminal work | Benchmark tradeoff; on selection first read is immediate after debounce, format-parser tests |
| 30 | seconds | Full pane/location and Git-root map; moves are uncommon | User explanation; new identities/r bypass timer, Jump resolves current UUID independently |
| 250 | milliseconds | UUID confirmation window after selection | Observed asynchronous iTerm2 front-window update; 10 ms retry pause, no extra full scan or substituted target |
| 150 | milliseconds | Selection debounce before content read | Avoid reading every intermediate session while navigating; no delay on Jump |
| 4 / 280 | lines / characters | Collapsed message preview | User requested readable summaries; expansion and long-scroll tests |
| 65,536 / 32 / 8,192 | characters / blocks / characters | Retained terminal history bounds | Bounded display work/memory; tail can omit older history; not full-conversation capture |
| 5 | seconds | Helper reply deadline | Prevent a stuck API from blocking exit indefinitely; helper failure/reset handling |
| 0.1.0 | version | Intended first release | Accepted plan, release/version checks |
| 0.0.0 | version | Development base | Neighboring increasing-version release flow |

Local benchmark methodology and limits: docs/PERFORMANCE.md. No fixed interval
can promise zero load on every machine; measured defaults balance freshness and
CPU cost, with manual refresh and independently configurable timers.
Commands follow neighbors: help, version, update, setup-config, themes/theme.
Themes: classic (default), forest, sunset, ocean, midnight; gtd color families,
simplified labels. XDG config and FLEETSCOPE_CONFIG override. Install/update
preserve configuration. Binary: ~/.x-cli-fleet-scope/bin. Build cache:
~/construction_side/fleet-scope. Public release: RoTorEx/fleet-scope, Apple Silicon
macOS only. Release preparation/push are separate; real release needs approval.

## Non-goals

No tasks/Inbox, archive-wide conversation search, AI classification, notifications,
daemon, LaunchAgent, mandatory hooks, account or AI Fleet integration. No SSH,
tmux, additional providers or terminals. No agent control or terminal writes.

## Code map

process/iterm own identity/linking and the read-only terminal bridge; model owns
cached inventory/grouping; content owns observed terminal state/text; tui owns
navigation/presentation and timers. CLI/TUI share Jump. benchmark owns explicit
local diagnostics. Delivery belongs to installer/updater/release scripts.

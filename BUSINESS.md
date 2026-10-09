# Sova

Personal macOS Rust CLI/TUI for finding already running interactive Codex and
Claude agents in existing iTerm2 panes and returning to the exact pane. Users
keep normal launch commands. Sova never types into panes or starts,
resumes, stops, or changes agent processes.

## Name and migration

The product is Sova, executable/package/repository `sova`, starting with minor
0.2.0. Default installation is `~/.x-cli-sova/bin/sova`, configuration is
`$XDG_CONFIG_HOME/sova/config.toml` (otherwise `~/.config/sova/config.toml`),
overrides are `SOVA_INSTALL_DIR` and `SOVA_CONFIG`. Earlier FleetScope tags and
release payloads retain their original identities. The renamed installer copies
legacy default configuration only when the new default is absent and no explicit
configuration override was supplied; existing Sova settings always win. Legacy
binaries and profile blocks can be removed after the new installation is verified.

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

At widths of at least 90 terminal columns, Sessions and History share the left
60% column and Details fills the right 40%. Sessions grows with its rows up to
40% of the left column height, leaving History the remaining space. Below 90
columns, Sessions, Details and History stack vertically; the usable minimum is
50 columns by 12 rows. These choices implement the user's compact asymmetric
layout; resize, focus and mouse fixtures protect pane boundaries.

Details contains the visible session name and provider/activity, terminal window,
tab and pane names, full working folder and differing Git root, and process/pane
identity. Horizontal ruled headings separate sections. The name comes from the
observed terminal title. Metadata is always available by scrolling. Latest
exchange is shown only in History, avoiding duplicated content.
Groups display the directory basename, or ~ for home. Duplicate basenames include
the parent directory for distinction; full paths stay in Details and search.

History shows every available message in chronological order, including action
blocks. There are no role filters. Each preview shows the first 100 Unicode
characters, flattened to one paragraph, with an ellipsis for longer messages.
Navigation selects individual messages (question, answer, or action), not lines.
e / Enter in History opens the selected full retained message in the right column
in place of Details. Focus stays in History; subsequent selections update the
reader. Esc/e restores Details. f focuses the right reader for line scrolling.
Refresh preserves the selected message when its text persists or grows; if the
message disappears, the reader closes rather than presenting a different message.
No guessed message timestamps or additional scans are introduced.

Startup focus is Sessions. f / Tab cycles Sessions → History → right pane;
Shift+Tab cycles backward. The active pane has a muted accent border and a footer
focus label, with no filled/bold title. Mouse clicks do not change focus; wheel
operates the pane under the pointer. Arrows/page keys operate on keyboard focus.
History has a selected-message marker; the right pane has an independent scrollbar.
Help/Quit remain visible. t cycles palettes and saves the choice immediately.
The default calm palette uses terminal-default text, neutral separators and a
muted blue focus accent. Existing gtd palettes remain available as options.

Sova does not rename agent chats, terminal tabs or panes and has no local aliases.
The user explicitly kept those controls with the agent/terminal; native chat
names can be changed inside supported agent CLIs using their /rename command.

Detailed content is read only from the selected, revalidated iTerm2 pane's
currently visible screen; no archived conversation is guessed by path/title or recency. Codex
and Claude prompt/response markers provide observed text blocks. The unfinished
bottom prompt is excluded because it can be a draft/queued message. Terminal
state markers provide Working, Waiting for input or Needs approval when known;
unrecognized output stays Live with activity unknown. These observations are
format-dependent, may be incomplete and are not authoritative provider events.
No conversation ID is invented. Capture output retains the last 65,536 characters,
up to 32 blocks, each at most 8,192 characters. The scripting bridge supplies
visible rows, not the complete scrollback. Nothing is persisted or uploaded.

## Activity and opening responses

The session list uses one fixed symbol column: ● Working, ○ Idle, ! Approval,
? question/action requested, × reported blocker, ◆ new response ready and not
observed opened, ✓ pane opened after readiness, · activity unknown. Questions
and blockers retain their symbol after opening because they still need user
attention. Details and help explain symbols; no animation is used.

All linked agents receive a read-only status pass on the content timer. Native
PID/start/TTY/ancestry is checked once per batch; the terminal bridge confirms
UUID-selected pane TTY/root and returns only its last 24 visible rows, at most
8,192 characters. Only parsed state and an in-memory normalized response hash
are retained for unselected sessions. No archived/full conversation is read.

Ready requires an observed Working → Idle transition and a different visible
agent response from the preceding baseline; tool output alone is insufficient.
Explicit request/blocker phrases in the last eight lines of the latest visible
agent response and a final question are heuristics, evaluated only while Idle, with approval/work taking priority.
Old responses before the latest submitted request are excluded. Unsupported or
missing markers mean unknown, not idle or successful task completion.

Opening is observed only when iTerm2 is frontmost and the exact linked pane is
active at a status sample, or after a successful identity-validated Jump. List
selection/details previews never acknowledge a result. Opening is not proof of
reading. State lasts only while Sova is open; initial Idle has unknown
opening history. Work/visits entirely between polls and offscreen response
markers may be missed. Closed/replaced process identities lose their state;
scan failures show unknown while retaining pending opening history.

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
| 5 | seconds | All-session status and selected content; lower Apple Event/terminal work | Benchmark tradeoff; on selection first read is immediate after debounce, format-parser tests |
| 30 | seconds | Full pane/location and Git-root map; moves are uncommon | User explanation; new identities/r bypass timer, Jump resolves current UUID independently |
| 24 / 8,192 | visible rows / characters | Per-pane status tail | Covers provider footer/recent requests with bounded response size; bridge and parser checks |
| 8 | response lines | Explicit request/blocker phrase search tail | Prioritize recent agent requests over older narrative; heuristic parser fixtures |
| 250 | milliseconds | UUID confirmation window after selection | Observed asynchronous iTerm2 front-window update; 10 ms retry pause, no extra full scan or substituted target |
| 150 | milliseconds | Selection debounce before content read | Avoid reading every intermediate session while navigating; no delay on Jump |
| 100 | Unicode characters | Collapsed message preview | User requested message-by-message navigation; Unicode, reader and refresh tests |
| 65,536 / 32 / 8,192 | characters / blocks / characters | Retained terminal history bounds | Bounded display work/memory; tail can omit older history; not full-conversation capture |
| 5 | seconds | Helper reply deadline | Prevent a stuck API from blocking exit indefinitely; helper failure/reset handling |
| 0.1.0 | version | First published release | GitHub Release and installed self-update/version checks; docs/VALIDATION.md |
| 0.0.0 | version | Development base | Neighboring increasing-version release flow |

Local benchmark methodology and limits: docs/PERFORMANCE.md. No fixed interval
can promise zero load on every machine; measured defaults balance freshness and
CPU cost, with manual refresh and independently configurable timers.
Commands follow neighbors: help, version, update, setup-config, themes/theme.
Themes: calm (default), classic, forest, sunset, ocean, midnight; independent calm
palette plus optional gtd color families. XDG config and SOVA_CONFIG override. Install/update
preserve configuration. Binary: ~/.x-cli-sova/bin. Build cache:
~/construction_side/sova. Public release: RoTorEx/sova, Apple Silicon
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

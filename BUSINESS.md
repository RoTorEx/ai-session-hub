# AI Session Hub

Personal macOS Rust CLI/TUI for finding already running interactive Codex and
Claude agents in existing iTerm2 panes and returning to the exact pane. Users
keep normal launch commands. AI Session Hub never types into panes or starts,
resumes, stops, or changes agent processes.

## Name and migration

The product is AI Session Hub, executable/package/repository `ai-session-hub`, starting with minor
0.3.0. Default installation is `~/.toolbelt/ai-session-hub/bin/ai-session-hub`, configuration is
`$XDG_CONFIG_HOME/ai-session-hub/config.toml` (otherwise `~/.config/ai-session-hub/config.toml`),
overrides are `AI_SESSION_HUB_INSTALL_DIR` and `AI_SESSION_HUB_CONFIG`. Earlier Sova and FleetScope tags and
release payloads retain their original identities. The renamed installer copies
Sova default configuration (then FleetScope as a fallback) only when the new default is absent and no explicit
configuration override was supplied; existing AI Session Hub settings always win. Legacy
binaries and profile blocks can be removed after the new installation is verified.

The descriptive name makes the purpose discoverable in public repositories.
The product spans agent providers; current support remains Codex and Claude.
A Swift app or AI-Fleet integration is exploratory, not part of this rename.

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
expanded. The session list is the entry point. Project headings separate task rows; each
row shows its title, explicit activity/opening status and last conversation update.
Updated remains visible at the 50-column minimum. One-cell gaps separate columns;
Unicode-aware ellipses keep titles out of the status column. Compact times omit
"ago" and approximation prefixes. The redundant request-time column is omitted. PID,
branch arrows and duplicate project/provider suffixes do not clutter each entry.
Tab/Shift+Tab cycles All, Needs you, Working, Idle and Unknown with counts.
Idle includes Untracked, New reply and Opened; Needs you includes approval/questions/blockers.
Unknown is not labelled inactive because absent markers do not prove inactivity. Refresh preserves selection
by identity and independent details scroll. Unknown paths do not exclude agents.

## Details and content

The TUI uses two full-height columns at every supported size. Sessions uses
65% for the task table and 35% for details, following the Codex session-manager
structure. History retains 40% for iterations and 60% for full text. The usable minimum is 50 columns by 12 rows. This layout
gives Sessions the available height rather than sharing it with History; resize,
mode, focus and mouse fixtures protect pane boundaries.

Startup is Sessions | Details. f switches both columns together to History |
Content for the selected session, and switches back from either history pane.
The selected session and iteration survive mode switches. History mode keeps a
fixed header with session name, provider/activity and folder above both panels;
o still performs the same identity-validated Jump. Search returns to Sessions.

Details shows the session name, provider/status, available model/reasoning and
update/request times, project, latest request and latest answer in that order.
Request and answer previews are limited to three wrapped lines each (two in short windows) so neither
buries the other; full retained content remains in History. No History shortcut
or technical sections interrupt these previews. Missing model/reasoning and
update times are explained, rather than displaying several unlabeled dashes.
A new provider request with no answer is never paired with an old cached terminal
answer. Visible selected-pane metadata is used only as an observed fallback.
The i key toggles a separate Session Info view with data-source limitations,
terminal location and process identifiers; i or Esc returns to Details. It shares
the mouse-scroll behavior of Details and never acknowledges a response.
Names still come from the observed terminal title. Project headings show the
directory basename; the home-directory heading is omitted. Duplicate basenames
include the parent directory; full paths stay in Details and search.

History groups each observed user request and all following agent/action blocks
up to the next request into one iteration. Leading output without its visible
request is kept as a partial iteration; requests awaiting an answer stay visible.
There are no role filters. Each message inside a block previews its first 100
Unicode characters, flattened to one paragraph, with an ellipsis for longer text.
Navigation selects iterations, not individual messages or lines. Selecting a
session resets History to its topmost iteration and scroll position. The heading
states oldest → newest: earlier iterations are above later ones.
Content automatically displays the full retained selected iteration, with
separate You/Agent/Action headings; subsequent selections update the reader.
Tab / Shift+Tab switches History ↔ Content for keyboard scrolling; e / Enter
in History also focuses Content, and e returns to History. f or Esc returns to
Sessions | Details. Details itself is excluded from keyboard focus; its metadata
remains scrollable with the mouse wheel.
Refresh preserves the selected iteration when its messages persist, grow or gain
following output. If it disappears, the selection is reconciled and Content is
reset to the new selected iteration, keeping the current mode. Losing the selected
session returns to Sessions. History timestamps are not guessed; the dashboard metadata source is described below.

The active pane has a muted accent border and a footer focus label, with no
filled/bold title. Mouse clicks do not change focus; wheel operates the visible
pane under the pointer. Arrows/page keys operate on keyboard focus. History has
a selected-iteration marker; Content has an independent scrollbar. Help/Quit
remain visible. t cycles palettes and saves the choice immediately.
The default calm palette uses terminal-default text, neutral separators and a
muted blue focus accent. Existing gtd palettes remain available as options.

AI Session Hub does not rename agent chats, terminal tabs or panes and has no local aliases.
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

## Dashboard metadata and timing

The requested dashboard adds local provider metadata to compact status sampling.
It never chooses a conversation by cwd, title or recency. Codex requires exactly
one JSONL file under its sessions root held open by the live process, with a
session_meta header. Claude requires its PID registry to match PID, process start
(second precision), interactive kind and darwin domain, then exactly one transcript
with that session ID. Native process identity is checked again after reads.
Unlinked sessions never receive provider-file reads. CODEX_HOME and
CLAUDE_CONFIG_DIR override the default ~/.codex and ~/.claude roots.

Read at most the last 1 MiB per file on the existing activity timer, dropping a
partial leading line and ignoring malformed/incomplete JSON. The limit bounds
work independently of archive size; missing older fields remain unknown. Keep
only model, reasoning, latest submitted request, latest answer and their provider
timestamps in memory. Tool results, sidechains and meta messages are not requests.
History remains the selected pane's visible-screen iteration browser; this is not
archive-wide history search. No new files are persisted or uploaded.

Updated is the latest retained user/agent text event, not a polling/mtime timer.
Last request is the submitted user event. Below one hour, show seconds (below one
minute) or whole minutes; at one hour and above, show calendar date and clock time
in Europe/Warsaw with date-specific CET/CEST. The one-hour threshold follows the
user's request to replace long-running elapsed timers with a useful clock time.
Initial terminal observations have unknown times. Later observed changes retain sampling times without a visual approximation
prefix; Session Info explains the distinction from provider events. In compact
tables, older events use HH:MM on the same Warsaw calendar day and DD Mon on
other days; Details retains the full Warsaw date, clock time and CET/CEST.
Unavailable times use an em dash in the table and an explanation in Details;
model/reasoning are never inferred from global configuration. Timer redraws run once per second only while focused/awake and a recent
time is visible; they do not keep helpers awake. Tests cover threshold, DST/day
rollover, tool-result exclusion, filter reconciliation, resize and history retention.

## Activity and opening responses

The session list uses one fixed symbol column: ● Working, ○ Untracked (idle; opening history unknown), ! Approval,
? question/action requested, × reported blocker, ◆ New reply (new response not
observed opened), ✓ Opened (pane observed open), · activity unknown. Questions
and blockers retain their symbol after opening because they still need user
attention. Details and help explain symbols; no animation is used.

While AI Session Hub has terminal focus, all linked agents receive a read-only status pass
on the separate activity timer (10 seconds by default). Native
PID/start/TTY/ancestry is checked once per batch; the terminal bridge confirms
UUID-selected pane TTY/root and returns only its last 24 visible rows, at most
8,192 characters. Parsed status and an in-memory normalized response hash are retained for
unselected sessions, alongside the bounded metadata described below.

New reply requires an observed Working → Idle transition and a different visible
agent response from the preceding baseline; tool output alone is insufficient.
Explicit request/blocker phrases in the last eight lines of the latest visible
agent response and a final question are heuristics, evaluated only while Idle, with approval/work taking priority.
Old responses before the latest submitted request are excluded. Unsupported or
missing markers mean unknown, not idle or successful task completion.

Opening is observed only when iTerm2 is frontmost and the exact linked pane is
active at a status sample, or after a successful identity-validated Jump. An initial idle session also becomes
Opened after a confirmed visit, even when its earlier response history is unknown.
Temporarily absent response markers do not erase a known opening. Status
sampling pauses without AI Session Hub terminal focus, so manual visits during the pause
are not retrospectively acknowledged. Jump still acknowledges immediately. List
selection/details previews never acknowledge a result. Opening is not proof of
reading. State lasts only while AI Session Hub is open; initial Idle has unknown
opening history. Work/visits entirely between polls and offscreen response
markers may be missed. Closed/replaced process identities lose their state;
scan failures show unknown while retaining pending opening history.

## Application lifecycle and synchronization

Scan at launch and while focused; no workers/monitoring after exit. Jump leaves the
TUI open in its original pane. English UI and docs. Separate worker lanes keep
Jump independent of inventory/content requests. Two task-owned JXA helpers reuse
API bindings while the TUI is open, have five-second reply deadlines and are
killed/reaped on exit or helper timeout. They never launch iTerm2.

Lightweight native inventory detects exits/starts; only new agent identities,
manual refresh or the metadata timer require a fresh pane map. Closed agents are
removed without re-reading all tabs. Git roots are cached until metadata refresh.
Full selected content refreshes in History | Content on its own five-second
timer after the 150 ms selection debounce. Details reads the same bounded visible
screen once per selection, refresh/focus return, or change to the selected
request/answer in compact status. It has no periodic full-content timer. This
recovers requests above the compact 24-row tail without archive guessing or an
extra helper. Entering History invalidates the timer for an immediate read.

AI Session Hub enables terminal focus reporting (DECSET 1004). FocusOut pauses new native,
metadata, status and content requests, also when another pane gains focus while
AI Session Hub remains visible. Already dispatched requests may finish. FocusIn immediately
requests a fresh pane/process map, status and selected preview/history content. A pending
scan does not discard the forced refresh; session, mode and iteration selection
are retained when still valid. No visibility polling or new helper is added.
Without focus events, polling continues until the user-idle timeout. After 60
seconds without keyboard/mouse input or FocusIn, AI Session Hub enters sleep, pauses all
background requests and drops both helper clients; their osascript processes are
killed and reaped. Input or FocusIn restarts both clients, forces fresh metadata,
status and relevant content, and retains selection/mode. Poll replies and agent
activity do not reset user-idle time. Queued/in-flight work may finish before the
sleep request is handled. Worker threads remain, blocked on their channels.
A SLEEP header marks user-idle sleep. Paused/sleeping input waits up to one second
instead of waking every 100 ms; focus or user input wakes it immediately. Quit
from sleep exits without warming helpers. Sleeping trades first-use latency for
released helper memory; the independent Jump lane remains. Helper count is at
most two regardless of agent count; status work grows with linked panes, while
full content remains limited to the selected session. Unchanged
snapshots do not redraw. Routine refresh has no transient Live/Updating indicator.
Failed scans are distinct from an empty list; prior results are marked stale.

## Defaults and delivery

| Value | Unit | Meaning / reason | Source / impact / checks |
|---|---|---|---|
| 2 | seconds | Native inventory; prompt opening/exit detection at low cost | User correction + measured benchmark; positive config, reconciliation/identity checks |
| 5 | seconds | Periodic full selected content only while History is focused/visible | Preserve reader freshness while avoiding unused Sessions reads; scheduler/reader checks |
| 10 | seconds | Compact all-session status while AI Session Hub has terminal focus | User chose reduced polling; twice the old interval, delayed readiness/opening observation; independent timer tests |
| 60 | seconds | No user input before releasing both helpers and pausing all scans | User request: reduce idle CPU and runtime memory; keyboard/mouse/FocusIn reset, timer scans do not; scheduler and installed lifecycle checks |
| 1 | second | Maximum idle event wait while unfocused or sleeping | Reduce wakeups; terminal FocusIn wakes immediately, signal/exit and PTY checks |
| 30 | seconds | Full pane/location and Git-root map; moves are uncommon | User explanation; new identities/r bypass timer, Jump resolves current UUID independently |
| 24 / 8,192 | visible rows / characters | Per-pane status tail | Covers provider footer/recent requests with bounded response size; bridge and parser checks |
| 8 | response lines | Explicit request/blocker phrase search tail | Prioritize recent agent requests over older narrative; heuristic parser fixtures |
| 250 | milliseconds | UUID confirmation window after selection | Observed asynchronous iTerm2 front-window update; 10 ms retry pause, no extra full scan or substituted target |
| 150 | milliseconds | Selection debounce before content read | Avoid reading every intermediate session while navigating; no delay on Jump |
| 3 | wrapped lines per preview | Maximum Details request/answer preview height; keep both visible | User narrow-terminal correction; bounded preview/ordering regression |
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
palette plus optional gtd color families. XDG config and AI_SESSION_HUB_CONFIG override. Install/update
preserve configuration. Binary: ~/.toolbelt/ai-session-hub/bin. Build cache:
~/construction_side/ai-session-hub. Public release: RoTorEx/ai-session-hub, Apple Silicon
macOS only. Release preparation/push are separate; real release needs approval.

## Non-goals

No tasks/Inbox, archive-wide conversation search, AI classification, notifications,
daemon, LaunchAgent, mandatory hooks, account or AI Fleet integration. No SSH,
tmux, additional providers or terminals. No agent control or terminal writes.

## Code map

process/iterm own identity/linking and the read-only terminal bridge; model owns
cached inventory/grouping; content owns observed terminal state/text; session_data owns bounded process-bound
provider metadata; tui owns
navigation/presentation and timers. CLI/TUI share Jump. benchmark owns explicit
local diagnostics. Delivery belongs to installer/updater/release scripts.

# AI Session Hub

A local Rust CLI/TUI for viewing already running interactive Codex and Claude
agents in iTerm2 and jumping to the exact existing pane. The normal way of
launching agents does not change. AI Session Hub types nothing into panes and does not
control agent processes.

## Installation

Requirements: macOS on Apple Silicon, iTerm2; Rust is needed only to build from source.
Verified environment: macOS 26.6.1, iTerm2 3.7.3.

```sh
make install-local
ai-session-hub
```

The binary is installed to `~/.x-cli-ai-session-hub/bin/ai-session-hub`; the installer adds a
managed PATH block to `.zshrc` or `.bashrc`. In an already open shell:

```sh
export PATH="$HOME/.x-cli-ai-session-hub/bin:$PATH"
ai-session-hub --version
```

A prebuilt binary can be installed without Rust:

```sh
curl -fsSL https://github.com/RoTorEx/ai-session-hub/releases/latest/download/ai-session-hub-install.sh | sh
ai-session-hub update
```

The installer and updater verify the SHA-256 and the version and replace the
binary atomically. Reinstalling preserves the configuration. If there is no
release yet or the download fails, update exits with an error and keeps the
installed binary.

On first access macOS may request Automation permission for iTerm2. If it is
denied, AI Session Hub shows why the link is missing and does not perform Jump.
The permission can be changed in System Settings → Privacy & Security → Automation
for the application from which the command is run. The Python API, shell
integration, hooks and starting AI Session Hub before the agents are not required.

## Migrating from Sova or FleetScope

Starting with 0.3.0 the product is **AI Session Hub** and the command is
`ai-session-hub`. Install it with the new installer above: `sova update` and
`fleet-scope update` do not migrate between package names.

If the new default configuration is absent, the installer copies
`~/.config/sova/config.toml`, falling back to `~/.config/fleet-scope/config.toml`
(respecting `XDG_CONFIG_HOME`). Existing AI Session Hub settings always win.
An explicit `AI_SESSION_HUB_CONFIG` creates starter settings at that path instead
of importing legacy defaults. Replace custom `SOVA_CONFIG` / `SOVA_INSTALL_DIR`
variables with `AI_SESSION_HUB_CONFIG` / `AI_SESSION_HUB_INSTALL_DIR` as needed.
Source configurations remain untouched.

After checking `ai-session-hub --version` and your settings, remove the old
`~/.x-cli-sova/bin/sova` binary and its managed `# x-cli-sova` …
`# /x-cli-sova` shell profile block (or the corresponding FleetScope paths).
Open a new terminal or add the new PATH as shown above. Existing running views
keep the old name until restarted. Repository history, tags and earlier release
assets retain their original identities; current sources live in `ai-session-hub`.

## Commands

```sh
ai-session-hub                    # Sessions | Details; f → History | Content
ai-session-hub list                # fresh list, then exit
ai-session-hub --plain             # the same list
ai-session-hub list --json         # session array for scripts
ai-session-hub jump SESSION_ID     # identifier from the list
ai-session-hub version             # also -V / --version
ai-session-hub update
ai-session-hub setup-config        # create a missing config
ai-session-hub themes
ai-session-hub theme ocean
ai-session-hub benchmark --samples 20 # local benchmark, without Jump
ai-session-hub --help
```

Without an interactive terminal, running with no arguments prints help.
help/version/setup-config/themes/theme do not require a running iTerm2. Errors
produce a nonzero exit code; diagnostics go to stderr, JSON only to stdout.

In JSON, `id` is the PID and start time of the instance, `provider` is codex/claude,
`cwd` is the actual directory, `project` is the Git worktree root or the directory,
`status` is Live. `location` contains the pane UUID, TTY, window and zero-based
tab/pane numbers; for an unconfirmed link it is null and the reason is in `link_reason`.
The ID is valid only while this instance is alive; tab numbers are not IDs.
The regular interface shows numbers starting from one.

## Controls

| Key | Action |
|---|---|
| ↑/↓, j/k | Select a session/iteration; on the right, scroll text |
| f | Switch Sessions \| Details ↔ History \| Content |
| Tab / Shift+Tab | In history mode, switch focus History ↔ Content |
| ← / → | Previous/next session or iteration |
| Mouse wheel | Navigate the pane under the pointer without changing focus |
| PageUp/PageDown, Home/End | Navigate the active pane |
| Enter in Sessions / o | Verify the link and jump to the existing pane |
| e / Enter in History | Focus Content for scrolling; e returns to History |
| t | Next color theme, applied and saved immediately |
| i | Toggle Details / Session Info (technical data and sources) |
| / | Search by agent, project, path and names |
| Enter while searching | Apply the filter |
| Esc while searching | Clear the filter and close search |
| r | Refresh pane locations and content |
| ? | Help |
| q / Ctrl+C | Quit |
| Esc | Close help/search/info; from history return to Sessions, otherwise quit |

The main letter keys also work in the Russian keyboard layout. In search, letters
are entered as text. To clear an applied filter, press `/`, then Esc.

Two columns take the full available height: Sessions uses 65% for the task table
and 35% for details; History keeps 40% / 60% for iterations and full text.
At startup this is **Sessions | Details**. **f** switches both panes to
**History | Content** of the selected session; pressing **f** again returns to the session list.
The minimum terminal size is 50×12; a narrow window keeps two columns.

Groups show the folder name; the home-directory heading is omitted. Identical
names are distinguished by the parent folder. Full paths stay in Details.
The table always shows title, status and Updated, including in narrow windows.
Titles end with an ellipsis and a gap before Status. Times omit `ago` and `~`.
Below an hour they show seconds/minutes; narrow columns then use a Warsaw clock
time for today or a date for older days. Details retains full date/time and zone.
Tab / Shift+Tab cycles All, Needs you, Working, Idle and Unknown with counts.

Details contains a concise overview, project, latest request and latest answer.
Each request/answer preview has at most three wrapped lines (two in short windows); **f** opens the full
retained history. Missing model/reasoning and times have explicit explanations.
**i** opens a separate Session Info view with source limitations, terminal and
process identifiers; **i / Esc** returns. Shortcuts stay in the footer/help.
Model/reasoning and exact times come from a local provider file only when bound
to the live process: an open Codex rollout or the Claude PID/start/session registry.
Only the last 1 MiB is read per refresh; unavailable older fields remain unknown.
No conversation is selected by matching folder or recency. Nothing is uploaded.
Current Codex clients using a shared app-server may hold no rollout file; their
model/reasoning and provider timestamps remain unknown unless visible in the pane.
The name comes from the terminal title; it may differ from the agent's chat name.
AI Session Hub does not rename chats or tabs: that stays in the agent/iTerm2.

At startup, focus is on Sessions. In history mode, **Tab / Shift+Tab** switches
between History and Content for scrolling the full text. Details does not receive
focus; its metadata can be scrolled with the wheel. The active pane is marked by a
calm colored border and its name at the bottom. A click does not change focus.
In history mode, the session name, provider, status and folder stay above the panes.
**o** in any mode opens the session's exact iTerm2 pane. **/** returns to the session
list for search. Switching modes preserves the selected session and iteration.

History combines a request and the responses and actions that follow it into one
iteration, up to the next request. If the beginning did not fit on screen, the
remaining response is shown as a partial iteration. A request without a response
also stays visible.
Older iterations are at the top, newer ones at the bottom. When switching sessions,
the top iteration is selected and history scrolling resets to the top.
↑/↓ select an iteration; each message preview contains the first **100 characters**.
The full text of the selected iteration is automatically shown on the right in Content.
Arrows in History select iterations; **Tab** or **e / Enter** moves focus
to Content for scrolling. **e / Tab** returns focus to History.
**Esc / f** returns both panes to Sessions | Details.
Message times are not guessed.
This is a snapshot of the visible terminal screen, not a full conversation archive.
The version and theme name are always visible at the top; **t** changes the palette immediately.
The calm theme **calm** is used by default; the other themes are available.

After Jump the overview stays open; you can return with normal iTerm2 navigation.
After exit AI Session Hub monitors nothing.

## Settings and themes

Config: `$XDG_CONFIG_HOME/ai-session-hub/config.toml` or
`~/.config/ai-session-hub/config.toml`. `AI_SESSION_HUB_CONFIG` overrides the path.
A missing file does not prevent startup: built-in values are used.

```toml
theme = "calm"
refresh_seconds = 2
content_seconds = 5
activity_seconds = 10
metadata_seconds = 30
```

All intervals must be positive. `refresh_seconds` controls the lightweight
process check: new/closed agents are detected without constant full metadata
collection. `metadata_seconds` is the window/tab and Git-root map; a new agent
or `r` refreshes it earlier. `activity_seconds` is the compact status of all linked
sessions (10 seconds by default). `content_seconds` is the full content of the selected
pane, periodically read in History mode (5 seconds). Details reads it once after
selection, refresh/focus return or a change to its compact request/answer, with
no periodic full-content timer. The status scan does not
reread the window map or Git roots. On entering History, reading starts right
after a short 150 ms pause, without waiting for the full interval.

When terminal focus is lost, AI Session Hub pauses background polling. Regaining
focus immediately refreshes processes, locations, statuses and open content.
This works through iTerm2 focus events, without additional visibility polling;
if the terminal does not send events, updates continue. The pause also triggers
when moving to a neighboring pane, even if AI Session Hub is still visible. Completions and
manual agent visits during the pause may be missed; `o` still immediately
records a confirmed jump.

After **60 seconds without user activity** AI Session Hub goes to sleep even in a visible
pane: it stops background polling and terminates both `osascript` processes,
releasing their memory. `SLEEP` appears in the header. Keyboard, mouse or regained
focus warm up the helpers and refresh data. The first action after sleep may take
longer. Status updates and agent responses do not count as user activity.
The current request may finish before shutdown. On exit all helpers are
terminated; exiting from sleep does not start them just to shut down.
A moved tab may show the previous location for a while; Jump still resolves the
current pane by UUID. It does not wait for the background collection queue.

Refresh does not toggle Live/Updating and does not redraw an unchanged screen.
Detailed measurements and the limits of the CPU estimate: [docs/PERFORMANCE.md](docs/PERFORMANCE.md).
To repeat locally: `ai-session-hub benchmark --samples 20`. Adding
`--jump SESSION_ID` also measures real jumps and changes the selected pane.

Themes: calm (default), classic, forest, sunset, ocean, midnight. `theme NAME` preserves the other
settings and comments. The CLI command applies the theme on the next launch; the `t` key in the TUI applies it immediately.

## Statuses in the session list

| Symbol | Meaning |
|---|---|
| ● | The agent is working |
| ○ | Untracked — idle; earlier opening history is unknown |
| ! | Approval required |
| ? | A question or action for the user |
| × | The agent reported a blocker |
| ◆ | New reply — a new response arrived; the pane has not been observed opened |
| ✓ | Opened — the idle pane was observed open or opened by Jump; reading is unconfirmed |
| · | Activity unknown |

Questions and blockers are recognized from explicit phrases in the visible response; this is a heuristic.
Details previews the request/answer; History contains full retained text. The legend is in `?` help.
A question/blocker stays marked after the pane is opened.

AI Session Hub tracks opening of the exact active pane in the foreground iTerm2 or
a successful Jump. Selecting a row and viewing Details do not count as opening.
This is not confirmation of reading. History is tracked only while AI Session Hub is
running: responses completed before launch, fast tasks/visits between
scans and markers hidden off screen may remain unknown.

## Limitations

Live means an existing interactive process and includes waiting for a command
or approval. Details additionally read status markers and the visible screen text
of the selected iTerm2 pane. Content comes from the terminal, not from an archived
conversation found by a similar path. It may be incomplete; roles and status depend
on the Codex/Claude format. An unknown format shows `activity unknown`.
The unfinished bottom prompt is excluded: it may be text not yet submitted.
No Conversation ID is invented.

Only the tail of the snapshot (65,536 characters) stays in memory, up to 32 blocks of 8192 characters each.
Details previews at most three wrapped lines per request/answer; History selects
iterations and Content displays their full retained text. This is not a full conversation archive. AI Session Hub does not store or
send content anywhere; reading does not affect the safety of Jump identification.

Local regular iTerm2 panes and interactive Codex/Claude are supported.
Noninteractive exec/print, helper processes and a shell after the agent exits
are not sessions. SSH, tmux, other terminals and additional providers
are not supported; unconfirmed candidates may be shown as Unlinked.

Detection requires available process metadata and stdin attached to
its TTY. Unusual wrappers or redirected stdin may prevent
detection. Jump rechecks the process/pane and refuses an ambiguous
link. Stale data after an error is marked STALE. Checks run before the
jump; a process exiting after the last check remains a possible
race, but no jump is made by title match or an old index.

A full iTerm2 restart with real user agents preserved has not been
verified yet. The published v0.1.0 and v0.1.1 were downloaded and verified on this machine:
self-update upgraded 0.0.0 → 0.1.0 → 0.1.1 and kept the config; a repeated update
confirmed the current version. The published installer was also verified in isolation.
Detailed observations: [docs/VALIDATION.md](docs/VALIDATION.md).

## Development and release

```sh
make check          # unchanged formatting, clippy, tests, release build, delivery checks
make run
make fmt
make version
make bench          # diagnostic measurements without Jump
make smoke-install  # packaging and isolated installation
make release        # interactive MAJOR.MINOR.PATCH prompt on a clean main
make release-push   # separate push of the branch and annotated tag
```

`make release` updates Cargo.toml/Cargo.lock/CHANGELOG.md and creates the release
commit and tag, but publishes nothing. Pushing the tag starts GitHub Actions:
Apple Silicon build, smoke-install, archive + checksum + installer, GitHub Release.
The target is the public `RoTorEx/ai-session-hub`; the version comes from Cargo.toml and is checked
against the tag. Releasing requires separate approval.

Before the first release or a workflow change, check it with `actionlint
.github/workflows/release.yml`, then run `make smoke-install`. This verifies
valid GitHub Actions contexts and real packaging/installation before the tag is pushed.

If publishing an existing tag did not start because of a workflow error, fix the
workflow on main and run `gh workflow run release.yml --ref main -f tag=vX.Y.Z`.
The manual run builds exactly the existing tag and checks its version and commit;
the tag does not need to be rewritten. Regular subsequent releases are started by pushing a tag.

Builds are stored in `~/construction_side/ai-session-hub/target`, packaging in `dist`
next to it. These directories can be deleted: the sources, the installed binary and the configuration
do not depend on them. A rebuild recreates them.

## Uninstallation

Remove `~/.x-cli-ai-session-hub` and the managed block between `# x-cli-ai-session-hub`
and `# /x-cli-ai-session-hub` from the shell profile. If needed, separately remove
the selected config and `~/construction_side/ai-session-hub`. Agent processes and their
data are not affected.

Product rules: BUSINESS.md; local agent instructions: AGENTS.md;
stage order: ROADMAP.md. Checks are performed personally, without subagents.

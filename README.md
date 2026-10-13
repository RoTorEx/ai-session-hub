# Sova

A local Rust CLI/TUI for viewing already running interactive Codex and Claude
agents in iTerm2 and jumping to the exact existing pane. The normal way of
launching agents does not change. Sova types nothing into panes and does not
control agent processes.

## Installation

Requirements: macOS on Apple Silicon, iTerm2; Rust is needed only to build from source.
Verified environment: macOS 26.6.1, iTerm2 3.7.3.

```sh
make install-local
sova
```

The binary is installed to `~/.x-cli-sova/bin/sova`; the installer adds a
managed PATH block to `.zshrc` or `.bashrc`. In an already open shell:

```sh
export PATH="$HOME/.x-cli-sova/bin:$PATH"
sova --version
```

A prebuilt binary can be installed without Rust:

```sh
curl -fsSL https://github.com/RoTorEx/sova/releases/latest/download/sova-install.sh | sh
sova update
```

The installer and updater verify the SHA-256 and the version and replace the
binary atomically. Reinstalling preserves the configuration. If there is no
release yet or the download fails, update exits with an error and keeps the
installed binary.

On first access macOS may request Automation permission for iTerm2. If it is
denied, Sova shows why the link is missing and does not perform Jump.
The permission can be changed in System Settings → Privacy & Security → Automation
for the application from which the command is run. The Python API, shell
integration, hooks and starting Sova before the agents are not required.

## Migrating from FleetScope

Starting with 0.2.0 the command is named `sova`. Install it with the new installer
above: the old `fleet-scope update` does not migrate between package names.
If there is no Sova configuration yet, the installer copies settings from
`~/.config/fleet-scope/config.toml` (respecting `XDG_CONFIG_HOME`). An existing
Sova config and an explicitly set `SOVA_CONFIG` take precedence. The old config
stays in place until the new installation is verified.

After checking `sova --version`, remove the old binary
`~/.x-cli-fleet-scope/bin/fleet-scope` and the old `# x-cli-fleet-scope` …
`# /x-cli-fleet-scope` block from the shell profile. In an already open shell, add
the new PATH as shown above, or open a new terminal. The sources now live in the
`sova` directory; old tags and releases keep their original file names.

## Commands

```sh
sova                    # Sessions | Details; f → History | Content
sova list                # fresh list, then exit
sova --plain             # the same list
sova list --json         # session array for scripts
sova jump SESSION_ID     # identifier from the list
sova version             # also -V / --version
sova update
sova setup-config        # create a missing config
sova themes
sova theme ocean
sova benchmark --samples 20 # local benchmark, without Jump
sova --help
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
| / | Search by agent, project, path and names |
| Enter while searching | Apply the filter |
| Esc while searching | Clear the filter and close search |
| r | Refresh pane locations and content |
| ? | Help |
| q / Ctrl+C | Quit |
| Esc | Close help/search; from history return to Sessions, otherwise quit |

The main letter keys also work in the Russian keyboard layout. In search, letters
are entered as text. To clear an applied filter, press `/`, then Esc.

Two columns take the full available height: 40% on the left, 60% on the right.
At startup this is **Sessions | Details**. **f** switches both panes to
**History | Content** of the selected session; pressing **f** again returns to the session list.
The minimum terminal size is 50×12; a narrow window keeps two columns.

Groups show the folder name; identical names are distinguished by the parent folder.
Full paths stay in Details. Details also shows the session name, window, tab,
pane, Folder and identifiers. Sections are separated by lines.
The name comes from the terminal title; it may differ from the agent's chat name.
Sova does not rename chats or tabs: that stays in the agent/iTerm2.

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
After exit Sova monitors nothing.

## Settings and themes

Config: `$XDG_CONFIG_HOME/sova/config.toml` or
`~/.config/sova/config.toml`. `SOVA_CONFIG` overrides the path.
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
pane, read only in History mode (5 seconds). The status scan does not
reread the window map or Git roots. On entering History, reading starts right
after a short 150 ms pause, without waiting for the full interval.

When terminal focus is lost, Sova pauses background polling. Regaining
focus immediately refreshes processes, locations, statuses and open content.
This works through iTerm2 focus events, without additional visibility polling;
if the terminal does not send events, updates continue. The pause also triggers
when moving to a neighboring pane, even if Sova is still visible. Completions and
manual agent visits during the pause may be missed; `o` still immediately
records a confirmed jump.

After **60 seconds without user activity** Sova goes to sleep even in a visible
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
To repeat locally: `sova benchmark --samples 20`. Adding
`--jump SESSION_ID` also measures real jumps and changes the selected pane.

Themes: calm (default), classic, forest, sunset, ocean, midnight. `theme NAME` preserves the other
settings and comments. The CLI command applies the theme on the next launch; the `t` key in the TUI applies it immediately.

## Statuses in the session list

| Symbol | Meaning |
|---|---|
| ● | The agent is working |
| ○ | Idle — waiting for the next command |
| ! | Approval required |
| ? | A question or action for the user |
| × | The agent reported a blocker |
| ◆ | A new response is ready; the pane has not yet been observed opened |
| ✓ | The pane was opened after the response became ready |
| · | Activity unknown |

Questions and blockers are recognized from explicit phrases in the visible response; this is a heuristic.
The request/response text is in History, the status explanation in Details, the legend in `?` help.
A question/blocker stays marked after the pane is opened.

Sova tracks opening of the exact active pane in the foreground iTerm2 or
a successful Jump. Selecting a row and viewing Details do not count as opening.
This is not confirmation of reading. History is tracked only while Sova is
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
By default a message shows its beginning (4 lines / 280 characters); `e` expands
the retained part. This is not a full conversation archive. Sova does not store or
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
The target is the public `RoTorEx/sova`; the version comes from Cargo.toml and is checked
against the tag. Releasing requires separate approval.

Before the first release or a workflow change, check it with `actionlint
.github/workflows/release.yml`, then run `make smoke-install`. This verifies
valid GitHub Actions contexts and real packaging/installation before the tag is pushed.

If publishing an existing tag did not start because of a workflow error, fix the
workflow on main and run `gh workflow run release.yml --ref main -f tag=vX.Y.Z`.
The manual run builds exactly the existing tag and checks its version and commit;
the tag does not need to be rewritten. Regular subsequent releases are started by pushing a tag.

Builds are stored in `~/construction_side/sova/target`, packaging in `dist`
next to it. These directories can be deleted: the sources, the installed binary and the configuration
do not depend on them. A rebuild recreates them.

## Uninstallation

Remove `~/.x-cli-sova` and the managed block between `# x-cli-sova`
and `# /x-cli-sova` from the shell profile. If needed, separately remove
the selected config and `~/construction_side/sova`. Agent processes and their
data are not affected.

Product rules: BUSINESS.md; local agent instructions: AGENTS.md;
stage order: ROADMAP.md. Checks are performed personally, without subagents.

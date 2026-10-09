# Validation evidence

Historical FleetScope checks below retain their original binary names. The project is now Sova.

## Stage 1 — 2026-10-09

macOS 26.6.1 arm64, iTerm2 3.7.3. Native libproc + osascript prototype.
User prepared waiting Claude sessions and arranged a tab in another window.
Three independent Claude processes in the same working directory were detected
with distinct PID/start-time identities and pane UUIDs. Jump to each succeeded
and read back the expected selected UUID across two windows. Working Codex Jump
also succeeded. Earlier Codex exits were reflected in later scans and Jump to
one recorded exited identity was refused. A deliberately mismatched TTY was
refused before selection. Helper app-server processes without TTY were excluded.

This established the main feasibility gate before CLI/TUI implementation.
Installed-build checks below extend that evidence.
No terminal text was read or written. Test sessions belong to the user.

## Initial installed build — 2026-10-09

Local 0.0.0 installed at ~/.x-cli-fleet-scope/bin/fleet-scope. Real split-pane
Claude awaiting workspace-trust approval was detected as pane 2 and Jump read
back that exact UUID; no approval was submitted to Claude. Original Codex pane
was selected afterward.

The installed TUI was run in a PTY: real sessions rendered, Claude filtering,
independent detail focus/PageDown, help open/close and q exit were exercised.
The alternate screen was restored and the owned TUI exited. Configuration was
preserved on reinstall. make check passed Rust checks plus isolated installer
and release-flow checks; those fixture tags are not real project releases.

A separate PTY with its own controlling terminal exercised Jump through the
installed TUI. The selected iTerm2 UUID was read back and matched a live Claude
pane; the TUI remained running afterward. SIGTERM exited with code 0 and emitted
the alternate-screen restore. Original Codex was selected afterward. These
checks stopped only task-owned FleetScope processes.

Full iTerm2 restart with user agents is not exercised (would disrupt current
work). Actual published upgrade remains pending first approved release. Busy
working Codex, waiting Claude, approval-waiting Claude and process exits were
observed; at that stage only Live was reported.

Task-created build cache and dist under ~/construction_side/fleet-scope were
removed after verification. Future builds recreate them. Installed executable
and configuration remain available independently; source truth is in the
repository. User agent processes, screenshots and input archives were preserved.


## UI/synchronization revision — 2026-10-09

Installed 0.0.0 rebuilt with session-only selection, gtd color families and dark
classic selection text, permanent version, structured details and selected-pane
terminal snapshots. Config was preserved; missing new timers use defaults.
make check passed 19 Rust tests, clippy/format/release build and isolated delivery
checks. Identity tests cover PID reuse/root changes and ambiguous shared panes.

The installed color TUI was exercised in a controlling PTY and its rendered
screen inspected with a temporary terminal emulator: version/section hierarchy,
selection contrast, project filtering, exact Jump readback and staying open,
Tab focus, PageDown and mouse-wheel scrolling, expanded/collapsed message hint,
technical metadata at End, help open/close and returning to sessions.
A separate final installed run confirmed q exit 0, alternate-screen/mouse-capture
restore and disappearance of both task-owned JXA helpers.

Direct API checks resolved a deliberately obsolete cached window/tab to the
exact UUID, refused missing UUID/mismatched TTY/root before selection, bounded
content output and closed the helper on input EOF. A real cross-window return
initially selected the correct pane but reported failure because front-window
state lagged selection. Bounded UUID confirmation fixed that race; ten alternating
cross-window transitions then succeeded (median 115.6 ms, max 196.3 ms), returning
to the original pane. No command was typed into any agent.

Real Codex terminal content and format-parser fixtures were checked. New Claude
content parsing is fixture-checked; no live Claude was available during this
revision. Terminal observations depend on provider UI formatting and are not
complete conversation archives or authoritative provider events. The prior
live-Claude discovery/split checks remain evidence for process/pane integration.
Full iTerm2 restart and published upgrade still have the limits stated above.

Operation/interval CPU and memory measurements, the separately observed iTerm2
baseline and reproduction commands are in PERFORMANCE.md. Color interval runs
closed only their own TUI/helper processes. The temporary terminal emulator,
benchmark configs (including the aborted PTY fixture), build cache and dist were
removed after final verification; installed binary/config and user data remain.

## Published release and self-update — 2026-10-09

Annotated v0.1.0 points to 2e2470c83d37f0fd25aa7a4385ec7b587386c43d.
The first push was rejected before any jobs ran: runner.temp was invalid in
job-level env. The repaired main workflow passed actionlint; its manual existing-
tag path checked out and verified the unchanged release tag. GitHub Actions run
[37938769095](https://github.com/RoTorEx/sova/actions/runs/37938769095)
succeeded on native aarch64-apple-darwin: format, clippy, 22 Rust tests, release
build, delivery fixtures and smoke-install. The publish job created the public
[v0.1.0 Release](https://github.com/RoTorEx/sova/releases/tag/v0.1.0)
with the arm64 archive, adjacent SHA-256 and installer.

Downloaded published assets, checked SHA-256 and archive members, ran the
published arm64 binary's version command, then ran the previously installed
0.0.0 binary's actual update command against GitHub. It replaced itself with
0.1.0. The installed binary matches the downloaded published binary byte for
byte; the existing configuration checksum is unchanged. Before publication,
update's 404 failure left the old version intact. After installation, a repeat
update reported Already up to date. version, -V and --version all report 0.1.0;
the command resolves through the user's configured shell PATH. setup-config
preserved the existing file and themes listed the five supported palettes.

The installed no-argument TUI rendered FleetScope v0.1.0 in a controlling PTY.
q exited with code 0, restored the alternate screen/mouse modes and reaped both
owned JXA helpers. Updater fixtures also exercise archive entries (missing,
duplicate, symlink and unrelated paths) and failed/successful atomic replacement.
The earlier pending published-upgrade gap is closed. Full iTerm2 restart with
user agents remains untested. Task-owned downloads, backup and build cache are
removed after verification; installed binary/config and user sessions remain.

A final repeated check exposed a temporary-directory name collision: macOS
returned the same timestamp to two updater fixtures. Names now combine process
identity, timestamp and an atomic per-process sequence. A concurrency regression
creates 256 live directories in 16 threads, checks independence and cleanup on
drop; make check passes all 23 tests. This fix and the workflow repair shipped
in the 0.1.1 patch; the published 0.1.0 tag remains unchanged.

The ordinary tag-push workflow published
[v0.1.1](https://github.com/RoTorEx/sova/releases/tag/v0.1.1) from
3587162860f31af1fcc1b2ac9db7f46c00bf5313; both jobs in
[run 37939561696](https://github.com/RoTorEx/sova/actions/runs/37939561696)
succeeded, including all 23 tests and both delivery checks. Downloaded and
verified the three assets. Actual installed self-update replaced 0.1.0 with
0.1.1; all three version commands, a repeat Already up to date, unchanged
original config checksum and byte-for-byte published/installed binary match
were checked. The published installer was also executed in an isolated install
directory with a pre-existing config and --no-path-update; version, binary match
and config preservation passed. The installed TUI rendered v0.1.1 and restored
screen/mouse modes and cleaned up its helpers on q exit. No user agent was
controlled or stopped.


## Compact activity and opening tracking — 2026-10-09

Local development build (package version 0.1.1, with Unreleased changes) installs
compact glyphs in the session list; it is distinct from the published v0.1.1
artifact. Configuration checksum stayed unchanged. The installed controlling
PTY rendered real activity glyphs, opened/closed the legend, switched to details
and scrolled; q restored screen/mouse modes and reaped both owned JXA helpers.

make check passes 27 tests, including observed completion/opening, question and
blocker precedence, excluding previous-turn responses/tool-only output,
keeping questions after opening, ignoring list selection as acknowledgement,
removing replaced identities and suppressing unchanged activity redraws.
The sidebar renderer checks the glyph with no Ready word consuming list space.
Fresh identity validation also rejects a newly ambiguous shared pane before
status reads or Jump.

The live read-only status bridge accepted a valid pane and rejected wrong TTY,
wrong root and missing UUID; its visible-screen output stayed within the bounds
and its foreground UUID belonged to the current pane inventory. No target pane
was selected and no agent received input. Ready/opening state transitions and
question/blocker detection are fixture-tested; a controlled live agent completion
and manual foreground visit were not induced. Rapid between-poll activity,
offscreen markers, initial history and actual reading remain uncertain by design.
Timing samples and their scope are recorded in PERFORMANCE.md. Task-owned build
and package artifacts are removed after verification; installed development build
and the user's sessions/config remain.


## Sova rename and minor release — 2026-10-09

Renamed the existing repository to RoTorEx/sova, preserving repository ID
R_kgDOVBsRLA, prior history, tags and releases. Annotated v0.2.0 points to
9681e579d854baec6f42ddc3ea3bf766e685fa0a. Native Apple Silicon build and publish
both succeeded in [run 37950947956](https://github.com/RoTorEx/sova/actions/runs/37950947956).
The workflow passed actionlint preflight; make check passed 27 Rust tests and
isolated delivery checks, including default legacy config migration, existing
Sova settings precedence and explicit config overrides. Smoke-install passed.

Downloaded the three [published v0.2.0 assets](https://github.com/RoTorEx/sova/releases/tag/v0.2.0).
Verified the archive checksum and exact four regular-file entries. Installed
through the published installer into ~/.x-cli-sova/bin/sova; installed bytes
match the published payload. --version, -V and version all report sova 0.2.0.
An isolated pre-release Sova 0.1.2 updated itself to published 0.2.0 through the
new repository URL; installed update then reported Already up to date. Updated
bytes match the installed published binary.

The real installed TUI in a controlling PTY displayed Sova v0.2.0, both panes
and all compact status symbols/help. Quit restored terminal/mouse modes and
reaped its own terminal helpers. Agent processes were not controlled.

Copied the actual user's legacy config byte-for-byte to ~/.config/sova;
SHA-256 stayed d7676f346fc407140ac9dd654d63bb3596c418b3e5347d6384eceb807d8789d8.
After replacement verification, removed the old binary and empty install/config
directories and only the old managed PATH block. The unrelated shell profile
bytes were preserved. A fresh interactive zsh resolved the new installed sova,
reported 0.2.0, and could no longer resolve fleet-scope.

Moved the source directory to ~/Documents/WorkSpace/Development/personal/cli/sova
and verified the working tree and canonical origin at the new path. Historical
Desktop input archives and prior release payloads were preserved.
The installed `sova list --json` subsequently returned eight live sessions, all
eight linked; it did not select any terminal pane. make check also passed after
the source-directory move.


## Three-pane UI local verification — 2026-10-09

Built and installed the current main UI locally; package version remains 0.2.0
and these changes are Unreleased. Sessions sits above History in the left 60%
column with full-height Details at right; below 90 columns they stack vertically.
30 Rust tests cover history role filters, preview/full-text tail visibility,
keyboard/mouse focus, independent scrolling and resize boundaries. make check
also passed delivery/config-preservation and isolated release checks.

Exercised the installed binary in a controlling PTY: actual linked sessions,
visible terminal window/tab/pane/folder and identity sections, double focus
borders, Tab transitions, 1/2/3 filters, arrows, Details End/Home scrolling,
all five live t theme changes with comment/timer preservation, sizes 150×40,
80×25 and 50×12, mouse filtering, help/Esc and q. Exit restored the terminal and
reaped owned helpers. Agent processes and terminal names were not modified.
The user's installed palette is sunset, matching their active gtd preset;
other configuration values were preserved. Restart an already-open Sova view
with q and sova to load the installed UI.

History remains a visible terminal snapshot; the filters do not create a full
provider archive. Session names shown in Details remain terminal-derived; native
chat renaming stays with the agent CLI and terminal tab renaming stays with iTerm2.

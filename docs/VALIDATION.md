# Validation evidence

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

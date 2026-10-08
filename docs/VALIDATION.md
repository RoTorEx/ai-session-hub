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

## Installed build — 2026-10-09

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
observed; no speculative activity classifier is implemented.

Task-created build cache and dist under ~/construction_side/fleet-scope were
removed after verification. Future builds recreate them. Installed executable
and configuration remain available independently; source truth is in the
repository. User agent processes, screenshots and input archives were preserved.

# Changelog

## [Unreleased]

- Start with Sessions focus; add f focus cycling and keep mouse clicks from changing it.
- Navigate all History messages individually with 100-character previews; open selected text in the right pane and restore Details with Esc.
- Preserve message selection on refresh, remove role filters and duplicate latest exchange details.
- Add the calm default palette, subtle focus borders and simpler directory groups.

## [0.2.1] - 2026-10-09

- Split the view into compact Sessions above filtered History and full-height
  Details, with a vertical layout for narrow terminals and short parent paths.
- Add ruled Details sections and visible session names, stronger focus borders,
  pinned history filters and explicit full-text/preview arrow controls.
- Apply and save gtd palettes directly with t; expose metadata in Details and
  explain when there is no longer text to expand.

## [0.2.0] - 2026-10-09

- Rename the CLI, repository, release assets, installation and configuration
  paths to Sova (`sova`); retain the repository history and earlier releases.
- Preserve existing Sova settings and copy legacy FleetScope settings when
  installing into an absent default Sova configuration.

## [0.1.2] - 2026-10-09

- Add compact session activity symbols, questions/action requests and reported
  blockers, bounded all-session status polling and in-memory ready/opened
  response tracking by exact foreground pane or confirmed Jump.
- Refuse a newly ambiguous shared pane during fresh status/Jump validation.

## [0.1.1] - 2026-10-09

- Keep concurrent updater temporary directories distinct even when the system
  clock returns the same timestamp; preserve automatic cleanup.
- Fix GitHub Actions artifact directory context and allow publication of an
  existing release tag after workflow repair without rewriting the tag.

## [0.1.0] - 2026-10-09

- Session-only navigation, simplified project headings and readable gtd-style
  text/selection contrast; permanent version header.
- Structured status/terminal/latest exchange/history details, read-only selected
  terminal context, mouse scrolling, message expansion and optional technical IDs.
- Separate native inventory/content/location timers, cached Git/pane metadata,
  unchanged-frame suppression and independent fast exact-pane Jump.
- Reused task-owned iTerm2 helpers with timeout/exit cleanup; explicit local
  benchmark with CPU timebase conversion and separately observed iTerm2 cost.

- Discover live interactive Codex and Claude processes, including agents waiting
  for input or approval, and distinguish independent instances in one project.
- Revalidate PID/start time, TTY and pane ancestry before selecting an existing
  iTerm2 window/tab/split pane; refuse ambiguous or changed links.
- Add grouped two-pane TUI, independent detail scrolling, search, refresh and
  five gtd-derived themes. No monitoring after exit; agents remain independent.
- Add plain/JSON lists, scriptable Jump, version/config/theme/update commands.
- Add config-preserving local/release installation, checksum-verified updater,
  isolated delivery checks and neighbor-compatible release preparation/push.

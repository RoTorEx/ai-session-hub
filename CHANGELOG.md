# Changelog

## [Unreleased]

## [0.3.3] - 2026-10-31

- Add bounded, metadata-only local product usage evidence; preserve business and recovery state.

## [0.3.2] - 2026-10-28

- Toolbelt migration: use `~/.toolbelt/<name>` for custom installations and update project-path references where applicable.

## [0.3.1] - 2026-10-20

- Keep Updated visible in narrow terminals, separate columns and truncate Unicode titles cleanly; remove redundant request-time columns, `ago`, approximation prefixes and the home heading.
- Distinguish New reply, Opened and Untracked opening history; record visits to initially idle panes and retain opening state when response markers disappear.
- Reorder Details into overview/project/request/answer with bounded previews; move technical fields and data-source explanations to a separate Session Info view.
- Read the selected visible screen once for Details and refresh it on selected text changes, recovering content above the compact status tail without periodic full reads outside History.

## [0.3.0] - 2026-10-20

- Rename the product to AI Session Hub (`ai-session-hub`), including the GitHub repository, CLI, TUI, release assets, installation and configuration paths.
- Copy existing Sova settings before falling back to FleetScope settings on first installation; preserve existing settings and all historical releases.

## [0.2.5] - 2026-10-20

- Redesign Sessions as a grouped task/status/update/request table with status filters and a 65% / 35% layout.
- Show model, reasoning, project and latest request/answer in Details while retaining the iteration History reader.
- Read bounded local provider metadata only with process/session binding; distinguish observed times and show Warsaw calendar times after one hour.

## [0.2.4] - 2026-10-13

- Read full selected content only in History; refresh compact statuses every 10 seconds on a separate timer.
- Pause background scans on terminal FocusOut and refresh immediately on FocusIn, retaining mode and selection.
- Release both iTerm2 helpers after 60 seconds without user activity; input or focus return warms them and refreshes data.

## [0.2.3] - 2026-10-13

- Use full-height Sessions | Details and History | Content views; f switches both columns while retaining session selection, metadata and Jump.
- Show iteration content automatically; Tab switches history/content focus for scrolling.

## [0.2.2] - 2026-10-12

- Group each request with its answers and actions in one History iteration, selecting the top iteration on session changes.
- Open the full iteration in Content; allow focus switching for scrolling and keep passive Details outside the focus cycle.
- Preserve iteration selection as responses grow; remove role filters and duplicate latest exchange details.
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

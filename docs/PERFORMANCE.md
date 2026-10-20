# Local measurements — 2026-10-09

Historical measurements retain their original FleetScope and Sova labels. Current commands use AI Session Hub.

macOS 26.6.1 arm64, iTerm2 3.7.3; live Codex agents and other terminal tasks
kept running. These are measurements of this machine, not a guarantee of a universal
"ideal" interval or zero load.

## Individual operations

Original installed CLI: 5 full collections, median **847 ms**; 5 Jumps to one
current agent pane, median **2114 ms**. The CLI additionally collected the list
before the shared Jump, which performed a full collection again. Its latency is not
equal to a single Apple Event. The set of live processes changed as the user worked.

After the changes, `ai-session-hub benchmark --samples 20 --jump SESSION_ID`:
7 agents. The inventory and channel are warmed up; Jump measures the same shared path
the TUI uses, without CLI startup/the first full list.

| Operation | Median, ms | p95, ms | CPU FleetScope + helpers, ms/operation | Observed iTerm2 CPU, ms/operation |
|---|---:|---:|---:|---:|
| Native process list | 1.01 | 1.26 | 1.17 | 0.86 |
| Inventory with metadata cache | 1.27 | 1.43 | 1.33 | 0.29 |
| Full iTerm2 map | 142.11 | 232.14 | 15.41 | 61.37 |
| Full uncached collection | 399.47 | 454.34 | 124.23 | 131.89 |
| Selected pane content | 60.35 | 86.77 | 7.73 | 35.70 |
| Exact Jump | 99.17 | 210.42 | 8.13 | 55.99 |

CPU includes the FleetScope process, live direct helper processes and exited
own children. Native `proc_taskinfo` time is converted from Mach ticks using the
system timebase; `getrusage` is counted in seconds. Early diagnostic
runs without this conversion are not used for CPU conclusions.

iTerm2 also serves other running panes, rendering and unrelated
requests. Its column is the entire observed increase during the series; it is not a measurement
of FleetScope's additional cost alone. p95 over 20 observations is approximate;
a moved tab, the first API load and macOS permissions can increase latency.
After an asynchronous window change, the UUID is confirmed with short repeated reads
for up to 250 ms; a match is still mandatory. After this fix, ten
jumps between two windows were confirmed: median 115.6 ms, maximum 196.3 ms.
This is a series of API jumps through a warmed channel; the additional native identity
check takes roughly the native collection time from the table.

## Real runs with different intervals

`scripts/benchmark-intervals.py` opens only its own TUI in a PTY,
leaves user agents alone and does not store terminal text.
Colored interface (`NO_COLOR` not set). After three seconds of
initialization it measures 15 seconds of each profile. This
counts the live TUI and API helpers; exited short-lived Git processes
are not fully counted in PTY runs (unlike the individual operations table).
All CPU
values are percentages of **one core**, not divided by the computer's core count.

| Processes / content / meta, s | CPU FleetScope + helpers | Observed iTerm2 CPU | RSS FleetScope + helpers, MiB |
|---|---:|---:|---:|
| 1 / 1 / 1 | 2.521% | 39.058% | 75.0 |
| 2 / 2 / 2 | 1.173% | 34.625% | 74.2 |
| 5 / 5 / 5 | 0.628% | 35.763% | 73.2 |
| 2 / 5 / 30 | **0.428%** | 43.248% | 73.4 |

On these short runs the split profile reduced the CPU cost of the live TUI/API helpers
by roughly 83% relative to full once-per-second polling. The change in total terminal
CPU does not prove a causal effect: background work in other panes varied. In a separate
15-second control without its own TUI, iTerm2 consumed 41.555% of one core;
this background value cannot simply be subtracted from unsynchronized runs.
RSS is the sum of the resident memory of three processes, not physical unique memory;
shared pages may be counted multiple times. The two JXA helper processes add a
constant memory cost in exchange for a fast Jump and fewer launches.

**2 / 5 / 30 seconds** was chosen: openings/closings are noticed quickly by cheap
native collection; selected pane content refreshes every five seconds;
locations every thirty seconds, on a new agent or on `r`. On selection, content
is read after a 150 ms debounce. Jump has a separate warmed channel and does not wait
for the background queue; the current location is resolved by UUID on every Jump.
In this 15-second run the unchanged screen produced no new output bytes. The optimal interval depends on
scrollback contents, the number of panes and terminal load.

## Reproduction

```sh
make bench
ai-session-hub benchmark --samples 20
# Only if you actually intend to select an existing pane:
ai-session-hub benchmark --samples 20 --jump SESSION_ID
python3 scripts/benchmark-intervals.py --binary ~/.x-cli-ai-session-hub/bin/ai-session-hub --seconds 15
```

The interval script closes only its own TUI/helper processes and temporary configs.
PTY closing on Darwin was fixed: parent descriptors are closed before
waiting for the session leader to exit, so the harness itself does not hang on shutdown.


## Status of all sessions — 2026-10-09

The new TUI reads the status of all linked agents every content_seconds (5 seconds by
default), separately from the native inventory and metadata. Only the tail of the
visible screen is returned (24 lines / 8192 characters); detailed content remains limited to the selected session.
The benchmark command now includes a separate `all session status` row.

On eight live sessions, ten measurements each: the initial variant with repeated
UUID getters gave a median of 772.75 ms and 75.27 ms CPU for FleetScope + owned helpers.
After removing redundant Apple Events, 249.77 ms / 16.00 ms CPU were obtained; a repeated
final run under different terminal load gave 709.25 ms / 71.57 ms CPU.
This is a range of roughly 0.32–1.43% of one core for the additional status
scan every 5 seconds, estimated from burst measurements. Existing selected-content,
inventory and metadata costs add separately. This is not a repeat of the earlier
15-second steady-state PTY profile; the load and number of agents changed.
The measured iTerm CPU includes other sessions and is not attributed entirely to FleetScope.
These data do not justify reducing the interval for all panes to 1 second;
5 seconds was kept as a compromise. Under heavy load content_seconds can be
increased, accepting less frequent observation of completions/openings.


## Mode-based polling and focus pause — 2026-10-13

Sequential runs on this machine, 20 seconds each after 5 seconds of
initialization. Compared are the installed published 0.2.3 and a new
local build with process/content/metadata intervals of 2/5/30 seconds.
Statuses every 5 seconds in the old version, every 10 seconds in the new one.
User agents kept working; iTerm2 load was not controlled.

| Build / mode | CPU Sova + helpers, % of one core | Total RSS, MiB | Output bytes per measurement |
|---|---:|---:|---:|
| Published 0.2.3 / Sessions | 2.070 | 83.4 | 19 |
| New / Sessions | 0.872 | 75.2 | 0 |
| New / History | 1.024 | 82.0 | 19 |
| New / pause after FocusOut | 0.005 | 73.3 | 0 |

Sessions in this series needed roughly 58% less CPU. These are short
unsynchronized measurements, not a guarantee of savings for any number of agents. RSS
sums processes and may count shared pages repeatedly; the RSS difference
between runs cannot be considered proven memory savings. During these short pauses the helpers stay
in memory; the later shutdown after 60 seconds is measured separately below. Observed iTerm2 CPU was 50.042%,
42.017%, 33.008% and 34.331% respectively; it includes other panes and cannot be
attributed entirely to Sova or subtracted between unsynchronized series.

Full content is read only in History; background requests are paused
by terminal focus events, without visibility polling. The harness introduces the pause
with the FocusOut sequence (`ESC [ O`), which the installed TUI understands;
this checks event handling, not a separate measurement of actually hiding the window.
On FocusIn (`ESC [ I`) refresh requests resume immediately.
Physical memory and rare wakeups remain, so this is not zero cost.

```sh
python3 scripts/benchmark-intervals.py --binary ~/.x-cli-ai-session-hub/bin/ai-session-hub --seconds 20 --profile 2/5/30 --view sessions
python3 scripts/benchmark-intervals.py --binary ~/.x-cli-ai-session-hub/bin/ai-session-hub --seconds 20 --profile 2/5/30 --view history
python3 scripts/benchmark-intervals.py --binary ~/.x-cli-ai-session-hub/bin/ai-session-hub --seconds 20 --profile 2/5/30 --view paused
```

`--activity-seconds` sets the independent status interval (10 by default).
`--activity-seconds 5` was used for comparison with the old version; old
binaries ignore the unknown field and use their content_seconds.
Temporary configs and harness processes are removed at the end of each run.


### Helper shutdown after 60 seconds without input

The final installed build was verified in its own PTY with the real
60-second timer, without shortening the timeout for the test. At startup there were
two direct helper processes; after 65 seconds without input SLEEP appeared and zero
helper processes remained. The old PIDs no longer existed. The RSS of the remaining Sova
was **5.3 MiB**. Over the next 10.1 seconds `ps time` showed no CPU
increase; its 0.01 CPU-second precision does not allow claiming absolute zero.
The unchanged screen produced no new bytes.

Input f created two new helper PIDs and restored History/Content. q exited the
TUI with code 0, disabled focus tracking mode and terminated both new
helpers. User processes and agent panes were not controlled.
The first jump/read after sleep requires starting the runtime; its latency was not
measured separately in this check.

To repeat the resource measurement after a real idle timeout:

```sh
python3 scripts/benchmark-intervals.py --binary ~/.x-cli-ai-session-hub/bin/ai-session-hub --seconds 20 --profile 2/5/30 --view idle
```

The idle mode waits for the real timeout, verifies that the helpers disappear and then
measures the remaining TUI. The number of helpers does not depend on the number of agents: at most
two while running, zero during sleep. Status scan work grows with the number of
linked panes; full content remains limited to the selected session.

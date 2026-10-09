#!/usr/bin/env python3
"""Measure actual owned TUI runs; user agents are read but never controlled.
CPU includes the TUI and its live direct helpers; iTerm2 is observed separately
and includes unrelated terminal activity. No terminal text is saved or printed.
"""
import argparse
import ctypes
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time

class Task(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in
                ("virtual", "resident", "user", "system", "thread_user", "thread_system")]
    _fields_ += [(f"field{i}", ctypes.c_int32) for i in range(12)]
class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]
lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
base = Timebase()
assert lib.mach_timebase_info(ctypes.byref(base)) == 0

def task(pid):
    value = Task()
    if lib.proc_pidinfo(pid, 4, 0, ctypes.byref(value), ctypes.sizeof(value)) != ctypes.sizeof(value):
        return (0, 0)
    return ((value.user + value.system) * base.numer / base.denom / 1e9, value.resident)

def children(pid):
    result = subprocess.run(['/usr/bin/pgrep', '-P', str(pid)], capture_output=True, text=True)
    return [int(x) for x in result.stdout.split()]

def run(binary, root, seconds, profile, iterm):
    native, content, metadata = profile
    with tempfile.TemporaryDirectory(prefix='interval-', dir=root) as directory:
        config = Path(directory) / 'config.toml'
        config.write_text(f'theme="classic"\nrefresh_seconds={native}\ncontent_seconds={content}\nmetadata_seconds={metadata}\n')
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 35, 140, 0, 0))
        def attach():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        process = subprocess.Popen([str(binary)], stdin=slave, stdout=slave, stderr=slave,
                                   preexec_fn=attach,
                                   env={**os.environ, 'TERM': 'xterm-256color', 'SOVA_CONFIG': str(config)})
        owned = [process.pid]
        byte_count = 0
        def drain(duration):
            nonlocal byte_count
            end = time.monotonic() + duration
            while time.monotonic() < end:
                if select.select([master], [], [], 0.1)[0]:
                    data = os.read(master, 65536)
                    byte_count += len(data)
                    if b'\x1b[6n' in data:
                        os.write(master, b'\x1b[1;1R')
                if process.poll() is not None:
                    raise RuntimeError('TUI exited unexpectedly')
        try:
            drain(3)  # Exclude initialization and first content read.
            owned = [process.pid, *children(process.pid)]
            start = [task(pid) for pid in owned]
            terminal_start = task(iterm)[0]
            wall = time.monotonic()
            byte_count = 0
            drain(seconds)
            elapsed = time.monotonic() - wall
            after = [task(pid) for pid in owned]
            cpu = sum(a[0]-b[0] for a, b in zip(after, start))
            terminal_cpu = task(iterm)[0] - terminal_start
            result = dict(profile=f'{native}/{content}/{metadata}', seconds=round(elapsed, 2),
                          owned_cpu_pct_one_core=round(100*cpu/elapsed, 3),
                          observed_iterm_cpu_pct_one_core=round(100*terminal_cpu/elapsed, 3),
                          owned_rss_mib=round(sum(a[1] for a in after)/1024/1024, 1),
                          output_bytes=byte_count)
            print(json.dumps(result), flush=True)
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
            # Close the parent's slave before waiting: Darwin can keep an
            # exiting session leader waiting on a still-open test PTY.
            os.close(slave)
            end = time.monotonic() + 0.5
            while time.monotonic() < end:
                if select.select([master], [], [], 0.1)[0]:
                    try: os.read(master, 65536)
                    except OSError: break
            os.close(master)
            try: process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait(timeout=3)
            assert all(task(pid)[0] == 0 for pid in owned), 'Owned TUI helper survived exit'

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--seconds', type=float, default=15)
args = parser.parse_args()
assert args.seconds >= 5
root = Path.home() / 'construction_side/sova'
root.mkdir(parents=True, exist_ok=True)
iterm = int(subprocess.check_output(['/usr/bin/pgrep', '-x', 'iTerm2'], text=True).strip())
for profile in [(1, 1, 1), (2, 2, 2), (5, 5, 5), (2, 5, 30)]:
    run(args.binary.resolve(), root, args.seconds, profile, iterm)

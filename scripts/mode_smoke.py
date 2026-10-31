#!/usr/bin/env python3
"""Verify mode persistence across actual processes, including abrupt termination."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/ticktop").resolve())


def session(data, flags, expected, switch_to=None):
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.execv(binary, [binary, "--demo", "--data-dir", data, *flags])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))

    def wait_for(label):
        output = bytearray()
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            if select.select([fd], [], [], 0.1)[0]:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    break
                if not chunk:
                    break
                output.extend(chunk)
                if label.encode() in output:
                    return
        raise AssertionError(f"Missing screen: {label}; output: {output[-500:]!r}")

    try:
        wait_for(expected)
        if switch_to is not None:
            os.write(fd, b"m")
            wait_for(switch_to)
            # Do not allow a periodic save or graceful shutdown to hide a failure.
            os.kill(pid, signal.SIGKILL)
        else:
            os.write(fd, b"q")
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            child, _ = os.waitpid(pid, os.WNOHANG)
            if child:
                return
            time.sleep(0.01)
        raise AssertionError("Application did not exit")
    finally:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.close(fd)


with tempfile.TemporaryDirectory(prefix="ticktop-mode-") as data:
    session(data, ["--full"], "ticktop_", "metrics --watch")
    state_path = Path(data, "demo-state.json")
    assert json.loads(state_path.read_text())["mode"] == "minimal"
    session(data, [], "metrics --watch", "ticktop_")
    assert json.loads(state_path.read_text())["mode"] == "full"
    session(data, [], "ticktop_")
    print("Mode restart passed: full -> minimal -> restart -> full -> restart; saved before abrupt termination")

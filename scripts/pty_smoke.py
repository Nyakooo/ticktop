#!/usr/bin/env python3
"""Real terminal smoke test. No public network or third-party Python dependencies."""
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
with tempfile.TemporaryDirectory(prefix="ticktop-pty-") as data:
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.execv(binary, [binary, "--demo", "--full", "--data-dir", data])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 130, 0, 0))
    output = bytearray()

    def drain(seconds=0.25):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            ready, _, _ = select.select([fd], [], [], max(0, deadline - time.monotonic()))
            if ready:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    break
                if not chunk:
                    break
                output.extend(chunk)

    def send(value):
        os.write(fd, value.encode())
        drain()

    try:
        drain(1)
        assert "ticktop_" in output.decode(errors="replace"), "initial render missing"
        send("a")
        send("宁德")
        send("\r")  # choose instrument
        send("\x1b[B")  # choose first named group
        send("\r")
        send("g")
        send("n")
        send("摸鱼观察")
        send("\r")
        send("m")
        send("h")
        send("\t")  # whole-market concept classification
        send("\x1b[F")  # last sector
        send("\x1b[5~")  # previous cloud page
        send("n")
        send("p")
        send("\x1b[<0;2;35M")  # SGR mouse: footer mode -> full
        send("\x1b[<0;2;35m")
        send("\x1b[<0;2;35M")  # footer mode -> minimal
        send("\x1b[<0;2;35m")
        send("\x1b[<65;10;10M")  # mouse wheel next page
        send(" ")
        send(" ")
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        os.kill(pid, signal.SIGWINCH)
        drain()
        send("?")
        send("\x1b")
        send("q")
        deadline = time.monotonic() + 3
        status = None
        while time.monotonic() < deadline:
            child, code = os.waitpid(pid, os.WNOHANG)
            if child:
                status = code
                break
            drain(0.1)
        assert status is not None, "application did not exit"
        assert os.waitstatus_to_exitcode(status) == 0, output.decode(errors="replace")[-1000:]
        state = json.loads(Path(data, "demo-state.json").read_text())
        assert state["mode"] == "minimal"
        assert state["heatmap"] is True
        assert state["sector_category"] == "concept"
        assert any(g["name"] == "摸鱼观察" for g in state["groups"])
        assert b"\x1b[?1006h" in output, "mouse capture not enabled"
        assert b"\x1b[?1006l" in output, "mouse capture not disabled"
        assert b"\x1b[?1049l" in output, "alternate screen not restored"
        print("PTY smoke passed: Chinese search, group creation, modes, heatmap, pause, resize, help, exit")
    finally:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.close(fd)

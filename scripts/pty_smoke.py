#!/usr/bin/env python3
"""Real terminal smoke test. Offline; install scripts/requirements-smoke.txt first."""
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import sys
import tempfile
import termios
import time
from terminal_screen import TerminalScreen

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/ticktop").resolve())
read_size = int(os.environ.get("TICKTOP_SMOKE_READ_SIZE", "65536"))
with tempfile.TemporaryDirectory(prefix="ticktop-pty-") as data:
    pid, fd = pty.fork()
    if pid == 0:
        # Set the initial size before exec, rather than racing the first render.
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 130, 0, 0))
        os.environ["TERM"] = "xterm-256color"
        os.execv(binary, [binary, "--demo", "--full", "--data-dir", data])
    screen = TerminalScreen(130, 36)
    output = bytearray()
    actions = []
    state_path = Path(data, "demo-state.json")

    def state():
        try:
            return json.loads(state_path.read_text())
        except (OSError, ValueError):
            return {}

    def drain(seconds=0.05):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if not select.select([fd], [], [], max(0, deadline - time.monotonic()))[0]:
                return
            try:
                chunk = os.read(fd, read_size)
            except OSError:
                return
            if not chunk:
                return
            output.extend(chunk)
            screen.feed(chunk)

    def wait_until(predicate, message, timeout=12):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            drain()
        raise AssertionError(f"{message}\nActions: {actions!r}\nScreen:\n{screen.text}"
                             f"\nRaw output tail: {bytes(output[-1500:])!r}")

    def wait_text(label, present=True):
        wait_until(lambda: (label in screen.text) == present,
                   f"screen {'missing' if present else 'still contains'}: {label}")

    def send(value):
        actions.append(value)
        os.write(fd, value.encode())

    def resize(columns, lines):
        screen.resize(columns, lines)
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", lines, columns, 0, 0))
        # TIOCSWINSZ already signals this controlling terminal's foreground group.

    def check_metric_order():
        wait_until(lambda: re.search(r"量比[\s\S]*?换手率[\s\S]*?成交量", screen.text),
                   "metric order must be volume ratio, turnover, volume")

    try:
        wait_text("ticktop_")
        check_metric_order()
        send("m")
        wait_text("metrics --watch")
        check_metric_order()
        send("m")
        wait_text("ticktop_")
        send("\x1b[C")  # intraday -> daily candles
        wait_text("成交量 日 K")
        send("\x1b[H")  # pan to oldest candle
        send("\x1b[F")  # latest candle
        send("\x1b[D")
        wait_text("成交量 分时")
        send("a")
        wait_text("添加自选")
        send("宁德")
        wait_text("宁德时代")
        send("\r")
        wait_text("添加到分组")
        send("\x1b[B\r")
        wait_until(lambda: state().get("group") == 1 and state().get("selected") == "sz300750",
                   "adding stock to named group was not saved")
        send("\x1b[1;5B")
        wait_until(lambda: next((g for g in state().get("groups", []) if g["id"] == 1), {}).get("symbols", [])[1:2] == ["sz300750"],
                   "manual order was not saved")
        send("t")
        wait_until(lambda: "sz300750" in state().get("pinned", {}).get("1", []), "pin not saved")
        send("e")
        wait_text("简约模式别名")
        send("worker-1\r")
        wait_until(lambda: state().get("custom_aliases", {}).get("sz300750") == "worker-1", "alias not saved")
        send("g")
        wait_text("自选分组")
        send("n")
        wait_text("新建分组")
        send("摸鱼观察\r")
        wait_until(lambda: any(g["name"] == "摸鱼观察" for g in state().get("groups", [])), "group not created")
        send("r")
        wait_text("重命名分组")
        send("\x15观察清单\r")
        wait_until(lambda: any(g["name"] == "观察清单" for g in state().get("groups", [])), "group not renamed")
        send("i")
        wait_text("常驻指数")
        send("\x15上证指数,深证成指,恒生指数,科创50,沪深300\r")
        wait_until(lambda: '"hkHSI"' in Path(data, "config.toml").read_text(), "index configuration was not saved")
        send("/")
        wait_text("搜索 ·")
        send("510300")
        wait_text("沪深300ETF")
        send("\r")
        wait_text("搜索 ·", present=False)
        send("m")
        wait_text("metrics --watch")
        send("h")
        wait_until(lambda: state().get("heatmap") is True, "cloud did not open")
        send("\t")
        wait_until(lambda: state().get("sector_category") == "concept", "cloud classification not saved")
        send("\x1b[F\x1b[5~np")
        # Footer mouse clicks are meaningful only after the layout is visible.
        wait_text("metrics --watch")
        send("\x1b[<0;2;35M\x1b[<0;2;35m")
        wait_text("ticktop_")
        send("\x1b[<0;2;35M\x1b[<0;2;35m")
        wait_text("metrics --watch")
        send("\x1b[<65;10;10M")
        send(" ")
        wait_text("hold")
        send(" ")
        wait_text("hold", present=False)
        resize(80, 24)
        wait_until(lambda: "m mode" in screen.text.splitlines()[22], "resized footer not rendered")
        send("?")
        wait_text("快捷键 / 编号映射")
        send("\x1b")
        wait_text("快捷键 / 编号映射", present=False)
        send("q")
        status = None

        def exited():
            global status
            child, code = os.waitpid(pid, os.WNOHANG)
            if child:
                status = code
                return True
            return False

        wait_until(exited, "application did not exit")
        assert os.waitstatus_to_exitcode(status) == 0, bytes(output[-1000:])
        saved = state()
        assert saved["mode"] == "minimal"
        assert saved["heatmap"] is True
        assert saved["sector_category"] == "concept"
        assert any(g["name"] == "观察清单" for g in saved["groups"])
        assert saved["custom_aliases"]["sz300750"] == "worker-1"
        assert "sz300750" in saved["pinned"]["1"]
        assert next(g for g in saved["groups"] if g["id"] == 1)["symbols"][1] == "sz300750"
        assert b"\x1b[?1006h" in output, "mouse capture not enabled"
        assert b"\x1b[?1006l" in output, "mouse capture not disabled"
        assert b"\x1b[?1049l" in output, "alternate screen not restored"
        print("PTY smoke passed: metric order in both modes, Chinese/ETF search, resident indices, groups, modes, heatmap, pause, resize, help, exit")
    finally:
        try:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except (ProcessLookupError, ChildProcessError):
            pass
        os.close(fd)

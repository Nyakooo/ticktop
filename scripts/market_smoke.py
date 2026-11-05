#!/usr/bin/env python3
"""Exercise market navigation and portable US preferences through the real Unix PTY."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import tempfile
import time
import termios
from terminal_screen import TerminalScreen

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else 'target/release/ticktop').resolve())

with tempfile.TemporaryDirectory(prefix='ticktop-market-smoke-') as directory:
    state_path = Path(directory, 'demo-state.json')
    def state():
        try:
            return json.loads(state_path.read_text())
        except (OSError, ValueError):
            return {}

    def launch(*flags):
        global screen
        screen = TerminalScreen(130, 36)
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 36, 130, 0, 0))
        proc = subprocess.Popen([binary, '--demo', '--data-dir', directory, *flags], stdin=slave, stdout=slave, stderr=slave, env={**os.environ, 'TERM': 'xterm-256color'})
        os.close(slave)
        return proc, master

    output = bytearray()
    def wait_until(predicate, message):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            if select.select([master], [], [], .05)[0]:
                try:
                    chunk = os.read(master, 65536)
                    output.extend(chunk)
                    screen.feed(chunk)
                except OSError:
                    pass
            if predicate():
                return
            if proc.poll() is not None:
                break
        raise AssertionError(message + '\n' + output[-1500:].decode(errors='replace'))

    def visible():
        return screen.text

    def send(value):
        os.write(master, value.encode())

    proc, master = launch('--full')
    try:
        wait_until(lambda: 'ticktop_' in visible(), 'startup missing')
        send('\x1bOQ')  # F2
        wait_until(lambda: state().get('overseas', {}).get('page') == 'indices', 'F2 did not select indices')
        wait_until(lambda: '日经225' in visible(), 'index page missing')
        send('j\r\x1b[C')
        wait_until(lambda: state().get('overseas', {}).get('index_selected') == '^KS11' and state()['overseas']['index_period'] == 'day', 'selection/period not saved')
        send('h')
        wait_until(lambda: state().get('heatmap') is True, 'h did not enter cloud')
        send('h')
        wait_until(lambda: state().get('overseas', {}).get('page') == 'indices', 'h did not return to index page')
        send('\x1b')
        wait_until(lambda: state()['overseas']['index_detail'] is False, 'Esc did not return to overview')
        send('a')
        wait_until(lambda: '添加指数' in visible(), 'index search missing')
        send('沪深300\r')
        wait_until(lambda: state()['overseas']['index_selected'] == 'sh000300' and not state()['overseas']['index_all'], 'index follow/search failed')
        send('t')
        wait_until(lambda: 'sh000300' in state()['overseas']['index_pinned'], 'index pin not saved')
        send('\r')
        wait_until(lambda: state()['overseas']['index_detail'] is True, 'Enter did not show index history')
        send('\x1b')
        wait_until(lambda: state()['overseas']['index_detail'] is False, 'Esc did not close domestic history')
        send('\x1bOR')  # F3
        wait_until(lambda: state().get('overseas', {}).get('page') == 'us', 'F3 did not select US page')
        send('a')
        wait_until(lambda: '订阅美股' in visible(), 'subscription input missing')
        send('微软\r')
        wait_until(lambda: state().get('overseas', {}).get('us_selected') == 'MSFT', 'Chinese subscription failed')
        send('\x1b[1;3A')  # Alt+Up reorders the current US subscription.
        wait_until(lambda: state()['overseas']['subscriptions'].index('MSFT') == 2, 'US order not saved')
        send('t')
        wait_until(lambda: 'MSFT' in state()['overseas'].get('us_pinned', {}).get('0', []), 'US pin not saved')
        wait_until(lambda: '^ MSFT' in visible(), 'US pin marker missing')
        send('eoffice\r')
        wait_until(lambda: state().get('overseas', {}).get('aliases', {}).get('MSFT') == 'office', 'US alias missing')
        send('m')
        wait_until(lambda: state().get('mode') == 'minimal', 'minimal mode not saved')
        send('\t')
        wait_until(lambda: state().get('overseas', {}).get('session') == 'pre', 'session switch missing')
        send('q')
        wait_until(lambda: proc.poll() is not None, 'exit failed')
        assert proc.returncode == 0
    finally:
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)
        os.close(master)

    proc, master = launch()
    output.clear()
    try:
        wait_until(lambda: 'office' in visible(), 'US alias/page not restored on restart')
        assert state()['overseas']['page'] == 'us' and state()['mode'] == 'minimal'
        assert 'MSFT' in state()['overseas']['us_pinned']['0']
        assert state()['overseas']['subscriptions'].index('MSFT') == 2
        assert 'sh000300' in state()['overseas']['index_watchlist'] and 'sh000300' in state()['overseas']['index_pinned']
        send('q')
        wait_until(lambda: proc.poll() is not None, 'restart exit failed')
        assert proc.returncode == 0
    finally:
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)
        os.close(master)
print('PASS: F1-F4 navigation, h return, periods, Chinese US subscriptions, aliases, sessions and restart persistence')

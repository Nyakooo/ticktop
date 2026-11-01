#!/usr/bin/env python3
"""Native Windows ConPTY check, using pywinpty only on the CI/test machine."""
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import time
from winpty import PtyProcess

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/ticktop.exe').resolve())
with tempfile.TemporaryDirectory(prefix='ticktop-conpty-') as data:
    def launch(*flags):
        process = PtyProcess.spawn([binary, '--demo', '--data-dir', data, *flags], dimensions=(36, 130), env=dict(os.environ))
        chunks = []
        def read():
            try:
                while True:
                    chunk = process.read(65536)
                    if chunk:
                        chunks.append(chunk)
            except (EOFError, OSError):
                pass
        threading.Thread(target=read, daemon=True).start()
        return process, chunks

    def wait_for(condition, chunks, message):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if condition():
                return
            time.sleep(0.05)
        raise AssertionError(message + '\n' + ''.join(chunks)[-2000:])

    process, chunks = launch('--full')
    try:
        wait_for(lambda: 'ticktop_' in ''.join(chunks), chunks, 'initial screen missing')
        process.write('/')
        time.sleep(0.15)
        process.write('宁德')
        wait_for(lambda: '宁德' in ''.join(chunks), chunks, 'Chinese input missing')
        process.write('\r')
        time.sleep(0.15)
        process.write('i')
        time.sleep(0.15)
        process.write('\x15')
        process.write('上证指数,恒生指数')
        process.write('\r')
        config = Path(data, 'config.toml')
        wait_for(lambda: config.exists() and 'hkHSI' in config.read_text(encoding='utf-8'), chunks, 'resident settings not saved')
        process.write('/')
        time.sleep(0.15)
        process.write('510300')
        time.sleep(0.3)
        process.write('\r')
        time.sleep(0.4)
        process.write('m')
        time.sleep(0.2)
        process.write('h')
        time.sleep(0.2)
        process.setwinsize(24, 80)
        time.sleep(0.2)
        process.write('q')
        wait_for(lambda: not process.isalive(), chunks, 'application did not exit')
        assert process.wait() == 0, ''.join(chunks)[-2000:]
        state = json.loads(Path(data, 'demo-state.json').read_text(encoding='utf-8'))
        assert state['mode'] == 'minimal' and state['heatmap'] is True
        assert state['selected'] == 'sh510300', 'ETF preview did not select the requested fund'
    finally:
        if process.isalive():
            process.terminate(force=True)
        process.close()
    process, chunks = launch()
    try:
        wait_for(lambda: 'metrics' in ''.join(chunks), chunks, 'minimal mode not restored')
        process.write('q')
        wait_for(lambda: not process.isalive(), chunks, 'restart did not exit')
        assert process.wait() == 0
    finally:
        if process.isalive():
            process.terminate(force=True)
        process.close()
print('Windows ConPTY passed: Unicode input, ETF search, index settings, modes, cloud, resize, exit and restart')

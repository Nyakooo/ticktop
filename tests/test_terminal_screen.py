"""Replay the incremental terminal updates that caused intermittent CI failures."""
import re
import unittest
from scripts.terminal_screen import TerminalScreen


class TerminalScreenTests(unittest.TestCase):
    def test_daily_volume_label_is_reconstructed_from_cursor_updates(self):
        initial = "\x1b[2J\x1b[4;1H成交量 分时".encode()
        update = "\x1b[4;8H日 K".encode()
        raw = initial + update
        # This is the former smoke-test assertion: stripping ANSI loses position.
        stripped = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", raw).decode()
        self.assertNotIn("成交量 日 K", stripped)
        for chunk_size in [1, 2, 7, len(raw)]:
            screen = TerminalScreen(40, 10)
            for start in range(0, len(raw), chunk_size):
                screen.feed(raw[start:start + chunk_size])
            self.assertIn("成交量 日 K", screen.text)
            self.assertNotIn("成交量 分时", screen.text)

    def test_metric_order_uses_positions_not_write_order(self):
        screen = TerminalScreen(60, 10)
        # Ratatui can update existing cells in any order, across multiple reads.
        screen.feed("\x1b[2;1H量比 1.2\x1b[4;1H成交量 100\x1b[3;1H换手率 2.0".encode())
        self.assertRegex(screen.text, r"量比[\s\S]*?换手率[\s\S]*?成交量")
        screen.feed("\x1b[2J\x1b[2;1H成交量\x1b[3;1H换手率\x1b[4;1H量比".encode())
        self.assertNotRegex(screen.text, r"量比[\s\S]*?换手率[\s\S]*?成交量")

    def test_cleared_screen_does_not_match_stale_content(self):
        screen = TerminalScreen(40, 10)
        screen.feed("\x1b[2;1H成交量 日 K".encode())
        screen.feed(b"\x1b[2J\x1b[Hmetrics --watch")
        self.assertNotIn("成交量 日 K", screen.text)
        self.assertIn("metrics --watch", screen.text)
        screen.resize(20, 6)
        self.assertEqual(len(screen.screen.display), 6)
        self.assertEqual(len(screen.screen.display[0]), 20)

    def test_wide_character_overwrite_can_leave_an_empty_trailing_cell(self):
        screen = TerminalScreen(40, 10)
        screen.feed("\x1b[H中文\x1b[H ".encode())
        self.assertEqual(screen.screen.buffer[0][1].data, "")
        self.assertIn("文", screen.text)

"""Reconstruct the current VT screen instead of searching historical output."""
import pyte


class TerminalScreen:
    def __init__(self, columns, lines):
        self.screen = pyte.Screen(columns, lines)
        self.stream = pyte.ByteStream(self.screen)

    def feed(self, data):
        # ByteStream retains split UTF-8 characters and incomplete escape sequences.
        self.stream.feed(data)

    def resize(self, columns, lines):
        self.screen.resize(lines=lines, columns=columns)

    @property
    def text(self):
        # Read cells directly: pyte.display assumes a wide character's trailing
        # cell is never visited alone, which incremental CJK overwrites can violate.
        return "\n".join(
            "".join(self.screen.buffer[y][x].data for x in range(self.screen.columns))
            for y in range(self.screen.lines)
        )

from __future__ import annotations

import unittest

from tools.check_firmware_size import parse_size


class ParseSizeTests(unittest.TestCase):
    def test_parses_platformio_memory_lines(self) -> None:
        output = """
RAM:   [==        ]  18.9% (used 485 bytes from 2560 bytes)
Flash: [====      ]  39.0% (used 11172 bytes from 28672 bytes)
"""
        self.assertEqual(parse_size(output), (485, 2560, 11172, 28672))

    def test_strips_ansi_sequences(self) -> None:
        output = (
            "\x1b[32mRAM:   [==        ]  18.9% (used 485 bytes from 2560 bytes)\x1b[0m\n"
            "\x1b[32mFlash: [====      ]  39.0% (used 11172 bytes from 28672 bytes)\x1b[0m\n"
        )
        self.assertEqual(parse_size(output), (485, 2560, 11172, 28672))

    def test_rejects_missing_memory_lines(self) -> None:
        with self.assertRaisesRegex(ValueError, "memory usage lines"):
            parse_size("SUCCESS")


if __name__ == "__main__":
    unittest.main()

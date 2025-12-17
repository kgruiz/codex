"""Print raw bytes received from the terminal (stdin).

Run this script, then press keys in your terminal. It will print the bytes your
terminal sends for each key event, which is useful for debugging keybinding
conflicts like Ctrl+M vs Enter.
"""

from __future__ import annotations

import argparse
import os
import select
import sys
import termios
import time
import tty


def _describe_bytes(data: bytes) -> str:
    if data == b"\r":
        return "CR (\\r) — often Enter/Return; also Ctrl+M"

    if data == b"\n":
        return "LF (\\n) — often newline; also Ctrl+J"

    if data == b"\t":
        return "TAB (\\t) — also Ctrl+I"

    if data == b"\x1b":
        return "ESC"

    if data == b"\x7f":
        return "DEL (0x7f) — often Backspace"

    if data.startswith(b"\x1b["):
        return "ANSI CSI sequence (starts with ESC [)"

    if data.startswith(b"\x1bO"):
        return "ANSI SS3 sequence (starts with ESC O)"

    if len(data) == 1 and 0x00 <= data[0] <= 0x1F:
        return f"Control byte 0x{data[0]:02x}"

    return ""


def _read_event(fd: int, coalesce_timeout_s: float) -> bytes:
    data = bytearray(os.read(fd, 1))

    while True:
        ready, _, _ = select.select([fd], [], [], coalesce_timeout_s)

        if not ready:
            break

        chunk = os.read(fd, 1024)

        if not chunk:
            break

        data.extend(chunk)

    return bytes(data)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--coalesce-ms",
        type=float,
        default=20.0,
        help="Wait this many ms for additional bytes to group into one event (default: 20).",
    )
    args = parser.parse_args()

    if not sys.stdin.isatty():
        print("stdin is not a TTY; run this in an interactive terminal.", file=sys.stderr)

        return 2

    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    coalesce_timeout_s = max(args.coalesce_ms, 0.0) / 1000.0

    print("Reading from stdin in raw mode.")
    print("Press keys to see the bytes your terminal sends.")
    print("Exit: Ctrl+C (0x03) or 'q'.")
    print()

    try:
        tty.setraw(fd)
        counter = 0

        while True:
            data = _read_event(fd, coalesce_timeout_s)

            if data in (b"q", b"\x03"):
                break

            counter += 1
            ts = time.strftime("%H:%M:%S")
            hex_bytes = " ".join(f"{b:02x}" for b in data)
            description = _describe_bytes(data)

            if description:
                description = f" | {description}"

            safe_repr = "".join(chr(b) if 32 <= b <= 126 else "." for b in data)
            print(f"[{counter:06d} {ts}] len={len(data):>3} hex={hex_bytes} repr={data!r} ascii='{safe_repr}'{description}")
            sys.stdout.flush()

    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())


#!/usr/bin/env python3
"""Drive a `try` binary inside a real pty.

The `--and-keys` test hooks feed keys straight into the selector and never
touch the terminal, so they cannot catch raw-mode or stdin-readiness bugs.
This driver runs the binary against an actual pty, sends real key bytes, and
reports whether it exited and whether it left the terminal usable.

Usage:
  pty_drive.py --keys '\\x1b' [--timeout 5] [--settle 0.6] -- BIN [ARGS...]
  pty_drive.py --pre-keys '\\x1b[B' --resize 24x40 --keys '\\r' -- BIN [ARGS...]

Prints a machine-readable report on stdout:
  TIMED_OUT=0|1
  EXIT_CODE=<n>            (absent when TIMED_OUT=1)
  SIGNAL=<n>               (only when killed by a signal)
  TTY_RESTORED=0|1         (ISIG+ICANON+ECHO all back on after exit)
  BYTES_AFTER_RESIZE=<n>   (only with --resize: output redrawn with no keypress)
  WIDEST_AFTER_RESIZE=<n>  (only with --resize: widest redrawn line, in characters)
  TTY_FLAGS_AFTER=isig=on,icanon=on,echo=on
  OUTPUT_B64=<base64 of everything the program wrote to the pty>
"""

import argparse
import base64
import os
import pty
import select
import signal
import struct
import sys
import re
import termios
import time

try:
    import fcntl
except ImportError:  # pragma: no cover - unix only
    fcntl = None


def parse_args():
    ap = argparse.ArgumentParser()
    ap.add_argument("--keys", default="", help="key bytes, python escapes allowed")
    ap.add_argument("--timeout", type=float, default=5.0, help="seconds to wait for exit")
    ap.add_argument("--settle", type=float, default=0.6, help="seconds to wait before sending keys")
    ap.add_argument("--pre-keys", default="", help="key bytes to send before resizing")
    ap.add_argument("--resize", default="", help="resize the pty mid-run, as ROWSxCOLS")
    ap.add_argument("--resize-quiet", type=float, default=1.5,
                    help="seconds to watch for an unprompted redraw after resizing")
    ap.add_argument("--cols", type=int, default=int(os.environ.get("TRY_WIDTH", 80)))
    ap.add_argument("--rows", type=int, default=int(os.environ.get("TRY_HEIGHT", 24)))
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    args = ap.parse_args()
    if args.cmd and args.cmd[0] == "--":
        args.cmd = args.cmd[1:]
    if not args.cmd:
        ap.error("no command given")
    return args


ANSI = re.compile(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][0-9A-B]|\x1b[=>]")


def widest_line(raw):
    """Widest rendered line in characters (not bytes), ANSI stripped."""
    text = ANSI.sub("", raw.decode("utf-8", "replace"))
    return max((len(line) for line in text.replace("\r", "\n").split("\n")), default=0)


def flag_state(attrs):
    lflag = attrs[3]
    return {
        "isig": bool(lflag & termios.ISIG),
        "icanon": bool(lflag & termios.ICANON),
        "echo": bool(lflag & termios.ECHO),
    }


def set_winsize(fd, rows, cols):
    if fcntl is None:
        return
    try:
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    except OSError:
        pass


def drain(fd, seconds):
    """Read whatever is available for up to `seconds`."""
    out = b""
    deadline = time.time() + seconds
    while time.time() < deadline:
        if not select.select([fd], [], [], 0.05)[0]:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        out += chunk
    return out


def main():
    args = parse_args()
    keys = args.keys.encode().decode("unicode_escape").encode("latin-1")

    pid, fd = pty.fork()
    if pid == 0:
        try:
            os.execv(args.cmd[0], args.cmd)
        finally:
            os._exit(127)

    set_winsize(fd, args.rows, args.cols)

    output = drain(fd, args.settle)

    resize_bytes = None
    resize_widest = None
    if args.resize:
        if args.pre_keys:
            os.write(fd, args.pre_keys.encode().decode("unicode_escape").encode("latin-1"))
            output += drain(fd, 0.4)
        rows, cols = (int(x) for x in args.resize.lower().split("x"))
        set_winsize(fd, rows, cols)
        # Nothing is typed here: whatever arrives is the program reacting to
        # the new window size on its own.
        redrawn = drain(fd, args.resize_quiet)
        resize_bytes = len(redrawn)
        resize_widest = widest_line(redrawn)
        output += redrawn

    os.write(fd, keys)

    timed_out = True
    status = None
    deadline = time.time() + args.timeout
    while time.time() < deadline:
        wpid, st = os.waitpid(pid, os.WNOHANG)
        if wpid:
            timed_out, status = False, st
            break
        if select.select([fd], [], [], 0.05)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                chunk = b""
            if chunk:
                output += chunk

    if timed_out:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    else:
        output += drain(fd, 0.2)

    after = flag_state(termios.tcgetattr(fd))

    print("TIMED_OUT=%d" % (1 if timed_out else 0))
    if not timed_out:
        if os.WIFSIGNALED(status):
            print("SIGNAL=%d" % os.WTERMSIG(status))
        else:
            print("EXIT_CODE=%d" % os.WEXITSTATUS(status))
    if resize_bytes is not None:
        print("BYTES_AFTER_RESIZE=%d" % resize_bytes)
        print("WIDEST_AFTER_RESIZE=%d" % resize_widest)
    print("TTY_RESTORED=%d" % (1 if all(after.values()) else 0))
    print("TTY_FLAGS_AFTER=" + ",".join("%s=%s" % (k, "on" if v else "off") for k, v in after.items()))
    print("OUTPUT_B64=" + base64.b64encode(output).decode())
    return 0


if __name__ == "__main__":
    sys.exit(main())

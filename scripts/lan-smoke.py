#!/usr/bin/env python3
"""Pairs and syncs two OTPKeep command-line processes on this machine over the real local network.

Each "device" has its own data directory under a temporary folder and a master password, so nothing touches
the system keychain or the real vault. Prompts are answered through pseudo-terminals. Exits non-zero when a
step fails. Usage: scripts/lan-smoke.py [path to the otpkeep binary]
"""

import os
import pty
import re
import select
import subprocess
import sys
import tempfile
import time

BINARY = sys.argv[1] if len(sys.argv) > 1 else "target/debug/otpkeep"
WORK = tempfile.mkdtemp(prefix="otpkeep-lan-smoke-", dir=os.environ.get("TMPDIR"))
ENV = dict(os.environ, OTPKEEP_KEYSTORE="memory")
PASSWORD = "smoke-test-password"


def plain(device, *args):
    env = dict(ENV, OTPKEEP_DATA_DIR=os.path.join(WORK, device))
    return subprocess.run([BINARY, *args], input=(PASSWORD + "\n").encode(), capture_output=True, env=env)


class Process:
    def __init__(self, name, device, *args):
        self.name, self.buffer, self.exit = name, b"", None
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.environ.update(ENV)
            os.environ["OTPKEEP_DATA_DIR"] = os.path.join(WORK, device)
            os.execv(BINARY, [BINARY, *args])

    def pump(self):
        ready, _, _ = select.select([self.fd], [], [], 0.05)
        if ready:
            try:
                self.buffer += os.read(self.fd, 4096)
            except OSError:
                pass
        if self.exit is None:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.exit = os.waitstatus_to_exitcode(status)

    def text(self):
        return self.buffer.decode(errors="replace").replace("\r", "")

    def send(self, line):
        os.write(self.fd, (line + "\n").encode())


def until(processes, condition, seconds):
    deadline = time.time() + seconds
    while time.time() < deadline:
        for process in processes:
            process.pump()
        if condition():
            return True
    return False


def session(open_args, join_args, label, digits):
    opener = Process("opener", "a", *open_args)
    until([opener], lambda: "Master password" in opener.text(), 10)
    opener.send(PASSWORD)
    if not until([opener], lambda: label in opener.text(), 20):
        sys.exit("the opener did not show a code:\n" + opener.text())
    code = re.search(label + r" ([0-9 ]+)", opener.text()).group(1).replace(" ", "")[:digits]
    joiner = Process("joiner", "b", *join_args)
    until([opener, joiner], lambda: "Master password" in joiner.text(), 10)
    joiner.send(PASSWORD)
    if not until([opener, joiner], lambda: "code shown on" in joiner.text(), 40):
        sys.exit("the joiner did not find the window:\n" + joiner.text())
    joiner.send(code)
    answered = set()

    def step():
        for process in (opener, joiner):
            if process.name not in answered and re.search(r"(Allow\?|check code [0-9 ]+\?).*\[y/N\]", process.text()):
                answered.add(process.name)
                process.send("y")
        return opener.exit is not None and joiner.exit is not None

    until([opener, joiner], step, 90)
    for process in (opener, joiner):
        print(f"--- {process.name} (exit {process.exit})\n{process.text().strip()}\n")
    if opener.exit != 0 or joiner.exit != 0:
        sys.exit("the session failed")


def main():
    for device in ("a", "b"):
        plain(device, "init", "--master-password", "--password-stdin")
    sheet = os.path.join(WORK, "a.txt")
    with open(sheet, "w") as f:
        f.write("# Smoke demo\notpauth://totp/Demo?secret=GEZDGNBVGY3TQOJQ\n")
    plain("a", "import", sheet, "--yes", "--password-stdin")
    session(["pair", "open", "--name", "Smoke A"], ["pair", "join", "--name", "Smoke B"], "Pairing code:", 8)
    sheet = os.path.join(WORK, "b.txt")
    with open(sheet, "w") as f:
        f.write("# From device B\notpauth://totp/B?secret=JBSWY3DPEHPK3PXP\n")
    plain("b", "import", sheet, "--yes", "--password-stdin")
    session(["sync", "wait"], ["sync", "join"], "Sync code:", 6)
    listed = plain("a", "list", "--password-stdin").stdout.decode()
    print("--- entries on A after the sync\n" + listed)
    if "From device B" not in listed:
        sys.exit("device A did not receive device B's entry")
    print("lan smoke test passed")


if __name__ == "__main__":
    main()

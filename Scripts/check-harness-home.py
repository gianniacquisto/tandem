#!/usr/bin/env python3
"""Check that the bundled harness can be confined to a Tandem-owned home.

The app spawns the pinned opencode binary as a child process. Every global path
that binary resolves is rooted at `os.homedir()` unless `XDG_DATA_HOME`,
`XDG_CACHE_HOME`, `XDG_CONFIG_HOME` or `XDG_STATE_HOME` are set, read once at
module load, so the environment inherited at spawn is the whole isolation
mechanism. This check is the proof: it starts the binary with those variables
pointed at a scratch home, drives the v2 API enough to write a session, and
asserts that the user's own opencode directories never change.

The check seals HOME. The three directories the binary falls back to are created
empty and made read-only inside that sealed HOME, so the run is safe on a
machine where another opencode process is live, and a write to a default path
fails loudly instead of landing in somebody's real store.

`--control` is the red half: the same pass without the isolation environment,
against a writable sealed HOME. The check must trip there. A check that cannot
fail proves nothing.

Usage:
    Scripts/check-harness-home.py [--harness PATH] [--control] [--keep]

Exit 0 when every assertion holds (or, under `--control`, when the isolation
assertion trips), 1 otherwise. The pinned version and digest are below; a bump
changes them and re-runs this file.
"""

import argparse
import base64
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

# The pin. `docs/adr/0038-the-harness-home.md` records why this version and what
# a bump costs; CI verifies the same digest before it ships the binary.
PINNED_VERSION = "2.2.5"
PINNED_SHA256 = {
    "darwin-arm64": "abe4d88594bffb12b364375b8a6732d2d1f7a977195ed1a35c5c76f8e89c9e0e",
}

PASSWORD = "tandem-harness-check"
START_TIMEOUT = 30.0
EXIT_TIMEOUT = 30.0


class Check(Exception):
    pass


def platform_key():
    machine = os.uname().machine
    machine = {"arm64": "arm64", "x86_64": "x64"}.get(machine, machine)
    return "%s-%s" % (sys.platform, machine)


def find_harness(explicit):
    if explicit:
        return explicit
    found = shutil.which("opencode")
    if not found:
        raise Check("no opencode on PATH; pass --harness <path>")
    return os.path.realpath(found)


def verify_pin(harness):
    version = subprocess.run(
        [harness, "--version"], capture_output=True, text=True, timeout=60
    ).stdout.strip()
    expected = "opencode v%s" % PINNED_VERSION
    if version != expected:
        raise Check("version %r is not the pin %r" % (version, expected))
    digest = hashlib.sha256()
    with open(harness, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    actual = digest.hexdigest()
    key = platform_key()
    if key not in PINNED_SHA256:
        raise Check("no digest pinned for %s; the pin is per platform" % key)
    if actual != PINNED_SHA256[key]:
        raise Check("sha256 %s is not the pin %s for %s" % (actual, PINNED_SHA256[key], key))
    return version


def ledger(root):
    """Every path under root with the size and mtime that would show a write."""
    entries = {}
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        filenames.sort()
        rel = os.path.relpath(dirpath, root)
        info = os.lstat(dirpath)
        entries[rel + "/"] = (info.st_mtime_ns,)
        for name in filenames:
            path = os.path.join(dirpath, name)
            info = os.lstat(path)
            entries[os.path.relpath(path, root)] = (info.st_size, info.st_mtime_ns)
    return entries


def changed(before, after):
    return sorted(
        set(path for path in set(before) | set(after) if before.get(path) != after.get(path))
    )


def writable(root, mode):
    for dirpath, dirnames, _ in os.walk(root):
        os.chmod(dirpath, mode)
        for name in dirnames:
            os.chmod(os.path.join(dirpath, name), mode)


def read_line(handle, process, deadline):
    # stdout goes to a file so the run leaves evidence behind; a regular file
    # reads as readable at EOF, so poll it rather than select on it.
    while time.time() < deadline:
        line = handle.readline()
        if line:
            return line.strip()
        if process.poll() is not None:
            return None
        time.sleep(0.1)
    raise Check("no handshake on stdout within %.0fs" % START_TIMEOUT)


def request(url, payload=None, timeout=10.0):
    headers = {
        "authorization": "Basic "
        + base64.b64encode(("opencode:" + PASSWORD).encode()).decode()
    }
    data = None
    if payload is not None:
        data = json.dumps(payload).encode()
        headers["content-type"] = "application/json"
    req = urllib.request.Request(
        url, data=data, headers=headers, method="POST" if data else "GET"
    )
    with urllib.request.urlopen(req, timeout=timeout) as response:
        return json.loads(response.read().decode())


def wait_until(url, deadline):
    last = None
    while time.time() < deadline:
        try:
            return request(url + "/api/session")
        except (urllib.error.URLError, ConnectionError, OSError) as error:
            last = error
            time.sleep(0.1)
    raise Check("the API never answered: %s" % last)


def pass_once(harness, isolated, root, index):
    sealed_home = os.path.join(root, "home-%d" % index)
    defaults = [
        os.path.join(sealed_home, ".local", "share", "opencode"),
        os.path.join(sealed_home, ".config", "opencode"),
        os.path.join(sealed_home, ".cache", "opencode"),
    ]
    harness_home = os.path.join(root, "harness-%d" % index)
    for path in defaults:
        os.makedirs(path)
    dirs = {
        "data": os.path.join(harness_home, "data"),
        "cache": os.path.join(harness_home, "cache"),
        "config": os.path.join(harness_home, "config"),
        "state": os.path.join(harness_home, "state"),
        "tmp": os.path.join(harness_home, "tmp"),
        "log": os.path.join(harness_home, "log"),
        "db": os.path.join(harness_home, "db"),
    }
    work = os.path.join(harness_home, "work")
    for path in list(dirs.values()) + [work]:
        os.makedirs(path)
    database = os.path.join(dirs["db"], "opencode.db")

    env = {key: value for key, value in os.environ.items() if not key.startswith("XDG_")}
    env["HOME"] = sealed_home
    env["OPENCODE_SERVER_PASSWORD"] = PASSWORD
    if isolated:
        env.update(
            {
                "XDG_DATA_HOME": dirs["data"],
                "XDG_CACHE_HOME": dirs["cache"],
                "XDG_CONFIG_HOME": dirs["config"],
                "XDG_STATE_HOME": dirs["state"],
                "TMPDIR": dirs["tmp"],
                "OPENCODE_DB": database,
                "OPENCODE_CONFIG_DIR": dirs["config"],
            }
        )
        writable(sealed_home, 0o500)
    # The control run has no isolation to write through, so the defaults stay
    # writable and the ledger assertion is expected to trip.

    before_home = ledger(sealed_home)
    stdout_path = os.path.join(root, "stdout-%d.log" % index)
    stderr_path = os.path.join(root, "stderr-%d.log" % index)
    with open(stdout_path, "w") as stdout, open(stderr_path, "w") as stderr:
        process = subprocess.Popen(
            [harness, "serve", "--stdio", "--port", "0"],
            stdin=subprocess.PIPE,
            stdout=stdout,
            stderr=stderr,
            env=env,
            cwd=work,
            text=True,
        )
        try:
            with open(stdout_path) as reader:
                line = read_line(reader, process, time.time() + START_TIMEOUT)
            if not line:
                raise Check("the harness exited before it printed a url")
            handshake = json.loads(line)
            url = handshake["url"]
            deadline = time.time() + START_TIMEOUT
            wait_until(url, deadline)
            created = request(url + "/api/session", {"directory": work})["data"]
            listed = request(url + "/api/session")["data"]
            config = request(url + "/api/config")
            process.stdin.close()
            try:
                code = process.wait(timeout=EXIT_TIMEOUT)
            except subprocess.TimeoutExpired:
                process.kill()
                raise Check("the harness outlived its closed stdin by %.0fs" % EXIT_TIMEOUT)
            if code != 0:
                raise Check("the harness exited %d on a closed stdin" % code)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    after_home = ledger(sealed_home)
    if isolated:
        writable(sealed_home, 0o700)
    return {
        "url": url,
        "defaults_diff": changed(before_home, after_home),
        "session": created,
        "listed": listed,
        "config": config,
        "database": database,
        "harness_home": harness_home,
        "dirs": dirs,
        "stdout": stdout_path,
        "stderr": stderr_path,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--harness", help="the opencode binary (default: PATH)")
    parser.add_argument(
        "--control",
        action="store_true",
        help="run without the isolation environment and require the check to trip",
    )
    parser.add_argument("--keep", action="store_true", help="keep the scratch tree")
    options = parser.parse_args(argv)

    results = []

    def ok(name, detail=""):
        results.append(True)
        print("ok   %s%s" % (name, ": " + detail if detail else ""))

    def fail(name, detail):
        results.append(False)
        print("FAIL %s: %s" % (name, detail))

    root = tempfile.mkdtemp(prefix="tandem-harness-check-")
    try:
        harness = find_harness(options.harness)
        version = verify_pin(harness)
        print("     harness %s, %s, sha256 pinned" % (harness, version))

        if options.control:
            run = pass_once(harness, False, root, 0)
            if run["defaults_diff"]:
                ok(
                    "control trips the check",
                    "%d paths under the sealed HOME changed without isolation"
                    % len(run["defaults_diff"]),
                )
            else:
                fail(
                    "control trips the check",
                    "a run without isolation left every default path byte-identical, "
                    "so this check cannot detect a leak",
                )
        else:
            run = pass_once(harness, True, root, 0)
            if run["defaults_diff"]:
                fail(
                    "nothing under HOME changed",
                    "isolation leaked: %s" % ", ".join(run["defaults_diff"][:5]),
                )
            else:
                ok("nothing under HOME changed", "mtime, size and the file set identical")

            host = run["url"].split("//", 1)[-1].split(":", 1)[0].split("/", 1)[0]
            if host in ("127.0.0.1", "localhost", "[::1]"):
                ok("the listener is loopback", run["url"])
            else:
                fail("the listener is loopback", run["url"])

            created = run["session"].get("id", "")
            if created.startswith("ses_") and len(run["listed"]) == 1:
                ok("a session was created and listed", created)
            else:
                fail(
                    "a session was created and listed",
                    "created %r, listed %d" % (created, len(run["listed"])),
                )

            database = run["database"]
            if os.path.isfile(database) and os.path.getsize(database) > 0:
                ok(
                    "the harness database is in Tandem's home",
                    "%s, %d bytes" % (database, os.path.getsize(database)),
                )
            else:
                fail("the harness database is in Tandem's home", database)

            log = os.path.join(run["dirs"]["data"], "opencode", "log", "opencode.log")
            if os.path.isfile(log):
                ok("XDG_DATA_HOME took effect", log)
            else:
                fail("XDG_DATA_HOME took effect", log)

            paths = [entry.get("path", "") for entry in run["config"]]
            if paths and all(path.startswith(run["dirs"]["config"]) for path in paths):
                ok("config.directory is in the harness home", ", ".join(paths))
            else:
                fail("config.directory is in the harness home", repr(paths))

            ok(
                "stdin EOF ended the run",
                "exit 0 within %.0fs of the closed pipe" % EXIT_TIMEOUT,
            )

        failed = results.count(False)
        print("%d passed, %d failed" % (results.count(True), failed))
        return 1 if failed else 0
    except Check as error:
        print("FAIL %s" % error)
        return 1
    finally:
        if options.keep:
            print("kept %s" % root)
        else:
            try:
                writable(root, 0o700)
            except OSError:
                pass
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())

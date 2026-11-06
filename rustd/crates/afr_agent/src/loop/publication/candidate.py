"""Inside the sandbox only: snapshot regular-file changes against a native tree.

The host never executes Git against tenant state. Native publication independently
reconstructs this tree against GitHub's expected head before issuing a ref write.
"""
import base64
import hashlib
import json
import os
import stat
import subprocess
import sys
import tempfile

MAX_FILE = 1024 * 1024
MAX_PATHS = 32
MAX_ENTRIES = 200000
env = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL="/dev/null",
           GIT_NO_REPLACE_OBJECTS="1", GIT_OPTIONAL_LOCKS="0")


def git(*args, data=None, limit=32 * 1024 * 1024):
    # Inputs are at most the bounded change set. Regular tree reads have no
    # stdin and are bounded before allocating their complete output.
    with subprocess.Popen(["git", "--no-optional-locks", "-c", "core.fsmonitor=false", *args],
                          env=env, stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as child:
        if data is not None:
            child.stdin.write(data)
            child.stdin.close()
        output = child.stdout.read(limit + 1)
        if len(output) > limit:
            child.kill()
            raise ValueError("Git metadata exceeds the snapshot bound")
        if child.wait() != 0:
            raise ValueError("Git could not read the native candidate base")
        return output


def path_parts(path):
    parts = path.split("/")
    if (len(path.encode()) > 1024 or len(parts) > 32 or "\\" in path
            or any(ord(c) < 32 or ord(c) == 127 for c in path)
            or any(p in ("", ".", "..") or p.lower() == ".git" for p in parts)):
        raise ValueError("Unsupported repository path")
    return parts


def contents(path):
    parts = path_parts(path)
    directory = os.open(".", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in parts[:-1]:
            next_directory = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = next_directory
        info = os.stat(parts[-1], dir_fd=directory, follow_symlinks=False)
        if stat.S_ISLNK(info.st_mode):
            body = os.readlink(parts[-1], dir_fd=directory).encode()
            return "120000", hashlib.sha1(b"blob " + str(len(body)).encode() + b"\0" + body).hexdigest(), None
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("Only regular-file changes are supported")
        descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
        with os.fdopen(descriptor, "rb") as source:
            before = os.fstat(source.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_size > 24 * 1024 ** 3:
                raise ValueError("Unsupported candidate file")
            digest = hashlib.sha1(b"blob " + str(before.st_size).encode() + b"\0")
            body = bytearray()
            size = 0
            while chunk := source.read(1024 * 1024):
                size += len(chunk)
                if size > before.st_size:
                    raise ValueError("File grew during capture")
                digest.update(chunk)
                if before.st_size <= MAX_FILE:
                    body.extend(chunk)
            after = os.fstat(source.fileno())
            identity = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns, value.st_ctime_ns)
            if identity(before) != identity(after) or size != before.st_size:
                raise ValueError("File changed during capture")
            return "100755" if before.st_mode & 0o111 else "100644", digest.hexdigest(), bytes(body) if size <= MAX_FILE else None
    except FileNotFoundError:
        return None
    finally:
        os.close(directory)


def unchanged_gitlink(path, expected, staged):
    # A populated submodule has its own untrusted index/config. Only an empty,
    # uninitialized checkout can be proved unchanged without consulting either.
    if staged.get(path) != [("160000", expected, "0")]:
        raise ValueError("Staged submodule changes are unsupported")
    directory = os.open(".", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in path_parts(path):
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = child
        with os.scandir(directory) as entries:
            if next(entries, None) is not None:
                raise ValueError("Populated submodule publication is unsupported")
    finally:
        os.close(directory)


def snapshot(base):
    if len(base) != 40 or any(c not in "0123456789abcdef" for c in base):
        raise ValueError("Invalid native tree")
    previous, gitlinks = {}, {}
    for row in git("ls-tree", "-rz", "--full-tree", base).split(b"\0"):
        if not row:
            continue
        metadata, raw_path = row.split(b"\t", 1)
        mode, kind, oid = metadata.decode("ascii").split(" ")
        path = raw_path.decode("utf-8", "strict")
        path_parts(path)
        if (mode, kind) == ("160000", "commit"):
            gitlinks[path] = oid
        elif kind != "blob":
            raise ValueError("Unsupported native tree entry")
        previous[path] = (mode, oid)
    extra = git("ls-files", "--others", "--exclude-standard", "-z").split(b"\0")
    paths = set(previous) | {path.decode("utf-8", "strict") for path in extra if path}
    # Include newly staged files as well as untracked ones; never trust the
    # tenant index to describe native base contents or pre-existing modes.
    staged = {}
    for row in git("ls-files", "--stage", "-z").split(b"\0"):
        if row:
            metadata, raw_path = row.split(b"\t", 1)
            path = raw_path.decode("utf-8", "strict")
            staged.setdefault(path, []).append(tuple(metadata.decode("ascii").split(" ")))
    paths.update(staged)
    if len(paths) > MAX_ENTRIES:
        raise ValueError("Candidate entry limit exceeded")
    updates, changes, total = [], [], 0
    for path in sorted(paths):
        parts = path_parts(path)
        if any("/".join(parts[:depth]) in gitlinks for depth in range(1, len(parts))):
            raise ValueError("Submodule descendant changes are unsupported")
        if path in gitlinks:
            unchanged_gitlink(path, gitlinks[path], staged)
            continue
        current = contents(path)
        old = previous.get(path)
        if current is not None and current[:2] == old:
            continue
        if current is None and old is None:
            continue
        if parts[0].lower() == ".gitmodules" or [p.lower() for p in parts[:2]] == [".github", "workflows"]:
            raise ValueError("Workflow and submodule changes are unsupported")
        if current is None:
            if old[0] != "100644":
                raise ValueError("Only regular-file deletions are supported")
            updates.append(b"0 " + b"0" * 40 + b"\t" + path.encode() + b"\0")
            changes.append({"path": path, "contents": None})
        else:
            mode, oid, body = current
            if mode != "100644" or mode != (old[0] if old else "100644") or body is None:
                raise ValueError("Unsupported changed-file mode or size")
            total += len(body)
            if total > MAX_FILE:
                raise ValueError("Publication content limit exceeded")
            saved = git("hash-object", "--no-filters", "-w", "--stdin", data=body, limit=64).decode().strip()
            if saved != oid:
                raise ValueError("Changed-file digest differs")
            updates.append(f"{mode} {oid}\t{path}\0".encode())
            changes.append({"path": path, "contents": base64.b64encode(body).decode("ascii")})
        if len(changes) > MAX_PATHS:
            raise ValueError("Publication path limit exceeded")
    descriptor, index = tempfile.mkstemp(prefix="agentsfleet-candidate-")
    os.close(descriptor)
    os.unlink(index)
    env["GIT_INDEX_FILE"] = index
    try:
        git("read-tree", base, limit=128)
        if updates:
            git("update-index", "-z", "--index-info", data=b"".join(updates), limit=128)
        tree = git("write-tree", limit=64).decode("ascii").strip()
    finally:
        for path in (index, index + ".lock"):
            try:
                os.unlink(path)
            except FileNotFoundError:
                pass
    return {"tested_tree": tree, "changes": changes}


try:
    print(json.dumps(snapshot(sys.argv[1]), separators=(",", ":")))
except (OSError, ValueError, UnicodeError):
    print("Candidate capture refused an unsupported, changing or incomplete workspace.", file=sys.stderr)
    sys.exit(1)

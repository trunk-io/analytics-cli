#!/usr/bin/env python3
"""Drop the objects in an .xcresult bundle that nothing in it references.

Xcode 26 writes ~140MB of dyld shared-cache symbolication data plus a payload per
loaded image into every result bundle, none of it reachable from the invocation
record. That is fine for a bundle you throw away and not fine for one checked into
git — it is ~95MB of the ~96MB a scenario produces.

This walks the object graph from the root, following every reference id, and
deletes the `Data/` entries nothing reached. `regenerate.sh` re-dumps the failure
summaries afterwards and fails if pruning changed them.

    ./prune-bundle.py <Scenario>.xcresult
"""

import json
import os
import re
import subprocess
import sys

# Object ids are content-addressed and are also the `Data/` file names, e.g.
# `data.0~aBc.../refs.0~aBc...`.
OBJECT_ID = re.compile(r"^0~[A-Za-z0-9_\-+/=]+$")


def get_object(path, object_id=None):
    cmd = [
        "xcrun",
        "xcresulttool",
        "get",
        "object",
        "--path",
        path,
        "--format",
        "json",
        "--legacy",
    ]
    if object_id:
        cmd += ["--id", object_id]
    result = subprocess.run(cmd, capture_output=True)
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def object_ids_in(node):
    """Every value anywhere in an object that is shaped like an object id."""
    if isinstance(node, dict):
        for key, child in node.items():
            if key == "_value" and isinstance(child, str) and OBJECT_ID.match(child):
                yield child
            else:
                yield from object_ids_in(child)
    elif isinstance(node, list):
        for child in node:
            yield from object_ids_in(child)


def root_id(path):
    """The invocation record's own id, which lives in Info.plist rather than in
    any object, so nothing in the graph points at it."""
    # Info.plist holds a date, which `plutil -convert json` refuses to write, so
    # the one field is extracted rather than the whole file converted.
    info = subprocess.run(
        [
            "plutil",
            "-extract",
            "rootId.hash",
            "raw",
            "-o",
            "-",
            os.path.join(path, "Info.plist"),
        ],
        check=True,
        capture_output=True,
    )
    return info.stdout.decode().strip()


def reachable_ids(path):
    root = get_object(path)
    if root is None:
        raise SystemExit(f"{path}: could not read the invocation record")

    seen = {root_id(path)}
    queue = list(object_ids_in(root))
    while queue:
        object_id = queue.pop()
        if object_id in seen:
            continue
        seen.add(object_id)
        child = get_object(path, object_id)
        if child is not None:
            queue.extend(object_ids_in(child))
    return seen


def main():
    path = sys.argv[1]
    data_dir = os.path.join(path, "Data")

    keep = reachable_ids(path)
    before = sum(
        os.path.getsize(os.path.join(data_dir, name)) for name in os.listdir(data_dir)
    )

    removed = 0
    for name in os.listdir(data_dir):
        prefix, _, object_id = name.partition(".")
        if prefix not in ("data", "refs"):
            continue
        if object_id in keep:
            continue
        file_path = os.path.join(data_dir, name)
        removed += os.path.getsize(file_path)
        os.remove(file_path)

    # Built lazily by xcresulttool and rebuilt on demand, so it is noise in a
    # checked-in fixture.
    index = os.path.join(path, "database.sqlite3")
    if os.path.exists(index):
        os.remove(index)

    print(
        f"  pruned {removed / 1e6:.0f}MB of unreferenced objects "
        f"({before / 1e6:.0f}MB -> {(before - removed) / 1e6:.0f}MB), "
        f"kept {len(keep)}"
    )


if __name__ == "__main__":
    main()

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:---check}"
if [[ "$MODE" != "--check" && "$MODE" != "--write" ]]; then
    echo "Usage: scripts/skills/update-changelog.sh [--check|--write]" >&2
    exit 1
fi

python3 - "$MODE" <<'PY'
import re
import subprocess
import sys
from pathlib import Path


def sh(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def section_exists(lines: list[str], version: str) -> bool:
    target = f"## [{version}]"
    return any(line.startswith(target) for line in lines)


mode = sys.argv[1]
root = Path(".")
changelog_path = root / "CHANGELOG.md"
version_path = root / "version.txt"

if not changelog_path.exists():
    print("CHANGELOG.md not found", file=sys.stderr)
    sys.exit(1)
if not version_path.exists():
    print("version.txt not found", file=sys.stderr)
    sys.exit(1)

version = version_path.read_text(encoding="utf-8").strip()
expected_unreleased = f"## [{version} Unreleased]"

lines = changelog_path.read_text(encoding="utf-8").splitlines()
heading_re = re.compile(r"^## \[([^\]]+)\]")

first_heading_idx = None
first_heading_val = None
for i, line in enumerate(lines):
    m = heading_re.match(line)
    if m:
        first_heading_idx = i
        first_heading_val = m.group(1)
        break

if first_heading_idx is None:
    first_heading_idx = 0
    first_heading_val = ""

tags_raw = sh("git", "for-each-ref", "refs/tags", "--sort=version:refname", "--format=%(refname:short)\t%(objectname)")
tag_rows = []
for row in tags_raw.splitlines():
    if not row:
        continue
    tag, obj = row.split("\t", 1)
    if tag.startswith("v"):
        tag_rows.append((tag, obj))

groups: dict[str, list[str]] = {}
for tag, obj in tag_rows:
    groups.setdefault(obj, []).append(tag)

required: list[tuple[str, list[str]]] = []
seen = set()
for tag, obj in tag_rows:
    if obj in seen:
        continue
    seen.add(obj)
    aliases = groups[obj]
    non_alpha = [t for t in aliases if "-alpha" not in t]
    if non_alpha:
        required.append((non_alpha[-1], aliases))
    else:
        for t in aliases:
            required.append((t, [t]))

existing_versions = set()
for line in lines:
    m = heading_re.match(line)
    if m:
        existing_versions.add(m.group(1))

issues = []
if f"{version} Unreleased" != (first_heading_val or ""):
    issues.append(f"Unreleased heading mismatch: expected '{version} Unreleased', got '{first_heading_val or '<none>'}'")

missing: list[tuple[str, list[str]]] = []
for tag, aliases in required:
    if not any((a[1:] in existing_versions) for a in aliases):
        missing.append((tag, aliases))

print(f"version.txt: {version}")
latest_tag = sh("git", "describe", "--tags", "--abbrev=0")
print(f"latest tag: {latest_tag}")
print(f"expected unreleased heading: {expected_unreleased}")
if issues:
    for issue in issues:
        print(f"[ISSUE] {issue}")
else:
    print("[OK] Unreleased heading is consistent")

if missing:
    print("[ISSUE] Missing released sections:")
    for tag, aliases in missing:
        alias_text = ", ".join(aliases)
        print(f"  - {tag} (aliases: {alias_text})")
else:
    print("[OK] Tag coverage is complete")

if mode == "--check":
    if issues or missing:
        sys.exit(2)
    sys.exit(0)

# --write mode
changed = False

if f"{version} Unreleased" != (first_heading_val or ""):
    if first_heading_val and "Unreleased" in first_heading_val:
        lines[first_heading_idx] = expected_unreleased
    else:
        lines.insert(0, expected_unreleased)
        lines.insert(1, "")
    changed = True

if missing:
    # Find insertion point: first released section (not Unreleased)
    release_idx = None
    for i, line in enumerate(lines):
        m = heading_re.match(line)
        if m and "Unreleased" not in m.group(1):
            release_idx = i
            break
    if release_idx is None:
        release_idx = len(lines)

    # Insert from older -> newer at same index to preserve newest-first final order
    for tag, _aliases in reversed(missing):
        date = sh("git", "show", "-s", "--format=%ad", "--date=short", tag)
        idx = next((i for i, (t, _o) in enumerate(tag_rows) if t == tag), None)
        prev = tag_rows[idx - 1][0] if idx is not None and idx > 0 else "<prev-tag>"
        block = [
            f"## [{tag[1:]}] - {date}",
            "",
            "### Changed",
            f"- **TBD**: Fill from `git log {prev}..{tag} --oneline`",
            "",
        ]
        lines[release_idx:release_idx] = block
        changed = True

if changed:
    changelog_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[WRITE] CHANGELOG.md updated")
else:
    print("[WRITE] No changes needed")
PY


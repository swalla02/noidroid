#!/usr/bin/env python3
"""The primary checkout is read-only. Work happens in a per-issue worktree, so two
sessions on two issues can never edit the same file.

The edit is allowed when any of these is true:
  - the file is outside this repository
  - the file is in a linked worktree (git-dir != git-common-dir)
  - NOIDROID_MAIN_EDIT=1 is set (a release commit, or editing this guard)
"""
import json
import os
import subprocess
import sys


def git(cwd, *args):
    out = subprocess.run(
        ("git", "-C", cwd, *args), capture_output=True, text=True
    )
    return out.stdout.strip() if out.returncode == 0 else None


def main():
    if os.environ.get("NOIDROID_MAIN_EDIT") == "1":
        return
    try:
        payload = json.load(sys.stdin)
    except ValueError:
        return
    args = payload.get("tool_input") or {}
    path = args.get("file_path") or args.get("notebook_path")
    if not path:
        return

    directory = os.path.dirname(os.path.abspath(path))
    while not os.path.isdir(directory) and directory != os.sep:
        directory = os.path.dirname(directory)

    git_dir = git(directory, "rev-parse", "--absolute-git-dir")
    if git_dir is None:
        return  # not a repository — none of our business
    common = os.path.realpath(
        os.path.join(directory, git(directory, "rev-parse", "--git-common-dir"))
    )
    if os.path.realpath(git_dir) != common:
        return  # a linked worktree, which is exactly where work belongs

    root = os.path.dirname(common)
    json.dump(
        {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": (
                    f"{root} is the primary checkout and is read-only. "
                    "Every issue gets its own worktree and its own session.\n\n"
                    f"  {root}/.claude/scripts/issue-worktree.sh start <issue>\n\n"
                    "Then work from the path it prints: EnterWorktree, or a new "
                    "`claude` session started in that directory. If this change has "
                    "no issue yet, open one first (gh issue create).\n\n"
                    "Deliberate exception — a release commit, or editing this guard: "
                    "export NOIDROID_MAIN_EDIT=1."
                ),
            }
        },
        sys.stdout,
    )


if __name__ == "__main__":
    main()

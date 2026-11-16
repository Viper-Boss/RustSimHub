# SPDX-License-Identifier: AGPL-3.0-only
"""Internal Windows job bootstrap; only reviewed check IDs, never arbitrary argv."""
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from checks import CHECKS
from windows_job import join


def main():
    if len(sys.argv) != 3:
        return 87
    checks = {c.name: c for c in CHECKS}
    if sys.argv[2] not in checks:
        return 87
    try:
        join(sys.argv[1])
    except (OSError, ValueError):
        print('OFFLINE_B_JOB_UNAVAILABLE', flush=True)
        return 87
    print('OFFLINE_B_JOB_READY', flush=True)
    command = checks[sys.argv[2]].argv
    executable = shutil.which(command[0])
    if executable is None:
        return 88
    # Windows CRT exec starts a replacement process and exits this bootstrap.
    # Keep the job member parent alive until the real check has exited, so an
    # orderly check cannot be mistaken for an orphan and terminated early.
    return subprocess.run([executable, *command[1:]], stdin=subprocess.DEVNULL,
                          env=os.environ.copy(), check=False).returncode


if __name__ == '__main__':
    raise SystemExit(main())

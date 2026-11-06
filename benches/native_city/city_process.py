"""Run a Linux City trial with ownership of the launcher and its children."""

import os
import signal
import subprocess
from contextlib import contextmanager


@contextmanager
def trial_process(command, **options):
    # Both perf and GNU time launch the adapter as a child. Killing only the
    # launcher on a failed handshake or timeout would leave the adapter running.
    with subprocess.Popen(command, start_new_session=True, **options) as process:
        try:
            yield process
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                # The trial's process group already exited.
                pass
            raise

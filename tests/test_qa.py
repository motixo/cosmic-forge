# SPDX-License-Identifier: GPL-3.0-only
"""Check that failed native QA startup does not leak its X server."""
import gc
import warnings
import importlib.util
import os
from pathlib import Path
import shutil
import unittest
import tempfile
ROOT = Path(__file__).resolve().parents[1]
READY = all((shutil.which(name) for name in ('Xvfb', 'dbus-run-session'))) and all((importlib.util.find_spec(name) for name in ('PIL', 'Xlib')))

@unittest.skipUnless(READY, 'Graphical QA tools are optional')
class QaCleanupTests(unittest.TestCase):

    def test_failed_startup_reaps_its_child_processes(self):
        spec = importlib.util.spec_from_file_location('qa_native', ROOT / 'tests/qa_native.py')
        qa = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(qa)
        children = Path(f'/proc/self/task/{os.getpid()}/children')
        before = set(children.read_text().split())
        with warnings.catch_warnings(record=True) as emitted:
            warnings.simplefilter('always', ResourceWarning)
            with tempfile.TemporaryDirectory() as temporary:
                with self.assertRaises(RuntimeError):
                    qa.Session(ROOT / 'build/does-not-exist', Path(temporary))
            gc.collect()
        self.assertFalse([warning for warning in emitted if warning.category is ResourceWarning])
        self.assertEqual(set(children.read_text().split()), before)
if __name__ == '__main__':
    unittest.main()

# SPDX-License-Identifier: GPL-3.0-only
import importlib.util
import contextlib
import io
import json
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('engine_install', ROOT / 'engine/install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
PIN = json.loads((ROOT / 'engine/release.json').read_text())['sources']['1.10.0']
PACKAGES = {name: name + ' 1:1.10.0-1' for name in installer.PACKAGES}


class EngineInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='forge renderer ')
        self.addCleanup(self.temporary.cleanup)
        self.prefix = Path(self.temporary.name)
        self.binary = self.prefix / 'new-engine'
        self.binary.write_bytes(b'\x7fELFcosmic-forge-engine-v1')
        self.launcher = self.prefix / 'bin/cosmic-comp'
        self.record = self.prefix / 'state/cosmic-forge/engine-installation.json'
        self.system = self.prefix / 'system-cosmic-comp'
        self.system.write_bytes(b'\x7fELForiginal-system-compositor')
        self.system.chmod(0o755)
        self.enterContext(patch.object(installer, 'SYSTEM_COMPOSITOR', self.system))

    def install(self):
        with patch.object(installer.subprocess, 'check_output', return_value=PIN):
            installer.install(self.binary, PACKAGES, '1.10.0', self.prefix)

    def legacy(self):
        self.launcher.parent.mkdir(parents=True, exist_ok=True)
        original = b'#!/bin/sh\n# Managed by cosmic-crystal\nexec /old/cosmic-comp "$@"\n'
        self.launcher.write_bytes(original)
        self.launcher.chmod(0o755)
        path = self.prefix / 'state/cosmic-crystal/installation.json'
        path.parent.mkdir(parents=True)
        path.write_text(json.dumps({'shims': {'cosmic-comp': original.decode()}}))
        return original, path, path.read_bytes()

    def test_restore_selects_original_system_instead_of_previous_crystal(self):
        original, legacy_record, legacy_bytes = self.legacy()
        settings = self.prefix / 'bin/cosmic-settings'
        settings.write_text('custom Settings launcher')
        self.install()
        self.install()
        text = self.launcher.read_text()
        self.assertIn('COSMIC_FORGE_DISABLE', text)
        self.assertIn('pacman -Q cosmic-comp', text)
        self.assertIn('cosmic-comp 1:1.10.0-1', text)
        installer.restore(self.prefix)
        self.assertFalse(self.launcher.exists())
        recovery = self.prefix / 'state/cosmic-forge/engine-previous-compositor'
        self.assertEqual(recovery.read_bytes(), original)
        self.assertEqual(settings.read_text(), 'custom Settings launcher')
        self.assertEqual(legacy_record.read_bytes(), legacy_bytes)
        self.assertFalse(self.record.exists())

    def test_restore_handles_legacy_launcher_without_a_forge_record(self):
        _, record, original = self.legacy()
        installer.restore(self.prefix)
        self.assertFalse(self.launcher.exists())
        self.assertEqual(record.read_bytes(), original)

    def test_restore_already_original_leaves_system_binary_untouched(self):
        before = self.system.read_bytes(), self.system.stat().st_mtime_ns
        with patch.object(installer, 'atomic', side_effect=AssertionError('must not reinstall')):
            installer.restore(self.prefix)
        self.assertEqual((self.system.read_bytes(), self.system.stat().st_mtime_ns), before)
        self.assertFalse(self.launcher.exists())

    def test_unavailable_system_compositor_preserves_active_installation(self):
        self.install()
        before = {p: p.read_bytes() for p in [self.launcher, self.record]}
        self.system.unlink()
        with self.assertRaises(RuntimeError):
            installer.restore(self.prefix)
        for data, mode in [(b'\x7fELFstock', 0o600), (b'#!/bin/sh\n', 0o755)]:
            self.system.write_bytes(data)
            self.system.chmod(mode)
            with self.assertRaises(RuntimeError):
                installer.restore(self.prefix)
        self.assertEqual({p: p.read_bytes() for p in before}, before)

    def test_restore_failure_rolls_back_the_launcher_and_preserves_recovery(self):
        original, _, _ = self.legacy()
        self.install()
        self.launcher.chmod(0o700)
        before = {p: p.read_bytes() for p in [self.launcher, self.record]}
        unlink = Path.unlink
        def fail_record(path, *args, **kwargs):
            if path == self.record:
                raise OSError('simulated disk failure')
            return unlink(path, *args, **kwargs)
        with patch.object(Path, 'unlink', fail_record):
            with self.assertRaises(OSError):
                installer.restore(self.prefix)
        self.assertEqual({p: p.read_bytes() for p in before}, before)
        self.assertEqual(self.launcher.stat().st_mode & 0o777, 0o700)
        self.assertEqual((self.prefix / 'state/cosmic-forge/engine-previous-compositor').read_bytes(), original)

    def test_fresh_install_restore_removes_only_the_managed_launcher(self):
        self.install()
        self.assertTrue(self.launcher.exists())
        installer.restore(self.prefix)
        self.assertFalse(self.launcher.exists())

    def test_same_renderer_skips_build_and_preserves_installed_files(self):
        self.install()
        record = json.loads(self.record.read_text())
        # Initial Forge installs have no revision field and represent revision 1.
        record.pop('renderer_revision')
        self.record.write_text(json.dumps(record))
        binary = Path(record['destination']) / 'cosmic-comp'
        before = {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in [self.launcher, self.record, binary]}
        output = io.StringIO()
        with patch.object(sys, 'argv', ['install.py', '--prefix', str(self.prefix)]), \
             patch.object(installer, 'installed_packages', return_value=(PACKAGES, '1.10.0')), \
             patch.object(installer.subprocess, 'run', side_effect=AssertionError('must not build')), \
             patch.object(installer, 'atomic', side_effect=AssertionError('must not reinstall')), \
             contextlib.redirect_stdout(output):
            installer.main()
        self.assertIn('FORGE_PROGRESS:already_installed', output.getvalue())
        self.assertEqual({p: (p.read_bytes(), p.stat().st_mtime_ns) for p in before}, before)

    def test_different_revision_or_packages_do_not_skip_installation(self):
        self.install()
        record = json.loads(self.record.read_text())
        self.assertTrue(installer.already_installed(record, PACKAGES, '1.10.0', self.prefix))
        record['renderer_revision'] = 0
        self.assertFalse(installer.already_installed(record, PACKAGES, '1.10.0', self.prefix))
        record['renderer_revision'] = 1
        updated = dict(PACKAGES, **{'cosmic-comp': 'cosmic-comp 1:1.10.0-2'})
        self.assertFalse(installer.already_installed(record, updated, '1.10.0', self.prefix))
        self.assertFalse(installer.already_installed(record, PACKAGES, '1.11.0', self.prefix))

    def test_missing_or_changed_renderer_is_not_reported_as_installed(self):
        self.install()
        record = json.loads(self.record.read_text())
        binary = Path(record['destination']) / 'cosmic-comp'
        binary.unlink()
        self.assertFalse(installer.already_installed(record, PACKAGES, '1.10.0', self.prefix))
        binary.write_text('modified')
        with self.assertRaises(RuntimeError):
            installer.already_installed(record, PACKAGES, '1.10.0', self.prefix)

    def test_unowned_and_modified_launchers_are_preserved(self):
        self.launcher.parent.mkdir()
        self.launcher.write_text('user launcher')
        with self.assertRaises(RuntimeError):
            self.install()
        with self.assertRaises(RuntimeError):
            installer.restore(self.prefix)
        self.launcher.unlink()
        self.install()
        self.launcher.write_text('modified by user')
        with self.assertRaises(RuntimeError):
            self.install()
        with self.assertRaises(RuntimeError):
            installer.restore(self.prefix)
        self.assertEqual(self.launcher.read_text(), 'modified by user')

    def test_record_write_failure_restores_the_launcher_and_allows_retry(self):
        original, _, _ = self.legacy()
        self.launcher.chmod(0o700)
        atomic = installer.atomic
        def fail_record(path, data, mode=0o600):
            if path == self.record:
                raise OSError('simulated disk failure')
            atomic(path, data, mode)
        with patch.object(installer, 'atomic', side_effect=fail_record):
            with self.assertRaises(OSError):
                self.install()
        self.assertEqual(self.launcher.read_bytes(), original)
        self.assertEqual(self.launcher.stat().st_mode & 0o777, 0o700)
        self.assertFalse(self.record.exists())
        self.install()
        installer.restore(self.prefix)
        self.assertFalse(self.launcher.exists())
        self.assertEqual((self.prefix / 'state/cosmic-forge/engine-previous-compositor').read_bytes(), original)

    def test_rejects_non_abi_binaries_and_launcher_symlinks(self):
        self.binary.write_bytes(b'\x7fELFold compositor')
        with self.assertRaises(RuntimeError):
            self.install()
        self.assertFalse(self.launcher.exists())
        self.launcher.parent.mkdir()
        self.launcher.symlink_to(self.prefix / 'missing')
        with self.assertRaises(RuntimeError):
            self.install()
        self.assertTrue(self.launcher.is_symlink())

    def test_modified_versioned_binary_is_preserved(self):
        self.install()
        record = json.loads(self.record.read_text())
        binary = Path(record['destination']) / 'cosmic-comp'
        binary.write_text('modified')
        with self.assertRaises(RuntimeError):
            self.install()
        self.assertEqual(binary.read_text(), 'modified')

    def test_package_detection_requires_matching_supported_versions(self):
        import subprocess
        def query(command, **_):
            return subprocess.CompletedProcess(command, 0, stdout=PACKAGES[command[-1]])
        with patch.object(installer.platform, 'freedesktop_os_release', return_value={'ID': 'arch'}), patch.object(installer.subprocess, 'run', side_effect=query):
            self.assertEqual(installer.installed_packages(), (PACKAGES, '1.10.0'))
        with patch.object(installer.platform, 'freedesktop_os_release', return_value={'ID': 'unknown'}):
            with self.assertRaises(RuntimeError):
                installer.installed_packages()

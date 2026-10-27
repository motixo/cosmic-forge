# SPDX-License-Identifier: GPL-3.0-only
"""Native Forge installation checks. Backend checks live in tests/."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import tomllib
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get('FORGE_BINARY', ROOT / 'target/release/cosmic-forge'))
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
INSTALL = [sys.executable, str(ROOT / 'scripts/install.py'), '--binary', str(BINARY)]

@unittest.skipUnless(BINARY.exists(), 'Build the Rust Forge executable first')
class ForgeInstallerTests(unittest.TestCase):

    def test_standalone_install_update_uninstall(self):
        with tempfile.TemporaryDirectory(prefix='forge install ') as tmp:
            prefix = Path(tmp) / 'prefix with spaces'
            (prefix / 'bin').mkdir(parents=True)
            settings = prefix / 'bin/cosmic-settings'
            settings.write_text('existing Settings launcher')
            comp = prefix / 'bin/cosmic-comp'
            comp.write_text('existing compositor launcher')
            command = INSTALL + ['--prefix', str(prefix)]
            subprocess.run(command, check=True, capture_output=True)
            subprocess.run(command, check=True, capture_output=True)
            output = subprocess.run([str(prefix / 'bin/cosmic-forge'), '--version'], check=True, capture_output=True, text=True).stdout
            self.assertIn('Rust / libcosmic', output)
            record = json.loads((prefix / 'state/cosmic-forge/installation.json').read_text())
            self.assertEqual((Path(record['destination']) / 'cosmic-forge').read_bytes()[:4], b'\x7fELF')
            subprocess.run(['desktop-file-validate', str(prefix / 'share/applications/io.github.motixo.CosmicForge.desktop')], check=True, capture_output=True)
            subprocess.run(command + ['--uninstall'], check=True, capture_output=True)
            self.assertFalse((prefix / 'bin/cosmic-forge').exists())
            self.assertEqual(settings.read_text(), 'existing Settings launcher')
            self.assertEqual(comp.read_text(), 'existing compositor launcher')

    def test_failed_update_rolls_back_launchers_and_record(self):
        spec = importlib.util.spec_from_file_location('forge_installer', ROOT / 'scripts/install.py')
        installer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(installer)
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            installer.install(prefix, BINARY)
            paths = [prefix / 'bin/cosmic-forge', prefix / 'share/applications/io.github.motixo.CosmicForge.desktop', prefix / 'state/cosmic-forge/installation.json']
            before = {p: p.read_bytes() for p in paths}
            original = installer.atomic

            def fail_desktop(path, data, mode=420):
                if path.suffix == '.desktop':
                    raise OSError('simulated write failure')
                return original(path, data, mode)
            with patch.object(installer, 'atomic', side_effect=fail_desktop):
                with self.assertRaises(OSError):
                    installer.install(prefix, BINARY)
            self.assertEqual({p: p.read_bytes() for p in paths}, before)

    def test_modified_launcher_is_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            command = INSTALL + ['--prefix', str(prefix)]
            subprocess.run(command, check=True, capture_output=True)
            launcher = prefix / 'bin/cosmic-forge'
            launcher.write_text('user replacement')
            result = subprocess.run(command, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(launcher.read_text(), 'user replacement')
            result = subprocess.run(command + ['--uninstall'], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(launcher.read_text(), 'user replacement')

    def test_uninstall_preserves_a_replaced_dangling_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            command = INSTALL + ['--prefix', str(prefix)]
            subprocess.run(command, check=True, capture_output=True)
            launcher = prefix / 'bin/cosmic-forge'
            launcher.unlink()
            launcher.symlink_to(prefix / 'user-missing-target')
            result = subprocess.run(command + ['--uninstall'], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(launcher.is_symlink())

    def test_update_preserves_a_replaced_dangling_binary_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            command = INSTALL + ['--prefix', str(prefix)]
            subprocess.run(command, check=True, capture_output=True)
            record = json.loads((prefix / 'state/cosmic-forge/installation.json').read_text())
            binary = Path(record['destination']) / 'cosmic-forge'
            binary.unlink()
            binary.symlink_to(prefix / 'user-missing-target')
            result = subprocess.run(command, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(binary.is_symlink())

    def test_uninstall_rejects_an_unrelated_path_in_the_record(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            command = INSTALL + ['--prefix', str(prefix)]
            subprocess.run(command, check=True, capture_output=True)
            victim = prefix / 'important.txt'
            victim.write_text('keep me')
            record = prefix / 'state/cosmic-forge/installation.json'
            data = json.loads(record.read_text())
            data['files'][str(victim)] = hashlib.sha256(victim.read_bytes()).hexdigest()
            record.write_text(json.dumps(data))
            result = subprocess.run(command + ['--uninstall'], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(victim.read_text(), 'keep me')
            self.assertTrue((prefix / 'bin/cosmic-forge').exists())

    def test_non_native_file_is_not_executed_during_validation(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            fake = prefix / 'fake-forge'
            fake.write_text('#!/bin/sh\ntouch "' + str(prefix / 'executed') + '"\n')
            fake.chmod(493)
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/install.py'), '--prefix', str(prefix), '--binary', str(fake)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((prefix / 'executed').exists())

    def test_prefix_with_line_breaks_is_rejected_without_writes(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp) / 'broken\npath'
            result = subprocess.run(INSTALL + ['--prefix', str(prefix)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(prefix.exists())

    def test_migrates_managed_python_installation_preserving_old_payload_and_preferences(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            launcher = prefix / 'bin/cosmic-forge'
            launcher.parent.mkdir(parents=True)
            launcher.write_text('#!/bin/sh\nexec python3 old-app.py\n')
            record = prefix / 'state/cosmic-forge/installation.json'
            record.parent.mkdir(parents=True)
            old = prefix / 'share/cosmic-forge/versions/old/forge/app.py'
            old.parent.mkdir(parents=True)
            old.write_text('# previous version')
            pref = prefix / 'state/cosmic-forge/backups/saved.json'
            pref.parent.mkdir()
            pref.write_text('original backup')
            record.write_text(json.dumps({'version': '0.1.0', 'destination': str(old.parent.parent), 'files': {str(launcher): hashlib.sha256(launcher.read_bytes()).hexdigest()}}))
            subprocess.run(INSTALL + ['--prefix', str(prefix)], check=True, capture_output=True)
            self.assertEqual(old.read_text(), '# previous version')
            self.assertEqual(pref.read_text(), 'original backup')
            self.assertNotIn('python', launcher.read_text())
            self.assertEqual(json.loads(record.read_text())['version'], VERSION)
if __name__ == '__main__':
    unittest.main()

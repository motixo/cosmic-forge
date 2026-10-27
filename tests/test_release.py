# SPDX-License-Identifier: GPL-3.0-only
"""Check the download and extraction boundary before any installer is executed."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('easy_install', ROOT/'scripts/easy_install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class ReleaseTests(unittest.TestCase):
    def release(self, data=b'archive', version='0.9.0', arch='x86_64'):
        return {'tag_name':'v'+version,'assets':[{
            'name':f'cosmic-forge-{version}-linux-{arch}.tar.xz','size':len(data),
            'digest':'sha256:'+hashlib.sha256(data).hexdigest(),
            'browser_download_url':f'https://github.com/motixo/cosmic-forge/releases/download/v{version}/cosmic-forge-{version}-linux-{arch}.tar.xz'}]}

    def test_wrong_repository_tag_size_or_digest_stops_before_download(self):
        installer.asset(self.release(), 'x86_64')
        for key,value in [('browser_download_url','https://example.com/app'),('digest',None),('size',2**40)]:
            release=self.release();release['assets'][0][key]=value
            with self.assertRaises(RuntimeError):
                installer.asset(release,'x86_64')
        for tag in ['../escape','v0.9.0;echo no','v0.9.0-rc1']:
            release=self.release();release['tag_name']=tag
            with self.assertRaises(RuntimeError):installer.asset(release,'x86_64')

    def make_archive(self, path, extra=None):
        names=['bin/cosmic-forge','Cargo.toml','LICENSE','README.md','scripts/install.py',f'resources/icons/{installer.ICON}']
        with tarfile.open(path,'w:xz') as tar:
            for name in names:
                data=b'release file'
                info=tarfile.TarInfo('cosmic-forge-0.9.0/'+name);info.size=len(data)
                tar.addfile(info,io.BytesIO(data))
            if extra is not None:
                tar.addfile(extra,io.BytesIO(b''))

    def test_extraction_allows_only_the_expected_regular_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archive=root/'release.tar.xz'
            self.make_archive(archive)
            result=installer.extract(archive,root/'safe','0.9.0')
            self.assertTrue((result/'bin/cosmic-forge').stat().st_mode & 0o111)
            for name,kind in [('../../outside',tarfile.REGTYPE),('/tmp/outside',tarfile.REGTYPE),('cosmic-forge-0.9.0/bin/link',tarfile.SYMTYPE),('cosmic-forge-0.9.0/bin/cosmic-forge',tarfile.REGTYPE)]:
                info=tarfile.TarInfo(name);info.type=kind;info.linkname='/tmp/outside'
                self.make_archive(archive,info)
                with self.assertRaises(RuntimeError):installer.extract(archive,root/'unsafe','0.9.0')
                self.assertFalse((root.parent/'outside').exists())

    def test_checksum_failure_never_executes_code_or_changes_preferences(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);prefs=root/'preferences';prefs.write_text('keep')
            def download(url,path,maximum):
                path.write_bytes(json.dumps(self.release()).encode() if url==installer.API else b'corrupt')
            with patch.object(installer,'download',side_effect=download), \
                 patch.object(installer.os,'geteuid',return_value=1000), \
                 patch.object(installer.platform,'freedesktop_os_release',return_value={'ID':'ubuntu'}), \
                 patch.object(installer.platform,'machine',return_value='x86_64'), \
                 patch.object(installer.os,'confstr',return_value='glibc 2.39'), \
                 patch.object(installer.subprocess,'run') as execute:
                with self.assertRaisesRegex(RuntimeError,'checksum'):
                    installer.install(root/'prefix')
                execute.assert_not_called()
            self.assertEqual(prefs.read_text(),'keep')
            self.assertFalse((root/'prefix').exists())

    @unittest.skipUnless((ROOT/'target/release/cosmic-forge').exists(),'Build the native executable first')
    def test_verified_portable_release_installs_updates_and_removes_only_forge(self):
        spec=importlib.util.spec_from_file_location('package_release',ROOT/'scripts/package_release.py')
        packager=importlib.util.module_from_spec(spec);spec.loader.exec_module(packager)
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);prefix=root/'prefix';(prefix/'bin').mkdir(parents=True)
            compositor=prefix/'bin/cosmic-comp';compositor.write_text('preserve compositor')
            settings=prefix/'bin/cosmic-settings';settings.write_text('preserve Settings')
            arch=installer.platform.machine()
            packager.portable(ROOT/'target/release/cosmic-forge',root,arch)
            data=(root/f'cosmic-forge-{packager.VERSION}-linux-{arch}.tar.xz').read_bytes()
            def download(url,path,maximum):
                path.write_bytes(json.dumps(self.release(data,packager.VERSION,arch)).encode() if url==installer.API else data)
            with patch.object(installer,'download',side_effect=download), \
                 patch.object(installer.os,'geteuid',return_value=1000), \
                 patch.object(installer.platform,'freedesktop_os_release',return_value={'ID':'ubuntu'}), \
                 patch.object(installer.platform,'machine',return_value=arch), \
                 patch.object(installer.os,'confstr',return_value='glibc 2.39'):
                installer.install(prefix)
                self.assertTrue((prefix/'bin/cosmic-forge').exists())
                installer.install(prefix)
                installer.install(prefix,uninstall=True)
            self.assertFalse((prefix/'bin/cosmic-forge').exists())
            self.assertEqual(compositor.read_text(),'preserve compositor')
            self.assertEqual(settings.read_text(),'preserve Settings')


if __name__ == '__main__':
    unittest.main()

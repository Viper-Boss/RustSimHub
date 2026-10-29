# SPDX-License-Identifier: AGPL-3.0-only
import pathlib,tempfile,unittest,shutil
import build
class BuildTests(unittest.TestCase):
    def test_exact_files_and_version_without_test_mock_or_secrets(self):
        with tempfile.TemporaryDirectory() as tmp:
            out=pathlib.Path(tmp)/'site';report=build.assemble(out)
            self.assertFalse(report['published']);self.assertEqual(sorted(p.relative_to(out).as_posix() for p in out.rglob('*') if p.is_file()),sorted(report['files']))
            self.assertNotIn('__STATIC_VERSION__',(out/'configurator/sw.js').read_text())
            self.assertFalse(any('tests' in name or 'mock' in name or '.private' in name for name in report['files']))
            with self.assertRaises(ValueError):build.assemble(out)
    def test_symlink_extra_manifest_path_and_size_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp)/'root';shutil.copytree(build.ROOT/'configurator',root/'configurator');shutil.copytree(build.ROOT/'docs/progress',root/'docs/progress')
            target=root/'configurator/src/app.js';target.unlink();target.symlink_to(build.ROOT/'configurator/src/app.js')
            with self.assertRaises(ValueError):build.assemble(pathlib.Path(tmp)/'out',root)
            target.unlink();target.write_bytes(b'x'*524289)
            with self.assertRaises(ValueError):build.assemble(pathlib.Path(tmp)/'out',root)
            target.write_text('synthetic');policy=root/'configurator/src/static-policy.js';policy.write_text(policy.read_text().replace("'index.html'","'tests/mock-provider.js'"))
            with self.assertRaises(ValueError):build.assemble(pathlib.Path(tmp)/'out',root)
if __name__=='__main__':unittest.main()

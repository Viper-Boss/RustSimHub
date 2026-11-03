# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import render_material

class Rendering(unittest.TestCase):
    def test_review_archive_has_exact_flat_hashes_and_stays_default_off(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'review.tar'
            result = render_material.render(path)
            self.assertFalse(result['install_permitted'])
            self.assertFalse(result['ids_reserved'])
            self.assertEqual(path.stat().st_mode & 0o777,0o600)
            with tarfile.open(path) as archive:
                names = archive.getnames()
                self.assertEqual(set(names),set(render_material.material_plan.FILES)|{'bridge-policy.proposal.json','review-manifest.json'})
                for name in names:
                    self.assertNotIn('/',name)
                    self.assertTrue(archive.getmember(name).isfile())
                for name,digest in result['sha256'].items():
                    self.assertEqual(hashlib.sha256(archive.extractfile(name).read()).hexdigest(),digest)
                self.assertEqual(json.load(archive.extractfile('bridge-policy.proposal.json')),render_material.POLICY)
                self.assertFalse(result['production_ready'])
                self.assertFalse(result['default_enabled'])
    def test_existing_output_and_symlink_cannot_overwrite_other_files(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)/'existing'
            target.write_bytes(b'keep')
            alias = Path(directory)/'link'
            alias.symlink_to(target)
            for path in (target,alias):
                with self.assertRaises(OSError):
                    render_material.render(path)
            self.assertEqual(target.read_bytes(),b'keep')
    def test_archive_is_reproducible_without_current_clock_or_uid(self):
        with tempfile.TemporaryDirectory() as directory:
            a,b = (Path(directory)/name for name in ('a.tar','b.tar'))
            render_material.render(a)
            render_material.render(b)
            self.assertEqual(a.read_bytes(),b.read_bytes())

if __name__ == '__main__':
    unittest.main()

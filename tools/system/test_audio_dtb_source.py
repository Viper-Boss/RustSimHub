# SPDX-License-Identifier: AGPL-3.0-only
import importlib.util
import json
import pathlib
import shutil
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('audio_dtb', ROOT/'tools/rebuild-audio-dtb.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class AudioDtbSourceTests(unittest.TestCase):
    def test_pinned_files_pass_and_modified_binding_or_escape_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            source = pathlib.Path(directory)/'source'
            shutil.copytree(ROOT/'firmware/kernel/audio', source)
            module.verify_sources(source)
            target = source/'include/dt-bindings/sound/qcom,q6voice.h'
            target.write_bytes(target.read_bytes()+b'\n/* changed binding */\n')
            with self.assertRaises(ValueError):
                module.verify_sources(source)
            profile = json.loads((source/'source.json').read_text())
            profile['files']['../outside'] = {}
            (source/'source.json').write_text(json.dumps(profile))
            with self.assertRaises(ValueError):
                module.verify_sources(source)

    def test_wrong_input_and_existing_output_never_run_a_compiler(self):
        with tempfile.TemporaryDirectory() as directory:
            base = pathlib.Path(directory)
            input_dtb = base/'wrong.dtb';input_dtb.write_bytes(b'wrong upstream')
            out = base/'out'
            with self.assertRaises(ValueError):
                module.rebuild(ROOT/'firmware/kernel/audio', input_dtb, out, dtc='compiler-must-not-run')
            self.assertFalse(out.exists())
            out.mkdir();(out/'evidence').write_bytes(b'preserve')
            with self.assertRaises(FileExistsError):
                module.rebuild(ROOT/'firmware/kernel/audio', input_dtb, out, dtc='compiler-must-not-run')
            self.assertEqual((out/'evidence').read_bytes(), b'preserve')

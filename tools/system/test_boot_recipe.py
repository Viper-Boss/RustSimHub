# SPDX-License-Identifier: GPL-2.0-only
import importlib.util
import pathlib
import tempfile
import unittest

ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('boot_recipe',ROOT/'tools/verify-audio-boot-recipe.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


class BootRecipeTests(unittest.TestCase):
    def test_only_root_selector_changes_and_original_header_is_preserved(self):
        fields=[None]*15
        fields[12]=b'earlycon root=UUID=synthetic console=ttyMSM0 rw'.ljust(512,b'\0')
        fields[14]=bytes(1024)
        result=module.root_label(fields)
        self.assertEqual(result[12].split(b'\0',1)[0],b'earlycon root=LABEL=rootfs console=ttyMSM0 rw')
        self.assertIn(b'root=UUID=synthetic',fields[12])
        self.assertEqual(len(result[12]),512)
        self.assertEqual(result[14],fields[14])

    def test_duplicate_missing_or_overlong_root_is_refused(self):
        for main,extra in [(b'console=ttyMSM0',b''),(b'root=a root=b',b''),(b'root=a',b'root=b'),(b'x'*510+b' root=a',b'')]:
            fields=[None]*15;fields[12]=main;fields[14]=extra
            with self.assertRaises(ValueError): module.root_label(fields)

    def test_wrong_input_and_existing_report_cannot_produce_an_image(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=pathlib.Path(temporary)
            upstream=root/'wrong';upstream.write_bytes(b'not an upstream boot image')
            report=root/'report.json'
            with self.assertRaises(ValueError): module.verify(upstream,root/'no-dtb',report)
            self.assertFalse(report.exists())
            report.write_bytes(b'preserve')
            with self.assertRaises(FileExistsError): module.verify(upstream,root/'no-dtb',report)
            self.assertEqual(report.read_bytes(),b'preserve')

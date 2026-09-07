from pathlib import Path
import tempfile
import unittest

from capture_matrix import select_inputs, fingerprint_reads


class CaptureSelectionTests(unittest.TestCase):
    def test_bounded_selection_is_sorted_and_excludes_unrelated_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["b.fits", "a.fits", "x.xisf", "notes.txt"]:
                (root / name).write_bytes(b"capture")
            self.assertEqual([p.name for p in select_inputs(root, False, 1)], ["a.fits", "x.xisf"])
            records = fingerprint_reads(select_inputs(root, False, 1))["samples"]
            self.assertEqual(len(records), 4)
            self.assertEqual(records[0]["sha256"], records[2]["sha256"])

    def test_recursion_is_explicit_and_both_formats_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.fits").write_bytes(b"capture")
            (root / "nested").mkdir()
            (root / "nested" / "b.xisf").write_bytes(b"capture")
            with self.assertRaises(ValueError):
                select_inputs(root, False, 1)
            self.assertEqual(len(select_inputs(root, True, 1)), 2)
            with self.assertRaises(ValueError):
                select_inputs(root, True, 9)


if __name__ == "__main__":
    unittest.main()

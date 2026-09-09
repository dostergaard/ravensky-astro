import copy
import unittest

import benchmark_suite as suite


def report():
    return {
        "schema_version": 1,
        "provenance": {"source_sha256": "a" * 64, "build_profile": "release"},
        "manifest": {
            "generator_version": 2,
            "recipe": {"frames": 2},
            "files": [
                {"sha256": "b" * 64, "stored_bytes": 100, "decoded_bytes": 200},
                {"sha256": "c" * 64, "stored_bytes": 100, "decoded_bytes": 200},
            ],
        },
        "samples": [{
            "workload": "full", "workers": 1, "completed_files": 2,
            "stored_bytes": 200, "decoded_bytes": 400, "read_bytes": 400,
            "wall_seconds": 0.1, "cpu_seconds": 0.09,
            "peak_rss_bytes": 1024, "peak_reserved_bytes": 100,
            "files": [{"index": 0}, {"index": 1}],
        }],
    }


class ReportTests(unittest.TestCase):
    def test_valid_report_and_unavailable_platform_telemetry(self):
        data = report()
        suite.audit_report(data, [1], 1)
        data["samples"][0]["peak_rss_bytes"] = None
        data["samples"][0]["cpu_seconds"] = None
        suite.audit_report(data, [1], 1)

    def test_rejects_missing_repetitions_and_duplicate_files(self):
        with self.assertRaises(ValueError):
            suite.audit_report(report(), [1], 2)
        data = report()
        data["samples"][0]["files"][1]["index"] = 0
        with self.assertRaises(ValueError):
            suite.audit_report(data, [1], 1)

    def test_rejects_false_byte_counts_and_over_budget_samples(self):
        for key, value in [("decoded_bytes", 399), ("stored_bytes", 199),
                           ("peak_reserved_bytes", 512 * 1024**2 + 1),
                           ("wall_seconds", float("nan"))]:
            data = report()
            data["samples"][0][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                suite.audit_report(data, [1], 1)

    def test_cross_run_comparison_requires_same_sources_and_fixture_bytes(self):
        a = report()
        b = copy.deepcopy(a)
        suite.require_comparable(a, b)
        b["provenance"]["source_sha256"] = "d" * 64
        with self.assertRaises(ValueError):
            suite.require_comparable(a, b)
        b = copy.deepcopy(a)
        b["manifest"]["files"][0]["sha256"] = "d" * 64
        with self.assertRaises(ValueError):
            suite.require_comparable(a, b)

    def test_matrix_has_both_orders_and_tile_size_extremes(self):
        cases = suite.scaling_cases()
        self.assertEqual(len(cases), 48)
        self.assertEqual({c["workers"] for c in cases}, {"1,2,4,8", "8,4,2,1"})
        self.assertEqual({c["tile_rows"] for c in cases}, {1, 32, 2048, 8192})
        self.assertEqual({c["height"] for c in cases}, {2048, 8192})
        self.assertTrue(all(c["repeats"] == 5 for c in cases))

    def test_nearest_rank_percentile_retains_tail_and_rejects_empty(self):
        self.assertEqual(suite.percentile([1, 2, 3, 4, 100], 95), 100)
        with self.assertRaises(ValueError):
            suite.percentile([], 95)


if __name__ == "__main__":
    unittest.main()

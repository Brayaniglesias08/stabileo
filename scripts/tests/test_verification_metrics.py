"""Tests of scripts/verification-metrics.py with a fake `gh` (no network).

    python3 -m unittest discover -s scripts/tests -v
"""

import importlib.util
import json
import os
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("vm", os.path.join(HERE, "..", "verification-metrics.py"))
vm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vm)


def run(i, conclusion="success", branch="main", event="schedule"):
    return {"databaseId": i, "status": "completed", "conclusion": conclusion, "event": event,
            "headBranch": branch, "createdAt": f"2026-10-{i:02d}T00:00:00Z", "headSha": f"sha{i}",
            "url": f"https://example/runs/{i}"}


class FakeGH:
    """runs: {workflow: [run, ...]} newest first; artifacts: {run_id: [name, ...]};
    files: {(run_id, name): {relative path: text}} written on download."""

    def __init__(self, runs, artifacts, files=None):
        self.ok = True
        self._runs, self._arts, self._files = runs, artifacts, files or {}
        self.tmp = tempfile.mkdtemp()
        self.branch_args = []

    def runs(self, workflow, max_runs, branch="main"):
        self.branch_args.append(branch)
        rs = self._runs.get(workflow, [])
        if branch:
            rs = [r for r in rs if r["headBranch"] == branch]
        return rs[:max_runs]

    def artifacts(self, run_id):
        return [{"name": n, "expired": False} for n in self._arts.get(run_id, [])]

    def download(self, run_id, name):
        files = self._files.get((run_id, name))
        if files is None:
            return None
        d = os.path.join(self.tmp, f"{run_id}-{name}")
        for rel, text in files.items():
            p = os.path.join(d, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w") as f:
                f.write(text)
        return d


def stryker(killed, survived):
    mutants = [{"status": "Killed"}] * killed + [{"status": "Survived"}] * survived
    return {"mutation.json": json.dumps({"files": {"a.ts": {"mutants": mutants}}})}


class ArtifactSelection(unittest.TestCase):
    def test_newest_run_with_matches_only(self):
        gh = FakeGH({"w.yml": [run(3), run(2), run(1)]}, {3: ["x-a"], 2: ["x-a", "x-b"], 1: ["x-c"]})
        hits = vm.find_artifacts(gh, "w.yml", r"x-.+", 10)
        self.assertEqual([(r["databaseId"], n) for r, n in hits], [(3, "x-a")])
        hits = vm.find_artifacts(gh, "w.yml", r"x-.+", 10, all_matches=True)
        self.assertEqual(len(hits), 4)

    def test_incomplete_runs_skipped(self):
        r3 = dict(run(3), status="in_progress")
        gh = FakeGH({"w.yml": [r3, run(2)]}, {3: ["x"], 2: ["x"]})
        self.assertEqual(vm.find_artifacts(gh, "w.yml", "x", 10)[0][0]["databaseId"], 2)

    def test_stryker_skips_failed_newest_run(self):
        # The newest run failed but uploaded a report; an older successful run has a usable one.
        gh = FakeGH({"mutation-ts.yml": [run(5, "failure"), run(4)]},
                    {5: ["stryker-report"], 4: ["stryker-report"]},
                    {(5, "stryker-report"): stryker(1, 9), (4, "stryker-report"): stryker(9, 1)})
        r = vm.ts_mutants(gh, 10)
        self.assertIsNotNone(r)
        self.assertEqual(r["score"], 90.0)
        self.assertEqual(r["run"], "https://example/runs/4")

    def test_stryker_none_when_no_success(self):
        gh = FakeGH({"mutation-ts.yml": [run(5, "failure")]}, {5: ["stryker-report"]},
                    {(5, "stryker-report"): stryker(1, 1)})
        self.assertIsNone(vm.ts_mutants(gh, 10))

    def test_coverage_found_on_tag_runs_not_feature_branches(self):
        summary = {"data": [{"totals": {"lines": {"percent": 81.5}, "branches": {"percent": 70.0},
                                        "functions": {"percent": 90.0}}}]}
        runs = [run(9, branch="feature/x", event="workflow_dispatch"),
                run(8, branch="v0.3.0", event="push"), run(7)]
        gh = FakeGH({"verification-dashboard.yml": runs},
                    {9: ["llvm-cov-summary"], 8: ["llvm-cov-summary"]},
                    {(9, "llvm-cov-summary"): {"c.json": json.dumps(dict(summary, data=[{"totals": {
                        "lines": {"percent": 1.0}, "functions": {"percent": 1.0}}}]))},
                     (8, "llvm-cov-summary"): {"c.json": json.dumps(summary)}})
        cov = vm.coverage(gh, 10, None)
        self.assertEqual(cov["lines"], 81.5)
        self.assertEqual(cov["branches"], 70.0)
        self.assertIn(None, gh.branch_args)  # searched beyond main

    def test_nightly_searches_stay_on_main(self):
        gh = FakeGH({"verify.yml": [run(2, branch="ci/x"), run(1)]}, {2: ["fuzz-stats-a"], 1: []})
        self.assertIsNone(vm.fuzz_execs(gh, 10))
        self.assertEqual(set(gh.branch_args), {"main"})

    def test_main_or_tag_filter(self):
        self.assertTrue(vm.is_main_or_tag_run(run(1)))
        self.assertTrue(vm.is_main_or_tag_run(run(1, branch="v1.2.3", event="push")))
        self.assertFalse(vm.is_main_or_tag_run(run(1, branch="v1.2.3", event="pull_request")))
        self.assertFalse(vm.is_main_or_tag_run(run(1, branch="docs/x", event="workflow_dispatch")))


class History(unittest.TestCase):
    def test_snapshots_merge_and_order(self):
        d = tempfile.mkdtemp()
        vm.write_release(d, {"release": "v0.10.0", "date": "d", "commit": "c", "values": {}})
        vm.write_release(d, {"release": "v0.2.0", "date": "d", "commit": "a", "values": {"x": "1"}})
        vm.write_release(d, {"release": "v0.9.1", "date": "d", "commit": "b", "values": {}})
        self.assertEqual([r["release"] for r in vm.load_history(d)], ["v0.2.0", "v0.9.1", "v0.10.0"])

    def test_rewriting_one_release_keeps_the_others(self):
        d = tempfile.mkdtemp()
        vm.write_release(d, {"release": "v0.2.0", "date": "d", "commit": "a", "values": {"x": "1"}})
        vm.write_release(d, {"release": "v0.3.0", "date": "d", "commit": "b", "values": {"x": "2"}})
        vm.write_release(d, {"release": "v0.3.0", "date": "d", "commit": "b", "values": {"x": "3"}})
        h = vm.load_history(d)
        self.assertEqual([(r["release"], r["values"]["x"]) for r in h], [("v0.2.0", "1"), ("v0.3.0", "3")])

    def test_bad_release_name_rejected(self):
        with self.assertRaises(ValueError):
            vm.write_release(tempfile.mkdtemp(), {"release": "../evil", "values": {}})


if __name__ == "__main__":
    unittest.main()

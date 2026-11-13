"""scripts/issue_labels.py のテスト（#523）。GitHubとClaude APIは使わない。

実行: python -m unittest discover -s scripts -p "test_*.py"
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import issue_labels as il  # noqa: E402


def issue(*labels, number=1):
    return il.Issue(number=number, title="t", body="b", labels=list(labels))


def fixed_judge(type_="bug", priority="priority: high"):
    calls = []

    def judge(i):
        calls.append(i.number)
        return il.Judgment(type_, priority, "reason")

    judge.calls = calls
    return judge


class Recorder:
    def __init__(self):
        self.calls = []

    def __call__(self, number, labels):
        self.calls.append((number, labels))


class MissingCategoriesTest(unittest.TestCase):
    def test_none(self):
        self.assertEqual(il.missing_categories([]), ["type", "priority"])

    def test_type_only(self):
        self.assertEqual(il.missing_categories(["bug", "database"]), ["priority"])

    def test_priority_only(self):
        self.assertEqual(il.missing_categories(["priority: low"]), ["type"])

    def test_complete(self):
        self.assertEqual(il.missing_categories(["security", "priority: medium"]), [])

    def test_old_priority_labels_do_not_count(self):
        # 基準外の古い優先度のラベル（#519）は、優先度として数えない
        self.assertEqual(il.missing_categories(["enhancement", "P0-Critical"]), ["priority"])


class TriageTest(unittest.TestCase):
    def test_adds_both_when_missing(self):
        apply = Recorder()
        r = il.triage(issue("database"), fixed_judge(), apply)
        self.assertEqual(r.status, "labeled")
        self.assertEqual(apply.calls, [(1, ["bug", "priority: high"])])

    def test_keeps_existing_type(self):
        # 種類が付いていれば、判定が違っても変えない。優先度だけを足す
        apply = Recorder()
        r = il.triage(issue("documentation"), fixed_judge("bug", "priority: low"), apply)
        self.assertEqual(apply.calls, [(1, ["priority: low"])])
        self.assertEqual(r.add, ["priority: low"])

    def test_keeps_existing_priority(self):
        apply = Recorder()
        il.triage(issue("priority: medium"), fixed_judge("security", "priority: high"), apply)
        self.assertEqual(apply.calls, [(1, ["security"])])

    def test_complete_issue_is_not_judged(self):
        judge = fixed_judge()
        apply = Recorder()
        r = il.triage(issue("bug", "priority: low"), judge, apply)
        self.assertEqual(r.status, "complete")
        self.assertEqual(judge.calls, [])
        self.assertEqual(apply.calls, [])

    def test_excluded_issue_is_not_judged(self):
        judge = fixed_judge()
        apply = Recorder()
        r = il.triage(issue("automated", "weekly-report"), judge, apply)
        self.assertEqual(r.status, "excluded")
        self.assertEqual(judge.calls, [])
        self.assertEqual(apply.calls, [])

    def test_report_only(self):
        apply = Recorder()
        r = il.triage(issue(), None, apply)
        self.assertEqual(r.status, "reported")
        self.assertEqual(r.missing, ["type", "priority"])
        self.assertEqual(apply.calls, [])

    def test_dry_run_does_not_apply(self):
        r = il.triage(issue(), fixed_judge(), None)
        self.assertEqual(r.status, "dry-run")
        self.assertEqual(r.add, ["bug", "priority: high"])

    def test_judgment_outside_criteria_is_ignored(self):
        # スキーマで絞っているが、基準外の答えが来ても付けない
        apply = Recorder()
        r = il.triage(issue(), fixed_judge("wontfix", "priority: critical"), apply)
        self.assertEqual(r.add, [])
        self.assertEqual(apply.calls, [])

    def test_judge_error_is_reported(self):
        def broken(_):
            raise RuntimeError("boom")

        apply = Recorder()
        r = il.triage(issue(), broken, apply)
        self.assertEqual(r.status, "error")
        self.assertIn("boom", r.error)
        self.assertEqual(apply.calls, [])

    def test_never_removes_labels(self):
        # 付ける処理だけで、外す処理は無い（applyに渡るのは足すラベルだけ）
        apply = Recorder()
        il.triage(issue("database", "P1"), fixed_judge(), apply)
        (number, labels), = apply.calls
        self.assertNotIn("database", labels)
        self.assertNotIn("P1", labels)


class SummaryTest(unittest.TestCase):
    def test_escapes_pipes(self):
        import tempfile

        r = il.Result(il.Issue(5, "a|b", "", []), ["type"], ["bug"], "x|y", "dry-run")
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "summary.md")
            il.write_summary([r], path)
            text = open(path, encoding="utf-8").read()
        self.assertIn("a\\|b", text)
        self.assertIn("x\\|y", text)


if __name__ == "__main__":
    unittest.main()

"""scripts/issue_labels.py のテスト（#523）。GitHubは使わない。

実行: python -m unittest discover -s scripts -p "test_*.py"
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import issue_labels as il  # noqa: E402


def issue(*labels, number=1):
    return il.Issue(number=number, title="t", labels=list(labels))


class Recorder:
    def __init__(self, fail=False):
        self.calls = []
        self.fail = fail

    def __call__(self, number, labels):
        if self.fail:
            raise RuntimeError("edit failed")
        self.calls.append((number, labels))


class ProblemsTest(unittest.TestCase):
    def test_ok(self):
        self.assertEqual(il.problems(["bug", "priority: high"]), [])

    def test_ok_with_components(self):
        self.assertEqual(il.problems(["enhancement", "priority: low", "database", "docker"]), [])

    def test_security_as_type(self):
        self.assertEqual(il.problems(["security", "priority: low"]), [])

    def test_security_as_component(self):
        # security は issue-labeler.yml がコンポーネントとしても付ける
        self.assertEqual(il.problems(["bug", "security", "priority: high"]), [])

    def test_no_type(self):
        self.assertEqual(il.problems(["priority: low"]), ["種類が0個"])

    def test_two_types(self):
        self.assertEqual(il.problems(["bug", "enhancement", "priority: low"]), ["種類が2個"])

    def test_no_priority(self):
        self.assertEqual(il.problems(["bug"]), ["優先度が0個"])

    def test_two_priorities(self):
        self.assertEqual(il.problems(["bug", "priority: high", "priority: low"]), ["優先度が2個"])

    def test_epic_is_allowed(self):
        # roadmap-health.yml がマイルストーンの集計に使う
        self.assertEqual(il.problems(["enhancement", "priority: medium", "epic"]), [])

    def test_unknown_labels(self):
        # 古い優先度のラベル等（#519）は、決まりにないラベルとして出す
        self.assertEqual(
            il.problems(["bug", "priority: low", "P0-Critical", "websocket"]),
            ["決まりにないラベル: P0-Critical"],
        )


class CheckTest(unittest.TestCase):
    def test_fix_adds_default_priority_only(self):
        apply = Recorder()
        r = il.check(issue("database"), apply)
        # 優先度の既定だけを付ける。種類は付けない
        self.assertEqual(apply.calls, [(1, ["priority: medium"])])
        self.assertEqual(r.added, ["priority: medium"])
        self.assertEqual(r.problems, ["種類が0個"])

    def test_fix_keeps_existing_priority(self):
        apply = Recorder()
        r = il.check(issue("bug", "priority: low"), apply)
        self.assertEqual(apply.calls, [])
        self.assertEqual(r.problems, [])

    def test_without_fix_does_not_edit(self):
        r = il.check(issue("bug"), None)
        self.assertEqual(r.added, [])
        self.assertEqual(r.problems, ["優先度が0個"])

    def test_excluded_issue(self):
        apply = Recorder()
        r = il.check(issue("automated", "roadmap-health", "weekly-report"), apply)
        self.assertTrue(r.excluded)
        self.assertEqual(apply.calls, [])
        self.assertEqual(r.problems, [])

    def test_edit_failure_is_reported(self):
        r = il.check(issue("bug"), Recorder(fail=True))
        self.assertIn("edit failed", r.edit_error)
        self.assertEqual(r.problems[0], "既定のラベルを付けられなかった（priority: medium）")
        self.assertIn("優先度が0個", r.problems)

    def test_never_removes_or_replaces(self):
        # 付けるのは足りない既定だけ。既にあるラベルを渡して外すことは無い
        apply = Recorder()
        il.check(issue("bug", "database"), apply)
        self.assertEqual(apply.calls, [(1, ["priority: medium"])])

    def test_no_default_over_unknown_labels(self):
        # 古い優先度のラベル（P1 等）の上に medium を足さない。報告だけにする
        apply = Recorder()
        r = il.check(issue("bug", "P1"), apply)
        self.assertEqual(apply.calls, [])
        self.assertEqual(r.problems, ["優先度が0個", "決まりにないラベル: P1"])


class ReportTest(unittest.TestCase):
    def test_all_ok(self):
        results = [il.check(issue("bug", "priority: low"), None), il.check(issue("automated"), None)]
        self.assertEqual(il.report(results, False), "開いているIssue 1件のラベルは、決まりに沿っています。")

    def test_lists_problems(self):
        results = [il.check(issue("bug", number=7), None)]
        text = il.report(results, False)
        self.assertIn("1件あります", text)
        self.assertIn("- #7 t: 優先度が0個", text)

    def test_truncated(self):
        text = il.report([], True)
        self.assertIn("1000件に達した", text)


class MainExitCodeTest(unittest.TestCase):
    def setUp(self):
        self.orig = (il.fetch_issue, il.fetch_open_issues, il.add_labels)
        self.env = os.environ.pop("GITHUB_STEP_SUMMARY", None)

    def tearDown(self):
        il.fetch_issue, il.fetch_open_issues, il.add_labels = self.orig
        if self.env is not None:
            os.environ["GITHUB_STEP_SUMMARY"] = self.env

    def run_main(self, argv):
        import contextlib
        import io

        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            return il.main(argv)

    def test_weekly_fails_on_problems(self):
        il.fetch_open_issues = lambda: [issue("priority: low")]
        self.assertEqual(self.run_main([]), 1)

    def test_weekly_passes_when_ok(self):
        il.fetch_open_issues = lambda: [issue("bug", "priority: low")]
        self.assertEqual(self.run_main([]), 0)

    def test_single_issue_ignores_rule_problems(self):
        # 起票時は、種類が無くても失敗にしない（週次で扱う）
        il.fetch_issue = lambda n: issue("database", number=n)
        il.add_labels = Recorder()
        self.assertEqual(self.run_main(["--fix", "--issue", "5"]), 0)

    def test_single_issue_fails_when_fetch_fails(self):
        def fail(n):
            raise RuntimeError("not found")

        il.fetch_issue = fail
        self.assertEqual(self.run_main(["--fix", "--issue", "5"]), 1)

    def test_single_issue_without_fix_fails_on_problems(self):
        # --fix なしの --issue は、手元での確かめ。決まりの誤りで失敗にする
        il.fetch_issue = lambda n: issue("database", number=n)
        self.assertEqual(self.run_main(["--issue", "5"]), 1)

    def test_single_issue_fails_when_edit_fails(self):
        il.fetch_issue = lambda n: issue(number=n)
        il.add_labels = Recorder(fail=True)
        self.assertEqual(self.run_main(["--fix", "--issue", "5"]), 1)


if __name__ == "__main__":
    unittest.main()

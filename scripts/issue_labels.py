#!/usr/bin/env python3
"""開いているIssueのラベルが、決まりに沿っているかを確かめる（#523）。

決まり（docs/project-management-guide.md の「Issueのラベルの基準」）:

- 種類（enhancement・bug・documentation・security）を1つ。security はコンポーネント
  としても付くので、ほかの種類と一緒に付いていてもよい
- 優先度（priority: high・priority: medium・priority: low）を1つ
- ほかに付けてよいのは、コンポーネント・epicと、自動で作られるIssueの専用ラベルだけ
- 自動で作られるIssue（automated・cargo-audit-scheduled）は確かめない

種類が1つでない・優先度が1つでない・決まりにないラベルがあるIssueを一覧にし、
1件でもあれば失敗にする（週次の実行で、失敗の通知から気づけるように）。

--fix を付けると、優先度の無いIssueに priority: medium を付ける。中身を読まずに
決まる既定だけで、種類は付けず、ほかのラベルは付け替えない・外さない。
決まりにないラベル（古い優先度のラベル等）があるIssueには付けない（報告だけ。
P1 等の上に medium を足すと、古いラベルを外したときに優先度が気づかれずに
下がるため）。種類と、既定と違う優先度は、人が付ける。

使い方:
    python scripts/issue_labels.py                 # 開いているIssueを確かめる
    python scripts/issue_labels.py --fix           # 既定を付けてから確かめる
    python scripts/issue_labels.py --fix --issue 123
        # このIssueにだけ既定を付ける（起票時。決まりの誤りでは失敗にしない）
    python scripts/issue_labels.py --issue 123   # このIssueだけを確かめる

Issueは gh で読み書きする（GH_REPO・GH_TOKEN、または gh auth login）。
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass, field
from typing import Callable, Optional

TYPE_LABELS = ["enhancement", "bug", "documentation", "security"]
PRIORITY_LABELS = ["priority: high", "priority: medium", "priority: low"]
DEFAULT_PRIORITY = "priority: medium"
# issue-labeler.yml がタイトルから付けるもの（security は種類と兼ねる）
COMPONENT_LABELS = ["database", "websocket", "docker", "kubernetes", "infrastructure"]
# 種類・優先度と一緒に付けてよいもの（roadmap-health.yml がマイルストーンの集計に使う）
OTHER_LABELS = ["epic"]
# 自動で作られるIssue。種類・優先度の決まりは当てはめない
EXCLUDED_LABELS = ["automated", "cargo-audit-scheduled"]
# 自動で作られるIssueに、専用ラベルと一緒に付くもの
AUTOMATION_LABELS = ["roadmap-health", "weekly-report"]
KNOWN_LABELS = set(
    TYPE_LABELS + PRIORITY_LABELS + COMPONENT_LABELS + OTHER_LABELS + EXCLUDED_LABELS + AUTOMATION_LABELS
)

LIMIT = 1000


@dataclass
class Issue:
    number: int
    title: str
    labels: list[str] = field(default_factory=list)


@dataclass
class Result:
    issue: Issue
    excluded: bool = False
    added: list[str] = field(default_factory=list)
    problems: list[str] = field(default_factory=list)
    edit_error: str = ""


def count(labels: list[str], axis: list[str]) -> int:
    return sum(1 for label in labels if label in axis)


def defaults_to_add(labels: list[str]) -> list[str]:
    """中身を読まずに決まる既定のうち、足りないもの（優先度が無ければ priority: medium）。

    決まりにないラベルがあれば付けない。古い優先度のラベル（P1 等）の上に
    medium を足すと、古いラベルを外したときに優先度が気づかれずに下がるため。
    """
    if unknown_labels(labels):
        return []
    return [DEFAULT_PRIORITY] if count(labels, PRIORITY_LABELS) == 0 else []


def unknown_labels(labels: list[str]) -> list[str]:
    return [label for label in labels if label not in KNOWN_LABELS]


def type_count(labels: list[str]) -> int:
    """種類の数。security はコンポーネントとしても付く（issue-labeler.yml）ので、
    ほかの種類があれば数えず、ほかの種類が無いときだけ種類として数える。"""
    others = count(labels, [t for t in TYPE_LABELS if t != "security"])
    if others == 0 and "security" in labels:
        return 1
    return others


def problems(labels: list[str]) -> list[str]:
    """決まりに沿っていない点を返す。"""
    found = []
    types = type_count(labels)
    if types != 1:
        found.append(f"種類が{types}個")
    priorities = count(labels, PRIORITY_LABELS)
    if priorities != 1:
        found.append(f"優先度が{priorities}個")
    unknown = unknown_labels(labels)
    if unknown:
        found.append(f"決まりにないラベル: {', '.join(unknown)}")
    return found


def check(issue: Issue, apply: Optional[Callable[[int, list[str]], None]]) -> Result:
    """1件のIssueを確かめる。applyがあれば、先に既定を付ける（--fix）。"""
    if any(label in EXCLUDED_LABELS for label in issue.labels):
        return Result(issue, excluded=True)
    result = Result(issue)
    labels = list(issue.labels)
    add = defaults_to_add(labels)
    if apply is not None and add:
        try:
            apply(issue.number, add)
            result.added = add
            labels += add
        except Exception as e:  # 1件で止めず、残りのIssueも確かめる
            result.edit_error = str(e)
            result.problems.append(f"既定のラベルを付けられなかった（{', '.join(add)}）")
    result.problems += problems(labels)
    return result


# --- GitHub（gh） ---------------------------------------------------------


def gh(*args: str) -> str:
    out = subprocess.run(
        ["gh", *args], capture_output=True, text=True, encoding="utf-8"
    )
    if out.returncode != 0:
        raise RuntimeError(out.stderr.strip() or f"gh {' '.join(args)} が失敗した")
    return out.stdout


def to_issue(data: dict) -> Issue:
    return Issue(
        number=data["number"],
        title=data.get("title") or "",
        labels=[label["name"] for label in data.get("labels") or []],
    )


def fetch_issue(number: int) -> Issue:
    return to_issue(json.loads(gh("issue", "view", str(number), "--json", "number,title,labels")))


def fetch_open_issues() -> list[Issue]:
    data = json.loads(
        gh("issue", "list", "--state", "open", "--limit", str(LIMIT), "--json", "number,title,labels")
    )
    return [to_issue(d) for d in data]


def add_labels(number: int, labels: list[str]) -> None:
    gh("issue", "edit", str(number), "--add-label", ",".join(labels))


# --- 出力 -------------------------------------------------------------------


def report(results: list[Result], truncated: bool) -> str:
    lines = []
    if truncated:
        lines.append(f"- 開いているIssueが{LIMIT}件に達したため、それより後のIssueは確かめていない")
    for r in results:
        if r.problems:
            lines.append(f"- #{r.issue.number} {r.issue.title}: {'、'.join(r.problems)}")
    checked = sum(1 for r in results if not r.excluded)
    if not lines:
        return f"開いているIssue {checked}件のラベルは、決まりに沿っています。"
    return (
        f"決まりに沿っていないIssueが{sum(1 for r in results if r.problems)}件あります"
        "（docs/project-management-guide.md の「Issueのラベルの基準」）。\n\n" + "\n".join(lines)
    )


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--fix", action="store_true", help="優先度の無いIssueに priority: medium を付ける")
    parser.add_argument("--issue", type=int, help="このIssueだけを見る（起票時）")
    args = parser.parse_args(argv)

    if args.issue:
        try:
            issues = [fetch_issue(args.issue)]
        except Exception as e:  # 起票から読むまでの間に、削除・移管された等
            print(f"#{args.issue} を読めなかった: {e}", file=sys.stderr)
            return 1
        truncated = False
    else:
        issues = fetch_open_issues()
        truncated = len(issues) >= LIMIT

    apply = add_labels if args.fix else None
    results = [check(issue, apply) for issue in issues]

    for r in results:
        if r.added:
            print(f"#{r.issue.number}: {', '.join(r.added)} を付けた")
        if r.edit_error:
            print(f"#{r.issue.number}: {r.edit_error}", file=sys.stderr)
    text = report(results, truncated)
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(f"## Issueのラベル\n\n{text}\n")

    edit_failed = any(r.edit_error for r in results)
    if args.issue and args.fix:
        # 起票時は、既定を付けられなかったときだけ失敗にする（決まりの誤りは週次で扱う）
        return 1 if edit_failed else 0
    return 1 if edit_failed or truncated or any(r.problems for r in results) else 0


if __name__ == "__main__":
    sys.exit(main())

# PR運用ルール (Pull Request Management Rules)

## 📋 概要

MCP-RSプロジェクトでは、効率的で一貫性のあるプルリクエスト管理のため、以下のルールに従います。

## 🗂️ ファイル管理規則

## 固定ファイル（変更管理対象）

これらのファイルは**固定名**で管理し、PR作成時に内容を更新します：

1. **`.github/PULL_REQUEST_TEMPLATE.md`** - GitHubのデフォルトPRテンプレート
2. **`PR_DESCRIPTION.md`** - PR説明の詳細版（プロジェクトルート）

## 禁止事項

- ❌ `PR_v1.2.3.md` のような個別PRファイルの作成
- ❌ `RELEASE_v1.2.3.md` のようなバージョン固有リリースファイル
- ❌ 古いPR/リリース説明ファイルの残存

## 推奨事項

- ✅ `PR_DESCRIPTION.md` の内容を各PR向けに更新
- ✅ マージ後は次のPR用にテンプレートをリセット
- ✅ 重要なPR内容は `CHANGELOG.md` や `RELEASE_NOTES.md` に記録

## 🔄 ワークフロー

## 1. 新しいPR準備

```bash

## developブランチで作業開始

git checkout develop
git pull origin develop

## PR_DESCRIPTION.mdを現在のPR向けに更新

## (このドキュメントの「PRテンプレート」セクションを参照)

```

## 2. PR作成時

1. `PR_DESCRIPTION.md` を該当PR向けに完全更新
2. GitHubでPRを作成（`.github/PULL_REQUEST_TEMPLATE.md` が自動適用）
3. `PR_DESCRIPTION.md` の内容をPR説明にコピー&ペースト

## 3. マージ後の整理

```bash

## マージ完了後、テンプレートをリセット

git checkout develop
git pull origin develop

## PR_DESCRIPTION.mdを次回PR用のテンプレートに戻す

```

## 📝 PRテンプレート（PR_DESCRIPTION.md用）

以下のテンプレートを `PR_DESCRIPTION.md` にコピーして、各PR向けにカスタマイズしてください：

```markdown

## 🚀 [機能名] - [概要]

## 📋 Summary

[このPRで実装する機能の簡潔な説明]

## 🎯 Objectives Completed

## ✅ **[主要機能1]**

- **[サブ機能1]**: [詳細説明]
- **[サブ機能2]**: [詳細説明]

## ✅ **[主要機能2]**

- **[サブ機能1]**: [詳細説明]
- **[サブ機能2]**: [詳細説明]

## 🏗️ Technical Implementation

## **[技術要素1]**

```rust
// コード例
```

## **[技術要素2]**

- [実装詳細]

## 📁 Files Added/Modified

## **新規実装**

- `src/path/to/new_file.rs` (XXX lines) - [説明]
- `src/path/to/another.rs` (XXX lines) - [説明]

## **修正ファイル**

- `src/existing/file.rs` - [変更内容]
- `README.md` - [更新内容]

## 🧪 Test Results

```bash
Total Tests: XXX ✅
├── Library Tests: XX passed ✅
├── Integration Tests: XX passed ✅
└── Doc Tests: XX passed ✅

Code Quality: 0 Clippy warnings ✅
```

## 🔒 Security Considerations

- [セキュリティ関連の変更があれば記載]
- [セキュリティテストの結果]

## 📊 Performance Impact

- [パフォーマンスへの影響]
- [ベンチマーク結果があれば]

## 🚨 Breaking Changes

**[あり/なし]** - [詳細説明]

## 🧭 Migration Guide

[必要に応じて移行ガイド]

## ✅ Pre-merge Checklist

- [ ] All tests passing ✅
- [ ] Zero Clippy warnings ✅
- [ ] Code formatting compliant ✅
- [ ] Documentation updated ✅
- [ ] Security review (if applicable) ✅

---

**Ready for review** 🚀
```

## 🏷️ ラベル付けルール

## PR重要度

- `priority: critical` - 緊急修正
- `priority: high` - 重要機能
- `priority: medium` - 通常機能
- `priority: low` - 軽微な改善

## PR種別

- `type: feature` - 新機能追加
- `type: bugfix` - バグ修正
- `type: docs` - ドキュメント更新
- `type: refactor` - リファクタリング
- `type: security` - セキュリティ関連
- `type: performance` - パフォーマンス改善

## 📋 レビュー基準

## 必須チェック項目

1. **機能性**: 実装が仕様通りに動作するか
2. **テストカバレッジ**: 適切なテストが含まれているか
3. **コード品質**: Clippy警告がないか、フォーマットが正しいか
4. **ドキュメント**: 必要なドキュメント更新が含まれているか
5. **セキュリティ**: セキュリティ上の問題がないか
6. **後方互換性**: 既存機能への影響がないか

## 推奨チェック項目

1. **パフォーマンス**: 性能への影響が適切か
2. **エラーハンドリング**: 例外処理が適切か
3. **ログ出力**: 適切なログレベルで出力されているか
4. **設定管理**: 新しい設定項目が適切に文書化されているか

## 🔄 CI/CDとの連携

## 自動チェック

- ビルド成功
- 全テスト通過
- Clippy警告なし
- フォーマットチェック通過
- セキュリティスキャン通過

## マージ条件

- CI/CDパイプライン成功
- 最低1名のレビュー承認
- コンフリクト解消済み
- ブランチ保護ルール遵守

---

このルールに従うことで、プロジェクトの品質と一貫性を保ちながら、効率的な開発を進めることができます。

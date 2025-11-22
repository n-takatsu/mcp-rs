# MCP-RS Documentation

このディレクトリには、MCP-RSプロジェクトの技術ドキュメントが含まれています。

## 📁 ディレクトリ構造

## `design/`

システム設計とアーキテクチャ仕様書

- `threat-intelligence.md` - 脅威インテリジェンス統合システムの設計仕様
- `rollback-system.md` - ロールバックシステムの設計仕様
- `secure-core-deployment-strategy.md` - セキュアコアデプロイメント戦略
- `secure-core-server-design.md` - セキュアコアサーバー設計

## `implementation/`

実装詳細とサマリー

- `threat-intelligence-summary.md` - 脅威インテリジェンス機能の実装サマリー
- `epic-15-completion-report.md` - EPIC 15完了レポート
- `policy-hot-reload-guide.md` - ポリシーホットリロード実装ガイド

## `api/`

API仕様書とリファレンス

- 将来のAPI仕様書がここに配置されます

## 📚 関連ドキュメント

プロジェクトルートにある主要ドキュメント:

- `README.md` - プロジェクト概要（英語版）
- `README.ja.md` - プロジェクト概要（日本語版）
- `CONTRIBUTING.md` - 貢献ガイドライン
- `CHANGELOG.md` - 変更履歴
- `ROADMAP.md` - 開発ロードマップ
- `RELEASE_NOTES.md` - リリースノート
- `VERSION_MANAGEMENT.md` - バージョン管理指針
- `WORDPRESS_BLOG_SERVICE_GUIDE.md` - WordPressブログサービスガイド
- `TEST_REPORT.md` - テストレポート

## 🎯 ドキュメント管理指針

## 設計仕様書 (`design/`)

- 新機能の設計時に作成
- アーキテクチャの決定事項を記録
- レビューと承認プロセスの対象

## 実装サマリー (`implementation/`)

- 機能実装完了時に作成
- 技術的詳細と実装のポイントを記録
- 今後の拡張や保守の参考資料

## API仕様書 (`api/`)

- 外部向けAPIの仕様を記録
- エンドポイント、パラメータ、レスポンス形式
- 開発者向けリファレンス

## 📝 ドキュメント作成ガイドライン

1. **明確性**: 技術的な内容を分かりやすく説明
2. **完全性**: 必要な情報を漏れなく記載
3. **更新性**: 実装変更に合わせて適時更新
4. **構造化**: 一貫した形式とテンプレートの使用
5. **検索性**: 適切なタイトルとキーワードの使用

## 🔄 更新プロセス

1. **設計フェーズ**: `design/`に仕様書作成
2. **実装フェーズ**: コードと並行してドキュメント更新
3. **完了フェーズ**: `implementation/`にサマリー作成
4. **リリースフェーズ**: ルートの関連ドキュメント更新

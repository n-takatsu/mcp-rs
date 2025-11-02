# GitHub Desktop を使用したワークフロー

## GitHub Desktop のメリット

### ✅ 解決される問題
- **ファイルロック問題**: Windowsでの適切なファイル処理
- **コマンドエラー**: GUIによる安全な操作
- **ブランチ切り替え**: クリック一つで完了
- **VS Code統合**: 自動同期とリアルタイム更新

### ✅ 推奨ワークフロー

#### 1. GitHub Desktop でのブランチ操作
```
Current branch: develop → mainに切り替え
1. GitHub Desktop を開く
2. "Current branch" をクリック
3. "main" を選択
4. エラーなしで即座に切り替え完了
```

#### 2. 新機能開発
```
1. GitHub Desktop で "New branch" をクリック
2. "feature/your-feature" と入力
3. "develop" から作成を選択
4. VS Code で開発作業
5. GitHub Desktop で変更をコミット
```

#### 3. マージ操作
```
1. GitHub Desktop で develop ブランチに切り替え
2. "Merge into current branch" をクリック
3. feature ブランチを選択
4. 自動マージまたはコンフリクト解決画面
```

## VS Code との統合

### Source Control パネル
- VS Code の Source Control で GitHub Desktop の操作が反映
- リアルタイムでブランチ状態が更新
- コミット履歴の視覚的確認

### 自動同期
- GitHub Desktop でブランチ切り替え → VS Code が自動更新
- ファイルロックなしで瞬時に完了
- コンフリクトがあれば事前に警告

## 現在の問題の即座解決

### 手順
1. **GitHub Desktop をダウンロード・インストール**
   - https://desktop.github.com/
   
2. **リポジトリを GitHub Desktop で開く**
   ```
   File → Add Local Repository → mcp-rs フォルダを選択
   ```

3. **ブランチ切り替えテスト**
   ```
   Current branch: develop → main → develop
   エラーなしで瞬時に完了
   ```

4. **VS Code との確認**
   ```
   VS Code でファイルが正しく更新されることを確認
   ```

## 比較: コマンドライン vs GitHub Desktop

| 操作 | コマンドライン | GitHub Desktop |
|------|---------------|----------------|
| ブランチ切り替え | `git switch main` + エラー | クリック一つ |
| 新ブランチ作成 | `git switch -c feature/name` | GUI で簡単入力 |
| コミット | `git add . && git commit -m "..."` | チェックボックスで選択 |
| マージ | `git merge` + 手動コンフリクト解決 | 視覚的マージツール |
| 履歴確認 | `git log --graph` | 美しいグラフ表示 |

## 推奨設定

### GitHub Desktop 設定
```
Preferences → Git
✅ Use Git Credential Manager
✅ Ask where to save changes for uncommitted changes when switching branches
```

### VS Code 設定
```json
{
    "git.enableSmartCommit": true,
    "git.autofetch": true,
    "git.confirmSync": false
}
```
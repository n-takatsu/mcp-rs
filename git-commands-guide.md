# 適切なGitコマンドガイド

## 古いコマンド vs 新しいコマンド

### ❌ 使わないでください (古い)
```bash
git checkout develop          # ブランチ切り替え
git checkout -b new-feature   # 新ブランチ作成+切り替え
git checkout HEAD -- file.txt # ファイル復元
```

### ✅ 使用推奨 (新しい)
```bash
# ブランチ切り替え
git switch develop

# 新ブランチ作成+切り替え
git switch -c new-feature

# ファイル復元
git restore file.txt
git restore --staged file.txt  # ステージングから除外
```

## 現在のワークフロー修正

### 1. ブランチ切り替え
```bash
# mainブランチに移動
git switch main

# developブランチに移動  
git switch develop
```

### 2. 新機能開発
```bash
# developブランチから新機能ブランチを作成
git switch develop
git switch -c feature/new-feature

# 作業完了後
git add .
git commit -m "feat: Add new feature"
git switch develop
git merge feature/new-feature
```

### 3. ファイル操作
```bash
# ファイルを最新コミット状態に復元
git restore filename.txt

# ステージングエリアから除外
git restore --staged filename.txt

# 全ての変更を破棄
git restore .
```

## Windowsでのファイルロック対処法

### VS Codeを一時的に閉じる
```bash
# VS Codeが開いている場合は閉じてから実行
git switch main
```

### 強制クリーンアップ (注意して使用)
```bash
# 追跡されていないファイルを削除
git clean -fd

# より強力なクリーンアップ (要注意)
git reset --hard HEAD
git clean -fdx
```

## 推奨されるGitワークフロー

1. **開発開始**
   ```bash
   git switch develop
   git pull origin develop
   git switch -c feature/your-feature
   ```

2. **開発中**
   ```bash
   git add .
   git commit -m "feat: Your changes"
   ```

3. **開発完了**
   ```bash
   git switch develop
   git merge feature/your-feature
   git branch -d feature/your-feature
   ```

4. **リリース準備**
   ```bash
   git switch main
   git merge develop
   git tag v1.0.0
   ```
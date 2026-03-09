#!/bin/bash
# Edit/Write 後に、編集先ディレクトリの CLAUDE.md チェックリストを出力する

input=$(cat)
ROOT=$(git rev-parse --show-toplevel 2>/dev/null)
if [ -z "$ROOT" ]; then exit 0; fi

# file_path を抽出（grep -oP を避け sed のみで処理）
file_path=$(echo "$input" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)

# Windows パス（C:\...）を正規化
file_path=$(echo "$file_path" | sed 's|\\\\|/|g; s|\\|/|g; s|^[Cc]:|/c|')

# sed 範囲抽出から末尾の次セクション見出し行を除く（head -n -1 を避け POSIX 互換）
trim_last() { sed '$d'; }

if echo "$file_path" | grep -q 'snotra-core/'; then
  echo "── snotra-core 実装前チェック ──"
  sed -n '/^## 実装前チェック（必須）/,/^## /p' "$ROOT/snotra-core/CLAUDE.md" | trim_last
  if echo "$file_path" | grep -q 'search\.rs'; then
    echo ""
    echo "search.rs 変更: /cache-check で incremental 単調性を検証してください"
  fi

elif echo "$file_path" | grep -q 'src-tauri/'; then
  echo "── src-tauri 注意事項 ──"
  sed -n '/^## WebView2 ウィンドウ生成の制約/,/^## /p' "$ROOT/src-tauri/CLAUDE.md" | trim_last
  echo "..."
  sed -n '/^## Win32 \/ Tauri 注意事項/,/^$/{ /^## /d; p; }' "$ROOT/src-tauri/CLAUDE.md" | head -5

elif echo "$file_path" | grep -qE 'ui/src/'; then
  echo "── ui 実装パターン ──"
  sed -n '/^## 実装パターン$/,/^## /p' "$ROOT/ui/CLAUDE.md" | trim_last
  echo ""
  sed -n '/^## Blob URL 管理の不変条件/,/^## /p' "$ROOT/ui/CLAUDE.md" | trim_last

elif echo "$file_path" | grep -q 'snotra-settings/'; then
  echo "── snotra-settings 注意点 ──"
  sed -n '/^## egui 実装の注意点/,/^## /p' "$ROOT/snotra-settings/CLAUDE.md" | trim_last
fi

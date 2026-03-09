#!/bin/bash
# Edit/Write 後に、編集先ディレクトリの CLAUDE.md チェックリストを出力する

input=$(cat)
ROOT="/c/workspace/Snotra"

# file_path を抽出
file_path=$(echo "$input" | grep -oP '"file_path"\s*:\s*"[^"]*"' | head -1 | sed 's/.*"file_path"\s*:\s*"//;s/"$//')

# Windows パス（C:\...）を正規化
file_path=$(echo "$file_path" | sed 's|\\|/|g' | sed 's|^C:|/c|i')

if echo "$file_path" | grep -q 'snotra-core/'; then
  echo "── snotra-core 実装前チェック ──"
  sed -n '/^## 実装前チェック（必須）/,/^## /p' "$ROOT/snotra-core/CLAUDE.md" | head -n -1
  if echo "$file_path" | grep -q 'search\.rs'; then
    echo ""
    echo "search.rs 変更: /cache-check で incremental 単調性を検証してください"
  fi

elif echo "$file_path" | grep -q 'src-tauri/'; then
  echo "── src-tauri 注意事項 ──"
  sed -n '/^## WebView2 ウィンドウ生成の制約/,/^## /p' "$ROOT/src-tauri/CLAUDE.md" | head -n -1
  echo "..."
  sed -n '/^## Win32 \/ Tauri 注意事項/,/^$/{ /^## /d; p; }' "$ROOT/src-tauri/CLAUDE.md" | head -5

elif echo "$file_path" | grep -qE 'ui/src/'; then
  echo "── ui 実装パターン ──"
  sed -n '/^## 実装パターン$/,/^## /p' "$ROOT/ui/CLAUDE.md" | head -n -1
  echo ""
  sed -n '/^## Blob URL 管理の不変条件/,/^## /p' "$ROOT/ui/CLAUDE.md" | head -n -1

elif echo "$file_path" | grep -q 'snotra-settings/'; then
  echo "── snotra-settings 注意点 ──"
  sed -n '/^## egui 実装の注意点/,/^## /p' "$ROOT/snotra-settings/CLAUDE.md" | head -n -1
fi

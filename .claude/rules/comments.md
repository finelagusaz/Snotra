---
paths:
  - "snotra-core/**/*.rs"
  - "snotra-egui-runtime/**/*.rs"
  - "snotra-settings/**/*.rs"
  - "src-tauri/**/*.rs"
---

# コメントの書き方（ルーター）

正本は `docs/comment-guidelines.md`。本 rule は「どこを読むか・何を実行するか」だけを示す（要約を置かない）。

## 読む正本

- 何を書き、何を書かないか: `docs/comment-guidelines.md`「第一原則」
- `///` / `//!` の様式・見出し構造: `docs/comment-guidelines.md`「rustdoc の様式」
- 改行位置と折返し: `docs/comment-guidelines.md`「日本語の折返し」
- 日英の選択と訳語: `docs/comment-guidelines.md`「言語（日英）」

## トリガー → 検査

- doc コメント（`///` / `//!`）を追加・変更したら `docs/build-commands.md`「変更後の検証チェックリスト」カテゴリ A の `cargo doc` 行を手で走らせる（intra-doc link 切れは **CI でのみ発火し PostToolUse hook は沈黙する**。コマンド本体を写さないのは、フラグが SSOT 側で変わったとき**古い形を走らせて「済んだ」と読む**経路を作らないため）

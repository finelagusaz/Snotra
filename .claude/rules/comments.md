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
- 長くなったコメントをどう畳むか・測定値の置き場: `docs/comment-guidelines.md`「短く保つ」
- 名指しの記法と、面ごとの保証: `docs/comment-guidelines.md`「名指しと正本の指名」
- `///` / `//!` の様式・見出し構造: `docs/comment-guidelines.md`「rustdoc の様式」
- 改行位置と折返し: `docs/comment-guidelines.md`「日本語の折返し」
- 日英の選択と訳語: `docs/comment-guidelines.md`「言語（日英）」

## トリガー → 検査

- doc コメント（`///` / `//!`）を追加・変更したら `docs/build-commands.md`「変更後の検証チェックリスト」カテゴリ A の `cargo doc` 行を手で走らせる（intra-doc link 切れは **CI でのみ発火し PostToolUse hook は沈黙する**。コマンド本体を写さないのは、フラグが SSOT 側で変わったとき**古い形を走らせて「済んだ」と読む**経路を作らないため）
  - **ただし `#[cfg(test)]` 配下の doc は `cargo doc` の視界の外である。** rustdoc がコンパイルしないため、壊れた intra-doc link を植えても **exit 0・診断 0 行**で通る（#1201 で対照つきに実測。`--document-private-items` を付けても変わらない）。**そこでの緑は「link が健全」ではなく「何も走らなかった」である**——ソーステキスト検査のように doc の厚い `mod tests` を触ったときに効く。**確かめたいなら壊れた link を 1 本植えて診断が出ることを見る**（出なければその経路は測っていない）。見出し参照と ADR の短縮引用だけは `npm run governance:check` が別に見る

---
paths:
  - "snotra-core/**/*.rs"
  - "snotra-egui-runtime/**/*.rs"
  - "snotra-settings/**/*.rs"
  - "src-tauri/**/*.rs"
---

# コメントの書き方（ルーター）

正本は `docs/comment-guidelines.md` である。この rule は「どこを読むか・何を実行するか」だけを示す（要約は置かない）。

## 読む正本

- 何を書き、何を書かないか: `docs/comment-guidelines.md`「第一原則」
- 長くなったコメントの縮め方と、測定値の置き場所: `docs/comment-guidelines.md`「短く保つ」
- 名指しの記法と、面ごとの保証: `docs/comment-guidelines.md`「名指しと正本の指名」
- `///` / `//!` の様式と見出しの構造: `docs/comment-guidelines.md`「rustdoc の様式」
- 改行の位置と折返し: `docs/comment-guidelines.md`「日本語の折返し」
- 日英の選び方と訳語: `docs/comment-guidelines.md`「言語（日英）」

## トリガー → 検査

- doc コメント（`///` / `//!`）を追加・変更したら、`docs/build-commands.md`「変更後の検証チェックリスト」カテゴリ A の `cargo doc` 行を手で走らせる。intra-doc link 切れを自動で検出するのは CI だけだからである。コマンドをここに写さないのは、SSOT 側でフラグが変わったときに、古い形を走らせて「済んだ」と読むことを防ぐためである。
  - ただし、`#[cfg(test)]` の下にある doc は `cargo doc` で検査されない。rustdoc がそこをコンパイルしないので、壊れた intra-doc link を入れても exit 0・診断 0 行で通る（#1201 で対照を置いて実測した。`--document-private-items` を付けても変わらない）。そこで緑になっても、「link が健全」という意味ではなく、「何も走らなかった」という意味である。この点は、ソーステキスト検査のように doc の厚い `mod tests` を触ったときに関係する。確かめたいときは、壊れた link を 1 本入れて、診断が出ることを見る（出なければ、その経路は測れていない）。見出し参照と ADR の短縮引用だけは、`npm run governance:check` が別に検査する。

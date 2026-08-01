# 独立レビュー — #858（観点: 既存検出器の挙動 / 変更ファイルの漏れ）

## 要対処

- **`docs/hooks.md:46` が計画の変更ファイル一覧（7 件）に含まれておらず、実装後に古くなる** — 同ファイル `:40-46` の「PostToolUse（post-edit.mjs）の発火一覧」表は
  ```
  | `*.rs` | clippy（各 Rust crate 配下ではその crate のテストも） |
  ```
  と書かれており、`:42` で「**正本は `selectChecks` である。** 下は現在の割り当てを読むための索引」と自己申告している——つまり `selectChecks` の**写し**である。計画の Phase 2 で `selectChecks` に `fmt` を足すと（`.claude/hooks/post-edit.mjs` の `selectChecks` を実測で確認・`:118-143`）、この行は「`*.rs` → clippy のみ」という誤った記述のまま取り残される。
  この写しがガバナンス検査で捕捉されないことも確認済み: `G-hook-commands`（`scripts/governance-check.mjs:614-658`）が照合するのは `docs/build-commands.md` のカテゴリ A だけで `docs/hooks.md` を見ない。`G-references`/`G-heading-refs` は参照先の実在・見出しの着地だけを見る構文チェックであり、表の内容（"clippy のみ" が事実と合っているか）という意味整合は判定しない。**ゆえに `npm run governance:check` は緑のまま、`docs/hooks.md` だけが黙って古くなる。**
  この失敗パターンはこのリポジトリで既に一度起きている——`scripts/governance-check.mjs:710-712` の `AREA_BUDGET` コメントが「`CLAUDE.md`『フック』表の PostToolUse 発火条件は `selectChecks` の写しで、実際にドリフトした履歴がある（#474〜#497）——一覧を `docs/hooks.md` へ退去させ」と記す、その退去先の文書自身が今回同じ形で古くなる。
  → 実装時は `docs/hooks.md:46` の行も `.claude/hooks/post-edit.mjs` / `.claude/hooks/post-edit.test.mjs` / `docs/build-commands.md` と同じコミットで直す（計画の「変更ファイルと対象シンボル」表へ 8 番目の行として追加すべき）。

- **`.claude/hooks/post-edit.test.mjs` の `.rs` 期待配列を直す箇所は 6 件ではなく最低 8 件——計画の対象行リスト（`:103,107,116,120,124,125`）に `:232` と `:458` が漏れている** — 実際に `.claude/hooks/post-edit.mjs` の `selectChecks` へ計画どおり `if (isRust) checks.push("fmt");` を（`clippy` push の前へ）挿入した状態で、使い捨て worktree（`git worktree add --detach` した `HEAD`）上で実測した:
  ```
  checksForPayload({file_path: ".../src-tauri/src/main.rs", old_string:"snotra-core", ...}, fakeResolver)
    実測: ["fmt","clippy","tauri-test"]
    post-edit.test.mjs:232 の既存期待値: ["clippy","tauri-test"]   ← 不一致・test 失敗

  resolveTarget({file_path: ".../src-tauri/src/main.rs"}, fakeResolver)
    実測: {root, rel:"src-tauri/src/main.rs", ids:["fmt","clippy","tauri-test"]}
    post-edit.test.mjs:458 の既存期待値: ids:["clippy","tauri-test"]   ← 不一致・test 失敗
  ```
  どちらも `.rs` ファイルを経由するテストで、計画が挙げた 6 箇所（`selectChecks` を直接呼ぶ `it` ブロック）とは別に、`checksForPayload`（`:232`）と `resolveTarget`（`:458`）を経由する `it` ブロックにも `.rs` 期待配列が埋め込まれている。
  Phase 2 の検証コマンド `npm test` を実行すれば `BUDGETS 完全性カナリア` 同様この 2 件も赤になり実装は止まるため**沈黙する経路ではない**が、計画の「対象シンボル」表が「6 箇所」と明記している以上、実装前にこの数を訂正しておくべき（研究の `research.md` 側の変更対象表 `:70` も同じ 6 行のみを挙げており、同じ漏れを共有している）。

## 軽微

- `.claude/rules/src-tauri.md:28` に「post-edit hook は A（clippy/test）だけで」という parenthetical 記述があるが、これは「hook が担うのはカテゴリ A（C の smoke ではない）」という区分の説明であって A の中身を網羅列挙する意図ではないため、fmt 追加で意味が壊れるとは言えない。直すなら「A（clippy/test/fmt）」への更新で足りる程度の軽微な陳腐化。

## 未検証

- `docs/adr/ADR-rustfmt-gate.md` の実際の本文冒頭が `G-adr-file-names`（`scripts/governance-check.mjs:1396-1430`）の要求する `# ADR-rustfmt-gate: <題>` 形（stem と見出しの完全一致）で書かれるかは、計画がまだ本文を書いていないため確認できない。ただし Phase 3 の最終ステップに `npm run governance:check` があるため、書き漏らしても沈黙はしない（赤になって実装中に気づく）。
- CI 側フォールトインジェクション（Phase 4「CI: 未整形の 1 ファイルを含むコミットを PR ブランチへ push」）が実際に rust-check を赤くするかは、push を要するため本レビューでは実行していない（計画の「セルフレビュー」節でも同様に未実測と自認している）。

## 検証の方法（付記）

- `G-hook-commands` / `G-ci-table` / `BUDGETS 完全性カナリア` の 3 点は、`git worktree add --detach HEAD` で作った使い捨てツリー上に計画どおりの変更（`cargo fmt --all` 63 ファイル実適用＋`post-edit.mjs` への `fmt` case 追加＋`docs/build-commands.md` カテゴリ A 行・CI 対応表行の追加＋`ci.yml` への `components: rustfmt` と `cargo fmt` step 追加）を実際に適用し、`node scripts/governance-check.mjs` を実行して確認した。結果は「全検査 passed（検査 18 件…）」——計画の 3 主張（片方向照合で通る／CI 対応表が verbatim 一致で通る／`selectChecks` へ `fmt` を足しても `BUDGETS` へ足し忘れればカナリアが赤くなる）はいずれも実測で裏付けが取れた。カナリア相当のロジックを `BUDGETS`/`selectChecks` から直接呼んで、`fmt` の予算未定義時に赤・定義後に緑になることも確認した。作業ツリーは検証後に `git worktree remove --force` で撤去済み（対象ブランチ・`workspace/` 配下は未編集）。

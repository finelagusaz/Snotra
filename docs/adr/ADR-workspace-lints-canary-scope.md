# ADR-workspace-lints-canary-scope: workspace lints カナリアの母集団と、必須 lint の名指し

## 文脈

#713 で `governance:check` に `G-workspace-lints` を足すにあたり、2 点で選択肢が割れた。どちらも「どの入力に対して緑を返すか」を決める判断であり、誤ると**検査が在るのに沈黙する**（#706 の再発を、検査済みという安心付きで見逃す）。

判断はすべて cargo 1.94.0 に対する `cargo doc --workspace --no-deps --document-private-items` の exit code の実測で行った。

## 決定

1. **母集団は全 workspace member とする**（`src/lib.rs` の有無で分岐しない）。
2. **ルート側の検査は必須 lint 名（`broken_intra_doc_links` / `invalid_html_tags`）を名指しする**。

## 検討した代替案と却下理由

- **母集団を「`src/lib.rs` を持つ member」に絞る**（bin-only crate に doc lint を課す意味があるか、という #713 の提起）: 却下。実測で bin-only crate でも deny は現に効く（`src/main.rs` の `//!` の切れリンクで exit 101）。一方で `src-tauri` と `snotra-settings` に `lib.rs` は無く、この案は**製品本体を含む 4 member 中 2 件を母集団から落とす**。分岐は守る対象を減らすだけで、腐りうる述語を 1 つ増やす。
- **ルート側を見ない（member 側の opt-in だけを検査する）**: 却下。member が全員 opt-in していても、ルートの `[workspace.lints.rustdoc]` を `warn` へ降格する・空にする・必須 lint の行を消すの 3 形はいずれも exit 0 で沈黙する。member 側だけの検査は #706 と同一の再発 1 パターンだけを止め、**同じ結果をもたらす 3 パターンを素通しにしたうえで「lints は検査済み」という印象を与える**。
- **ルート側を「`[workspace.lints.rustdoc]` が非空かつ全エントリ deny」だけで判定する**（lint 名を名指しせず、カテゴリ指定に留めて写しを避ける案）: 却下。実測で、`broken_intra_doc_links` の行だけが消え `invalid_html_tags = "deny"` が残る形（表を編集して 1 行消すだけで起きる）を**緑で通す**。「写しを増やさない」は SSOT の内容の写しに掛かる原則であって、**カナリアが「消えたら困る識別子」そのものを持つのは正しい形**である（先例: `.claude/hooks/post-edit.test.mjs` が member 名 4 件をハードコードし、意識的な更新を強制している）。
- **`[workspace.lints.*]` の全カテゴリへ広げる**: 却下。`[workspace.lints.clippy] all = "warn"` はごく普通の設定であり、このリポジトリの clippy は `cargo clippy ... -- -D warnings` がコマンドライン側で昇格させていて workspace テーブルが担っていない。広げると正当な設定が赤になり、**次の人の最も安い直し方が「検査を緩める」になる**。
- **`members` の glob（`crates/*`）を展開する**: 却下（YAGNI）。現に 0 件であり、入れば「母集団の欠落」として赤で気づく。展開器は沈黙しない問題のために腐る述語を増やす。
- **root に `[workspace.lints]` が無い形・member の `workspace = false` も検査する**: 却下。どちらも cargo が manifest エラーにする＝**沈黙しない**。放っておいても明示的に失敗するものに見張りは要らない。

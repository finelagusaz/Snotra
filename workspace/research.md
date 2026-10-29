# 調査 — issue #1284 フォルダ絞り込みが SPEC §4.2 の空白正規化を行わない

## issue の要約

`SPEC.md`「4.2 クエリ正規化」は、クエリ**およびエントリ名**を「前後空白除去・小文字化・連続空白の1文字化・アクセント折りたたみ」で正規化すると定め、射程を「全検索モード・フォルダ展開共通」とする。フォルダ展開の絞り込みは小文字化とアクセント折りたたみ（`to_lower_folded`）しか当てていない。

**要求判断（ユーザー回答 2026-10-29）**: 「両側を正規化 (推奨)」——絞り込み文字列とエントリ名の両方に同じ正規化を掛ける。本体検索の名前側の空白（下の「範囲外」）は別 issue に切り出す。

分類: **バグ**（SPEC の記述に合わせる）。`SPEC.md` の変更は不要。

## 関連ファイル・シンボル

- `snotra-core/src/folder.rs`
  - `read_dir_entries`（L39-）: `to_lower_folded(filter)` で `filter_lower` と `needle` を作り、`!filter.is_empty() && !matches_filter(..)` で落とす（L57）。**空判定が生の `filter` に掛かっている**
  - `filter_sorted`（L195-）: `filter.is_empty()` なら先頭 `max` 件（L201）。それ以外は `to_lower_folded(filter)` で絞る
  - `matches_filter`（L226-）: `to_lower_folded(name)` を Prefix(`starts_with`) / Substring(`contains`) / Fuzzy(`fuzzy_match_score`) へ渡す
  - `build_sort_keys`（L105-）: ソートキーにも `to_lower_folded(&e.name)` を使うが、**絞り込みとは独立**（フィルタ非依存の確定順序・`//!`）。今回触らない
  - テスト内の `read_dir_entries_for_test`（L748-）: `read_dir_entries` のフィルタ段の写し。製品側を変えたら同じ形へ揃える
- `snotra-core/src/query.rs`
  - `to_lower_folded`（L22）: 小文字化 + `nucleo_normalize`。空白に触れない
  - `normalize_query`（L152）: trim → 空なら `""` → 大文字・アクセント・非 `' '` 空白・連続空白があれば所有、無ければ借用。所有側は各空白連を `' '` 1 つへ、非空白は `to_lowercase().map(nucleo_normalize)`
- `snotra-core/src/engine.rs`: `FolderListContext::filter_sorted`（L46）が `folder::filter_sorted` を呼ぶ。`Engine::list_folder`（L204）が `folder::list_folder` を呼ぶ
- `src-tauri/src/egui_shell/launcher_controller/search_flow.rs`: 打鍵で `set_folder_filter(buf)`（L257）→ `run_search` → `ctx.filter_sorted(sorted, self.state.folder_filter())`（L76）
- `src-tauri/src/egui_shell/search_state.rs`: `folder_filter` は**入力欄に表示される生の文字列**でもある（`view.rs` L770 が読む）。ゆえに正規化は状態へ書き込む時点ではなく、照合側（core）で行う

## 再利用できる既存パターン

- 検索側 `search/query_plan.rs` `prepare_query_plan`（L78）: `normalize_query(query)` → 空なら `None` → `Utf32String::from(norm_query)` を needle にする。フォルダ側も同じ形にする
- `normalize_query` の非空白文字の扱いは `to_lower_folded` と同じ（`to_lowercase` → `nucleo_normalize`）。**`to_lower_folded` を `normalize_query` へ置き換えると、空白以外の挙動は変わらない**（例外は下の「未解決の疑問」）

## 技術的制約

- 名前側の正規化は打鍵ごと・エントリごとに走る（キャッシュ済みの `SearchResult` に正規化済みの列は無い）。現状も `to_lower_folded` がエントリごとに `String` を確保しているので、`normalize_query` に替えても確保回数は増えない（既に小文字・アクセント無し・単一 `' '` の名前では借用になり、むしろ減る）
- 空白だけの絞り込み（`"   "`）は正規化後に空になる。検索側は空クエリを「空結果」とするが、フォルダの空絞り込みは「全件」（L201）なので、**空白だけの絞り込みは全件表示になる**。現状は空白で始まる名前だけが残る（ほぼ 0 件）。SPEC §4.2 の「前後空白除去」の帰結として受け入れる
- Fuzzy の needle に `' '` が残る（`a b`）。検索側と同じで、nucleo は空白を通常の文字として照合する
- 名前は表示に使うので、正規化は照合用の一時値だけに掛け、`SearchResult.name` は変えない

## 範囲外（別 issue へ切り出す）

- 本体検索の名前側も `to_lower_folded` だけで空白を正規化していない（`search/build.rs` L117・`indexer/columns.rs` L185 `derive_columns`）。SPEC §4.2 は「エントリ名も」と言っており同型のずれである。直すと `lower_names` の意味が変わり `index.bin` の版バンプと移行を要するため、この issue では触らない

- 「直すと `index.bin` の版バンプが要る」は**未実測**（敵対的調査 W3）。要否は別 issue で決める

## 未解決の疑問

- [x] 名前側も正規化するか — ユーザー回答「両側を正規化」
- [x] titlecase の割れ — **実在を実測した**（scratch crate・2026-10-29）: `normalize_query("ǅ")="ǅ"` / `"aǅ"→"aǅ"` / `"Aǅx"→"aǆx"`、`to_lower_folded` は 3 つとも `ǆ`。`is_uppercase()` が偽で `to_lowercase()` が変わる文字は敵対的調査の全数走査で 31 字（Lt の `ǅ ǈ ǋ ǲ` と ギリシャ文字の iota 付き Lt 群。表示は先頭 20 字までで、残りは範囲からの推定）。**`snotra-core/CLAUDE.md`「`query.rs` の正規化と `Cow<str>` 遅延アロケーション」の契約（チェック条件とビルド条件が同じ結果）を `normalize_query` 自身が破っている**。直すと履歴のクエリキー（`normalize_history_query_key`）の正規化も変わる＝永続キーの変更なので、この issue では扱わず別 issue へ。フォルダ側はこの 31 字を含む名前と絞り込みで、今は一致するものが一致しなくなりうる（例: 名前 `Aǅx`・絞り込み `aǅ` の Substring）。実害は無視できると判断し、PR 本文に残余として書く

## 敵対的調査（`workspace/adversarial-1284.txt`）の採否

壊せた:

- **B1 採用**: 製品で生きている照合経路は `search_flow.rs` L76 → `FolderListContext::filter_sorted` → `folder::filter_sorted` だけ。`read_dir_entries` の製品呼び出しは `folder_nav.rs` L76 の `""` のみで、フィルタ枝は製品では走らない。`Engine::list_folder` に呼び出し元は無い（自分で grep して確認）。等価テスト（`unlimited_filter_take_equals_score_entries`）は製品の `read_dir_entries` ではなく写し `read_dir_entries_for_test` と比べている → テストの主対象を `filter_sorted` にし、`read_dir_entries` は同じ関数を通すことだけを揃える
- **B2 採用**: 実 `config.toml` は `normal_mode = "substring"` / `folder_mode = "fuzzy"`（自分で読んで確認）。新テストは 3 モードすべてで走らせ、Fuzzy を必ず含める

壊せなかった: 争点 2（空白以外の差は上の 31 字だけ）・3（正規化を `set_folder_filter` に置くと末尾空白が打てなくなる）・4（確保は増えない）・6 の前半（検索の名前側も空白未正規化）・他経路の不在・行番号。

⚠️ の採否:

- W1（空白だけの絞り込みは全件・検索側は空結果で逆）: **受け入れる**。フォルダの空絞り込み＝全件は既存の意味であり、SPEC §4.2 の前後空白除去の帰結。PR 本文に書く
- W2（`foo ` ≡ `foo`）: 受け入れる。PR 本文に書く
- W3: 上の「範囲外」へ反映
- W4: 上の通り実測し、別 issue へ
- **W5 採用**: 空判定を生 `filter` のまま残しても、空の needle は全モードで全件一致するので出力は同じ（Prefix/Substring は空文字列に一致、nucleo は空 needle で即返る）。**空判定の修正を捕まえるテストは書けない**——テストが固定すると主張しない。修正は「早期 return を正しく通す」整合のためだけに行う
- W7 採用: PR・コミットで「§4.2 を満たした」と書かない（検索の名前側が残る）
- W8 採用: 名前側のテストには Windows のファイル名に置ける空白（U+3000・NBSP・連続空白）を使う

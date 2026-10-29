# 計画 — issue #1284 フォルダ絞り込みに SPEC §4.2 の空白正規化を通す

## 目的と受け入れ条件

フォルダ展開の絞り込みで、絞り込み文字列とエントリ名の両方へ `SPEC.md`「4.2 クエリ正規化」の正規化（前後空白除去・小文字化・連続空白の1文字化・アクセント折りたたみ）を掛ける。判定は「バグ」（SPEC の記述に合わせる）。要求判断は `workspace/research.md`「issue の要約」（ユーザー回答「両側を正規化」）。

製品で生きている照合経路は `folder::filter_sorted` だけである（`workspace/research.md`「敵対的調査の採否」B1）。受け入れ条件は `filter_sorted` に対し、**Prefix / Substring / Fuzzy の 3 モードすべて**で成り立つ（実運用点の `folder_mode` は Fuzzy・B2）:

1. 末尾空白: 絞り込み `report ` が名前 `report.txt` に一致する
2. 先頭空白: 絞り込み ` report` が名前 `report.txt` に一致する
3. 連続空白（入力側）: 絞り込み `a  b` が名前 `a b.txt` に一致する
4. 連続空白（名前側）: 絞り込み `a b` が名前 `a  b.txt` に一致する。`a  b` → `a  b.txt` も一致する（現状一致しているものの退行防止）
5. 全角空白・NBSP（名前側）: 絞り込み `a b` が名前 `a　b.txt`（U+3000）と `a\u{a0}b.txt`（NBSP）に一致する
6. 空白だけの絞り込み（`"   "`）は空の絞り込みと同じく先頭 `max` 件を返す
7. 返る `SearchResult.name` は元の綴りのまま（正規化されない）
8. 既存テスト（大文字小文字無視・アクセント折りたたみ・表示名のみ照合・Fuzzy の結合文字）がすべて緑

`read_dir_entries` は製品ではフィルタ枝が走らないが、同じ `matches_filter` を通るので同じ正規化へ揃える。それを固定するテストとして `list_folder` 経由の末尾空白 1 件（Fuzzy）を置く。

## 変更ファイル一覧と対象シンボル

- `snotra-core/src/folder.rs`
  - `read_dir_entries`: `to_lower_folded(filter)` → `normalize_query(filter)`。早期の空判定（L57 `!filter.is_empty()`）を正規化後の値で行う
  - `filter_sorted`: 先に `normalize_query(filter)` を取り、空なら先頭 `max` 件。それ以外は正規化後の値で絞る
  - `matches_filter`: 名前側を `to_lower_folded(name)` → `normalize_query(name)`。引数 `filter_lower` → `filter_norm` に改名し、doc に「絞り込みと名前は同じ `normalize_query` を通す（片側だけ替えると一致が割れる）」と書く
  - import: `use crate::query::{normalize_query, to_lower_folded};`（`build_sort_keys` は `to_lower_folded` のまま——ソートは絞り込みと独立）
  - テストの写し `read_dir_entries_for_test`: 製品側と同じ形へ揃える
  - テスト追加（下の「テスト方針」）
- `SPEC.md`: 変更なし
- 新規 issue 1 件（下の Phase 3）

## 実装順序

1. 失敗するテストを `folder.rs` の `mod tests` に書き、Red を確認する
2. `read_dir_entries` / `filter_sorted` / `matches_filter` / `read_dir_entries_for_test` を変更する（Green）
3. 検証
4. 範囲外の別 issue を起票する

## 不変条件と異常系

- **絞り込みと名前は同じ関数（`normalize_query`）を通す**——`matches_filter` の doc に書く
- ソート順は絞り込みに依存しない（`build_sort_keys` は不変）
- 列挙失敗時のエラー行は絞り込みの対象外（`search_flow.rs` の `folder_error` 分岐は不変）
- 入力欄に表示する `folder_filter` は生のまま（`set_folder_filter` に正規化を置かない——置くと末尾空白が打てなくなる・research 争点 3）
- 受容する残余（PR 本文に書く）:
  - 空白だけの絞り込みは全件表示（本体検索は空結果で、両面の振る舞いが逆）
  - `normalize_query` の借用判定が titlecase 等の 31 字で所有側と割れる（`workspace/research.md`「未解決の疑問」）。フォルダ側は、これらを含む名前・絞り込みで今は一致する組が一致しなくなりうる
  - 本体検索の名前側は空白を正規化しないまま——**PR・コミットで「§4.2 を満たした」と書かない**

## テスト方針と検証コマンド

- 追加テスト（`snotra-core/src/folder.rs` `mod tests`）:
  - `filter_sorted` に対し受け入れ条件 1〜7 を 3 モードの表駆動で 1 本（`sr()` で fixture を作る既存の形を使う）
  - `list_folder`（`temp_dir_with_contents` の実ディレクトリ）に対し、Fuzzy で末尾空白の絞り込みが一致する 1 本——`read_dir_entries` 側の照合が同じ正規化を通ることを固定する
- **書けないテスト**: 空判定を生 `filter` に残す退行は出力を変えない（空の needle は全モードで全件一致する・research W5）。固定すると主張しない
- 検証: `npm run verify:tree`（`docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ判定を道具が行う）
- 変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う。候補: 名前側だけ `to_lower_folded` に戻す（条件 4・5 で落ちる）／絞り込み側だけ戻す（条件 1〜3 で落ちる）／`read_dir_entries` の絞り込み側だけ戻す（`list_folder` のテストで落ちる）

## 作業項目

### Phase 1 — テスト（Red）

- [ ] `filter_sorted` の 3 モード表駆動テストと `list_folder` の末尾空白テストを追加し、失敗を確認する

### Phase 2 — 実装（Green）

- [ ] `read_dir_entries` / `filter_sorted` / `matches_filter` を `normalize_query` へ切り替え、空判定を正規化後の値で行う
- [ ] `read_dir_entries_for_test` を製品側と同じ形へ揃える
- [ ] `matches_filter` の doc と引数名を更新する

### Phase 3 — 検証と後始末

- [ ] `npm run verify:tree` が緑
- [ ] 別 issue を起票する: (a) 本体検索の名前側の空白正規化（`search/build.rs`・`indexer/columns.rs` `derive_columns`。`index.bin` の版バンプ要否は未測定）、(b) `normalize_query` の借用判定が titlecase 等で所有側と割れる（履歴クエリキーへの影響ゆえ `/persistence-check` 対象）

## 未確定（実装前に潰す）

- [x] titlecase 文字で `normalize_query` の借用/所有が割れるか — 実測で割れる（`"aǅ"→"aǅ"`、`"Aǅx"→"aǆx"`）。履歴キーに触れるため別 issue へ。フォルダ側は残余として受け入れる
- [x] 敵対的調査の所見を反映する — B1・B2・W1〜W8 の採否は `workspace/research.md`「敵対的調査の採否」。B1 で検証の主対象を `filter_sorted` へ、B2 でテストを 3 モードへ、W5 で「書けないテスト」を明記した

## 人間レビュー

- [x] 承認済み — 2026-10-29 / 問い: "`workspace/plan.md` に注釈を書き込んでいただくか、明示的にご承認くださいまし。" / 回答: "承認します"

## セルフレビュー

- リスク: 通常（永続形式・並行性・状態遷移・ガバナンスに触れない。`folder.rs` の照合関数 3 つの内部だけ）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（敵対的調査）
- 要対処: 2 件（B1: 検証対象を生きた経路へ／B2: 実運用のモード Fuzzy を含む 3 モードへ）を反映
- 未検証: 空判定の修正（出力が変わらないためテストで固定できない・W5）
- 散文の全件列挙: 未実施（通常リスク）
- 試行（#1300・高リスクのみ）: 対象外（通常リスク）
- 照合: issue の要件（trim・連続空白）→ 条件 1〜3。SPEC の「エントリ名も」→ 条件 4・5。境界（空白だけ・名前の綴り保持）→ 条件 6・7。新しい状態・リソースは無い。単純な既存パターン（`normalize_query`）の再利用で済み、新しい関数は作らない。不変条件「両辺が同じ関数」は変異注入（片側だけ戻す）で検知する

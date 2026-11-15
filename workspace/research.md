# 調査: #1014 — `drain_index` の `begin_in` → `rebuild_and_save` の順序を CI で固定できるか

main = f533ab59。ブランチ `fix/scan-log-start-order`。

## issue の要約

- 本文が対象にしていた背景再スキャンの計器（`rescan-log.jsonl`）は #1023 で撤去済み。2026-11-15 のコメントが問いを移した: **`src-tauri/src/indexing.rs` の `drain_index` が `scan_log::begin_in`（`start` 行の追記）を `indexer::rebuild_and_save`（走査）より前に呼ぶことを CI で固定できるか**
- 不変条件の正本: `src-tauri/src/scan_log.rs` の `//!`「2 行 1 組」——走査の**前**に `start`、差し替えの後に `end`。`start` だけの行 = 完走しなかった構築
- 受け入れ（本文）: (1) ローカルと CI の両方で、素の実装が緑・順序を壊す変異で赤の実測記録 (2) CI 側は連続実行で flaky でないことの実測 (3) 割に合わなければクローズしてよい

## 実測（この調査で行った）

| # | 状態 | 結果 |
|---|---|---|
| R0 | 変異 M1 を入れ、既存スイートのみ（`cargo test -p snotra -q`） | **339 passed / 0 failed / 4 ignored**——変異は素通し。**変異前の基準件数は取っていない**（R1 の 340 = 339 + 試作 1 から、既存は 339 と推定） |
| R1 | 素の実装 + 試作検査 P | 340 passed（P 緑） |
| R2 | M1 + P | P が **assert で** FAILED（「drain_index が走査の後で start 行を書いている」・コンパイルは通った）。他 339 は緑 |
| R3 | M1 + P、`indexing.rs` を CRLF 化 | P が assert で FAILED |
| — | 素の実装 + P、CRLF 化 | **未測**（R3 の後に `git checkout` で試作ごと戻したため） |

- **変異 M1**: `let log = Config::config_dir().map(|dir| { … begin_in … })` のブロックを `rebuild_and_save(…);` の直後へ移す。`state.begin_scan_progress(log…)` は `log` を参照するのでコンパイルが通らず、**`begin_scan_progress(None)` へ差し替えることがコンパイル上強制された**（変異の一部）
- **試作 P**（撤去済み・作業ツリーに残していない）: `indexing.rs` の `mod tests` に既存の `top_level_fn_body(include_str!("indexing.rs"), "fn drain_index(", "begin_index_drain(")` で本体を切り出し、`//` で始まる行を除き、`scan_log::begin_in(` と `indexer::rebuild_and_save(` がそれぞれ**ちょうど 1 回**現れること・前者の位置が後者より前であることを assert する

## 関連するファイル・シンボル

- `src-tauri/src/indexing.rs`
  - `fn drain_index(app_handle: &AppHandle)`（列 0・非 pub）。1 反復で `begin_index_drain` → `Config::config_dir().map(|dir| … scan_log::begin_in(…))` → `begin_scan_progress` → `indexer::rebuild_and_save(…)` → `build_index_from_material` → `complete_index_drain` → `scan_log::append_in(end_line)`
  - `mod tests` の `top_level_fn_body(src, anchor, canary)`（列 0 アンカー・列 0 の `}` 終端・canary の 3 assert、改行コード非依存）と、その回帰テスト `top_level_fn_body_is_line_ending_agnostic` / `_rejects_a_population_without_a_terminator` / `_rejects_an_indented_anchor` / `_rejects_a_population_without_the_canary`。利用者は現在 `start_index_build_invalidates_the_icon_cache` の 1 本
- `src-tauri/src/scan_log.rs`: `begin_in`（prune → 分母の読み → start 追記）、テスト `begin_in_prunes_then_reads_the_denominator_then_appends_start`（`begin_in` 内部の順序だけを縛る）
- `snotra-core/src/indexer/cache.rs`: `pub fn rebuild_and_save`
- ADR: `docs/adr/ADR-source-text-probe-helper-locality.md`（サイト局所の helper）・`ADR-source-text-probe-predicate-polarity.md`（述語の極性で母集団への要求が裏返る）・`ADR-source-text-probes-not-lifted-to-types.md`
- `.github/workflows/ci.yml`: `on: push (main) / pull_request (main)` のみ。`workflow_dispatch` なし。`cargo test -p snotra` を Windows で実行

## 再利用できる既存パターン

- **ソーステキスト検査**（`start_index_build_invalidates_the_icon_cache`）と同じ helper・同じファイル。ADR-helper-locality に従い helper は共有せず同じ `mod tests` 内で使う（同ファイル内の再利用であり、新しい helper は不要）

## 技術的な制約

1. **issue 本文が想定した「走査中に別スレッドでログを poll する」形は、組めなくはないが割に合わない**（3b で訂正）。tauri 2.12.5 には `test` feature と `pub mod test`（`mock_app`）が在る（registry の `tauri-2.12.5/Cargo.toml` の `test = []`・`src/lib.rs` の `pub mod test;` を確認）。ただし `src-tauri` は使っておらず、使うには (a) dev-dependency に `tauri` の `test` feature を足す (b) `drain_index(&AppHandle)`（= `AppHandle<Wry>`）を `R: Runtime` へ一般化する (c) プロセス全体の env `SNOTRA_CONFIG_DIR` を並列テストと衝突させずに差し替える（未設定だと実ユーザーの `scan-log.jsonl` に書く）、が要る。走査を差し替える口は無いので、poll 型は時間競合も抱えたまま
2. 振る舞いで測るなら、反復の本体を `AppHandle` 抜きの関数へ切り出し、走査をクロージャで注入して「クロージャの中でログに `start` 行が在る」ことを読む形（候補 B）になる。**これは決定論的で時間に依存しない**が、`drain_index` の構造を変える。さらに、`drain_index` が切り出した関数を使わずに `begin_in` を直接呼ぶ迂回は、B の検査だけでは捕まらない（`begin_in` が `indexing.rs` から呼べる限り）
3. 候補 A（ソーステキスト検査）の入力はコンパイル時の `include_str!` だけで、時間・I/O・スレッドに依存しない。**環境差は改行コードだけ**で、helper が吸収する（R3 で CRLF の変異赤を実測）。ゆえに「連続 N 回で flaky でない」は構成上自明（同じバイナリは同じ文字列を見る）
4. CI で赤を実測するには、変異を含むコミットを main 向け PR に載せる必要がある（scratch ブランチの push だけでは CI が走らない）

## 候補の比較

| | A: ソーステキスト順序検査 | B: 反復本体の切り出し + 注入 |
|---|---|---|
| M1 で赤 | 実測済み（R2・R3） | 未測（実装していない） |
| 製品コードの変更 | なし | `drain_index` の再構成 |
| 時間依存・flaky | なし（構成上） | なし（注入なので） |
| 単独で捕まえる経路 | 本体内の並べ替え・`begin_in` の本体外への移設（綴り 0 回）・重複 | 切り出した関数の内部の並べ替え。`drain_index` が関数を迂回する経路は捕まえない |
| 死角 | テキストであって実行順ではない: `Option::map` を遅延評価（クロージャを保持して後で呼ぶ）に変える・マクロ経由・綴りを変えた別名 import | 迂回・切り出しの外側の並べ替え |

## 敵対的調査（3b・sonnet 1 体・`workspace/adversarial-1014.txt`）の所見と採否

| 命題 | 判定 | 採否と理由 |
|---|---|---|
| 1. `drain_index` のテストは書けない | **壊せた** | **採る**。制約 1 を「`mock_app` で組めるが、製品コードの一般化・dev-dep・env 衝突対策が要り割に合わない」へ訂正。registry のソースで自分で確認した |
| 2. 既存スイートは M1 を素通し | 壊せなかった（⚠️ 基準件数未測・M1 は `begin_scan_progress(None)` を含む） | 実測表に既に明記済み。計画で変異前の基準件数を取る |
| 3. A の偽緑経路は限られる | **壊せた**: 走査前の行末コメント／`/* */`／文字列に綴りを残し実呼び出しは別名で後ろ・走査前にクロージャを作り後で呼ぶ・`if false {}` / `#[cfg(any())]` で無効化 | **一部採る**。行末コメントは `//` 以降を落とす前処理で塞ぐ（`drain_index` 本体に `//` を含む文字列リテラルは無い——計画で確かめる）。残り（ブロックコメント・文字列・遅延クロージャ・死んだ分岐・別名）は**順序の偶発的な並べ替えではなく、検査を欺く意図的な書き換え**なので受容残余として検査の doc に書く。広がる側・狭まる側の母集団の壊れ方は「ちょうど 1 回」と canary で赤に倒れる（壊せなかった） |
| 3⚠️. `//` 除去が trim の前か後か未定 | ⚠️ | 採る。試作は `trim_start()` 後で判定していた。計画では「各行の `//` 以降を落とす」に一本化（行頭コメントも包含） |
| 4. 環境差は改行だけ・flaky は構成上ない | 半分壊せた: **CRLF で素の実装が緑かは未測** | **採る**。CI（windows-latest・autocrlf=true）が実際に走る組なので、計画の検証に入れる |
| 5. CI は main push / main 向け PR のみ | 壊せなかった（smoke.yml の `workflow_dispatch` は `cargo test` を走らせない） | そのまま |
| 6. B は迂回を捕まえない | 壊せなかった | そのまま |
| 測定環境 | `config_dir()` は env → `dirs::config_dir()/Snotra`。A は文字列だけを読み config_dir に依存しない（測る分岐と触る分岐は一致）。実機 `scan-log.jsonl` は start 5・end 4、start だけの 1 行あり（前提と矛盾しない・傍証） | 採る（A を選ぶ理由の補強） |

## 要求の裁定（ユーザー・2026-11-15）

- Q1 → 「CRLF ローカルで代替 (Recommended)」: CI は本 PR の緑だけを記録し、赤は CRLF 化したローカル変異で代える
- Q2 → 「課さない (Recommended)」: 入力がコンパイル時の文字列だけで flaky の機序が無いことを記録して代える

## 未解決の疑問

- Q1・Q2: 上の裁定で解消
- Q3: `begin_scan_progress` も走査より前に置かれている（進捗の初期化）。同じ検査で縛るか——**範囲外として計画に入れない**。必要なら別 issue
- Q4: 述語の極性（ADR-predicate-polarity）: 位置比較 `find(a) < find(b)` は、母集団が本体より**広がる**と、本体外の先行出現で真へ誤る可能性がある。helper の終端・canary の assert と「ちょうど 1 回」がこれを塞ぐかを確かめる
- Q5: コメント行除去（`//` 始まり）は、行末コメント（`code; // scan_log::begin_in(`）や doc 文字列内の綴りを除かない。それが偽緑を生む経路はあるか

# plan — issue #1027: `LoadOrScanStats` の total を `Duration` で運ぶ

調査は `workspace/research.md`。敵対的調査の全文は `workspace/adversarial-1027.txt`。
ブランチ: `chore/1027-load-or-scan-total-duration`

## 目的

`index_load_unattributed_ms` の非負性が乗る**前提 2（内外とも切り捨てであること）を、
構造で消す**。`LoadOrScanStats.total_ms: u128` を `total: Duration` にし、ミリ秒への
丸めを `startup.rs` の `to_ms` 1 か所へ寄せる。

**前提 1（外側の区間が内側を包む）は構造では消えない。** ハーネス
（`SnotraStartupContract.psm1`）の `>= 0` 検査は**残す**——この計画はその検査を減らさない。

範囲は `total` の 1 フィールドのみ（ユーザー判断・2026-08-25）。他の `*_ms` は触らない。

## 受け入れ条件

1. `LoadOrScanStats` に `total_ms: u128` が無く、`total: Duration` がある
2. **生成側が丸めない**——`snotra-core/src/indexer.rs` に `total` をミリ秒へ落とす行が無く、
   生成 3 か所すべてが `total_started.elapsed()` をそのまま入れる。
   （`memory_footprint.rs` の `as_millis()` は**そのテスト自身の表示境界**なので、この条件に反しない）
3. `index_load_unattributed_ms` の**出力値が変更前と一致する**（表示境界の丸めは 1 回のまま）
4. `to_json` が内側だけ別の丸めを通すようになったら**落ちるテスト**がある
5. `total_ms` という綴りが、`LoadOrScanStats` の意味では生きた文書・コードに 1 つも残らない
   （凍結文書 `docs/superpowers/**` と、無関係な `SNOTRA_EGUI_PAINT_TRACE` の `total_ms` を除く）
6. カテゴリ A・F の検証コマンドと Pester がすべて緑

## 変更ファイル一覧と対象シンボル（7 ファイル）

| # | ファイル | 対象 |
|---|---|---|
| 1 | `snotra-core/src/indexer.rs` | `LoadOrScanStats.total_ms` → `total: Duration`／生成 3 か所（778・819・872）／`use std::time::Instant;`（22 行）へ `Duration` 追加／struct doc（417-421）・`cache_save_ms` の doc（442）・3590 行の引用 |
| 2 | `snotra-core/tests/memory_footprint.rs` | 319・331 の `s.total_ms` → `s.total.as_millis()` |
| 3 | `src-tauri/src/main.rs` | 185（`eprintln!` の `s.total_ms` → `s.total.as_millis()`）／195（`as u64` を落として `result.stats.total` を渡す） |
| 4 | `src-tauri/src/startup.rs` | フィールド `index_load_stats_ms: Option<u64>`（277・287）→ `index_load_stats_total: Option<Duration>`／`Timeline::set_index_load_stats_ms`（314）と自由関数（494）→ `set_index_load_stats_total(Duration)`／`to_json` の当該アーム（412 付近）を `to_ms(inner) as i64` へ／前提を書いた doc コメント（392-403）を前提 1 だけへ／既存テスト（823-838）／**新規テスト 1 本** |
| 5 | `snotra-core/CLAUDE.md` | 219 行の `total_ms` → `total` |
| 6 | `scripts/lib/SnotraStartupContract.psm1` | 63 行の散文と 162-164 行の失敗メッセージ内の `LoadOrScanStats.total_ms` → `LoadOrScanStats.total`。**`>= 0` の述語自体は 1 文字も変えない** |
| 7 | `PERFORMANCE.md` | 1883・2177 の `total_ms` → `total`。**数値・散文・出所は変えず、識別子の綴りだけ**。2704 行は別フィールド（`SNOTRA_EGUI_PAINT_TRACE`）ゆえ**触らない** |

### 触らないもの（明示）

- `LoadOrScanStats` の他の `*_ms`、`LoadCacheResult`、`Scanned` — 範囲外
- JSON のキー `index_load_unattributed_ms` — 表示境界の名前で ms のままが正しい
- `SnotraStartupContract.psm1` の `>= 0` 述語と必須キー列挙（89 行）、`scripts/bench-startup.ps1`
- `docs/superpowers/plans/` / `specs/` — 凍結された歴史
- `SPEC.md` — 内部計器の型は語彙層に載らない（`total_ms` の言及も 0 件・grep 実測）

## 実装順序

### Phase 1 — `snotra-core` の型変更

- [x] `indexer.rs:22` の import を `use std::time::{Duration, Instant};` にする
- [x] `LoadOrScanStats.total_ms: u128` を `pub total: Duration,` にする
- [x] 生成 3 か所（778・819・872）の `total_ms: total_started.elapsed().as_millis()` を `total: total_started.elapsed()` にする
- [x] **`cargo clippy --workspace --all-targets -- -D warnings`** を走らせ、**落ちた行を移行漏れの
      一覧として記録する**（下流 compile-fail を検出器に使う）。
      ⚠ **`cargo check --workspace` を検出器に使ってはならない**——`tests/` 配下の統合テストは
      既定ターゲットの外であり、`cargo check --workspace -v` の出力に `memory_footprint` が
      **1 度も現れないことを実測した**（独立導出 A-2 の指摘・主エージェントが再実測）。
      これを使うと `memory_footprint.rs` の消費 2 か所が黙って漏れる

### Phase 2 — 消費側の追随と検知器

- [x] `memory_footprint.rs:319, 331` を `s.total.as_millis()` にする（残余計算の型は `u128` のまま・意味も丸めの向きも不変）
- [x] `main.rs:185` を `s.total.as_millis()` に、`main.rs:195` を `startup::set_index_load_stats_total(result.stats.total)` にする
- [x] `startup.rs` のフィールドと 2 つの関数を `Duration` 版へ改名する（`index_load_stats_total` / `set_index_load_stats_total`）
- [x] `to_json` のアームを `(Some(measured), Some(inner)) => json!(to_ms(measured) as i64 - to_ms(inner) as i64)` にする。**`i64` のままにする**（前提 1 が破れたときに負値が出力へ現れる性質を保つ）
- [x] 既存テスト `index_load_unattributed_is_the_gap_against_load_stats` の `set(42)` を `Duration::from_millis(42)` にする（期待値 8 は不変）
- [x] **新規テスト**を `startup.rs` の `#[cfg(test)]` へ足す:
      `index_load_unattributed_is_zero_when_both_fall_in_the_same_millisecond`
      — `mark(Phase::IndexLoad, Duration::from_micros(50_900))` + `set_index_load_stats_total(Duration::from_micros(50_500))` → `0`
- [x] **新規テストが発火しうることを変異注入で実測する**——`to_json` の内側だけを
      四捨五入（`(inner.as_secs_f64() * 1000.0).round() as u64`）へ変えると
      `0` が `-1` になって落ちることを確かめ、**変異を戻す**（`measure-whether-detector-can-fire`）
- [x] `cargo test -p snotra-core` / `cargo test -p snotra` が緑

### Phase 3 — doc と散文の追随（compile-fail が見ない面）

- [x] `indexer.rs` の struct doc（417-421）・442 行・3590 行の `total_ms` を `total` にする
- [x] `startup.rs` の `to_json` 前のコメント（392-403）を書き直す。**前提 2 の記述を消し、前提 1 だけを残す。** 主張は `index_load_unattributed_ms` の計算に限定する（§後述の不変条件）
- [x] `snotra-core/CLAUDE.md:219` の `total_ms` を `total` にする
- [x] `SnotraStartupContract.psm1:63, 163` の散文を `LoadOrScanStats.total` にする
- [x] `PERFORMANCE.md:1883, 2177` の `total_ms` を `total` にする（2704 行は触らない）
- [x] `git grep -n "total_ms"` の残りを**振り分ける**（件数を数えない）。残ったすべての出現が
      次のどちらかへ入ること: (a) 凍結文書 `docs/superpowers/**`、
      (b) `SNOTRA_EGUI_PAINT_TRACE` の**別概念**（正本は `snotra-egui-runtime/src/renderer.rs`）。
      **どちらにも入らない出現だけが直す対象である。**
      ⚠ **除外リストを書き足す形で閉じない**——`renderer.rs:183` は必ずヒットするので、
      「0 件」を条件にすると実装者が**範囲外の `renderer.rs` を直してしまう**

### Phase 3.5 — 実装中に判明（計画外・/symmetric-check で発見）

- [x] **引き算の向きを守る責務がテストへ移ったことを書き留める**。#1027 で内側も `Duration`
      になり `to_json` の 2 引数が同じ型になったため、**入れ替えが表現可能になった**
      （変更前は内側が `u64` で compile-fail）。反転の変異で
      `index_load_unattributed_is_the_gap_against_load_stats` が `-8` を出して落ちることを実測し、
      新規テストの側は**反転を検出しない**（50.9/50.5 はどちらも 50 ms ゆえ対称）ことを
      テストの隣へ明記した

### Phase 4 — 検証（下記「検証コマンド」を全実行）

- [x] カテゴリ A・F の全コマンドが緑（委譲先が `95ae25ac` で全件 exit 0。A は fmt/check/clippy/
      core-test 605/snotra-test 303/`cargo doc` warning 0 行、F は `governance:check` 全 23 検査）
- [x] Pester が緑（`npm run test:powershell` exit 0・128 passed。先に `cargo build -p snotra`）
- [x] カテゴリ C も実行（拡張子でなく意味で該当）——`smoke:startup` exit 0（5 runs）/
      `smoke:egui` exit 0 / `npm test` exit 0（38 files・901 tests）
- [x] 挙動不変の確認——`index_load_unattributed_is_the_gap_against_load_stats` が期待値 `8` を
      変えないまま緑。**自明な緑ではないことを変異で確かめてある**（向きを反転すると `-8` で落ちる）

### Phase 5 — レビュー指摘への fix-forward（委譲先の報告を受けて）

- [x] **H-1: `startup.rs` の「内側だけを別の丸めへ変える経路は無い」が偽の全称否定だった。**
      生成側（`indexer.rs` の 3 か所）が丸めた `Duration` を入れれば `to_json` を触らずに
      内側だけ丸め方が変わり、**その変異では検査が 1 つも落ちない**（委譲先が実測）。
      しかも旧コメントに在った「内側を四捨五入へ変える」という**警告を削除して**不可能性の
      主張に置き換えていた。→ **警告を復活させ、残る経路と「検査が落ちない」実測を明記した**
- [x] **H-2: `indexer.rs` の `total` の doc が、この計画自身が「書いてはならない」と
      名指した形（「ミリ秒へ落とすのは表示境界の 1 か所だけ」）になっていた。**
      実際は消費 4 か所のうち 3 か所が `as_millis()` を直呼びする。→ 主張を引き算の両辺に限定し、
      **「1 か所だという主張ではない」ことと他の消費者 2 か所を明記した**
- [x] **M-1: 引き算の向きの保証がテスト側にしか書かれていなかった。**
      `to_json` を編集する人の目に入るのは `to_json` 直上である。→ そちらへも明記
- [x] fix-forward 差分に対しカテゴリ A・F を再実行（fmt 0 / clippy 0 / `cargo doc` warning 0 /
      snotra-test 303 passed / `governance:check` 全検査 passed）

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| `index_load_unattributed_ms` の出力値が変更前と一致する | `startup.rs` の既存テスト（期待値 8）が変更前後で同じ値を主張する。加えて `to_ms(d) == d.as_millis()` は代数的に恒真（敵対的調査 B2 が `rustc` で 9 ケース実測） |
| 丸めが 2 か所へ戻らない | **新規テスト**（Phase 2）。`to_json` が内側だけ四捨五入へ変わると `-1` になって落ちる——**この変異で実際に落ちることを実装時に確かめる**（`measure-whether-detector-can-fire`） |
| 前提 1 が破れたら負値が出力に現れる | `to_json` を `i64` のまま保つ + `SnotraStartupContract.psm1` の `>= 0`（**この計画で変更しない**） |
| `total` を読む行の移行漏れ | **`cargo clippy --workspace --all-targets` と `cargo test -p snotra-core`**。⚠ 2 つの死角を実測済み: (a) `cargo check --workspace` は `tests/` を見ない（`-v` 出力に `memory_footprint` が 0 件）、(b) `main.rs:185` は `#[cfg(debug_assertions)]` 下ゆえ **release 単独ビルドでは検出されない**（`Cargo.toml` 37-42 行に上書き無し）。カテゴリ A は全て dev プロファイル + `--all-targets` なので規定フローに穴は無い |
| 散文に残る幽霊識別子 | **機構は無く、しかも恒久的に無い**（受容する残余・下記） |

**異常系は増えない。** 分岐・失敗経路・リソースのライフサイクルを 1 つも足さない変更である。
`Duration` は `Copy` なので `#[derive(Debug, Clone, Copy)]` はそのまま通る。

### 受容する残余（宣言）

1. **`total_ms` の腐りを検出する機構は、母集団を広げても作れない。** `G-stale-identifiers` の
   「現行語彙」は production の**非コメント本文**（文字列リテラルを含む）であり、
   `snotra-egui-runtime/src/renderer.rs:183` の `eprintln!` が
   `"... total_ms={:.2} ..."` を**恒久的に供給し続ける**（実測。これは
   `SNOTRA_EGUI_PAINT_TRACE` の別概念で、本 issue の範囲外ゆえ改名しない）。
   ゆえに仮に `PERFORMANCE.md` を母集団へ入れても、この検査は永久に鳴らない。
   **これは予測ではなく実測である**——委譲先が `PERFORMANCE.md` を旧語へ戻す変異を注入し、
   `governance:check` が exit 0（件数まで同一）で通ることを確かめた。
   **`AGENTS.md` の言う幽霊識別子そのものであり、機構ではなく Phase 3 末尾の `git grep` で閉じる**
   （独立導出 A-3 の二次機序・主エージェントが `renderer.rs:181-185` で再実測）
2. **psm1 の散文を直しても、それを観測する検査は無い。**
   `SnotraStartupContract.Tests.ps1:175` の `Should -Match 'index_load_unattributed_ms が負'` は
   **キー名しか見ない**ので、`psm1:163` のメッセージ中の `LoadOrScanStats.total_ms` を
   直しても壊しても緑である（独立導出 A-4）。加えて `post-edit.mjs` の `selectChecks` は
   `.md` と `.psm1` に検査を 1 つも積まない——**編集時の沈黙は「何も走らなかった」**（#497）
3. **同クラスの腐りが既にリポジトリに残っている。** `digest_ms` は撤去済みなのに
   `startup.rs:380` のコメントと `PERFORMANCE.md:2178` に残り、誰の検査にも掛かっていない
   （独立導出 A-7）。**本計画はこれを直さない**——範囲外であり、直すなら別 issue

### doc に書く主張の射程（全称にしない）

**書いてよい**: 「`index_load_unattributed_ms` の計算に関して、ミリ秒への丸めは
`to_ms` の 1 回だけになった。ゆえに『内外とも切り捨てである』という前提は不要になった」

**書いてはならない**: 「丸めは表示境界でだけ起きるようになった」
——`hash_ms` / `cache_load_ms` / `cache_read_ms` / `scan_ms` / `sort_ms` / `cache_save_ms` は
**依然 `snotra-core` 内で生成時に丸めている**ので、書いた日に偽になる（#977 / #1091 の再生パターン）。

## テスト方針と検証コマンド

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items   # doc コメントを触るため必須
npm run governance:check                                    # *.md を触るため必須
cargo build -p snotra && npm run test:powershell            # psm1 を触るため（本体が要る）
npm run smoke:startup                                       # payload の生成元を書き換えるため
git grep -n "total_ms"                                      # 残存の数え上げ
```

- **`cargo clippy --workspace --all-targets` は必須である**——移行漏れの検出器を兼ねる（上表）
- **`npm run smoke:startup`** は独立導出の指摘で足した: `index_load_unattributed_ms` を**生む側**を
  書き換えるので、`>= 0` とキー網羅を端から端まで通す経路を 1 回は踏む
- **不要**: カテゴリ B（TS を触らない）・C（`smoke:egui`。窓生成・hotkey・表示経路に触れない）・
  D（UI の見た目に影響しない）・E（`.githooks/` を触らない）

**挙動不変の確認は代理でなく対象で測る**: `startup.rs` の
`index_load_unattributed_is_the_gap_against_load_stats` が変更前後で同じ `8` を主張することを、
実際にテストを走らせて確かめる（`git stash` での前後比較ではなく、期待値を変えないまま緑になることで示す）。

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**。内部計器のフィールド型は語彙層に載らない（`total_ms` の言及 0 件・grep 実測）
- `docs/architecture.md`: **不要**（`total_ms` の言及 0 件）
- `snotra-core/CLAUDE.md`: **要**（Phase 3・上表 #5）
- `PERFORMANCE.md`: **要**（Phase 3・上表 #7。当初「不要」と書いたが敵対的調査 A1 で覆した）
- `docs/adr/`: **不要**。否定の知識が生じていない（採らなかった「全 `*_ms` を `Duration` へ」は
  **却下ではなく範囲外**であり、follow-up issue の対象）

## 未確定（実装前に潰す）

（なし——下記のとおり 4 件すべて解消済み）

- [x] **`PERFORMANCE.md` を直すか** — 敵対的調査 A1 が「`digest_ms` の先例」を崩した。
      `git show ae3335df`（#1023）で `pub digest_ms: u128,` と生成 3 行の削除を実測し、
      あれは**撤去**であって改名ではないと裁定。#1027 は概念が生きたままの真の改名ゆえ先例は効かない。
      規約 36 行「遡及して補完しない」も自分で射程（出所の遡及補完）を書いており使えない。
      **判断: 直す**（識別子の綴りのみ）。上表 #7 へ反映済み
- [x] **doc の主張をどう書くか** — 全称にすると書いた日に偽になる（他の `*_ms` が残るため）。
      **判断: `index_load_unattributed_ms` の計算に限定した主張にする。** 「不変条件」節へ逐語で反映済み
- [x] **前提 2 の消滅を守る検知器を置くか** — 置く。**発火しうることを先に書き下ろした**:
      `to_json` が内側だけ四捨五入へ変わる変異（`(inner.as_secs_f64()*1000.0).round()`）で
      外側 50.9 ms / 内側 50.5 ms が `0` → `-1` になり落ちる。実装時にこの変異を注入して実測する。
      Phase 2 の新規テストへ反映済み
- [x] **`set_index_load_stats_ms` から `_ms` を落とすか** — 落とす。引数が ms でなくなる以上、
      `_ms` を残すと**その場で嘘になる名前**である（本 issue が消しに来ている欠陥と同型）。
      影響はフィールド 1・関数 2・呼び出し 2（`main.rs` と `startup.rs` のテスト）。
      **これは issue の指示を超える判断なので、人間レビューで名指しして諾否を仰ぐ**

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の 2 条件に該当——`LoadOrScanStats` は `pub` ＝**公開 API の変更**、`snotra-core/CLAUDE.md` ＝**ガバナンス文書の変更**）
- plan-review: 独立レビュー 1 体（Step 2b・独立導出による網羅性レビュー）
- エージェント数: 2（3b 敵対的調査 1 体 + Step 2b 独立導出 1 体）
- 要対処: **2 件を計画へ反映**（下記 plan-review 結果）
- 未検証: **A/B の payload 実測**（下記「未検証」の理由）

## plan-review 結果

- リスク: **高**
- レビュー方式: **独立導出 1 体**（Step 2b。`plan.md` / `research.md` / `adversarial-1027.txt` を
  読ませず、`git grep ... ':!workspace'` で走査範囲も絞らせた）
- エージェント数: 1（3b の敵対的調査 1 体を含めるとサイクル合計 2）
- 全文: `workspace/plan-review-1027-independent.md`

### 導出 ∖ plan（漏れ候補）— 2 件、いずれも採用

| # | 指摘 | 再照合した根拠 | 反映 |
|---|---|---|---|
| A-2 | **`cargo check --workspace` は `tests/` を見ない**ので、移行漏れ検出器にすると `memory_footprint.rs` の消費 2 か所が黙って漏れる | **主エージェントが再実測**: `cargo check --workspace -v` の出力に `memory_footprint` は **0 件**。捕まえるのは `cargo clippy --workspace --all-targets` と `cargo test -p snotra-core` | Phase 1 の検出器コマンドを差し替え、「不変条件」表の死角を (a)(b) 2 件へ |
| A-3 二次機序 | 散文の腐りは**母集団を広げても検出できない**——`renderer.rs:183` の文字列リテラル `total_ms={:.2}` が「現行語彙」へ恒久供給し免罪する | **主エージェントが再実測**（`renderer.rs:181-185`）。研究 §4 は「母集団外」までしか言っておらず、**残余が恒久であることを言えていなかった** | 「受容する残余」1 を新設 |

### plan ∖ 導出（スコープ過剰候補）— 0 件

導出した変更ファイル集合は本計画の 7 ファイルと一致した。凍結扱い（`docs/superpowers/**`）と
`renderer.rs` の別概念 `total_ms` も独立に同じ判定へ到達している。

### 判断の不一致 — 1 件（`PERFORMANCE.md` を改名するか）

**独立導出の B-2 は「改名しない」を推奨し、本計画は「改名する」である。**

- 相手の根拠: `digest_ms` が `PERFORMANCE.md` に残っている先例、および規約 30-40 行
- **本計画の裁定（変えない）**: `git show ae3335df`（#1023）で
  `pub digest_ms: u128,` と生成 3 行の**削除**を実測済み。あれは概念ごとの撤去であり、
  `total_ms` のような**概念が生きたままの改名とは別クラス**である。
  **相手自身の A-7 も「`digest_ms` は `LoadOrScanStats` から削除済み」と書いており、
  先例の記述が先例としての効力を自ら否定している。**
  規約 36 行も自分で射程（＝出所の遡及補完）を書いているため、改名の反映を禁じない
- 相手の**「どちらへ倒すにせよ `digest_ms` の扱いと揃えること」は正当な要求**であり、
  揃っていると考える: **生きているフィールドの名前は正しく指し、消えた概念の名前は
  当時の記録のまま残す**——`digest_ms` に「現在の正しい名前」は存在しない
- ⚠ **2 つの独立枠が逆へ倒れた唯一の論点なので、人間レビューで名指しして諾否を仰ぐ**

### 軽微

- B-1（`_ms` 接尾辞が嘘になる）— 未確定欄 4 で既に「落とす」と裁定済み。相手も同じ追随先
  （フィールド・メソッド・自由関数・`main.rs:195`・`startup.rs:835`）と、
  **JSON キー `index_load_unattributed_ms` は変えない**ことを独立に導いた
- B-3（intra-doc link）— `cargo doc --workspace --no-deps --document-private-items` が裁定する。検証コマンドに既にある
- A-4 / A-8（psm1 の散文を観測する検査が無い・`.md` / `.psm1` は hook 沈黙）— 「受容する残余」2 へ記録
- A-7（`digest_ms` の腐りが既に残存）— 「受容する残余」3 へ記録。**本計画では直さない**

### 未検証

- **実装前後の payload の A/B 実測を取らない。** 理由: `to_ms(d) == d.as_millis()` は
  代数的に恒真で、**2 つの独立枠がそれぞれ実測で確かめている**（敵対的調査 B2 は `rustc` で
  9 ケース、独立導出は式の同一性）。加えて payload の他フィールドは実行ごとに揺れるため
  クリーンな A/B にならない。**代わりに `index_load_unattributed_is_the_gap_against_load_stats`
  の期待値 `8` を変えないまま緑にすることで示す**（受け入れ条件 3）
- LSP `findReferences` による列挙（rust-analyzer が本セッション中 "not fully indexed" を返し続けた）。
  **compile-fail が上位互換の検出器として代替する**（研究 §2 の条件つき主張）

### 判断

- 実装着手: **人間の裁定待ち**（`PERFORMANCE.md` の改名と `_ms` の除去の 2 点）

### 主エージェントの自己照合（5 点）

1. **issue の全要件に作業項目が対応する** — issue のチェックリスト 4 項目はそれぞれ
   Phase 1（型）・Phase 2（`main.rs` 追随・`set_index_load_stats_ms` の `Duration` 化）・
   Phase 3（doc への反映）に対応。「ハーネスの `>= 0` 検査は残す」は「触らないもの」で明示
2. **境界条件を列挙し、各条件に検証がある** — (a) 同一ミリ秒に落ちる内外 → 新規テスト、
   (b) 内側 42 ms / 外側 50 ms の通常ケース → 既存テスト、(c) `LoadOrScanStats` 不在（first-run）
   → 既存テスト `index_load_unattributed_is_null_without_stats`、(d) `u64` 溢れ →
   敵対的調査 B2 が非現実的（約 5,849 億年）と実測、対処不要
3. **新しい状態・リソース・プロセスに正常/失敗/破棄経路がある** — **該当なし**。
   分岐もリソースも足さない、型の付け替えだけの変更である
4. **より単純な既存パターンで置き換えられないか** — `startup.rs` が既に採用している
   「`Duration` で持ち回り、表示境界で `to_ms` する」パターンそのものであり、
   新しい設計を持ち込んでいない（研究 §5）
5. **壊してはならない不変条件に検知手段がある** — 上の「不変条件」表。
   ⚠ **散文の幽霊識別子だけは機構が無く、`git grep` の手作業が唯一の手段である**（受容する残余）

## 人間レビュー

- [x] 承認済み — 2026-08-25 / 問い: "`workspace/plan.md` を承認して `/implement` へ渡してよいでしょうか。" / 回答: "承認する"
- [x] 分岐 1 の裁定 — 2026-08-25 / 問い: "`PERFORMANCE.md:1883, 2177` の `total_ms` を `total` へ直しますか。**2 つの独立枠が唯一逆へ倒れた論点**です（数値・散文・出所は変えず、識別子の綴りだけ）。" / 回答: "直す（推奨・計画の現行判断）"
- [x] 分岐 2 の裁定 — 2026-08-25 / 問い: "`startup::set_index_load_stats_ms` から `_ms` を落として `set_index_load_stats_total(Duration)` にしますか。**issue の指示を超える判断**なので伺います（JSON キー `index_load_unattributed_ms` は据え置きです）。" / 回答: "落とす（推奨）"

**3 件とも計画の現行判断と一致したため、要件・対象ファイル/シンボル・インターフェース・
不変条件・テスト期待値のいずれも変わっていない。ゆえに `/plan-review` は再実行しない**
（`/plan-review` Step 3 の再レビュー条件に当たらない）。

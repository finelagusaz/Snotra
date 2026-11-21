# 計画 — issue #1400 実 index.bin を読む検知器が、読めない版のとき黙ってスキップする

## 目的と受け入れ条件

区分: バグ（`SPEC.md` の記述を変えない。テストの入口の振る舞いを直す）。

- AC1: `load_cached_entries` は、索引を返せなかったとき、その理由を型で返す。理由は
  「config_dir が引けない / `index.bin` が無い / ヘッダーが読めない（短い・magic 違い） / 現行より古い版 /
  現行より新しい版 / 現行版だが受け付けなかった（`config_hash` 不一致か検証失敗）」を区別する
- AC2: 実データの helper（`search/tests/common.rs` の `real_index_entries` と `tests/path_query_cost.rs` の
  `load_real_index`）は、どの理由でも panic せずスキップし、理由を区別した `println!` を出す。
  helper 側の理由（実 config が無い・読めない / scan が空）も区別する。版が理由のときは、見つけた版と現行版
  （`INDEX_CACHE_VERSION` を読む）を出す。古い版のときは「現行版を書く製品を起動すれば走る」を添える
- AC3: スキップの文言は helper で出す。呼び手（`real_index_entries` の 7 本と
  `derives_same_bytes_as_normalize_entry_key`）は文言を持たない
- AC4: 製品の経路（`load_cached_material`・`load_or_scan_with_stats`・`load_cache_with_stats`・`load_cache_in`）の
  挙動を変えない
- AC5: 開発機（実 `index.bin` が v7）で既定スイートを `--nocapture` で走らせると、6 本が「v7（現行 v10）」を含む
  古い版の文言を出して通る

## 変更するファイル

- `snotra-core/src/indexer/cache.rs`
  - 新設 `#[doc(hidden)] pub enum CachedEntriesMiss`（`Debug, Clone, Copy, PartialEq, Eq`）:
    `NoConfigDir` / `NoFile` / `Unrecognized` / `Older { found: u32 }` / `Newer { found: u32 }` / `Rejected`
  - `load_cached_entries` の返り値を `Option<Vec<AppEntry>>` → `Result<Vec<AppEntry>, CachedEntriesMiss>` へ替える。
    本体は `Config::config_dir()` を引いて（無ければ `NoConfigDir`）新設の `load_cached_entries_in` へ渡すだけにする
  - 新設 `fn load_cached_entries_in(dir: &Path, config_hash: u64) -> Result<Vec<AppEntry>, CachedEntriesMiss>`:
    `load_cache_in(dir, config_hash, LegacyUpgrade::Skip)` が `Some` なら実体化して返す。`None` なら
    `classify_cache_miss(dir)` を返す
  - 新設 `fn classify_cache_miss(dir: &Path) -> CachedEntriesMiss`: `cache_bin_file_in(dir).load_bytes()` が `None` なら
    `NoFile`。先頭 4 バイトが `INDEX_MAGIC` でないか `binfmt::peek_version` が `None` なら `Unrecognized`
    （`peek_version` は magic を見ないので、照合を先に置く）。版を `INDEX_CACHE_VERSION` と比べ、小さければ
    `Older`、大きければ `Newer`、等しければ `Rejected`。**フォールバックチェーンの版の一覧を写さない**
    （大小比較だけで分ける。doc に理由を書く。`Older` は v7〜v9 の読まない版と、v2〜v6 の `config_hash` 不一致を
    含む——どちらも現行版で置き換えれば読める）
  - `load_cached_entries` の doc を更新（返り値の意味・分類は大小比較であること・読み直しの間に置き換わると
    分類がずれうるがテストの入口でしか通らないこと）
- `snotra-core/src/indexer.rs`: re-export に `CachedEntriesMiss` を足す
- `snotra-core/src/indexer/cache/tests.rs`: `load_cached_entries_in` の分類テストを足す（下の「テスト方針」）
- `snotra-core/src/search/tests/common.rs`: `real_index_entries` を、理由ごとの `println!` に替える。
  doc の「上の 3 条件のいずれかで…自動スキップ」を、理由を区別して出すことに合わせて書き直す
- `snotra-core/src/search/tests/basic.rs`・`build.rs`・`path.rs`: `real_index_entries` の呼び手 7 本から `println!` を消し、
  `let Some(entries) = real_index_entries() else { return; };` にする
- `snotra-core/tests/path_query_cost.rs`: `load_real_index` を `real_index_entries` と同じ扱いに替え（統合テストは
  `pub(super)` を呼べないので写しのまま。doc に「`search/tests/common.rs` の `real_index_entries` と同じ扱い」と書く）、
  `derives_same_bytes_as_normalize_entry_key` の `println!` を消す

## 実装の順序

1. Phase 1（core）: 分類テストを先に書いて Red を確かめてから、`CachedEntriesMiss`・`load_cached_entries_in`・
   `classify_cache_miss` を実装する。re-export を足す
2. Phase 2（helper と呼び手）: `real_index_entries`・`load_real_index` を替え、呼び手 8 本の `println!` を消す。
   `cargo build -p snotra-core --all-targets` で呼び手の移行漏れが無いことを確かめる
3. Phase 3（検証）: 下の「テスト方針と検証コマンド」

## 不変条件と異常系

- 製品の経路は変えない: `load_cache_in` / `load_cache` / `load_cached_material` の本体と返り値の型に触らない。
  `classify_cache_miss` は `load_cached_entries_in` からしか呼ばない
- テストは実 `%APPDATA%\Snotra` を書かない: 分類はバイト列を読むだけ。`LegacyUpgrade::Skip` を保つ
  （既存の検知器 `load_cache_skip_does_not_upgrade_a_legacy_format` が守る）
- core は文言を持たない: `CachedEntriesMiss` は文言を持たず、文言は helper が組み立てる
- 読み直しの競合: `load_cache_in` と `classify_cache_miss` の間に `index.bin` が置き換わると分類がずれうる。
  テストの入口でしか通らず、ずれても出る文言が変わるだけなので許容する
- CI: runner に実 config が無いので helper は config の段でスキップする（どの理由でも panic しないので、CI と開発機で
  振る舞いを分ける機構は要らない）

## テスト方針と検証コマンド

`cache/tests.rs` に、`temp_dir` へ書いたバイト列で `load_cached_entries_in` を呼ぶテストを足す:

| 入力 | 期待 |
|---|---|
| ファイル無し | `Err(NoFile)` |
| `GOLDEN_V7` | `Err(Older { found: 7 })` |
| v4 を hash 42 で書き、43 で読む | `Err(Older { found: 4 })` |
| 現行版を hash 42 で書き、43 で読む | `Err(Rejected)` |
| 現行版のバイト列で版のバイトだけ `INDEX_CACHE_VERSION + 1` に替える | `Err(Newer { found: INDEX_CACHE_VERSION + 1 })` |
| 4 バイトのごみ | `Err(Unrecognized)` |
| magic だけ替え、版のバイトは 5 | `Err(Unrecognized)` |
| 現行版を hash 42 で書き、42 で読む | `Ok`（件数一致） |

現行版の期待値は `INDEX_CACHE_VERSION` を参照し、リテラルを写さない（v7・v4・5 は fixture の版なので定数でよい）。

変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (a) `classify_cache_miss` の大小比較の向きを逆にする →
Older/Newer のテストが落ちる (b) `Rejected` を `Older` に寄せる（`<=`）→ 現行版の hash 不一致テストが落ちる
(c) magic の照合を消す → magic 違いのテストが落ちる。

検証コマンドは `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリから当てはまるもの（Rust の
fmt・clippy・test・doc）をすべて走らせる。加えて、開発機の実データで AC5 を観測する（`--nocapture` で
`real_index_entries` の呼び手 5 本と `path_query_cost` の `derives_same_bytes_as_normalize_entry_key` の出力を読む）。
製品を v0.19.5 以降へ更新した後に件数つきで通ることは、ユーザーの操作が要るので作業項目にしない。

## SPEC.md・関連文書の更新

- `SPEC.md`: 変更なし（テストの入口の振る舞いであって、製品の挙動ではない）
- `snotra-core/CLAUDE.md`: 変更なし（裁定で「版上げのチェックリストへ足さない」）
- `PERFORMANCE.md`: 変更なし（「開発機の実 `index.bin` が v7 で…測っていない」は 2026-11-05 の記録として真のまま）

## 作業項目

### Phase 1 — core

- [x] `cache/tests.rs` に分類テスト 8 件を書き、コンパイルエラー/失敗（Red）を確かめる
- [x] `CachedEntriesMiss`・`load_cached_entries_in`・`classify_cache_miss` を実装し、`load_cached_entries` の返り値を替える
- [x] `indexer.rs` の re-export に `CachedEntriesMiss` を足す
- [x] 分類テストが通る（Green）

### Phase 2 — helper と呼び手

- [x] `common.rs` の `real_index_entries` と doc を替える
- [x] `basic.rs`・`build.rs`・`path.rs` の呼び手 7 本から `println!` を消す
- [x] `path_query_cost.rs` の `load_real_index` と `derives_same_bytes_as_normalize_entry_key` を替える
- [x] `cargo build -p snotra-core --all-targets` が通る

実行記録: 分類テストは 8 行を 4 本のテストに束ねた（Red は `todo!()` で 3 本が落ち、読める場合の 1 本だけ通った）。
計画の外の変更が 1 つある: `load_cache` は呼び手が `load_cached_entries` だけだったので、`dead_code` が立った。そのため消し、
`load_cache_in` の doc の「`load_cache` と同じ読み込み」を書き直した（製品の経路の挙動は変わらない）。
AC5: `over_real_index` の既定 5 本と `derives_same_bytes_as_normalize_entry_key` が「v7（現行 v10）」の文言を出して通った。
`cargo test -p snotra-core` は全件緑。

- ラウンド 1: sha e1a27840 / verify:tree green（A・E・F）/ 指摘 3（Medium 1・Low 2）/ 変異 6 のうち生き残り 1（入口の `Skip`→`Write`。検知器が無かった）/ 実地の確かめは該当なし（手順・挙動を述べない差分。実 index.bin の SHA256 はテストの前後で不変）。
  Medium（入口の `Skip` を守る検知器が無い）は `cached_entries_does_not_rewrite_a_legacy_index` を足して直した。主の手元で同じ変異を当て、assert で赤になることを確かめた。
  Low-1（`Older`/`Rejected` の doc が分岐を列挙していて既に偽）は、自分が書いた偽の記述なので下限の形へ直した。Low-2（到達可能性の主張）は挙動を変えないので捨てた。
- ラウンド 2: sha 2c6eac84 / verify:tree green（A・E・F）/ 前ラウンドの Medium-1・Low-1 は解消（レビュアーが変異 (d) を再注入し、新テストの assert で赤）/ 新しい指摘は Low⚠️ 2（assert で落ちたとき一時 dir が残る・一意性の宣言は今は真）。どちらも挙動を変えないので捨てた / 実地の確かめは該当なし（実 index.bin は不変）。
  軸の違うレビューは該当なし: (1) 挙動にかかわる所見はすべて採った (2) 安全機構・回復機構を足していない (3) 規範・ADR の前提を変えていない。
- 否定の知識: 「古い版なら panic させる」を却下した（開発機の製品が古いビルドのうちは、版上げのたびに既定スイートが赤になる）。ADR にはしない。分岐点の doc（`common.rs` の `real_index_entries`「どの理由でも落とさない」）で守れるからである。PR 本文の「残したもの」へ書く。

### Phase 3 — 検証

- [x] カテゴリ別の検証コマンドを走らせる
- [x] 開発機の v7 で 6 本が「v7（現行 v10）」を含む古い版の文言を出して通ることを観測する（AC5）

## 未確定（実装前に潰す）

- [x] （裁定）読めない版のとき失敗させるか — `AskUserQuestion` を 2 回（2026-11-22）。1 回目の問い: 実 index.bin が読めない版のとき検知器はどう振る舞うべきか。回答: 「旧版だけ失敗させる」。その後、敵対的調査で「開発機の製品 `C:\toolbox\Snotra\snotra.exe`（2026-10-08 ビルド）は v7 を書き、起動しても置き換わらない。版上げのたびに、リリースと製品の更新まで既定スイートが赤になる」と分かったので問い直した。2 回目の問い: この前提でも失敗させるか（スキップし理由を出す / それでも失敗させる / env で失敗へ切り替える）。回答: 「スキップし理由を出す」
- [x] （裁定）版上げのチェックリストに「開発機で起動して置き換える」を足すか — 1 回目と同じ問いの中で。回答: 「足さない」

## 承認

- [x] 承認済み — 2026-11-22 / 問い: "workspace/plan.md の計画を承認しますか？" / 回答: "承認して実装へ"

## セルフレビュー

- リスク: 通常（`load_cached_entries` の返り値を替えるが、`#[doc(hidden)]` でテスト専用。呼び手 2 つは
  `--all-targets` の compile-fail で漏れなく挙がる。永続形式・状態遷移・並行性に触れない）
- plan-review: 該当なし（通常リスク）
- エージェント数: 0（3b の敵対的調査 1 体は別に数える）
- 要対処: 1 件——3b の壊1（製品を起動しても v7 のまま）で裁定の前提が崩れたので問い直し、panic をやめた。
  壊2（magic の照合）はテストの行を 1 つ足した
- 自己照合: issue の 3 論点 → 理由の区別は AC1・AC2、失敗させるかは裁定（させない）、版上げの手順は裁定（足さない）。
  境界条件（ファイル無し・短い・magic 違い・古い・新しい・現行で不一致・正常）はテスト表に 1 行ずつ。
  新しい状態・リソースは無い。より単純な案（文言だけ変える）は版の区別を出せない。製品の経路を変えないことは、
  `load_cache_in` に触らないことと既存の検知器で守る
- 未検証: なし
- check スキル: なし（新しい関数は既存の `peek_version` を再利用し、版の判定を写さない。`/dry-check` に当たる
  同等ロジックは `memory_footprint.rs` の `report_cache_bytes` だが、あちらは計測コードの読める版の判定で別物）
- 散文の全件列挙: 該当なし（通常リスク）

# 計画: #1281 検索の照合で結果に寄与しない CPU 時間を払わない

調査は `workspace/research.md`、敵対的調査の出力は `workspace/adversarial-1281.txt`。

## 目的と受け入れ条件

検索の照合から、結果に寄与しない CPU 時間を 2 つ取り除く。

1. **候補 1**: `has_dot` 枝で、`file_name_is_lower_name` が立っているエントリの file name 照合を飛ばす
2. **候補 2**: `str::find` に代えて、検索 1 回につき 1 度だけ作る `memchr::memmem::Finder` で照合する。対象は Substring の名前照合・kana・パスの 3 か所

受け入れ条件:

- **A1 結果不変**: 実 `index.bin` の PATH マージあり・なし × Substring・Fuzzy × 固定のクエリ列で、件数と `(name, path)` 列のダイジェストが A（main）と 1 件も変わらない。既存のテストはすべて緑
- **A2 壁時計**: 同一セッションで A と B を交互に 3 回ずつ回し、レンジで読む。候補ごとに、効くはずの行でレンジが分離し、効かないはずの行（Fuzzy の名前クエリ）でレンジが悪化方向へ分離しない
- **A3 記録**: `PERFORMANCE.md` に出所（日付・機体 GPDWINMini・規模・版・標本数）を付けて記録する。額は「全件走査になる打鍵 1 回あたり」として書く（incremental の再利用が効く打鍵では、前回の候補しか走査しない・research B2）

## 実装前の実測（未確定を潰すために取った値）

2026-10-29・GPDWINMini・release。試作は scratch worktree で取った（A = `feaa2de8`、C1 = 候補 1、C2 = 候補 1+2）。

- 計器は使い捨てのハーネス。各標本の前に `zzzz` を打って再利用を外し、p50 は 30 標本から取った。A → C1 → C2 を 1 巡として 3 巡回した
- 生ログは scratchpad に置き、リポジトリには入れない
- **ダイジェストは 48 行 × 9 回の実行すべてで版をまたいで一致した**（相異なるダイジェストは 27 種で、空振りではない）

p50 のレンジ（µs、3 巡）:

| 構成 | クエリ | A | C1 | C2 |
|---|---|---|---|---|
| 実 312,108 件・Substring | `code` | 2,700–2,934 | 2,572–3,254 | **1,372–1,481** |
| 同 | `.exe` | 3,793–3,960 | **2,540–2,745** | **1,219–1,434** |
| 同 | `setup.exe` | 4,395–4,934 | **3,161–3,325** | **1,064–1,224** |
| 同 | `c:\users\` | 9,051–9,984 | 8,477–8,906 | **5,286–6,536** |
| 実・Fuzzy | `a.d` | 16,039–18,547 | **10,750–10,920** | 10,231–10,575 |
| 実・Fuzzy | `\bin\` | 4,911–5,706 | 5,245–5,797 | **3,492–3,886** |
| 実・Fuzzy | `code`（効かないはずの行） | 14,342–14,952 | 13,919–15,485 | 14,048–14,580 |
| 合成 300,000・Substring | `app.lnk` | 3,206–3,354 | **2,564–2,725** | **1,455–1,745** |
| 合成 300,000・Fuzzy | `code.` | 1,028–1,210 | **707–815** | 753–894 |

## 変更ファイルと対象シンボル

- `snotra-core/Cargo.toml`: `[dependencies]` に `memchr = "2"` を足す。`Cargo.lock` は snotra-core の依存行が 1 行増えるだけで、パッケージは増えない
- `snotra-core/src/search/query_plan.rs`
  - `QueryPlan` に `substring_finder: Option<Finder<'static>>`（`mode == Substring` のときだけ `Some`・E3）、`kana_finder: Option<Finder<'static>>`、`path_finder: Option<Finder<'static>>` を足す
  - `prepare_query_plan` で、needle 文字列から `Finder::new(..).into_owned()` で作る
  - **文字列のフィールド（`norm_query` / `kana_query` / `path_query`）は残す。** `can_reuse` の単調性判定（`kana_query`・`norm_query`）と `path_query.is_none()` の分岐が文字列や存在を読むためである
  - **`path_query` は `path_finder` で置き換えない。** 存在判定を `path_finder.is_some()` に移すと、同じ意味の述語が 2 つの名前に分かれる
  - `Finder` は rayon の葉や fold-init では作らない（research B3）
- `snotra-core/src/search/scoring.rs`
  - `match_score_single_cached`: Substring の腕を `finder.find(lower_name.as_bytes())` にする。Finder の受け取り方は「実装順序」の 2 に書く
  - `kana_substring_score`: 第 2 引数を `&Finder` にする
  - `score_one_entry`
    - `needs_fn_score` に `&& !v.entry.file_name_is_lower_name` を足す
    - パスの照合を `path_finder` に替える
    - doc の「内部順序の不変条件」に、フラグで file name 照合を飛ばすことと、その根拠（同じバイト列を純関数で 2 度照合しない）を 1 行足す
  - Finder を使う照合 3 か所のコメントに、`str::find` と位置が一致する根拠を書く（妥当な UTF-8 の needle なら、最左のバイト一致は文字境界に来る）
- `snotra-core/src/search/path_store.rs`: `CompactEntry::file_name_is_lower_name`（95 行）の doc に、`score_one_entry` が「同じ文字列の 2 度目の照合を飛ばす」ためにもこのフラグを読むことを 1 行足す（読み手が `entry_view` だけではなくなるため）。`query.rs` 111 行の同名フィールドは共有判定の結果の型であり、対象ではない
- `snotra-core/tests/match_cost.rs`（新規・`#[ignore]` の計器）
  - 実 `index.bin`: PATH マージあり・なし × Substring・Fuzzy × 固定のクエリ列。件数・p50・ダイジェストだけを出し、**パスの原文は出さない**
  - 合成ラダー: フォルダとファイルが半々で、フォルダにフラグが立つ構成。100,000 と 300,000 件
  - 再利用は `RESET_QUERY` で外す。`load_or_scan_with_stats` が cache-miss したら assert で止める（実 `index.bin` を書き換えないため）
  - 試作ハーネスを清書し、`//!` に責務・測る枝・**測らない枝**（kana は migemo が無効な実運用点では発火しない）を書く
- `snotra-core/src/search/tests/basic.rs`: 回帰テストを足す（「テスト方針」）
- `snotra-core/CLAUDE.md`「モジュール構成」: `tests/match_cost.rs` の行を足す
- `docs/build-commands.md`: 計測コマンドの節に `match_cost` の 1 行を足す
- `PERFORMANCE.md`: 採否の節を足す（A3）

## 実装順序

1. 回帰テストを先に書き、main で緑なことを確かめる（挙動不変の変更なので Red は無い。テストが守るのは等価性の側である）
2. 候補 1（1 行と doc）
3. 候補 2: 依存の追加 → `QueryPlan` → `scoring.rs` の 3 か所
   - Substring の Finder は `Option` で持つ。`match_score_single_cached` には `&QueryPlan` を渡して、腕の中で `plan.substring_finder` を取る（引数が 1 つ増える形より読みやすい）
   - Substring のときに `None` なら `expect` で落とす（`prepare_query_plan` の構築規則と 1 対 1 に対応するため）
4. 計器 `tests/match_cost.rs`
5. A/B を測り、`PERFORMANCE.md` に記録する
   - **A は「main ＋ 計器のファイルだけ」**とする（main には `tests/match_cost.rs` が無いので、main のままではハーネスを走らせられない）。detached worktree を main に置き、計器のファイルを写して建てる。B はブランチの先頭
   - 交互に 3 巡回し、**巡ごとに順序を入れ替える**（A→B、B→A、A→B）。試作の計測は順序を A→C1→C2 に固定していた
   - 測り終えたら worktree を `git worktree remove` で外す
6. 文書（`snotra-core/CLAUDE.md`・`docs/build-commands.md`）

## 不変条件と異常系

- **スコアと順位は変えない**（A1）。候補 1 は構造で証明できる（research「候補 1 の等価性」）。候補 2 は位置の一致に依存する
- **空の needle**: `norm_query` と `path_query` は構造上空にならない。`kana_query` が仮に空でも、`str::find` と `memmem` はどちらも `Some(0)` を返す
- **Finder は検索 1 回につき needle ごとに 1 度だけ作る。** 確保は検索ごとに最大 3 回の定数で、差分型の検知器（`search_allocations.rs` の 4 本）は赤にならない。`measure_search_allocations` の表は定数ぶん動きうるが、その表は 2026-10-24 の歴史の記録なので書き換えない
- **incremental**: `can_reuse` と `update` は触らない。読むフィールドも変えない（`snotra-core-search.md` のトリガーを確認した。述語も状態も足さない）
- **pre-filter は関数の先頭のまま**（`score_one_entry` の doc にある順序の不変条件）

## テスト方針と検証コマンド

- **回帰テスト**（`search/tests/basic.rs`）
  - (a) 名前に `.` を含むフォルダ（フラグが立つ）を `.` のクエリで引き、Substring と Fuzzy の両方でヒットしてスコアが期待値になる
  - (b) フラグが立たないエントリ（名前 ≠ file name）は、`.` のクエリで file name 側に一致する。既存の `has_dot_uses_cached_lower_file_name` が Substring で守っているので、Fuzzy の版を足す
  - (c) Substring・kana・パスの位置ペナルティが、非 ASCII を含む haystack で `str::find` 由来の値と一致する。名前の途中に一致する非 ASCII 名で、スコアが `SUBSTRING_BASE - byte_idx` になることを見る
    - **期待値の `byte_idx` はテストの中で std の `lower_name.find(query)` から計算する。** Finder から取ると不動点になり、何も検査しない
  - (d) `prepare_query_plan` の契約: 3 モードすべてで `substring_finder.is_some() == (mode == Substring)` を確かめる。`match_score_single_cached` の `expect` が依存する前提である
- **変異注入**（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）
  - 条件を反転する（`file_name_is_lower_name` を否定せずに足す）変異で (b) か既存テストが落ちること
  - Finder の位置を 1 ずらす変異で (c) が落ちること
- **検証**: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（`npm run verify:tree`）。計器は `cargo test --release -p snotra-core --test match_cost -- --ignored --nocapture --test-threads=1`
- **結果不変（A1）**: 計器のダイジェストを A と B で突き合わせる

## SPEC.md・関連文書

- `SPEC.md`: 更新しない。§4 の照合方式・スコア式は変わらない
- `docs/architecture.md`: 更新しない（照合の手段は書かれていない）
- `snotra-core/CLAUDE.md`・`docs/build-commands.md`・`PERFORMANCE.md`: 上のとおり

## 作業項目

### Phase 1 — 等価性のテスト

- [x] `search/tests/basic.rs` に回帰テスト (a)(b)(c) を足し、変更前の木で緑にする（(d) は Phase 3 で `substring_finder` と一緒に足す）

### Phase 2 — 候補 1

- [x] `score_one_entry` の `needs_fn_score` にフラグの条件を足し、doc を更新する
- [x] `CompactEntry::file_name_is_lower_name` の doc に、2 人目の読み手を足す

### Phase 3 — 候補 2

- [x] `snotra-core/Cargo.toml` に `memchr = "2"` を足す
- [x] `QueryPlan` に Finder 3 つを足し、`prepare_query_plan` で作る（Substring の Finder は Substring のときだけ）。回帰テスト (d) を足す
- [x] `match_score_single_cached`・`kana_substring_score`・パスの照合を Finder に替え、位置が一致する根拠をコメントに書く

### Phase 4 — 計器と記録

- [ ] `snotra-core/tests/match_cost.rs` を書く（実 `index.bin` × PATH マージあり・なし × 2 モード、合成ラダー、ダイジェスト、cache-miss の assert）
- [ ] A（main ＋ 計器のみ）と B（ブランチ先頭）を、巡ごとに順序を入れ替えて 3 巡測る。A1（ダイジェスト一致）と A2（レンジ。合成 300,000 件の Fuzzy `vis` / `code` を含む）を確かめ、worktree を外す
- [ ] `PERFORMANCE.md` に採否の節を書く（出所・全件走査の打鍵 1 回あたりという額の単位・kana を測っていないこと）
- [ ] `snotra-core/CLAUDE.md` のモジュール構成と `docs/build-commands.md` に `match_cost` を足す

## 未確定（実装前に潰す）

- [x] 候補 1 は壁時計で勝つか — 試作で A/B を測った。実 Substring の `.exe` が 3,793–3,960 → 2,540–2,745、実 Fuzzy の `a.d` が 16,039–18,547 → 10,750–10,920 で、レンジが分離した。**採る**
- [x] 候補 2 は壁時計で勝つか — 実 Substring の `code` が 2,572–3,254 → 1,372–1,481、`c:\users\` が 8,477–8,906 → 5,286–6,536 で分離した。**採る**
  - **実 `index.bin` の Fuzzy の名前クエリは悪化していない**（C1 と C2 のレンジが重なる）
  - **合成 300,000 件の Fuzzy では、C1 → C2 で上へ分離した行があり、説明が付いていない**（`vis` 2,554–2,675 → 2,965–3,483、`code` 2,899–3,106 → 3,546–3,912。A のレンジが広いので、A とは重なる）
  - 疑える原因は 2 つ。試作が `norm_query` の Finder を Fuzzy でも無条件に作っていたこと（E3 で消える形なので、本実装は測った版と違う）と、順序を A→C1→C2 に固定して C2 が常に最後だったこと
  - 本実装の A/B（Phase 4）でこの行を名指しで確かめる（受け入れ条件 A2 の「効かないはずの行」に含める）。順序は巡ごとに入れ替える
- [x] Finder の持ち方 — `QueryPlan` に `into_owned` で持つ (a) を採る。(b)（`score_one_entry` へ別に渡す）は引数が広がるわりに得るものが確保 3 回ぶんだけで、その 3 回は差分型の検知器に効かない
- [x] kana にも Finder を使うか — 使う。等価性は同じ論証で閉じ、照合の手段がモードで割れない。**実運用点（migemo 無効）では測れない**ことは `PERFORMANCE.md` と計器の `//!` に明記する
- [x] 計器を残すか — 残す。パスでないクエリを実 `index.bin` で交互に測れる計器が無く（research B1）、A3 の出所になる
- [x] `memchr` を直接依存にするか — する。パッケージは増えない（Cargo.lock の memchr は 2.8.3 の 1 件だけで、nucleo-matcher 0.3.1 経由）

## 人間レビュー

- [x] 承認済み — 2026-10-29 / 問い: "`C:\workspace\Snotra\workspace\plan.md` に注釈を書き加えていただくか、このまま承認してくださるか、どちらかをお知らせくださいませ。" / 回答: "承認します"

## セルフレビュー

- リスク: 通常（永続形式・公開 API・状態遷移・並行性・ガバナンス文書のどれにも触れない。依存の追加はあるが、パッケージは増えない）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査のみ）
- 要対処: 5aの自己照合で 3 件を反映した
  - E3: Substring の Finder を `Option` にした
  - E4: A1 に PATH マージを含めた
  - フラグの読み手が `entry_view` 以外にも増えるので、`path_store.rs` の `CompactEntry` の doc を更新対象に足した（advisor の指摘で `query.rs` から訂正）
- 未検証: kana の Finder の壁時計（実運用点で migemo が無効なため。計器の `//!` と `PERFORMANCE.md` に書く）
- 散文の全件列挙: 未実施（通常リスク）。参考に `prose:sweep` を 7 語で回した（総件数 130 行）。この変更で偽になる散文は `scoring.rs` の doc と `path_store.rs` のフラグの doc で、どちらも計画に載せてある
- 試行（#1300・高リスクのみ）: 対象外（通常リスク）
- 5a の照合
  1. 要件と作業項目: 候補 1・2 と受け入れ条件 2 つが、それぞれ Phase 2・3・4 に対応している
  2. 境界条件: 空の needle・非 ASCII・フラグあり/なし・PATH マージあり/なし・モード 3 種に検証がある。Prefix はヒットすれば短絡して不変、外れれば `starts_with` が 1 回減るだけ
  3. 新しい状態: Finder は `QueryPlan` と同じ寿命で、生成と破棄は検索 1 回に閉じる
  4. より単純な既存パターン: 無い（std の `find` は needle を事前計算できない）
  5. 不変条件の検知: 結果不変は回帰テスト (a)(b)(c) と計器のダイジェストで、確保の定数性は既存の差分型の検知器で見る

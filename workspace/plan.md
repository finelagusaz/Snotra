# 実装計画: issue #984 — `rebuild_and_save` が保存の返り値を捨てている

## 目的と受け入れ条件

`rebuild_and_save` → `drain_index` の枝が、保存が計算した `CachedMasks` をそのまま索引の表現に使う。

1. `rebuild_and_save` が `(IndexTree, Option<CachedMasks>)` を返し、`drain_index` がその第 2 要素を `PrebuiltIndex` へ渡す（D1 で訂正）
2. drain 側の PATH マージが `extend_cached_masks` を通る。**「構造的に不可能」と書いてはならない**——閉じるのは**現存する 2 つの呼び出し点**（`main.rs` と `indexing.rs`）が同じ 1 関数を通ることであって、`IndexTree::extend_with_roots` は `pub` のまま残るので、**将来 3 つ目の併合経路を `merge_path_entries` を通さずに書くことはできる**（`src-tauri/CLAUDE.md` が raw 窓操作について「ただし表現不能化ではない」と書くのと同じ性格の**受容する残余**）。可視性を絞る案は #984 の射程を大きく超えるので採らない
3. `indexer.rs` の `rebuild_and_save` の doc から「意図的・受容する残余」の記述が消える（issue の撤去条件）
4. `PERFORMANCE.md`「次の反復の候補」の該当行が同節から消え、行き先が確定している（同節の撤去条件）
5. `PrebuiltIndex::from_tree` を「製品の主経路」として書いた散文が 1 つも残っていない（**関数は残るので、名指しそのものは残る**——D3 で訂正）
6. **呼び忘れを再現する変異で落ちる検知器**が CI に居る
7. `cargo fmt --check` / `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra-core` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items` / `npm run governance:check` がすべて green

**`index.bin` のバイト形式には触らない**（`INDEX_CACHE_VERSION` 据え置き。`index_cache_on_disk_format_is_stable` の golden が無変更で緑であることがその証拠）。

## 設計判断（実装者が再導出しないための確定事項）

### D1. `rebuild_and_save` の返りは `(IndexTree, Option<CachedMasks>)`

`PERFORMANCE.md` の候補行のとおり。**`Option` を外してはならない。**

**この判断は一度誤り、一次証拠で訂正した。** 当初は「`save_cache_sorted_in`（`indexer.rs:859`）が `(IndexTree, CachedMasks)` を無条件に返すから `Option` は到達不能な腕になる」と書いたが、`rebuild_and_save` が呼ぶのは `_in` ではなく**包み側の `save_cache_sorted`（`indexer.rs:764`）**であり、そちらの返りは `(IndexTree, Option<CachedMasks>)` である。同関数の doc（`:761-763`）が理由を明言している:

> **マスクが `Option` なのは保存先が無い枝があるからである。** `config_dir` が引けないときは `index.bin` を書かないので派生データも計算しない——ここを `CachedMasks` 直返しにすると、誰も読まない列を全件ぶん組み立てることになる。

`_in` だけを読んで包み側へ一般化したのが誤りの機序である（`AGENTS.md`「主張は代理ではなく対象そのもので測ってから書く」）。`LoadOrScanResult::cached_masks` の doc が「**`None` になる場合を数え上げてはならない**……`Engine::new_from_tree` の腕は到達不能ではない」と書くのと同じ構図で、ここも `None` は実在する枝である。

### D2. 共通化する単位は「PATH スキャン」ではなく「PATH エントリの併合」

新設: `indexer::merge_path_entries(tree: &mut IndexTree, masks: Option<&mut CachedMasks>, entries: Vec<AppEntry>)`

- **`scan_path_env` を内側に入れない**——実 PATH 環境変数を読むため決定的なユニットテストに乗らず、検知器が書けなくなる（`research.md`「技術的制約」）。スキャンは呼び出し点に残す（忘れれば `entries` が無いので目に見える）
- **`masks` は `Option<&mut CachedMasks>`**。`main.rs` は `cached_masks.as_mut()`、`drain_index` は `Some(&mut masks)` を渡す。`&mut Option<CachedMasks>` にすると drain 側が `Some` へ包んで後で `unwrap` する形になり、到達不能な panic 経路が生える
- 中身は「マスクへ追記 → 木へ根として追加」の順（起動経路の現行順序をそのまま移す。順序の理由は既存の検知器 doc）
- **`!entries.is_empty()` ガードは関数の内側に置かない**——`extend_with_roots` は空で早期 return し、`extend_cached_masks` は空スライスの `for` で no-op である（実測: `index_tree.rs:404`）。`main.rs` にある現行のガードは**挙動を変えずに落とせる**

これが受け入れ条件 2 の「構造的に不可能」の実体である。文書契約ではなく、**両呼び出し点が同じ 1 関数を通る**ことで担保する。

### D3. `PrebuiltIndex::from_cache` を足し、`from_tree` は**残して doc を書き換える**

D1 の訂正の帰結。`masks` が `None` の枝（`config_dir` が引けない）は実在するので、drain 側も `Some` / `None` で分岐する必要があり、`from_tree` は**その受け皿として呼ばれ続ける**。当初は「呼び出し元ゼロになるので削除」と書いたが、それは D1 の誤りに乗った結論だった。

- 足す: `PrebuiltIndex::from_cache(tree, masks: CachedMasks, migemo_enabled)` → `SearchEngine::new_with_cached_masks`
- 残す: `PrebuiltIndex::from_tree`。doc の「**製品はこちらを通る。**」（`engine.rs:54`）を「**`config_dir` が引けず保存が派生データを返さなかったときの枝が通る**」へ書き換える。**版の番号や頻度を書かない**（`Engine::new_from_cache` / `LoadOrScanResult::cached_masks` の doc と同じ規律。「稀である」と書くと実測していない主張になる）
- 残す: `SearchEngine::new_from_tree` / `Engine::new_from_tree`——初回起動と、派生文字列を持たない古い版を読んだキャッシュヒットが通る（`engine.rs:112-116`）。`snotra-core/tests/` の 3 か所も使う

**帰結として、drain 経路と起動経路は分岐の形まで同じになる**（`Some` → `*_from_cache` / `None` → `*_from_tree`）。D2 の `Option<&mut CachedMasks>` はこの対称性にそのまま乗る。

## 変更ファイルと対象シンボル

### コード

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/indexer.rs` | `rebuild_and_save` | 返りを `(IndexTree, Option<CachedMasks>)` へ。`save_cache_sorted(...).0` の `.0` を外す。doc の「受容する残余」3 行を、返り値の契約と `merge_path_entries` への案内へ差し替える |
| `snotra-core/src/indexer.rs` | `merge_path_entries`（新設・`pub`） | D2 の署名。doc に「2 本の追記の長さが揃うこと」と、`assemble` の長さ検証が `debug_assert` ゆえ release で消えることを書く |
| `snotra-core/src/engine.rs` | `PrebuiltIndex::from_cache`（新設） | `(tree, masks: CachedMasks, migemo_enabled)` → `SearchEngine::new_with_cached_masks`。doc は `Engine::new_from_cache` と同じ「版の番号を書かない」規律を守る |
| `snotra-core/src/engine.rs` | `PrebuiltIndex::from_tree` | **残す**。doc の「製品はこちらを通る」を `None` 枝の受け皿である旨へ書き換える（D3） |
| `src-tauri/src/indexing.rs` | `drain_index` | `let (mut tree, mut masks) = rebuild_and_save(...)` → `merge_path_entries(&mut tree, masks.as_mut(), path_entries)` → `match masks { Some(m) => from_cache(tree, m, ..), None => from_tree(tree, ..) }` |
| `src-tauri/src/indexing.rs` | `start_index_build` のコメント（`:41`） | panic 発火点の列挙に `from_cache` と `extend_cached_masks` を反映する（`catch_unwind` の設計自体は不変） |
| `src-tauri/src/main.rs` | `main` の PATH マージ（`:193-204`） | インライン 3 手を `indexer::merge_path_entries(&mut tree, cached_masks.as_mut(), path_entries)` へ置換 |

### 検知器

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/search/tests/build.rs` | `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree` | B 側の手組み 2 行を `merge_path_entries` の呼び出しへ置換し、**製品コードそのものを測る形**にする。doc の「`drain_index` は今も `extend_cached_masks` を呼んでいない……そのときこの検知器が形の手本になる」を、「両経路が同じ関数を通る」の記述へ差し替える |
| `snotra-core/src/search/tests/build.rs` | `merge_path_entries_without_masks_matches_deriving_over_the_extended_tree`（新設） | `masks = None` で渡したときに木だけが伸び、`new_from_tree` と一致すること。**doc に「この腕は到達可能な製品経路である」と書く**（`config_dir` が引けないとき `rebuild_and_save` が `None` を返し、drain が `from_tree` で建てる。書かないと将来の読者が「起こりえない状態のテスト」として削る） |

### 文書（`.rs` のコメント含む・すべて grep で所在を確認済み）

| 位置 | 現状の主張 | 対応 |
|---|---|---|
| `PERFORMANCE.md:588` | 「次の反復の候補」表の該当行 | 同節から**移す**（行き先は下の未確定 U1） |
| `PERFORMANCE.md:663-666` | 反復 11 の「**残余（意図的）**: `rebuild_and_save` → `drain_index` の枝は `PrebuiltIndex::from_tree` のままで、返るマスクを捨てている」 | 解消済みとして書き換え（後続 issue で閉じた旨と行き先を指す） |
| `snotra-core/src/index_tree.rs:288` | `materialize` を通る経路として「設定からの再構築（`PrebuiltIndex::from_tree`）」を挙げる | **主経路ではなくなる**（`None` 枝に限る）ので、そう読める形へ直す |
| `snotra-core/src/engine.rs:53-61` | `from_tree` の doc「製品はこちらを通る」 | D3 のとおり書き換え |
| `snotra-core/src/indexer.rs:51-54` | `CachedMasks` の doc「**出所は 2 つある。**……`save_cache_sorted_in` が書いたその足で返したもの（**cache-miss**・反復 11）」 | 括弧内の例示が唯一の実例に読める。drain（force-rebuild）も同じ出所になるので、**経路を数え上げない**形へ直す（独立導出の ⚠ 2） |
| `src-tauri/CLAUDE.md:17` | drain ループが「`PrebuiltIndex::new`」を呼ぶ（**既に古い**——現行は `from_tree`） | `from_cache` へ直す |
| `snotra-core/CLAUDE.md:185` | 「ロック外で `PrebuiltIndex::new(entries)` を構築」 | 同上 |
| `snotra-core/CLAUDE.md`「記録側が潰したものを、cache-miss の枝がそのまま索引の表現に使う」bullet 末尾 | 「cache-miss の直後に PATH エントリを併合する経路は `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree` が別に守る」 | 併合が 1 関数に寄ったこと・drain もそこを通ることを書く |
| `snotra-core/src/search/tests/build.rs:243-245` | 「`drain_index` は今も `extend_cached_masks` を呼んでいない」 | 上の検知器の行で差し替え |

**触らないもの**:

- `docs/design/2026-05-31-coherence-staleset.md`（`:135` / `:224` / `:235` / `:257` / `:264`）——日付つきの設計記録。**凍結されている根拠は「この変更の前から既にずれている」ことそのものである**: 疑似コードは drain のビルダーを `PrebuiltIndex::new` と書くが、現行コードは `from_tree` を呼ぶ（`indexing.rs:148`）。#347/#348-A 当時の記述として残す（独立導出の ⚠ 1 への回答）
- `snotra-core/src/search/tests/performance.rs:237`——ベンチの説明で `PrebuiltIndex::new` を挙げるが、`new` は残るので偽にならない

## 実装順序

`rebuild_and_save` の返り値型を変えた時点で `src-tauri` はコンパイルできなくなる。**Phase 1 と 2 の間でコンパイルが通らない状態を経由する**のは意図的で、その compile-fail が移行漏れ検出器そのものである（`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）。コミットは Phase 2 の完了後に初めて可能になる。

### Phase 1 — snotra-core（返り値・併合関数・コンストラクタ）

- [x] `indexer::rebuild_and_save` の返りを `(IndexTree, Option<CachedMasks>)` にし、doc の「受容する残余」を差し替える
- [x] `indexer::merge_path_entries` を新設する（D2 の署名・doc に長さの不変条件を書く）
- [x] `PrebuiltIndex::from_cache` を足し、`from_tree` の doc を D3 のとおり書き換える
- [x] `cargo check -p snotra-core` が通る

### Phase 2 — src-tauri（呼び出し点の移行）

- [x] `cargo build -p snotra` を先に走らせ、**compile-fail が移行漏れを名指しする**ことを確認する → **実測**: PostToolUse hook の clippy が `indexing.rs` の 5 点を名指しした（`:106` `&IndexTree` 不一致 / `:107` `extend_with_roots` 無し / `:132` `len` 無し / `:133` `path_into` 無し / `:148` `IndexTree` 不一致）。返り値型 1 か所の変更が下流の全使用点を挙げる形になった
- [x] `drain_index` を `(tree, masks)` → `merge_path_entries` → `Some`/`None` の分岐へ繋ぐ。**`masks.as_mut()` の可変借用が `match masks` より前に終わること**を実測（`cargo clippy --workspace --all-targets -- -D warnings` が通る＝NLL で借用が call で終わっている）
- [x] `main.rs` の PATH マージを `merge_path_entries` へ置換する（`!is_empty()` ガードの削除は挙動不変——`extend_with_roots` は `index_tree.rs:404` で空を早期 return し、`extend_cached_masks` は空スライスの `for` で no-op）
- [x] `indexing.rs:41` のコメントの関数名を直す
- [x] `cargo check --workspace` が通る

### Phase 3 — 検知器と変異試験

- [x] 既存検知器の B 側を `merge_path_entries` 経由へ書き換え、doc を差し替える
- [x] `masks = None` の腕の検知器を足す（`merge_path_entries_extends_the_tree_even_without_masks`）
- [x] `cargo test -p snotra-core` が green（**553 passed / 0 failed / 11 ignored**）
- [x] **変異試験 1**（追記の欠落）: `merge_path_entries` から `extend_cached_masks` を消す → `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree` が **赤**。落ち方は `search/build.rs:184` の `debug_assert`「SearchEngine: 派生文字列の長さが entries と一致しない」——**計画が予告した機序そのもの**（release ではこれが消え、後の検索で添字外 panic になる）
- [x] **変異試験 2**（木への追加を `if let Some` の内側へ）: `merge_path_entries_extends_the_tree_even_without_masks` が **赤**（`assert_eq!(tree.len(), total)`）。**既存の検知器は緑のまま通った**——`Some` の枝しか通らないので、この誤りには原理的に当たらないことも同時に実測できた
- [x] **両変異が `migemo_enabled` の両設定で赤になる**ことを確かめた。ループは `[false, true]` の順で最初の失敗で止まるため、**順序を `[true, false]` へ反転させて再測**し、どちらの変異も `true` 側で落ちることを確認してから元に戻した

**順序の変異試験は置かない**（`/symmetric-check` Step 2c の結果）。`extend_cached_masks(masks, &entries)` → `tree.extend_with_roots(entries)` の順序は**所有権が強制する**——`extend_with_roots` は `Vec<AppEntry>` を値で取るため、逆順にするには `entries.clone()` が要る。**入れ替えは「うっかり」では書けない**ので、検知器を置く対象が無い。意味の上でも両者は独立である（`extend_cached_masks` は木を読まず、`extend_with_roots` はマスクを読まない）。守るべき順序の不変条件は `derive_entry_collapsed` の内側（潰す前にマスクを取る）にあり、そちらは既存の `derived_masks_come_from_the_uncollapsed_strings` が守る。

### Phase 4 — 文書

- [x] 上の「文書」表の **9 か所**を直す（表の行数と一致・当初「8 か所」と書いていたのは `CachedMasks` の doc を後から足す前の数。**数え上げた散文が自分の表と食い違う典型**だったので直した）
- [x] `PERFORMANCE.md` の候補行を「採用」節へ移す（数字の欄は `未実測`）
- [x] `docs/architecture.md` は更新不要と grep で確認（`:53` の `PrebuiltIndex`（ロック外で構築→スワップ）は総称の記述で、コンストラクタを名指ししていない）
- [x] `npm run governance:check` が green（検査 19 件 / 見出し参照 175 件を md 47 + .rs 96 から照合 / 散文の識別子 70 件）
- [x] `cargo doc --workspace --no-deps --document-private-items` が green。**自分が増やした warning を 2 件見つけて潰した**——`[`save_cache_sorted`]` は private ゆえ public な doc からリンクすると `private_intra_doc_links` が鳴る（`indexer.rs:53` と `:903`）。リンクを外して識別子表記へ落とし、warning を 11 → 9 件（残る 9 件はすべて既存）へ戻した

### Phase 5 — 全体検証

- [x] `cargo fmt --all -- --check`（FMT-OK）
- [x] `cargo clippy --workspace --all-targets -- -D warnings`（exit 0）
- [x] `cargo test -p snotra-core`（553 passed / 0 failed）/ `cargo test -p snotra`（218 passed / 0 failed）
- [x] 実装差分を確定させた（`git diff --stat` を**引数 1 個の形**で読み、9 ファイル・+225/-50 を確認）

## 不変条件と異常系

| 不変条件 | 壊れたときの症状 | 検知手段 |
|---|---|---|
| マスク 2 本と木の長さが揃う | **ビルド時は完全に沈黙し、ユーザーの検索時に落ちる。** `assemble` の長さ検証（`search/build.rs:184-187` / `:260-266`）は `debug_assert` ゆえ release では消え、`Collapsed` 枝は `needs_measuring = false` で測定ループも回らないので短い列がそのまま `SearchEngine` へ入る。破綻は検索ホットパス `search/scoring.rs:331-332` の `self.char_masks[i]` の**添字外 panic**（Vec の境界検査は `debug_assert` に依らず常に効く）→ `panic = "abort"` でプロセスごと終了。**症状は「drain 完了後に初めて検索した瞬間のクラッシュ」**（独立導出が追った帰結・`Cargo.toml:37-42` の release プロファイルに `debug-assertions` の明示が無いことを確認済み） | Phase 3 の検知器 ＋ 変異試験 |
| 追記側の潰し方が、拡張後の木から導出した結果と一致する | 潰れ方がずれた分だけ `entry_view` の読み替えが外れ、**検索結果は返るのにスコアだけが静かにずれる** | 既存検知器（`assert_engines_agree`）が `merge_path_entries` 経由になることで drain 経路も覆う |
| `index.bin` のバイト形式が不変 | 旧 `index.bin` が読めなくなる／毎起動で書き直す | `index_cache_on_disk_format_is_stable`（golden・**差分ゼロで緑**であることが証拠） |
| 保存の失敗が索引を壊さない | — | `save_cache_sorted_in` の既存契約（書けなければ次回 cache-miss になるだけ）。返り値の意味を変えないので新しい異常系は生えない |
| drain のビルドスレッドが panic しても flag が固着しない | UI が永久「構築中」 | 既存の `catch_unwind`。panic 発火点の名前が変わるだけで構造は不変 |

**「初めて生きる組み合わせ」の列挙**（`AGENTS.md`「どの分岐が選ばれるかを決める値の出所を変更」）——1 行も変えないのに drain 経由で初めて走る行:

1. `extend_cached_masks` の `Collapsed` 枝（drain からは今まで一度も呼ばれていない）
2. `SearchEngine::new_with_cached_masks` の `Collapsed` 腕（drain 経由では初）
3. `PathStore::adopt` が `sorted_by_path = false` の木に対して呼ばれる組み合わせ（起動経路では既に生きているが、drain の `rebuild_and_save` が返す木は整列済みで、PATH 併合後に false へ落ちる——起動経路と同じ形になる）
4. **migemo 有効時の不揃い**（独立導出の ⚠ 5・静的読解のみで未実測）: `new_with_cached_masks` は kana 系 2 本を**拡張後の木から**フルサイズで作る一方、渡された `char_masks` 等は短いままになりうる。「一部だけフルサイズ」の状態は Phase 3 の検知器が `[false, true]` の両方を通すことで覆う。**変異試験は migemo 両設定で行う**（片方だけ緑になる形を排除する）

## テスト方針と検証コマンド

- 新規・既存の検知器はいずれも `snotra-core` のユニットテスト（`search/tests/build.rs`）。`migemo_enabled` は `[false, true]` の両方を通す（既存 fixture の作法）
- 検証コマンドの本体は `docs/build-commands.md` カテゴリ A（`.rs`）＋ F（`.md`）。PostToolUse hook が fmt / clippy / test を自動発火するので**沈黙は合格**。`cargo doc` と `governance:check` は hook が発火しないため手で走らせる
- カテゴリ C（smoke）: `src-tauri/**` を含むため PR では `Smoke` workflow が paths で自動起動する。ローカルでの `smoke:startup` は任意（窓生成・ホットキー・表示経路に触らないため）
- カテゴリ D（目視）: **該当しない**（UI のスタイル・レイアウト・テキスト表示に触らない）

## `SPEC.md`・関連文書の更新要否

- **`SPEC.md`: 更新不要。** 挙動は変わらない（同じ索引を、同じ入力から、建て直さずに得るだけ）。`SPEC.md` 記載のフロー・状態遷移に差分は無い
- **`docs/architecture.md`: 更新不要**（`PrebuiltIndex` の名指しが無いことを Phase 4 で grep 確認する）
- 上の「文書」表が更新対象の全量

## 未確定（実装前に潰す）

- [x] **U1: `PERFORMANCE.md` の候補行の行き先** → **選択肢 A を採用**（2026-08-08・ユーザー判断「A: 未実測と明記して「採用」へ」）。**却下理由（B）**: 代償 3 つのうち (2) が決定的——`derive_columns` を `pub(crate)` に留めている理由そのもの（検知器が I/O と型に無いロック契約を巻き込まないこと・`indexer.rs:772-779`）が、可視性を開けた時点で形骸化する。額の情報量（反復 11 と同一の導出の再確認）がその損失に見合わない。**Phase 4 の書き方**: 「採用」節へ移し、数字の欄は **`未実測`**、額は反復 11 の実測（構築段 539 → 24 ms・allocs -2,716,070・peak -83.27 MiB）を**引用**して「同一の導出である」と述べ、独立に測っていない理由（同じ関数列を通る／**起動レイテンシではない**ため予算の判断材料にならない）を併記する。**見積もり欄の「起動経路と同額」をそのまま数字の欄へ移してはならない**（予測が実測の書に住み着く #977 型の誤り）
**判断の経緯（記録）** — 上の `- [x]` が結論であり、以下は却下した側を含む審議の記録である（作業項目ではない）:

  - 争点: 同節の撤去条件は「採用」節か「試みたが機能しない手法」節への移動を要求し、「どちらでもないまま残してはならない」と書く。一方その節の存在理由は「**実測と見積もりを分けてある**」であり、既存の「採用」項目はすべて実測表を持つ。見積もり（「起動経路と同額」）を実測の節へそのまま持ち込むと、#977 と同型の誤り（予測が実測の書に住み着く）になる。
  - **選択肢 A（推奨）**: 「採用」節へ移し、**数字の欄を「未実測」と明記**して、額は反復 11 の実測（構築段 539 → 24 ms・allocs -2,716,070・peak -83.27 MiB）と**同一の導出**であることを引用で示す。この枝を独立に測らない理由も書く（同じ関数列を通り、かつ**起動レイテンシではない**ため予算の判断材料にならない）。追加の足場を作らない
  - **選択肢 B**: 計測ハーネスを新設して実測する。`snotra-core/tests/memory_footprint.rs` の計数アロケータを倣い、`rebuild_and_save` の返りに対して A（`new_from_tree`）と B（`new_with_cached_masks`）を 1 プロセスで測る。**代償が 3 つある**:
    1. `IndexTree` / `CachedMasks` はどちらも `Clone` を持たないので、**木を 2 度建てる（＝フルスキャン 2 回）か `Clone` を足す**かが要る
    2. `derive_columns` / `sort_entries_canonical` を `pub(crate)` から `pub` へ開ける必要が生じうる。**これは悪い取引である**——`derive_columns` を分けた理由そのものが「検知器がファイルシステムと、型に無いロック契約を巻き込まないようにする」ことだから（`indexer.rs:772-779` の doc）。可視性を開けるとその分離が形骸化する
    3. `AGENTS.md`「一時的な足場」の規約により、撤去条件を自前の doc へ書く義務が乗る
  - 潰し方: ユーザーへ 1 問だけ聞く（Step 5c の承認と同時）とした。**実施済み** — A が選ばれ、計測 Phase は増えていない

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の 3 条件に該当——公開 API の変更〔`rebuild_and_save` の署名・`PrebuiltIndex::from_cache` の新設〕／複数モジュール間インターフェースの変更〔snotra-core ↔ src-tauri〕／ガバナンス文書の変更〔`PERFORMANCE.md`・`CLAUDE.md` 2 枚〕）
- plan-review: **独立レビュー 1 体（Step 2b・独立導出）**。網羅性（この変更で偽になる散文の全量）が今回の核心であるため、計画準拠の Step 2 ではなく独立導出を選んだ
- エージェント数: 1
- 主エージェントの自己照合（5 項目）:
  1. **issue の全要件に作業項目が対応する** — 撤去条件は 2 つ（`indexer.rs` の doc から「受容する残余」が消える／`PERFORMANCE.md` の候補行が「採用」か「試みたが機能しない手法」へ移る）。受け入れ条件 3・4 と Phase 4 が対応する
  2. **境界条件と検証** — `masks = None`（初回起動・派生文字列を持たない古い版）／`entries` が空／`migemo_enabled` の真偽。前 2 者は新設検知器と `extend_with_roots` の早期 return（`index_tree.rs:404` 実測）、3 つ目は既存 fixture の `for migemo_enabled in [false, true]` が覆う
  3. **新しい状態・リソース・プロセスの正常/失敗/破棄経路** — 該当なし。`CachedMasks` は `drain_index` のループ 1 反復に閉じたローカルで、`INDEX_WRITE_LOCK` の保持範囲も変わらない（`with_index_write_lock` のクロージャの返り値が 1 要素増えるだけ）
  4. **より単純な既存パターンで置き換えられないか** — `save_cache_sorted_in` の `(IndexTree, CachedMasks)` と `Engine::new_from_cache` がすでにその形であり、今回はそれを倣うだけ。新しい概念を持ち込まない
  5. **壊してはならない不変条件に検知手段がある** — 「マスクと木の長さが揃う」は release で `debug_assert` が消えるため、Phase 3 の検知器 ＋ **呼び忘れを再現する変異で赤になることの実測**が唯一の担保
- `/dry-check` の結果: **手書き重複は変更後に残らない。** `extend_with_roots` の製品呼び出し点は `src-tauri/src/indexing.rs:107` と `src-tauri/src/main.rs:202` の 2 つだけ（grep 実測）で、両方が `merge_path_entries` へ置換される。`extend_cached_masks` の製品呼び出し点は `main.rs:198` の 1 つのみ。`snotra-core/src/indexer.rs:3872`/`3929`/`4078` の 3 件は `extend_cached_masks` 自身の単体検査ゆえ [維持]（primitive を直接測るのが正しい粒度）
- `/symmetric-check` の結果: 対称ペアは「マスクへ追記／木へ追加」の 1 組で、変更後は 1 関数の内側に入るため片側だけの適用が**書けなくなる**。同型ペアの取り違え（Step 2c）は `merge_path_entries` の 3 引数が相異なる型（`&mut IndexTree` / `Option<&mut CachedMasks>` / `Vec<AppEntry>`）ゆえ成立しない。リソースライフサイクル対称（Step 2b）は該当なし。**順序の不変条件は所有権が強制するため検知器を置かない**（Phase 3 の記述が根拠）
- 独立レビューの成果物: `workspace/plan-review-984-derivation.md`（要対処 8 / 軽微 4 / 未検証 3 / ⚠ 5）
- 要対処: **8 件すべて反映済み。うち 2 件はこの計画の設計判断を覆した。**
  1. **D1 の反転**（要対処 1・未検証 3）——`rebuild_and_save` の返りは `Option` 付き。**一次証拠（`indexer.rs:761-770`）で裁定し、私の当初判断が誤りだった**。誤りの機序は `save_cache_sorted_in` だけを読んで包み側 `save_cache_sorted` へ一般化したこと
  2. **D3 の反転**（要対処 3）——`PrebuiltIndex::from_tree` は削除できない（`None` 枝の受け皿）。doc の書き換えに変更
  3. 要対処 2 / 7 / 8 は元から計画にあった（doc の全量表）
  4. 要対処 4 / 6 は D2（共有ヘルパー）と Phase 3（検知器＋変異試験）が対応。**独立導出も同じ形を独立に導いた**（軽微 3 が「呼び忘れ・順序反転のクラスを構造的に表現不能にできる」と提案しており、D2 と一致した）
  5. 要対処 5 の帰結（release では添字外 panic → `panic = "abort"` でプロセス終了）を「不変条件と異常系」表へ取り込んだ
- 軽微 4 件: 1（`indexing.rs:41` の panic 発火点列挙）・2（`src-tauri/CLAUDE.md` の `PrebuiltIndex::new`）は変更表に反映済み。3 は D2 と一致。4（`docs/design/…`）は触らない判断を根拠つきで記載
- ⚠ 5 件: 1（設計記録の凍結性）→「触らないもの」で回答。2（`CachedMasks` の doc の例示）→ 文書表へ追加。3（`from_cache` の署名）→ D3 で確定。4（`.get(i)` ガードのある別経路があるか）→ **未検証のまま残す**（下記）。5（migemo 有効時の不揃い）→「初めて生きる組み合わせ」4 へ取り込み、変異試験を両設定で行う
- 未検証（受容する残余として明記する）:
  - **⚠ 4**: マスク長不一致の帰結を `scoring.rs:331-332` の 1 経路でしか追っていない。他に `.get(i)` でガードされ「結果が静かに欠ける」形になる経路が在るかは未確認。**受容する理由**: どちらの帰結でも検知器の要件は同じ（長さが揃わないことを検出する）であり、Phase 3 の変異試験は帰結の形に依存しない。全経路の読み切りは額に見合わない
  - 独立導出の未検証 2（ベースラインの green）は**解消済み**——worktree 作成直後に `cargo test -p snotra-core` を実行し exit 0 を実測した
  - **U1（`PERFORMANCE.md` の行き先）が未確定のまま残る**——ユーザーへの 1 問で潰す

## Step 4 レビュー記録（実装後）

### 4a. check スキル（実装差分に対して再実行）

- `/dry-check`: **違反なし。** `extend_cached_masks` / `extend_with_roots` の製品呼び出し点は `merge_path_entries` の内側だけ（grep 実測）。`indexer.rs` の 3 件は `extend_cached_masks` 自身の単体検査、`build.rs` の 2 件は A/B 検知器の **A 側**（独立オラクルゆえ迂回が正しい）→ どちらも [維持]
- `/symmetric-check`: 対称ペア「マスクへ追記／木へ追加」は 1 関数の内側。同型ペアの取り違え（Step 2c）は引数の型が相異なるため不成立。**順序の不変条件は所有権が強制するので検知器を置かない**（Phase 3 の記述が根拠）
- `/persistence-check`・`/race-check`・`/state-check`: **該当なし**（`index.bin` に触らない／並行構造を変えず `cached_masks` はループ 1 反復のローカル／UI モードとガードに触らない）

### 4b. code-reviewer（ラウンド 1）

`workspace/code-review-984.txt`。**Critical 0 / High 0 / Medium 3 / Low 5 / ⚠ 3。** 挙動に関わる 3 点（消えた `!is_empty()` ガード／消えた警告の再確立地点／`as_mut()` の順序）はいずれも呼び先のコードで検算され適合。**副産物として `extend_with_roots` の早期 return が末尾の `*sorted_by_path = false` より前にあることが load-bearing だと判明した**——逆だったら「PATH が空でも整列済みの木が遅い経路へ落ちる」退行になっていた。

当てた修正（[ ] は残す判断）:

- [x] **M1**: 署名を `Option<&mut CachedMasks>` → **`&mut Option<CachedMasks>`**。D1 の反転で製品の呼び出し点はどちらも `Option` の束縛を持つようになり、`as_mut()` を書く手が消える＝「`Some` を持ちながら `None` を渡す」形が書けなくなる。代償は検知器 1 か所の包み直し（`unwrap` 1 つ）
- [x] **M2**: `IndexTree::materialize` の doc の自己矛盾（第 1 段落が `from_tree` を通る経路に挙げ、第 2 段落が「設定からの再構築はもう通らない」と書いていた）。**D3 を反転させた当の誤読を再導出させる形だった**
- [x] **M3**: 「現存する **2 つ**の呼び出し点」を 4 か所へ写していた。**同じ差分の別の 2 か所で「数え上げてはならない」と書いている**——`docs/comment-guidelines.md` 第一原則が名指しで禁じた形。数詞を落とした
- [x] **L1**: `indexing.rs` の見出し参照が改行をまたぎ `governance-check` の `HEADING_REF` の**母集団に入っていなかった**（照合失敗ではなく不可視）。1 段落 1 行へ直して解消——`見出し参照 175 → 176 件`が実測証拠
- [x] **L2**: 新設検知器の A/B 比較は `masks = None` では原理的に失敗しない。**落とさず、何を測っているかを doc に書いた**（`tree.len()` が効いており、A/B は「根として足す」が親解決へ変わる別の壊し方を捕まえる）
- [x] **L3**: `PERFORMANCE.md` の時制（同じ差分が候補行を削除している事実と食い違っていた）
- [x] **L4**: `snotra-core/CLAUDE.md` の同一 bullet に数詞が 3 つ並び「1 本だけ」の射程が読めなかった。検知器を名前で挙げて数詞を落とした
- [x] **L5**: `search/tests/performance.rs` が製品の構築を `PrebuiltIndex::new` と名指ししており、同関数の doc「製品経路は通らない」と矛盾。**この差分は同じ stale な事実を 2 枚直して 3 枚目を落としていた**（#977 と同型）。計画の除外理由「`new` は残るので偽にならない」は、偽になっている主張を取り違えていた
- [x] **⚠1**: `from_tree` の doc の「到達不能ではない」が第一原則の禁止列（到達可能性）に字面上当たる。誤読の予防価値は残したまま、その句を落として分岐の正本を指す形へ
- [x] **⚠2**: **レビュアーの「この差分の欠陥として挙げるのは不当」という判断は誤りだった。** `docs/comment-guidelines.md:53` は「1 段落 1 行。**適用は新規に書くコメントと、その変更で触った段落だけである**」と書き、「既存が手折りだから揃える」という緩和を規約自身が先に潰している。新規・touched の doc 段落をすべて 1 行へ直した
（⚠3 は作業項目ではないので下の「受容した残余」へ移した——チェックボックスのまま残すと `pre-bash` hook が `gh pr create` を拒否する・`/code-review` の指摘 [8] で実測）

### 受容した残余（作業項目ではない・チェックボックスを持たない）

- **⚠3**: `Some(CachedLower::Raw)` が drain 経路へ来る組み合わせ。`Raw` 腕は単体検査とキャッシュヒットで既に生きており、追記と構築は独立ゆえ害が無い。レビュアーも静的読解のみと明記しており、追う額に見合わない
- **⚠4**（`/code-review` ラウンド 1）: マスク長不一致の帰結を `scoring.rs` の 1 経路でしか追っていない。**Phase 6 で意味が変わった**——`IndexMaterial::from_untrusted` がディスク側の入口で列長を検証するようになったので、この帰結に至る経路自体が塞がった（残るのは crate 内でマージを迂回した場合だけ）
- **crate 内からのマージ迂回**: `IndexTree::extend_with_roots` は `pub(crate)` ゆえ `snotra-core` の中からは呼べる。現状その呼び出し点は無く（検知器の A 側オラクルのみ）、可視性をさらに絞ると A/B 比較の独立性が失われる

**レビュアーが実施できなかった検証**: 変異注入（サンドボックスの分類器が `perl -i` を拒否）。ゆえに変異試験の一次証拠は主エージェント側の実測（Phase 3）だけである。**独立な再現ではない**ことを残余として明記する。

## Phase 6 — `IndexMaterial` へ束ねる（レビュー後の設計転換）

**ユーザー指示（逐語）**: "2件ともやろう。設計の背景深堀してより良い案があればそちらを採用しよう"

### 背景の調査結果（着手前）

- **指摘 [9] の提案（`PrebuiltIndex::from_parts`）は 5 か所のうち 1 か所しか直せない。** 同じ `Option` 分岐は **3 つの型の層**に散っている: `PrebuiltIndex::{from_cache, from_tree}`（`indexing.rs:152`）・`Engine::{new_from_cache, new_from_tree}`（`main.rs:211` / `path_query_cost.rs:190`）・`SearchEngine::{new_with_cached_masks, new_from_tree}`（`memory_footprint.rs:353` / `path_query_cost.rs:248`）。根は最下層の 2 コンストラクタで、上の 2 層はその写しである。**この却下は否定の知識ゆえ `IndexMaterial` の doc へ 1 行残す**（ADR は起こさない）
- **束ねる案はこのリポジトリが既に採用している原理の 1 段上への適用である。** `SearchEngine::new_with_cached_masks` の doc（`search/build.rs:418-422`）が「**組のまま受け取る。** 3 引数へほどく形だと……**取り違えてもコンパイルが通る**……境界を跨ぐ手前でほどかない」と書き、`DerivedColumns` の doc も「**タプルにしてはならない**……同じ理由で `CachedMasks` は組のまま渡す」と書く。**却下の ADR は無い**（`docs/adr/` 38 本を grep・0 件）
- 名前は発明ではない——`LoadOrScanResult.tree` の doc が既にこの組を「**索引の材料**」と呼んでいる

### 設計判断

- **D4: `IndexMaterial { tree, masks }` を `indexer.rs` に新設し、フィールドを private にする。** これで「木を伸ばしたのにマスクを追記し忘れる」が**本当に表現不能**になる（`&mut Option<_>` への署名変更では閉じていなかった・指摘 [4]）。`merge_path_entries` は `IndexMaterial::extend_with_path_entries(&mut self, ..)` へ移す
- **D5: 型が不変条件を所有する。** `IndexMaterial::from_untrusted(tree, masks) -> Option<Self>` を作り、**ディスクから来る全枝をそこへ通してマスク列長を検証する**（`load_cache_in` は今まで `IndexTree::from_parts` で木の整合しか見ておらず、切り詰められた `index.bin` が release で添字外 panic になりえた・指摘 [6]）。`derive_columns` の出力は構成上正しいので中身検証を持たない `pub(crate)` の口を通す。**これで「長さが揃う」が散文の約束から真の主張へ変わる**
- **D6: 消す/絞る対象は grep で数え上げてから決める**（D3 の誤りの再戦）。`PrebuiltIndex::{from_cache, from_tree}` と `Engine::{new_from_cache, new_from_tree}` は `from_material` が代替したら削除、`SearchEngine` の 2 本は **A/B 検知器が 2 経路を別々に要するので残す**（`pub(crate)` へ）、`IndexTree::extend_with_roots` は `pub(crate)`、`PrebuiltIndex::new` は指摘 [2] のとおり `#[cfg(test)]`

### 写しの母集団（編集前に grep で数え上げた）

- **「表現不能化ではない」系の残余宣言 = 4 件**（`indexer.rs:1671` / `snotra-core/CLAUDE.md:46` / `PERFORMANCE.md:634-635` / `search/tests/build.rs:237`）。**D4 でこの 4 件は消える**（count 修正ではなく削除）
- **`PERFORMANCE.md` の 1 件は行をまたいで書いてあり grep に当たらなかった**——`.rs` で直したばかりの L1 と同じ機序が `.md` で再発していた。**母集団を取る grep 自身が取りこぼす**という一段深い事実
- **「もうここを通らない」= `search/build.rs:377` の 1 件**（指摘 [1]）。`search/build.rs:301` は別の主題（`Collapsed` の話）で写しではない
- **「併合」= 17 件。うち 2 件は既存**（`PERFORMANCE.md:794` の見出しと `RETROSPECTIVE.md:23`）——**指摘 [10] の検証者はこれを見ていない**。`:794` の見出しはどこからも参照されていないので改題は自由（正準参照の対象外・grep 実測）

### 作業項目

- [x] `IndexMaterial` を新設（`from_tree` / `derived` / `from_untrusted` の 3 系統）
- [x] `merge_path_entries` を `IndexMaterial::extend_with_path_entries` へ移し、`extend_with_roots` を `pub(crate)` へ
- [x] `load_cache_in` の全枝を `from_untrusted` へ通す（v7〜v3 の 5 枝。v2 はマスクを持たないので `from_tree`）
- [x] `SearchEngine::from_material` / `Engine::from_material` / `PrebuiltIndex::from_material` を足し、**5 か所の `match` を 0 にした**
- [x] `rebuild_and_save` と `LoadOrScanResult` を `IndexMaterial` へ（`LoadOrScanResult` の `tree` / `cached_masks` の 2 フィールドが `material` の 1 つになった）
- [x] 旧 API を削除／可視性を絞った（compile-fail が移行漏れを名指しした）: `PrebuiltIndex::{from_cache, from_tree}` と `Engine::{new_from_cache, new_from_tree}` を削除、**呼び出し元ゼロだった `indexer::load_or_scan` も削除**（同時に `memory_footprint.rs` がその名を参照していた散文も直した——放置すれば実在しない API を指す散文を新たに作ることになる）、`PrebuiltIndex::new` を `#[cfg(test)]` へ（指摘 [2] のとおり「統合テストがリンクする」という理由は実例ゼロだった）
- [x] **変異試験を再注入した**（Phase 3 の証拠は関数名が変わった時点で失効するため）:
  - 変異 1（追記を欠く）→ `path_merge_after_cache_miss_...` が `assemble` の `debug_assert` で赤。**migemo 逆順でも赤**（ループを `[true, false]` にして再測）
  - 変異 2（木への追加を `if let Some` の内側へ）→ `path_merge_extends_the_tree_even_without_derived_data` が赤
  - **変異 3（マスクを取り落とす・新設）→ `has_masks()` の assert（`build.rs:287`）が赤。** 発火位置を実測して確かめた——**A/B 一致では捕まらない**（両側が木から導出するので一致は成立したまま削減だけが消える）。これが `has_masks` を足した根拠である
- [x] 生き残った doc 指摘を直した（[1] `search/build.rs:377` の偽の全称 / [2] / [3] v3 の到達経路 / [5] 数え上げ / [7] 残余宣言 4 か所を**削除** / [10] 訳語 / ラウンド 2 の High と Medium は該当散文ごと消えた）
- [x] `plan.md` の ⚠3 をチェックボックスの無い「受容した残余」節へ移した（指摘 [8]）
- [x] カテゴリ A・F を全件再実行（fmt OK / clippy exit 0 / core 553 passed / snotra 218 passed / `cargo doc` warning 9 件＝**自分が増やした 2 件を潰して既存のみへ戻した** / governance:check 全検査 passed・見出し参照 176 件）

## 人間レビュー

- [x] 承認済み — 2026-08-08 / 問い: "workspace/plan.md（および research.md・plan-review-984-derivation.md）を確認して、実装へ進むことを承認しますか？" / 回答: "承認する"

併せて U1 を同じ問いで確定した — 問い: "PERFORMANCE.md「次の反復の候補」の該当行の行き先をどちらにしますか？（同節の撤去条件が「採用」節か「試みたが機能しない手法」節への移動を要求し、どちらでもないまま残すことを禁じています）" / 回答: "A: 未実測と明記して「採用」へ（推奨）"

注釈は無し。ゆえに `/plan-review` の追加実行はしない（要件・対象シンボル・インターフェース・不変条件・テスト期待値のいずれも承認後に動いていない）。

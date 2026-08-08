# 実装計画: issue #984 — `rebuild_and_save` が保存の返り値を捨てている

## 目的と受け入れ条件

`rebuild_and_save` → `drain_index` の枝が、保存が計算した `CachedMasks` をそのまま索引の表現に使う。

1. `rebuild_and_save` が `(IndexTree, CachedMasks)` を返し、`drain_index` がその第 2 要素を `PrebuiltIndex` へ渡す
2. drain 側の PATH マージが `extend_cached_masks` を通る。**「構造的に不可能」と書いてはならない**——閉じるのは**現存する 2 つの呼び出し点**（`main.rs` と `indexing.rs`）が同じ 1 関数を通ることであって、`IndexTree::extend_with_roots` は `pub` のまま残るので、**将来 3 つ目の併合経路を `merge_path_entries` を通さずに書くことはできる**（`src-tauri/CLAUDE.md` が raw 窓操作について「ただし表現不能化ではない」と書くのと同じ性格の**受容する残余**）。可視性を絞る案は #984 の射程を大きく超えるので採らない
3. `indexer.rs` の `rebuild_and_save` の doc から「意図的・受容する残余」の記述が消える（issue の撤去条件）
4. `PERFORMANCE.md`「次の反復の候補」の該当行が同節から消え、行き先が確定している（同節の撤去条件）
5. `PrebuiltIndex::from_tree` を名指しする散文が 1 つも残っていない（削除するため）
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

- [ ] `indexer::rebuild_and_save` の返りを `(IndexTree, Option<CachedMasks>)` にし、doc の「受容する残余」を差し替える
- [ ] `indexer::merge_path_entries` を新設する（D2 の署名・doc に長さの不変条件を書く）
- [ ] `PrebuiltIndex::from_cache` を足し、`from_tree` の doc を D3 のとおり書き換える
- [ ] `cargo check -p snotra-core` が通る

### Phase 2 — src-tauri（呼び出し点の移行）

- [ ] `cargo build -p snotra` を先に走らせ、**compile-fail が移行漏れを名指しする**ことを確認する（`indexing.rs:102` の返り値型不一致）
- [ ] `drain_index` を `(tree, masks)` → `merge_path_entries` → `Some`/`None` の分岐へ繋ぐ。**`masks.as_mut()` の可変借用が、直後の `match masks`（値で消費）より前に終わることを実測で確かめる**（NLL で通る見込みだが、`Option` 化で唯一署名が刺されうる箇所ゆえ仮定しない）
- [ ] `main.rs` の PATH マージを `merge_path_entries` へ置換する（`!is_empty()` ガードの削除は D2 の根拠を持つ挙動不変の単純化）
- [ ] `indexing.rs:41` のコメントの関数名を直す
- [ ] `cargo check --workspace` が通る

### Phase 3 — 検知器と変異試験

- [ ] 既存検知器の B 側を `merge_path_entries` 経由へ書き換え、doc を差し替える
- [ ] `masks = None` の腕の検知器を足す
- [ ] `cargo test -p snotra-core` が green
- [ ] **変異試験**: `merge_path_entries` から `extend_cached_masks` の行を消して `cargo test -p snotra-core` が **赤になる**ことを実測し、結果をこの計画へ書き戻してから元に戻す（呼び忘れが検知されることの唯一の証拠）

**順序の変異試験は置かない**（`/symmetric-check` Step 2c の結果）。`extend_cached_masks(masks, &entries)` → `tree.extend_with_roots(entries)` の順序は**所有権が強制する**——`extend_with_roots` は `Vec<AppEntry>` を値で取るため、逆順にするには `entries.clone()` が要る。**入れ替えは「うっかり」では書けない**ので、検知器を置く対象が無い。意味の上でも両者は独立である（`extend_cached_masks` は木を読まず、`extend_with_roots` はマスクを読まない）。守るべき順序の不変条件は `derive_entry_collapsed` の内側（潰す前にマスクを取る）にあり、そちらは既存の `derived_masks_come_from_the_uncollapsed_strings` が守る。

### Phase 4 — 文書

- [ ] 上の「文書」表の 8 か所を直す
- [ ] `PERFORMANCE.md` の候補行を「採用」節へ移す（**数字の欄は `未実測`**・書き方の全文は U1 の `- [x]` が正本）
- [ ] `npm run governance:check` が green
- [ ] `cargo doc --workspace --no-deps --document-private-items` が green（intra-doc link 切れ——hook は沈黙する）

### Phase 5 — 全体検証

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test -p snotra-core` / `cargo test -p snotra`
- [ ] 実装差分を確定させる（未コミットの差分に対して `git diff` を**引数 1 個の形**で読む——`main...HEAD` は commit 同士の比較ゆえ作業ツリーを見ない・#922）

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
  - **潰し方**: ユーザーへ 1 問だけ聞く（Step 5c の承認と同時）。A なら Phase 4 のみ、B なら Phase 4 の前に計測 Phase が 1 つ増える

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

## 人間レビュー

- [x] 承認済み — 2026-08-08 / 問い: "workspace/plan.md（および research.md・plan-review-984-derivation.md）を確認して、実装へ進むことを承認しますか？" / 回答: "承認する"

併せて U1 を同じ問いで確定した — 問い: "PERFORMANCE.md「次の反復の候補」の該当行の行き先をどちらにしますか？（同節の撤去条件が「採用」節か「試みたが機能しない手法」節への移動を要求し、どちらでもないまま残すことを禁じています）" / 回答: "A: 未実測と明記して「採用」へ（推奨）"

注釈は無し。ゆえに `/plan-review` の追加実行はしない（要件・対象シンボル・インターフェース・不変条件・テスト期待値のいずれも承認後に動いていない）。

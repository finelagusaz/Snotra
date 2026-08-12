# `kana_lower_names` を密な文字列アリーナで持つ 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `SearchEngine.kana_lower_names` を `Vec<Box<str>>` から `index_tree::NameArena` へ移し、索引に残る最後の per-entry 確保（migemo 有効時のみ現れる 312,108 個）を消す。

**Architecture:** 表示名が使っている密なアリーナ（連結バイト列 + `u32` オフセット列）をそのまま kana 列へ適用する。この列は `index.bin` に保存しないので、線上表現の一致を証明する検知器も `INDEX_CACHE_VERSION` のバンプも要らない。**唯一の非自明な制約は並列構築の保持**——毎起動の経路（`kana_for_cached`）は現在 `into_par_iter` であり、逐次 push へ落とすと `to_kana` の単価 3.08 µs × 312,108 件 ＝ 約 0.96 秒が起動に乗る。

**Tech Stack:** Rust / rayon / postcard（触らない）/ `cargo test -p snotra-core`

**設計の正本:** `docs/superpowers/specs/2026-08-12-kana-lower-names-arena-design.md`（issue #1056）

## Global Constraints

- **`main` へ直接コミットしない。** 作業ブランチは `perf/kana-lower-names-arena`（作成済み）
- **`index.bin` の形式・`INDEX_CACHE_VERSION`・`NameArena` の `Serialize` / `Deserialize` を触らない。** kana 列はディスクに存在しない
- **不変条件「kana 系 2 列は両方空 or 両方 `entries.len()`」を保つ**（`assemble` の `debug_assert` が正本・#337）
- **`footprint.rs` は「1 行 = 1 確保」で数える。** 2 つの確保を 1 行に束ねると未帰属 blocks が出る（#1003 実測）
- **計測は migemo ON で取る。** 実運用点（migemo OFF）だけを見るとこの反復は「何も変わらない」と出る
- 検証コマンドの正本は `docs/build-commands.md`。PostToolUse hook は `.rs` 編集で自動発火し、**沈黙 = 合格**

---

### Task 1: 型の移行一式（`Vec<Box<str>>` → `NameArena`）

**1 コミットで行う。** フィールドの型を変えると `build.rs` / `search.rs` / `scoring.rs` / `footprint.rs` / `tests/build.rs` が同時にコンパイルエラーになり、途中の状態は存在しない。新 API だけ先に入れる形も採れない——`-D warnings` 下で未使用の `pub(crate)` は `dead_code` で落ちる（ルート `CLAUDE.md`「関数・型を新規定義」）。

**Files:**
- Modify: `snotra-core/src/index_tree.rs`（`NameArena` の口を開ける・`//!` に一行）
- Modify: `snotra-core/src/search.rs`（フィールド型・`kana_available`）
- Modify: `snotra-core/src/search/build.rs`（`Wave1Strings` / `compute_wave1` / `compute_kana_char_masks` / `assemble` / `kana_for_cached`）
- Modify: `snotra-core/src/search/scoring.rs`（読み口）
- Modify: `snotra-core/src/search/footprint.rs`（2 行化）
- Test: `snotra-core/src/search/tests/build.rs`（既存の A/B 突き合わせが `.len()` / 添字を使っている箇所）

**Interfaces:**
- Consumes: `crate::index_tree::NameArena`（既存。`new` / `len` / `get(i) -> &str` / `blob` / `shrink_to_fit` / `footprint_bytes() -> (usize, usize)`）
- Produces:
  - `NameArena::push(&mut self, s: &str)` — `pub(crate)` へ格上げ
  - `NameArena::with_capacity(n: usize, bytes: usize) -> Self` — `pub(crate)` へ格上げ
  - `NameArena::is_empty(&self) -> bool`
  - `NameArena::from_chunks(chunks: Vec<(String, Vec<u32>)>) -> Self` — 塊ごとの（連結バイト列, 要素末尾オフセット）を順に併合する
  - `SearchEngine.kana_lower_names: NameArena`
  - `compute_kana_char_masks(kana_lower_names: &NameArena) -> Vec<u64>`

- [x] **Step 1: 併合の失敗するテストを書く**

`snotra-core/src/index_tree.rs` の `mod tests` へ追加する。**`from_chunks` はオフセットを塊の先頭ぶん底上げする**ので、境界の底上げを忘れる／二重に足す誤りがここで落ちる。

```rust
/// 塊ごとに組んだアリーナを併合しても、1 本で組んだものと同じ切り出しになる。
///
/// **塊の境界を 1 つでは足りない**——先頭の塊だけ底上げが 0 で正しくなり、
/// 2 つ目以降の底上げの誤りが素通りする。空の塊も混ぜる（rayon の分割は均等とは限らない）。
#[test]
fn from_chunks_concatenates_without_shifting_offsets() {
    let names = ["projects", "", "ünïcode 名前", "アプリ", "c:\\dir\\sub", "tool.exe"];

    let mut flat = NameArena::new();
    for s in names {
        flat.push(s);
    }

    let chunks: Vec<(String, Vec<u32>)> = [&names[0..2], &names[2..2], &names[2..5], &names[5..6]]
        .iter()
        .map(|part| {
            let mut blob = String::new();
            let mut offsets = Vec::new();
            for s in *part {
                blob.push_str(s);
                offsets.push(blob.len() as u32);
            }
            (blob, offsets)
        })
        .collect();
    let merged = NameArena::from_chunks(chunks);

    assert_eq!(merged.len(), names.len(), "併合で件数が変わった");
    for (i, want) in names.iter().enumerate() {
        assert_eq!(merged.get(i), *want, "{i} 番の切り出しがずれた");
    }
    assert_eq!(merged.blob(), flat.blob(), "連結バイト列が 1 本組みとずれた");
}
```

- [x] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra-core from_chunks_concatenates_without_shifting_offsets`
Expected: FAIL（`no function or associated item named 'from_chunks' found`）

- [x] **Step 3: `NameArena` の口を開ける**

`snotra-core/src/index_tree.rs`。既存の `fn push` / `fn with_capacity` の `fn` を `pub(crate) fn` にし、以下を `impl NameArena` へ追加する。

```rust
    /// 要素を 1 つも持たないか。
    ///
    /// **kana 列の「migemo 無効」を表す**（`SearchEngine::kana_lower_names`）。
    /// `len() == 0` と同値だが、呼び出し側の意図が「空か」であることを型の側に置く。
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 塊ごとに並列で組んだ `(連結バイト列, 要素末尾オフセット)` を順に併合する。
    ///
    /// **アリーナへの push は逐次だが、逐次化してはならない。** kana 列を毎起動で組む
    /// `SearchEngine::new_with_cached_masks` は `to_kana`（実測 3.08 µs/件）を全件へ当てるので、
    /// 逐次化すると 31 万件で約 1 秒が起動に乗る。塊の中だけ並列に組み、ここで繋ぐ。
    ///
    /// **渡す側が順序を保つ契約である**——`collect` が順序を保つ indexed parallel iterator を
    /// 通すこと。ここは受け取った順に繋ぐだけで、並べ替えない。
    pub(crate) fn from_chunks(chunks: Vec<(String, Vec<u32>)>) -> Self {
        let total_bytes: usize = chunks.iter().map(|(blob, _)| blob.len()).sum();
        let total_len: usize = chunks.iter().map(|(_, offsets)| offsets.len()).sum();
        let mut arena = Self::with_capacity(total_len, total_bytes);
        for (blob, offsets) in chunks {
            let base = arena.blob.len() as u32;
            arena.blob.push_str(&blob);
            arena.offsets.extend(offsets.into_iter().map(|o| o + base));
        }
        arena
    }
```

- [x] **Step 4: 通ることを確認する**

Run: `cargo test -p snotra-core from_chunks_concatenates_without_shifting_offsets`
Expected: PASS

- [x] **Step 5: `//!` に消費者を一行足す**

`snotra-core/src/index_tree.rs` の `//!` は「オンディスクと索引が共有する表現」と述べている。`NameArena` にメモリ専用の消費者が付いたので、その節へ次を足す。

```
//! **`NameArena` にはディスクを通らない消費者が 1 つある**——`SearchEngine::kana_lower_names`
//! （migemo 有効時のみ）。あちらは `index.bin` に保存しないので serde impl を通らず、
//! 線上表現の制約もかからない。**表現の核が同じであることだけを共有している。**
```

- [x] **Step 6: 構築側を移す（`build.rs`）**

`Wave1Strings` の 3 要素目、`compute_wave1` の kana 枝、`compute_kana_char_masks`、`assemble` の引数と `shrink_to_fit` / `debug_assert`、`kana_for_cached` を替える。

```rust
type Wave1Strings = (Vec<Box<str>>, Vec<Option<Box<str>>>, NameArena);
```

`compute_wave1` の kana 枝（`rayon::join` の第 2 引数）:

```rust
        || {
            // migemo 無効時は kana を構築しない（空のアリーナ）。
            if migemo_enabled {
                // **ここは逐次のままでよい。** この枝が通るのは cache-miss（走査 22〜30 秒）の
                // 内側だけで、毎起動の経路は `kana_for_cached` である（そちらは並列を保つ）。
                let mut arena = NameArena::with_capacity(entries.len(), 0);
                for e in entries {
                    arena.push(&to_kana(&to_lower_folded(&e.name)));
                }
                arena
            } else {
                NameArena::new()
            }
        },
```

`compute_kana_char_masks`:

```rust
/// migemo 有効時の kana pre-filter 用並列 Vec を構築する。kana 未構築時は空 Vec を保つ。
fn compute_kana_char_masks(kana_lower_names: &NameArena) -> Vec<u64> {
    (0..kana_lower_names.len())
        .map(|i| kana_char_mask(kana_lower_names.get(i)))
        .collect()
}
```

`assemble` の引数と本体（`kana_lower_names` は `Vec` ではなくなるが `shrink_to_fit` / `len` / `is_empty` は同名で在る）:

```rust
        kana: (NameArena, Vec<u64>),
```

`kana_for_cached`（**並列を保つ**。`chunks` は indexed parallel iterator の順序保存に依存する）:

```rust
        // kana は毎起動再計算する（キャッシュに持たない）。migemo 無効時は空のアリーナのまま。
        let kana_for_cached = |tree: &IndexTree| {
            if migemo_enabled {
                // **塊ごとに並列で組んでから併合する。** アリーナへの push は逐次だが、
                // `to_kana` は 3.08 µs/件（実測）で、31 万件を逐次で回すと約 1 秒が
                // **毎起動に**乗る。`chunks` + `collect` は順序を保つので、併合は受け取った
                // 順に繋ぐだけでよい（`NameArena::from_chunks`）。
                const CHUNK: usize = 4096;
                let chunks: Vec<(String, Vec<u32>)> = (0..tree.len())
                    .into_par_iter()
                    .chunks(CHUNK)
                    .map(|idxs| {
                        let mut blob = String::new();
                        let mut offsets = Vec::with_capacity(idxs.len());
                        for i in idxs {
                            blob.push_str(&to_kana(&to_lower_folded(tree.name_at(i))));
                            offsets.push(blob.len() as u32);
                        }
                        (blob, offsets)
                    })
                    .collect();
                NameArena::from_chunks(chunks)
            } else {
                NameArena::new()
            }
        };
```

**`use` を足す**: `build.rs` 冒頭の `crate::index_tree::...` の import へ `NameArena` を加える。`rayon::iter::IndexedParallelIterator`（`chunks` のため）も必要なら足す。

- [x] **Step 7: 読み側を移す（`search.rs` / `scoring.rs`）**

`search.rs` のフィールド宣言:

```rust
    /// エントリ名をひらがな正規化したアリーナ（katakana→hiragana、ASCII はそのまま）。
    /// migemo 検索（ローマ字→かな変換マッチ）で使用。インデックスキャッシュには保存しない
    /// ——**ゆえに線上表現の制約はかからない**（`crate::index_tree::NameArena` の `//!`）。
    kana_lower_names: NameArena,
```

`search.rs` の `kana_available`:

```rust
        let kana_available = !self.kana_lower_names.is_empty();
```

`scoring.rs` の読み口:

```rust
                .and_then(|kq| kana_substring_score(self.kana_lower_names.get(i), kq))
```

- [x] **Step 8: `footprint.rs` を 2 行にする**

現行の `boxed_strs` + `vec_body::<Box<str>>` の 2 行を、blob と offsets の 2 行へ置き換える。**束ねて 1 行にしない**（1 行 = 1 確保）。

```rust
        // migemo 無効なら 3 本とも 0 で、行は残る（**消さない**——「測って 0 だった」と
        // 「測っていない」は別物であり、消すと後者に見える）。
        //
        // **アリーナは 2 行に分ける。** `arena_part` は「1 行 = 1 確保」でブロックを数えるので、
        // blob とオフセットを束ねると 1 つ数え落とす（#1003 で未帰属 +1 blocks として実測）。
        let (blob, offsets) = kana_lower_names.footprint_bytes();
        rows.push(arena_part(
            "kana_lower_names（アリーナの連結バイト列）",
            blob,
            kana_lower_names.len(),
        ));
        rows.push(arena_part(
            "kana_lower_names（アリーナのオフセット）",
            offsets,
            kana_lower_names.len() + 1,
        ));
        rows.push(vec_body::<u64>(
            "kana_char_masks",
            kana_char_masks.capacity(),
        ));
```

**`boxed_strs` の呼び出しが 0 件になったら、その関数ごと消す**（`-D warnings` の `dead_code` が教える）。他に呼び出しが残っているなら残す——`grep -n "boxed_strs" snotra-core/src/` で数える。

- [x] **Step 9: 既存テストの添字を直す（`tests/build.rs`）**

`a.kana_lower_names[i]` → `a.kana_lower_names.get(i)`、`.len()` / `.is_empty()` はそのまま通る。**A/B 突き合わせの構造は変えない**——A 側（`compute_wave1`・逐次）と B 側（`kana_for_cached`・塊併合）が 2 実装のままであることが、Step 6 の併合のずれを捕まえる唯一の経路である。

- [x] **Step 10: 全テストと clippy を通す**

Run: `cargo test -p snotra-core`
Expected: PASS（585 本）

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 警告なし

Run: `cargo doc --workspace --no-deps --document-private-items`
Expected: intra-doc link エラーなし（**PostToolUse hook は `cargo doc` を走らせない**。手で打つ）

- [x] **Step 11: コミット**

```
perf(core): kana_lower_names を密な文字列アリーナで持つ（残る唯一の per-entry 確保）
```

本文には「並列構築を塊併合で保った」ことと理由（`to_kana` 3.08 µs/件 × 31 万件）を書く。

---

### Task 2: 余剰容量の検知器を kana 列へ広げる

`assemble` の `shrink_to_fit` を落としても検索結果は変わらない（余剰容量が最後まで常駐するだけ）。既存の検知器が `lower_names` / `lower_file_names` を見ているのと同じ形で kana 列も見る。

**Files:**
- Modify: `snotra-core/src/index_tree.rs`（`NameArena::excess_capacity_bytes` を `#[cfg(test)]` で追加）
- Test: `snotra-core/src/search/tests/build.rs`

**Interfaces:**
- Consumes: `NameArena`（Task 1 の形）
- Produces: `NameArena::excess_capacity_bytes(&self) -> usize`（`#[cfg(test)]`）

- [x] **Step 1: 失敗するテストを書く**

`snotra-core/src/search/tests/build.rs` へ追加する。既存の余剰容量テストの隣に置く（`grep -n "excess_capacity" snotra-core/src/search/tests/build.rs` で位置を見る）。

```rust
/// **kana 列も `shrink_to_fit` の対を持つ。** 索引は構築後に伸長しないので、余剰容量は
/// 最後まで解放されない常駐である（`assemble` の doc）。**検索結果は変わらないので
/// 挙動テストでは捕まらない。**
#[test]
fn kana_arena_has_no_excess_capacity_after_assemble() {
    let entries = common::make_entries(&["ドキュメント", "設定", "firefox", "アプリ"]);
    let engine = SearchEngine::new_with_migemo(entries, true);
    assert!(
        !engine.kana_lower_names.is_empty(),
        "migemo 有効で kana が空では検査が空虚である"
    );
    assert_eq!(
        engine.kana_lower_names.excess_capacity_bytes(),
        0,
        "kana アリーナに余剰容量が残っている（assemble の shrink_to_fit を確認する）"
    );
}
```

- [x] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra-core kana_arena_has_no_excess_capacity_after_assemble`
Expected: FAIL（`no method named 'excess_capacity_bytes'`）

- [x] **Step 3: `excess_capacity_bytes` を足す**

`snotra-core/src/index_tree.rs` の `impl NameArena` へ。**`str_arena::OptionalStrArena` の同名メソッドと同じ理由で `footprint_bytes` では代用できない**（あちらは容量しか返さないので、`shrink_to_fit` を外しても「その容量が正しい」としか読めない）。

```rust
    /// 余剰容量（`shrink_to_fit` が消し残した分）。**検知器専用。**
    ///
    /// [`Self::footprint_bytes`] では代用できない——あちらは容量しか返さないので、
    /// `shrink_to_fit` を外しても「その容量が正しい」としか読めない。余剰は `len` との差に
    /// しか現れない（`crate::str_arena::OptionalStrArena::excess_capacity_bytes` と同じ理屈）。
    #[cfg(test)]
    pub(crate) fn excess_capacity_bytes(&self) -> usize {
        (self.blob.capacity() - self.blob.len())
            + (self.offsets.capacity() - self.offsets.len()) * std::mem::size_of::<u32>()
    }
```

- [x] **Step 4: 通ることを確認する**

Run: `cargo test -p snotra-core kana_arena_has_no_excess_capacity_after_assemble`
Expected: PASS

- [x] **Step 5: 検知器が発火することを変異注入で確かめる**

`assemble` の `kana_lower_names.shrink_to_fit();` を一時的にコメントアウトし、上のテストが落ちることを実測する。**落ちなければ検知器が効いていない**（`with_capacity` が偶然ぴったりだと余剰が 0 になりうる）。

Run: `cargo test -p snotra-core kana_arena_has_no_excess_capacity_after_assemble`
Expected: FAIL（確認したら `shrink_to_fit` を戻す）

- [x] **Step 6: コミット**

```
test(core): kana アリーナの余剰容量に検知器を置く
```

---

### Task 3: 計測（migemo ON）

**この反復は migemo OFF では自分の額を測れない。** 実運用点の計測は OFF なので、ON で取らないと「何も変わらない」と出る。

**Files:**
- 変更なし（計測のみ。結果は Task 4 で文書へ）

- [x] **Step 1: 合成ラダーを migemo ON/OFF で 3 回取る**

Run: `cargo test -p snotra-core --release --test memory_footprint -- --ignored --nocapture`
記録: `migemo=on` 行と `off` 行の **blocks の差**と live の差。**3 回でバイト数・ブロック数が完全に一致すること**を確かめる。

判定: 差が N（10 万件で 100,002）から **3 前後**へ落ちていること。3 は blob・offsets・masks の 3 確保であり、**リテラルの 0 ではない**。

- [x] **Step 2: 構築コストを取る（両経路）**

Run: `cargo test -p snotra-core --release bench_new_migemo_on_off -- --ignored --nocapture`

これは `compute_wave1` 側（逐次のまま）を見る。**毎起動の経路はこちらではない**ので、`kana_for_cached` 側をキャッシュヒット起動の実測で見る（`docs/build-commands.md` の起動段計測、`cache_load_ms` を含む段）。**逐次化していれば 31 万件で約 1 秒の差として出る。**

- [x] **Step 3: 検索レイテンシを対で取る**

Run: `cargo test -p snotra-core --release bench_fuzzy_search_scaling -- --ignored --nocapture`

**migemo ON で・同日・同セッション・各 3 標本以上。** `kana_lower_names.get(i)` は添字 2 回のスライスになるので、kana 経路を通るクエリ（ローマ字）を必ず含める。

- [x] **Step 4: A 側（変更前）と突き合わせる**

`git stash` ではなく **`git switch main` で成果物ごと A 側へ戻して**同じセッションで測る（`ab-baseline-needs-drift-control`）。A 側の標本を取ってからブランチへ戻る。

- [x] **Step 5: 数値を作業メモへ書き出す**

このファイルの下の「計測結果」節へ実測値を貼る（Task 4 で `PERFORMANCE.md` へ移す）。**測った機体名（`Get-CimInstance Win32_ComputerSystem`）を必ず併記する**——開発機は 2 台あり、「開発機」とだけ書くと過去の表を現在値の基準に使えなくなる。

---

### Task 4: 文書の同期

**Files:**
- Modify: `snotra-core/CLAUDE.md`（並列レイアウト節）
- Modify: `PERFORMANCE.md`（採用の項目）

- [x] **Step 1: `snotra-core/CLAUDE.md` の並列レイアウト節を直す**

現行（`grep -n "並列レイアウト" snotra-core/CLAUDE.md` で位置を出す）:

```
  - **並列レイアウト**: `SearchEngine` は `entries` / `lower_names` / `lower_file_names` / `char_masks` / `file_name_char_masks` / `kana_lower_names` / `kana_char_masks` を添字で対応づけた並列の列として持つ（cache locality）。**すべてが `Vec` ではない**——`lower_names` / `lower_file_names` は `str_arena` のアリーナ、表示名は `PathStore` の `NameArena` である。**エントリ数に比例する確保を持つ列は 1 つも残っていない**（`kana_*` は migemo 有効時のみ per-entry・額は `PERFORMANCE.md`「採用: 派生文字列 2 列も文字列アリーナで持つ」）
```

差し替え後（**括弧の但し書きが本文の全称を否定していた状態を解消する**——kana も per-entry ではなくなったので、条件なしで真になる）:

```
  - **並列レイアウト**: `SearchEngine` は `entries` / `lower_names` / `lower_file_names` / `char_masks` / `file_name_char_masks` / `kana_lower_names` / `kana_char_masks` を添字で対応づけた並列の列として持つ（cache locality）。**すべてが `Vec` ではない**——`lower_names` / `lower_file_names` は `str_arena` のアリーナ、表示名と `kana_lower_names` は `index_tree` の `NameArena` である。**エントリ数に比例する確保を持つ列は 1 つも残っていない**（migemo 有効時も。額は `PERFORMANCE.md`「採用: kana 列も文字列アリーナで持つ」）
```

**`kana_char_masks` は `Vec<u64>` のまま**なので「`Vec` ではない」の列挙に混ぜない。同じ節の下方にある「`kana_lower_names` / `kana_char_masks` は `migemo_enabled` が true のときのみ構築し、無効時は空 Vec」の一文は、kana 側が `Vec` でなくなったので「無効時は空」へ言い換える（**条件付き構築という不変条件そのものは変えない**）。

- [x] **Step 2: `PERFORMANCE.md` へ採用の項目を足す**

見出しは既存の採用項目に倣う（`grep -n "^## 採用" PERFORMANCE.md`）。載せるのは Task 3 の実測値のみ:

- 合成ラダーの `on` − `off` の blocks 差（前 / 後）と live 差
- 構築コスト（`kana_for_cached` 側を含む）
- 検索レイテンシの対（migemo ON）
- **機体名**

**見積もりを実測として書かない。** 掛け算で出した値は「見積もり」と明示する。

- [x] **Step 3: `governance:check` を通す**

Run: `npm run governance:check`
Expected: 全検査 passed

- [x] **Step 4: コミット**

```
docs: kana 列のアリーナ化を実測値で記録する
```

---

### Task 5: レビューと PR

- [x] **Step 1: `code-reviewer` エージェントを起動する**

**渡すもの**（ルート `CLAUDE.md`「レビューの委譲」）:
- 設計書のパス（`docs/superpowers/specs/2026-08-12-kana-lower-names-arena-design.md`）
- **意図的に分けてある構造 3 件**（DRY 違反として必ず挙がるので先に渡す）:
  1. `compute_wave1` の kana 枝（逐次）と `kana_for_cached`（塊併合の並列）が **2 実装のままである**こと——A/B 突き合わせの検知器の効力がこの 2 実装性に依存する
  2. `NameArena` を `str_arena` のアリーナと統合しないこと——線上表現の凍結（`arena_wire_format_is_identical_to_vec_of_string`）と疎/密の違い
  3. `footprint.rs` の kana が 2 行であること——1 行に束ねると未帰属 blocks が出る
- **逆向きの監査を 1 枠**: 「この差分が消した行の不変条件を名指しし、再確立地点を探す」。`git log -S` / `git blame` をこの枠にだけ渡す
- 成果物は呼び出し側が指定したパスへ書かせる（`report*.md` にしない。`.txt` か別 basename）

- [x] **Step 2: 指摘へ fix-forward したら、指摘を出した枠組みを修正差分へ再実行する**

修正は指摘箇所へ注意が集中し、周辺に新しい誤りを生む（`AGENTS.md` 条件別チェック表）。**「解消した」の判定は再実行の結論を受け取るのではなく、指摘を見つけた道具で自分で測る。**

- [x] **Step 3: push して PR を作る**

Run: `git push -u origin HEAD && gh pr create ...`
**鎖に `cd` を含めない**（対象リポジトリを判定できず hook に拒否される）。**この plan.md の未チェック項目が 0 でないと hook が `gh pr create` を拒む。**

PR 本文には issue #1056 を closing keyword で結ぶ（`Closes #1056`）。

---

## 計測結果

**正本は `PERFORMANCE.md`「採用: `kana_lower_names` も文字列アリーナで持つ」である。** ここに残すのは実行の記録（何を何回、どの経路で測ったか）だけで、数値表を二重に持たない。

計測機: GPD WIN MINI / G1617-01 / 23.8 GB。すべて 2026-08-12・同日・同セッション・release。

- 合成ラダー（`memory_footprint`・`--test-threads=1`）: B 側 3 回で live・blocks が完全一致。A 側は `git switch main` で成果物ごと戻して同セッションで取得
- kana 読み口のレイテンシ: 既存 bench が kana 経路を通らないため一時計測を両ブランチへ同文で当て、各 3 標本。**判断後に撤去済み**（`git status` が空であることを確認）
- 構築コスト: `new_with_cached_masks`（キャッシュヒット起動の経路）を各 3 標本。既存の `bench_new_migemo_on_off` は `compute_wave1` 側しか見ないため別立て
- 導出案（B 案）却下のための pre-filter 選択率: 実 `index.bin` 312,108 件・migemo ON・ローマ字 10 クエリ

### 計画からのずれ（実施時に判明したもの）

- **Task 2 は Task 1 へ吸収した。** 既存の `assemble_shrinks_parallel_vecs_to_fit` が `kana_lower_names.capacity()` を見ており、アリーナ化と同時に直さないとコンパイルが通らなかった。新規テストを別に書くと同じ性質の検査が 2 本になるため、既存テストの余剰容量側へ移した
- **その検知器は `new_with_cached_masks` 経路では発火しない。** `from_chunks` が合計バイト数ちょうどで確保するため余剰が最初から 0 になる。変異注入で実測し、伸長に任せる `new_with_migemo` 側の索引を同テストへ足して発火を確かめた
- **受け入れ 1 の「3 前後」は実測 2 だった。** 空のアリーナが持つ番兵オフセット 1 ブロックを数えていなかった（設計書 §5 を訂正済み）

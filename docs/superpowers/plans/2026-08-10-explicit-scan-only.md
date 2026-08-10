# 索引の更新を明示操作だけに一本化する — 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 自動の背景再スキャンを撤去し、索引が更新されるのを初回構築・`/s`・設定変更の 3 契機だけにする。対で設定アプリに最終構築日時を表示する。

**Architecture:** 撤去が主体だが、撤去する経路が 2 つの責務を抱えているため**先に移設してから消す**。形式昇格は `load_cache_in` の旧版枝へ（`save_cache_sorted_in` の返り値が material そのものなので 1 行の置換で済む）、アイコンキャッシュの無効化は `start_index_build`（ビルド要求の単一入口）へ。移設が済んでから `snotra-core` と `src-tauri` の再スキャン経路・digest 比較・世代機構・計器を消す。

**Tech Stack:** Rust 2024 / rustc 1.94・postcard 1・chrono 0.4・eframe 0.35（設定アプリ）・Tauri 2

## Global Constraints

- **`main` へ直接コミットしない。** 作業ブランチは `feat/explicit-scan-only`（作成済み）
- **設計の正本は `docs/superpowers/specs/2026-08-10-explicit-scan-only-design.md`、判断の経緯は `docs/adr/ADR-rescan-explicit-only.md`**
- **文書の写しは、それを偽にするタスクと同じコミットで直す**（#977）。各タスクの Files に該当文書を明示してある。「最後にまとめて直す」タスクは置かない
- **検知器を消すときは、守っていた対象が本当に消えたことを対で確認する**
- **テストは `%APPDATA%\Snotra` を書かない。** dir 注入の入口（`*_in`）を使う。`SNOTRA_CONFIG_DIR` での迂回は禁止（プロセス大域の env であり並列実行中の他テストの保存先まで動かす）
- **各タスクの完了時に実行する検証**（`docs/build-commands.md` カテゴリ A）:
  ```
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test -p <変更した crate>
  cargo doc --workspace --no-deps --document-private-items   # doc コメントを触ったら必須（hook は沈黙する）
  ```
  ガバナンス文書（`*.md`）を触ったタスクは `npm run governance:check` も実行する（カテゴリ F）
- **秒数・件数を恒久文書へ書かない。** 実測値は日付と標本数つきで設計書・ADR にのみ置く

## File Structure

| ファイル | このタスク群での責務 |
|---|---|
| `snotra-core/src/binfmt.rs` | **追加**: `BinFile::peek_first_field`（ヘッダー直後の最初のフィールドだけを復号する。ヘッダー配置の知識を binfmt に閉じたまま保つ） |
| `snotra-core/src/indexer.rs` | **追加**: `index_built_at_in`・`load_or_scan_with_stats_in`。**移設**: 形式昇格を `load_cache_in` の旧版枝へ。**撤去**: 再スキャン一式・digest・世代機構 |
| `snotra-core/src/rescan_log.rs` | **ファイルごと削除** |
| `snotra-core/src/lib.rs` | `mod rescan_log;` の削除 |
| `src-tauri/src/indexing.rs` | **移設先**: `start_index_build` でアイコンキャッシュを無効化する |
| `src-tauri/src/main.rs` | **撤去**: `setup_background_rescan` / `apply_rescanned_index` とその検知器 |
| `snotra-settings/src/tabs/index.rs` | **追加**: 最終構築日時の 1 行と、その整形関数 |
| `snotra-settings/src/i18n.rs` | **追加**: `TrKey` 2 個（ja / en） |
| `snotra-settings/Cargo.toml` | **追加**: `chrono`（表示の整形。core と同じ版指定） |

---

### Task 1: `built_at` だけを読む口を作る

**Files:**
- Modify: `snotra-core/src/binfmt.rs`（`BinFile` に `peek_first_field` を足す）
- Modify: `snotra-core/src/indexer.rs`（`index_built_at_in` を足す・`index_cache_on_disk_format_is_stable` に assertion を 1 本足す）

**Interfaces:**
- Consumes: `binfmt::peek_version`（既存・ヘッダー配置を知る唯一の場所）・`indexer::cache_bin_file_in`（既存）
- Produces:
  - `pub fn BinFile::peek_first_field<T: DeserializeOwned>(&self, max_payload: usize) -> Option<T>`
  - `pub fn indexer::index_built_at_in(dir: &Path) -> Option<u64>`

**背景**: `index.bin` のヘッダは magic 4 B + version u32 LE の 8 バイトで、その直後の最初のフィールドが `built_at: u64`（postcard の varint）である。**v2 から v7 まで 6 版すべてで `built_at` が先頭であることを確認した**が、これは観測された性質であって誰かが保証した契約ではない。Step 7 の assertion がその依存を固定する。

**ヘッダー配置の知識を binfmt の外へ写さないこと。** `binfmt::peek_version` の doc が「ヘッダーの配置を知る場所をここ 1 つに閉じるための口である」と宣言しており、`indexer.rs` で `bytes[4..8]` を読み直すのはその規約違反にあたる。

- [ ] **Step 1: `peek_first_field` の失敗するテストを書く**

`snotra-core/src/binfmt.rs` の `mod tests` へ追加する。

```rust
#[test]
fn peek_first_field_reads_only_the_head_and_ignores_the_version() {
    #[derive(serde::Serialize)]
    struct Payload {
        first: u64,
        rest: Vec<u32>,
    }
    let dir = temp_dir("binfile_peek_first");
    // 版 9 で書く（`BinFile` が名乗る版とわざと違える）。
    let bytes = try_serialize_with_header(
        *b"TEST",
        9,
        &Payload { first: 1_700_000_000, rest: vec![1, 2, 3] },
    )
    .expect("serialize");
    let bf = BinFile::new_in(&dir, *b"TEST", 1, "data.bin");
    assert!(bf.save_bytes(&bytes), "save");

    // 版が一致しなくても先頭フィールドは読める（版に依らない口である）。
    assert_eq!(bf.peek_first_field::<u64>(16), Some(1_700_000_000));
}

#[test]
fn peek_first_field_rejects_a_foreign_magic_and_a_truncated_file() {
    let dir = temp_dir("binfile_peek_reject");
    let bf = BinFile::new_in(&dir, *b"TEST", 1, "data.bin");

    // magic 違い。
    let other = try_serialize_with_header(*b"XXXX", 1, &7u64).expect("serialize");
    assert!(bf.save_bytes(&other), "save");
    assert_eq!(bf.peek_first_field::<u64>(16), None, "magic 不一致は None");

    // ヘッダーより短い。
    assert!(bf.save_bytes(&[1, 2, 3]), "save");
    assert_eq!(bf.peek_first_field::<u64>(16), None, "切り詰めは None");

    // ファイル不在。
    bf.remove();
    assert_eq!(bf.peek_first_field::<u64>(16), None, "不在は None");
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra-core peek_first_field`
Expected: FAIL（`no method named peek_first_field`）

- [ ] **Step 3: `peek_first_field` を実装する**

`snotra-core/src/binfmt.rs` の冒頭の `use` に `std::io::Read` を足し、`impl BinFile` へ追加する。

```rust
    /// ヘッダーを検証し、**その直後の最初のフィールドだけ**を復号する。
    ///
    /// **本体を読まない。** 読むのは先頭 `HEADER_LEN + max_payload` バイトだけで、
    /// 17 MiB の `index.bin` から `built_at` を取り出すために全体を確保しない。
    ///
    /// **版は問わない**（`self.version` と一致しなくても読む）。「その値が全版で
    /// 先頭にある」ことを保証するのは呼び出し側の責務である。ヘッダーが読めない
    /// （短い・magic 違い）ときと復号できないときは `None`。
    pub fn peek_first_field<T: DeserializeOwned>(&self, max_payload: usize) -> Option<T> {
        let f = fs::File::open(&self.path).ok()?;
        let mut head = Vec::with_capacity(HEADER_LEN + max_payload);
        f.take((HEADER_LEN + max_payload) as u64)
            .read_to_end(&mut head)
            .ok()?;
        if head.len() < HEADER_LEN || head[0..4] != self.magic {
            return None;
        }
        // 版そのものは使わないが、ヘッダーが版を名乗れる長さであることは確かめる。
        peek_version(&head)?;
        postcard::take_from_bytes::<T>(&head[HEADER_LEN..])
            .ok()
            .map(|(value, _rest)| value)
    }
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p snotra-core peek_first_field`
Expected: PASS（2 本とも）

- [ ] **Step 5: `index_built_at_in` の失敗するテストを書く**

`snotra-core/src/indexer.rs` の `mod tests` へ追加する。既存のテストが使っている一時ディレクトリのヘルパー（`temp_dir` 等）に合わせること。

```rust
    /// 設定アプリが最終構築日時を出すための口。**17 MiB を読まない**ことが要点で、
    /// 読めない・無いときは黙って `None` を返す（表示は「未構築」へ倒れる）。
    #[test]
    fn index_built_at_reads_the_timestamp_without_loading_the_index() {
        let dir = temp_dir("built_at_read");
        assert_eq!(index_built_at_in(&dir), None, "不在は None");

        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let _ = save_cache_sorted_in(&dir, entries, 42);

        let built_at = index_built_at_in(&dir).expect("保存した直後は読めること");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // 保存は今なので、未来ではなく、かつ極端に古くもない。
        assert!(built_at <= now, "未来の値を返してはならない: {built_at} > {now}");
        assert!(now - built_at < 300, "保存直後の値とかけ離れている: {built_at}");
    }

    /// **旧版でも読める**（`built_at` は全版で先頭フィールドである）。
    #[test]
    fn index_built_at_reads_a_legacy_version_too() {
        let dir = temp_dir("built_at_legacy");
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: 1_700_000_000,
                entries: vec![],
                config_hash: 1,
                char_masks: vec![],
                file_name_char_masks: vec![],
                lower_names: vec![],
                lower_file_names: vec![],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");
        assert_eq!(index_built_at_in(&dir), Some(1_700_000_000));
    }
```

- [ ] **Step 6: 落ちることを確認する**

Run: `cargo test -p snotra-core index_built_at`
Expected: FAIL（`cannot find function index_built_at_in`）

- [ ] **Step 7: `index_built_at_in` を実装し、golden へ依存を固定する assertion を足す**

`snotra-core/src/indexer.rs` に追加する（`cache_bin_file_in` の近く）。

```rust
/// `index.bin` が名乗る最終構築時刻（UNIX 秒）を読む。読めなければ `None`。
///
/// **索引本体を読まない。** 設定アプリがこれを呼ぶので、17 MiB の確保を持ち込まない。
///
/// **`built_at` が全版で先頭フィールドであることは、観測された性質であって契約ではない**
/// （v2〜v7 の 6 版で確認した）。新しい版を足すときも先頭へ置くこと——この依存は
/// `index_cache_on_disk_format_is_stable` の assertion 1 本が固定している。
pub fn index_built_at_in(dir: &Path) -> Option<u64> {
    // u64 の postcard varint は最大 10 バイト。
    cache_bin_file_in(dir).peek_first_field::<u64>(10)
}
```

`index_cache_on_disk_format_is_stable` の末尾（`assert_eq!(bytes, GOLDEN_V7, ...)` の後）へ追加する。

```rust
        // **`index_built_at_in` はヘッダー直後の最初のフィールドが `built_at` である
        // ことに依存している。** フィールドを並べ替えると golden も落ちるが、落ちた側が
        // 「並べ替えた」だけを報せて依存の所在を報せない。ここで名指ししておく。
        assert_eq!(
            crate::binfmt::peek_first_field_from_bytes::<u64>(&bytes, INDEX_MAGIC),
            Some(1_700_000_000),
            "ヘッダー直後の最初のフィールドが built_at でなくなった。index_built_at_in が\
             黙って別の値を返すようになる（表示だけが壊れ、テストは他が全部通る）"
        );
```

この assertion のために、`binfmt.rs` へバイト列版の口も足す（`peek_first_field` はこれをファイル読み込みの後に呼ぶ形へ書き直す）。

```rust
/// バイト列に対する [`BinFile::peek_first_field`]。golden テストが**保存せずに**
/// 同じ復号を当てるための口である。
pub fn peek_first_field_from_bytes<T: DeserializeOwned>(bytes: &[u8], magic: [u8; 4]) -> Option<T> {
    if bytes.len() < HEADER_LEN || bytes[0..4] != magic {
        return None;
    }
    peek_version(bytes)?;
    postcard::take_from_bytes::<T>(&bytes[HEADER_LEN..])
        .ok()
        .map(|(value, _rest)| value)
}
```

`BinFile::peek_first_field` の本体は、ファイルを読んだ後 `peek_first_field_from_bytes(&head, self.magic)` へ委譲する形にする（判定の写しを作らない）。

- [ ] **Step 8: 通ることを確認する**

Run: `cargo test -p snotra-core index_built_at peek_first_field index_cache_on_disk_format_is_stable`
Expected: PASS（すべて）

- [ ] **Step 9: 変異で落ちることを確かめる**

`IndexCache` の `built_at` と `names` の宣言順を入れ替えて `cargo test -p snotra-core index_cache_on_disk_format_is_stable` を実行し、**追加した assertion が落ちること**を目で見る。確認したら入れ替えを戻す。

- [ ] **Step 10: 検証を実行してコミット**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo doc --workspace --no-deps --document-private-items
git add snotra-core/src/binfmt.rs snotra-core/src/indexer.rs
git commit -m "feat(core): index.bin の built_at だけを読む口（本体を読まない）"
```

---

### Task 2: 形式昇格を `load_cache_in` の旧版枝へ移設する

**Files:**
- Modify: `snotra-core/src/indexer.rs:1076-1173`（v6 / v5 / v4 / v3 / v2 の各フォールバック枝）
- Modify: `snotra-core/CLAUDE.md`「indexer.rs の背景再スキャン」（「昇格をロード側に置いてはならない」の射程を v7 の枝へ限定する）

**Interfaces:**
- Consumes: `save_cache_sorted_in(dir, entries: Vec<AppEntry>, config_hash: u64) -> (IndexTree, CachedMasks)`（既存）・`with_index_write_lock`（既存）
- Produces: 旧版を読んだ起動が `index.bin` を現行版で書き戻す。`LoadCacheResult.version` の意味は変わらない（**読めた**版であって、書き戻した後の版ではない）

**背景**: 旧版 `index.bin` を現行版へ書き戻せる唯一の場所が背景再スキャンだった。Task 4・5 でそれを消すので、**先にここへ移す**。移さずに消すと、2026-08-07 に実運用点で実測した「v4 が残り、毎起動 `normalized_keys` 35.98 MiB を読んでは捨てる」が恒久化する。

**なぜ v7 の枝には置けないか**: v7 は木を直読みするので `entries: Vec<AppEntry>` が存在せず、木から作り直すと反復 6 で消した 62.5 MiB の複製が復活する。一方 v2〜v6 の枝は `cache.entries` を手に持って `IndexTree::build(cache.entries)` へ渡している——**その 1 行の置換なので複製は発生しない**。

- [ ] **Step 1: 失敗するテストを書く**

`snotra-core/src/indexer.rs` の `mod tests` へ追加する。

```rust
    /// **旧版を読んだ起動が、その場で現行版へ書き戻す。** 移す前はここが背景再スキャンの
    /// 責務だった（#1001 で再スキャンごと撤去した）。書き戻さないと、索引の中身が
    /// 変わらないユーザーの `index.bin` は旧版のまま何日でも残り、新形式の削減を
    /// 永久に受け取らない（2026-08-07 実測。症状は「遅い」だけで検索結果は正しいまま）。
    #[test]
    fn load_cache_upgrades_a_legacy_format_in_place() {
        let dir = temp_dir("load_upgrade");
        let entries = vec![
            AppEntry { name: "a".into(), target_path: "C:\\a".into(), is_folder: false },
            AppEntry { name: "b".into(), target_path: "C:\\b".into(), is_folder: true },
        ];
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: 1_700_000_000,
                entries: entries.clone(),
                config_hash: 42,
                char_masks: vec![0; entries.len()],
                file_name_char_masks: vec![0; entries.len()],
                lower_names: vec!["a".into(), "b".into()],
                lower_file_names: vec![None, None],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

        let result = load_cache_in(&dir, 42).expect("v4 が読めること");
        assert_eq!(result.version, 4, "`version` は**読めた**版のままである");
        assert_eq!(result.material.tree().len(), 2, "材料が正しいこと");

        // ディスクは現行版になっていること。
        let raw = cache_bin_file_in(&dir).load_bytes().expect("読み直せること");
        assert_eq!(
            crate::binfmt::peek_version(&raw),
            Some(INDEX_CACHE_VERSION),
            "旧版を読んだ後、ディスクは現行版で書き戻されていること"
        );
    }

    /// **現行版を読んだときは書き直さない。** ここが退行すると毎起動 17 MiB を書く
    /// （結果は正しいまま静かに遅くなるので挙動テストでは捕まらない）。
    #[test]
    fn load_cache_does_not_rewrite_when_the_format_is_current() {
        let dir = temp_dir("load_no_rewrite");
        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let _ = save_cache_sorted_in(&dir, entries, 42);
        let before = index_built_at_in(&dir).expect("built_at");

        // 同じ秒に収まると差が出ないので、`built_at` が動いていないことで測る。
        let result = load_cache_in(&dir, 42).expect("v7 が読めること");
        assert_eq!(result.version, INDEX_CACHE_VERSION);
        assert_eq!(
            index_built_at_in(&dir),
            Some(before),
            "現行版のロードで index.bin を書き直してはならない"
        );
    }
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra-core load_cache_upgrades load_cache_does_not_rewrite`
Expected: `load_cache_upgrades_a_legacy_format_in_place` が FAIL（ディスクは v4 のまま）。`load_cache_does_not_rewrite_when_the_format_is_current` は PASS（現状でも書かない）

- [ ] **Step 3: 旧版枝を昇格する形へ書き換える**

v6 / v5 / v4 / v3 / v2 の 5 枝それぞれで、`IndexMaterial::from_untrusted(IndexTree::build(cache.entries), masks)` / `IndexMaterial::from_tree(IndexTree::build(cache.entries))` を昇格ヘルパー呼び出しへ差し替える。まず `load_cache_in` の直前へヘルパーを置く。

```rust
/// 旧版を読んだ枝の共通処理: **走査結果を既に手に持っているので、その場で現行版へ書き戻す。**
///
/// **返す材料は書き戻しの副産物である**（`save_cache_sorted_in` が返す木とマスクをそのまま
/// 使う）。旧版が持っていたマスクは捨てて derive し直すが、これは**昇格する起動でだけ払う
/// 一回性の代価**であり、以後は現行版の枝に入る。
///
/// **保存に失敗してもロードは成功させる。** 昇格は最適化であって、失敗が索引の可用性を
/// 落としてはならない——落とすと、書けない環境（読み取り専用・ディスク満杯）で
/// 旧版ユーザーだけが索引を失う。
fn upgrade_legacy_cache_in(
    dir: &Path,
    entries: Vec<AppEntry>,
    config_hash: u64,
    read_ms: u128,
    version: u32,
) -> Option<LoadCacheResult> {
    // **`index.bin` を書く経路はすべて書き込みロックを経由する契約である。**
    let (tree, masks) = with_index_write_lock(|| save_cache_sorted_in(dir, entries, config_hash));
    Some(LoadCacheResult {
        material: IndexMaterial::from_untrusted(tree, masks)?,
        read_ms,
        // **`version` は「読めた」版のままにする。** 呼び出し側はこれで「旧版だった」を
        // 知る。書き戻した後の版を入れると、その事実が消える。
        version,
    })
}
```

v6 の枝を例に、差し替え後はこうなる（v5 / v4 / v3 / v2 も同じ形。`masks` の組み立てはもう要らないので削除する）。

```rust
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV6>(&bytes, INDEX_MAGIC, 6) {
        if cache.config_hash != config_hash {
            return None;
        }
        return upgrade_legacy_cache_in(dir, cache.entries, config_hash, read_ms, 6);
    }
```

各枝の既存コメント（「背景再スキャンが v7 へ昇格させるまでの 1 回だけ通る経路である」「昇格させるのは背景再スキャンで、`version` がその判断材料である」）を、**この場で昇格する**という事実へ書き換える。

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p snotra-core load_cache`
Expected: PASS（新規 2 本と既存のフォールバック系すべて）

- [ ] **Step 5: 変異で落ちることを確かめる**

`upgrade_legacy_cache_in` の `with_index_write_lock(...)` を `IndexTree::build` + 旧マスクへ戻す変異を当て、`load_cache_upgrades_a_legacy_format_in_place` が落ちることを目で見る。確認したら戻す。

- [ ] **Step 6: `snotra-core/CLAUDE.md` の射程を直す**

「indexer.rs の背景再スキャン」節の「**旧版の `index.bin` を現行版へ書き戻すのもこの経路の責務である**」から始まる段落を、次の趣旨へ書き換える。

- 昇格は `load_cache_in` の**旧版枝**が行う（`upgrade_legacy_cache_in`）
- 「昇格をロード側に置いてはならない」の**射程は v7 の枝だけ**である——v7 は木を直読みするので `entries` が無く、作り直すと反復 6 で消した 62.5 MiB の複製が復活する。v2〜v6 の枝は `entries` を手に持っているので複製は発生しない
- 検知器は `load_cache_upgrades_a_legacy_format_in_place` と `load_cache_does_not_rewrite_when_the_format_is_current` の対

- [ ] **Step 7: 検証を実行してコミット**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
git add snotra-core/src/indexer.rs snotra-core/CLAUDE.md
git commit -m "refactor(core): 形式昇格を背景再スキャンからロードの旧版枝へ移す"
```

---

### Task 3: アイコンキャッシュの無効化を `start_index_build` へ移設する

**Files:**
- Modify: `src-tauri/src/indexing.rs`（`start_index_build` に無効化を足す・`drain_index` の「アイコンキャッシュにはもう触らない」コメントを直す）
- Modify: `SPEC.md` §3.4（アイコンキャッシュ破棄の 2 条件のうち `RescanOutcome::Changed` を書き換える）

**Interfaces:**
- Consumes: `crate::icon::invalidate_icon_cache(&IconCacheState)`（既存）・`IconCacheState`（既存）
- Produces: 権威的ビルド要求が受理されるたびにアイコンキャッシュが無効化される

**背景**: #996 が再構築時のキャッシュ掃除を撤去したため、**エントリ集合が変わったときの無効化は `RescanOutcome::Changed` が唯一の担い手**である。Task 4 でそれを消すので、先にここへ移す。

**今日の機構が守っているものを正確に把握しておくこと**（移設先の妥当性はこれで決まる）。キーはエントリの `target_path` であり、`Changed` はエントリ**集合**が変わったときだけ立つ。`.lnk` の張り替えやアプリ更新で**アイコンだけ**変わった場合は集合が同じなので `Unchanged` になり、**今日も無効化されない**。ゆえに生き残ったキーの刷新は「他所で何かが増減したときの巻き添え」である。移設後の引き金は「ユーザーが再構築を要求したとき」になり、巻き添えより狙いが良い。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/indexing.rs` へ `mod tests` を新設する（このファイルには現在テストが無い）。`AppHandle` を組み立てる治具がこの crate に無いため、**ソーステキストを見る**形にする——`main.rs` の `rescan_application_does_not_kick_a_full_rebuild_or_forge_the_ledger` と同じ手である。

```rust
#[cfg(test)]
mod tests {
    /// **アイコンキャッシュの無効化はここが唯一の担い手である。** #996 が再構築時の
    /// 掃除を撤去したため、かつては背景再スキャンの `RescanOutcome::Changed` が
    /// 担っていた。再スキャンごと撤去した（#1001）ので、ここが落ちると
    /// **エントリ集合が変わってもアイコンが古いまま FIFO 上限まで残る**——
    /// 検索結果は正しいままなので挙動テストでは捕まらない。
    ///
    /// **残る死角**: 母集団は `start_index_build` のソーステキストだけであり、
    /// 呼び出しグラフは辿らない。この関数の外のヘルパー経由で無効化する形へ
    /// 変えると、母集団の外なので捕まらない。
    #[test]
    fn start_index_build_invalidates_the_icon_cache() {
        let src = include_str!("indexing.rs");
        let after = src
            .split_once("pub fn start_index_build(")
            .expect("start_index_build が見つからない（改名したらこの検査も直す）")
            .1;
        let body = match after.find("\npub(crate) fn ") {
            Some(idx) => &after[..idx],
            None => after,
        };
        // **母集団が黙って空にならないことを、まずそれ自体で確かめる。**
        assert!(
            body.contains("try_begin_index_build("),
            "母集団が start_index_build の本体を含まない——終端の切り出しがずれた。\
             沈黙する検知器は検知器ではない"
        );
        assert!(
            body.contains("invalidate_icon_cache("),
            "start_index_build がアイコンキャッシュを無効化していない（#996 撤去後、\
             ここが唯一の担い手である）"
        );
    }
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra start_index_build_invalidates`
Expected: FAIL（`invalidate_icon_cache(` を含まない）

- [ ] **Step 3: 無効化を足す**

`src-tauri/src/indexing.rs` の `start_index_build` で、`try_begin_index_build` が成功した直後（`notify_indexing_started(app)` の前）へ挿入する。

```rust
    if !state.try_begin_index_build() {
        return false;
    }

    // **アイコンキャッシュを捨てる。** #996 が索引照合の剪定を撤去し、以後は背景再スキャンの
    // `RescanOutcome::Changed` が唯一の担い手だった。その再スキャンを撤去した（#1001）ので、
    // 担い手はここである。**判定を置かない**——ユーザー（あるいは config 変更）が再構築を
    // 要求した事実そのものが引き金であり、集合が変わったかを測り直す必要は無い。
    //
    // ここで撃つのは CAS に成功した側だけである（要求のたびに撃つと、走行中ビルドへの
    // 重複要求で無駄に捨てる）。engine ロックは `mark_index_stale()` の中で解放済みで、
    // ロックを跨いだ取得にはならない。
    if let Some(icons) = app.try_state::<crate::icon::IconCacheState>() {
        crate::icon::invalidate_icon_cache(&icons);
    }

    notify_indexing_started(app);
```

- [ ] **Step 4: `drain_index` の古くなったコメントを直す**

`drain_index` の中の「**アイコンキャッシュにはもう触らない。** 索引照合の剪定は #996 で撤去し、無効化時の破棄も `config_watcher` の true → false のエッジへ移した」というコメントは偽になる。次の趣旨へ書き換える。

- drain ループはアイコンキャッシュに触らない（無効化は `start_index_build` が要求受理時に 1 回だけ撃つ）
- 索引照合の剪定は #996 で撤去したままである
- 表示無効化時の破棄は `config_watcher` の true → false のエッジのままである
- ゆえに `IndexInputs` は索引を建て直す入力だけを持つ（この結論は変わらない）

- [ ] **Step 5: 通ることを確認する**

Run: `cargo test -p snotra start_index_build_invalidates`
Expected: PASS

- [ ] **Step 6: `SPEC.md` §3.4 を直す**

「キャッシュを丸ごと破棄するのは次の 2 つの場合に限る」の 1 つ目を書き換える。

- 変更前: 「背景再スキャンでエントリ集合が変わったとき（`RescanOutcome::Changed`）」
- 変更後: 「**索引の再構築が始まったとき**（初回構築・`/s`・設定変更のいずれの契機でも）: メモリ内キャッシュと `icons.bin` の両方を無効化し、次回検索時に再抽出する」

2 つ目（アイコン表示を無効へ切り替えたとき）はそのまま。

- [ ] **Step 7: 検証を実行してコミット**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
git add src-tauri/src/indexing.rs SPEC.md
git commit -m "refactor(tauri): アイコン無効化を背景再スキャンからビルド要求の単一入口へ移す"
```

---

### Task 4: `src-tauri` から背景再スキャンを撤去する

**Files:**
- Modify: `src-tauri/src/main.rs`（`setup_background_rescan` / `apply_rescanned_index` とその検知器・`//!` の説明・`s.digest_ms` の trace）
- Delete（`main.rs` 内）: `rescan_application_does_not_kick_a_full_rebuild_or_forge_the_ledger`
- Modify: `docs/superpowers/specs/2026-08-10-rescan-applies-its-result-design.md`（撤去された旨の 1 行）

**Interfaces:**
- Consumes: なし（撤去のみ）
- Produces: `snotra-core` の `BackgroundRescanTask` / `RescanOutcome` / `RescanRun` / `LoadOrScanResult.rescan_task` に**呼び出し元がいなくなる**。Task 5 の削除で compile-fail が出なければ移行漏れが無いことの証拠になる

**注意**: `main.rs:175-210` の `let (mut material, initial_indexing, rescan_task) = ...` は 3 要素タプルである。`rescan_task` を落とすとき、cache-miss 側（`rescan_task: None` 相当）と cache-hit 側の両方を 2 要素へ揃えること。`-D warnings` 下では未使用の束縛が落ちる。

- [ ] **Step 1: 撤去する対象を数え上げる**

Run:
```bash
grep -n "rescan\|RescanOutcome\|digest_ms" src-tauri/src/main.rs
```
出た行をすべて控える。**この一覧が Step 5 の検算の母集団になる。**

- [ ] **Step 2: `setup_background_rescan` と `apply_rescanned_index` を削除する**

`main.rs:605` からの `fn setup_background_rescan` と、それに続く `fn apply_rescanned_index` を関数ごと削除する。`main.rs:372` の呼び出し `setup_background_rescan(&app_handle, rescan_task);` も削除する。

- [ ] **Step 3: 束縛とタプルを 2 要素へ揃える**

`main.rs:175` の `let (mut material, initial_indexing, rescan_task) = if is_first_run {` を `let (mut material, initial_indexing) = if is_first_run {` にし、両分岐の返り値から 3 要素目を落とす（`main.rs:210` の `(result.material, false, result.rescan_task)` → `(result.material, false)`）。

- [ ] **Step 4: 死ぬ検知器と trace を消す**

- `rescan_application_does_not_kick_a_full_rebuild_or_forge_the_ledger`（`main.rs:751` 付近）を doc コメントごと削除する。**守っていた対象（`apply_rescanned_index` が `start_index_build` を呼ばないこと）が、その関数ごと消えたことを確認してから消す**
- `main.rs:193` の `s.digest_ms` を trace の項目から外す（`LoadOrScanStats.digest_ms` は Task 5 で消える）
- `main.rs` の `//!`（5 行目付近）から背景再スキャンの説明を削除する

- [ ] **Step 5: 母集団が空になったことを検算する**

Run:
```bash
grep -n "rescan\|RescanOutcome\|digest_ms" src-tauri/src/main.rs
```
Expected: 0 件。Step 1 で控えた行がすべて消えていること

- [ ] **Step 6: ビルドとテストが通ることを確認する**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test -p snotra`
Expected: PASS

- [ ] **Step 7: 設計書へ撤去の 1 行を足す**

`docs/superpowers/specs/2026-08-10-rescan-applies-its-result-design.md` の冒頭へ追加する。

```markdown
> **2026-08-10 追記: この反復の成果は撤去された。** 自動の背景再スキャンそのものを消し、索引の更新を明示操作だけにした（`docs/adr/ADR-rescan-explicit-only.md`）。`apply_rescanned_index` はもう存在しない。**本書が記録した判断（材料と `scanned_config_hash` を対で運ぶ理由・snapshot 同士の比較では足りない理由）は、再スキャンを再導入する日に読み直す価値がある。**
```

- [ ] **Step 8: 検証を実行してコミット**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
git add src-tauri/src/main.rs docs/superpowers/specs/2026-08-10-rescan-applies-its-result-design.md
git commit -m "refactor(tauri): 背景再スキャンの spawn と結果適用を撤去する"
```

---

### Task 5: `snotra-core` から背景再スキャン・digest・世代機構・計器を撤去する

**Files:**
- Modify: `snotra-core/src/indexer.rs`（大量削除）
- Delete: `snotra-core/src/rescan_log.rs`
- Modify: `snotra-core/src/lib.rs`（`mod rescan_log;` の行）
- Modify: `snotra-core/tests/memory_footprint.rs:309,314,417-419`（`digest_ms` の参照）
- Modify: `SPEC.md` §3.3・§13.4
- Modify: `snotra-core/CLAUDE.md`「indexer.rs の背景再スキャン」節（削除）・「index.bin 書き込みの排他」節（書き手の一覧）
- Modify: `docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md`（撤去の 1 行）

**Interfaces:**
- Consumes: なし（撤去のみ）
- Produces: `LoadOrScanResult` が `{ material, cache_changed, stats }` の 3 フィールドになる。`LoadOrScanStats` から `digest_ms` が消える

**撤去する識別子の一覧**（Step 1 の母集団）:

`BackgroundRescanTask` / `RescanOutcome` / `RescanRun` / `try_background_rescan` / `try_background_rescan_in` / `entries_digest` / `digest_over` / `DigestSource` / `LoadOrScanResult.rescan_task` / `LoadOrScanStats.digest_ms` / `try_with_index_write_lock` / `INDEX_GENERATION` / `current_index_generation` / `snapshot_index_generation` / `load_with_index_generation` / `lower_current_thread_priority`（※ 読者が他に無いことを確認してから）

**世代機構について**: `INDEX_GENERATION` の読者は再スキャンの世代検査だけであることを実測済み（`indexer.rs:638, 1325` とテスト 3 か所）。`with_index_write_lock` の中の `INDEX_GENERATION.fetch_add(1, ...)` も一緒に消える。**消す前に必ず grep で読者を数え直すこと**——他に読者が居れば残す。

- [ ] **Step 1: 撤去する対象を数え上げる**

Run:
```bash
grep -rn "BackgroundRescanTask\|RescanOutcome\|RescanRun\|try_background_rescan\|entries_digest\|digest_over\|DigestSource\|rescan_task\|digest_ms\|try_with_index_write_lock\|INDEX_GENERATION\|index_generation\|rescan_log\|lower_current_thread_priority" snotra-core/ src-tauri/ snotra-settings/ --include=*.rs
```
出た行をすべて控える。**この一覧が Step 8 の検算の母集団になる。**

- [ ] **Step 2: `rescan_log.rs` を消す**

```bash
git rm snotra-core/src/rescan_log.rs
```
`snotra-core/src/lib.rs` から `mod rescan_log;`（`pub mod` かもしれない）の行を削除する。

- [ ] **Step 3: `indexer.rs` から再スキャン本体を消す**

`try_background_rescan` / `try_background_rescan_in` / `BackgroundRescanTask`（`impl` を含む）/ `RescanOutcome` / `RescanRun` を削除する。`lower_current_thread_priority` は再スキャンスレッドのためだけに在るので、`grep -rn lower_current_thread_priority` で読者ゼロを確認してから削除する（`#[cfg(not(windows))]` 版も対で消す）。

- [ ] **Step 4: digest を消す**

`entries_digest` / `digest_over` / `DigestSource`（trait と impl 群）を削除する。`load_or_scan_with_stats` の cache-hit 枝から `digest_started` / `digest_over(material.tree())` / `digest_ms` の計算を消し、`LoadOrScanStats` から `digest_ms` フィールドを消す（cache-miss 枝の `digest_ms: 0` も）。`LoadOrScanResult` から `rescan_task` フィールドを消す。

`LoadOrScanStats.digest_ms` の doc（「**`cache_load_ms` と `total_ms` の間に処理を足すときは、必ずここに並ぶ項目を作ること。**」）は**規範として生かす**——`cache_read_ms` の doc か構造体自身の doc へ移し、「反復 6 で `digest_ms` を足して塞いだ穴の教訓」として残す。**規範ごと消さないこと。**

- [ ] **Step 5: 世代機構を消す**

`INDEX_GENERATION` / `current_index_generation` / `snapshot_index_generation` / `load_with_index_generation` / `try_with_index_write_lock` を削除する。`with_index_write_lock` の中の `INDEX_GENERATION.fetch_add(1, Ordering::Relaxed);` も削除し、`load_or_scan_with_stats` の `load_with_index_generation(|| load_cache(current_hash))` を `load_cache(current_hash)` へ戻す（タプルの分解も併せて直す）。

- [ ] **Step 6: 死ぬ検知器を消す**

以下を、**それぞれ「何を守っていたか」と「その対象が消えたこと」を対で確認してから**削除する。

- `background_rescan_*` 系すべて
- `sorted_comparison_ignores_enumeration_order`（守っていたのは「digest の両辺を `sort_entries_canonical` に通すこと」。digest ごと消えた）
- `try_with_index_write_lock_skips_closure_when_lock_held`
- `rescan_generation_is_snapshotted_before_cache_load`
- `rescan_log.rs` 内のテスト（ファイルごと消える）

`with_index_write_lock_holds_lock_during_closure` は**残す**（`with_index_write_lock` は生きている）。

- [ ] **Step 7: `memory_footprint.rs` を直す**

`snotra-core/tests/memory_footprint.rs:309` の `s.digest_ms` 出力と `:314` の和の検算から `digest_ms` を外す。`:417-419` の散文（「cache-hit 枝の `digest_ms` は…」）も、`digest_ms` を指さない形へ書き直す。

- [ ] **Step 8: 母集団が空になったことを検算する**

Run: Step 1 と同じ grep
Expected: 0 件（`docs/` と `PERFORMANCE.md` の歴史的記述は対象外。`--include=*.rs` で絞ってある）

- [ ] **Step 9: ビルドとテストが通ることを確認する**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test -p snotra-core && cargo test -p snotra`
Expected: PASS。**compile-fail が出たら移行漏れである**（Task 4 で消し残した呼び出し点）

- [ ] **Step 10: `SPEC.md` を直す**

**§3.3「インデックス構築タイミング」**: 「通常起動時はハイブリッド方式」以下の 5 行（起動時キャッシュ即時ロード / バックグラウンド差分スキャン / 差分があればキャッシュ更新とセッション索引の差し替え / 権威的再構築との競合時スキップ / 再構築予定時は譲る）を、次の趣旨へ置き換える。

- 通常起動はキャッシュを即時ロードして終わりである。**背景での走査は行わない**
- 索引が更新される契機は 3 つ: 初回構築・スラッシュコマンド `/s` による手動再構築・設定変更による再構築
- 読めた `index.bin` が旧形式だったときは、ロード時にその場で現行形式へ書き戻す（索引の中身は変えない）

同節の「設定画面から手動再構築可能」は**現在の実装と食い違っている**（`snotra-settings` に再構築を撃つ経路は無い）。「スラッシュコマンド `/s` から手動再構築可能（§15.2）」へ直す。

**§13.4「計器の記録（テキスト・使い捨て）」**: 節ごと削除する。§13 の最後の小節なので番号は詰めなくてよい。

- [ ] **Step 11: `snotra-core/CLAUDE.md` を直す**

- 「indexer.rs の背景再スキャン」節を**削除**する。ただし節の中の 2 つの知識は生かす:
  - `scan_all` の重複排除（`root_roles`）に関する段落は、再スキャンと独立した事実なので**別の節へ移す**（「`scan_all` の重複排除」等の見出しで独立させる）
  - 「digest は列の順序に依存する」は digest ごと消えるので削除してよい
- 「index.bin 書き込みの排他」節の書き手の一覧から「日和見的書き手（`try_background_rescan`）」の行を削除し、**ロードの旧版枝からの昇格を書き手として足す**（Task 2 で追加した経路）
- `rescan_log.rs` を指すモジュール索引の行を削除する

- [ ] **Step 12: 計器の設計書へ撤去の 1 行を足す**

`docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md` の冒頭へ追加する。

```markdown
> **2026-08-10 追記: この計器は撤去された。** 測る対象（毎起動の全走査）そのものを消したためである（`docs/adr/ADR-rescan-explicit-only.md`）。**§8.2 が却下した「記録ファイルを間引きの入力に兼ねさせる」案の理由は今も有効である**——計器を振る舞いを決める部品に変えてはならない。
```

- [ ] **Step 13: 検証を実行してコミット**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
git add -A
git commit -m "refactor(core)!: 背景再スキャン・digest 比較・世代機構・計器を撤去する"
```

---

### Task 6: 「キャッシュヒットの起動で走査が起きない」検知器を置く

**Files:**
- Modify: `snotra-core/src/indexer.rs`（`load_or_scan_with_stats_in` を足す・`load_or_scan_with_stats` をその薄い包みにする）

**Interfaces:**
- Consumes: `load_cache_in`（既存）・`save_cache_sorted_in`（既存）
- Produces: `pub fn load_or_scan_with_stats_in(dir: &Path, scan: &[ScanPath], show_hidden_system: bool) -> LoadOrScanResult`

**この検知器は Red から始まらない。** Task 5 の前後どちらでも緑である（再スキャンは `load_or_scan_with_stats` の中で走るのではなく、返したタスクを `src-tauri` が spawn していた）。守るのは**将来の退行**——cache-hit 枝へ走査を足す変更である。ゆえに **Step 4 の変異確認を省いてはならない**。省くと「置いただけで何も守っていない検知器」になる。

**dir 注入の入口を作る理由**: 現在 `load_or_scan_with_stats` は `Config::config_dir()` を内部で解決するため、統合テストが実 `%APPDATA%\Snotra` を読み書きする（#1013 の Gotcha）。`load_cache_in` / `save_cache_sorted_in` と同じ `*_in` の形へ揃える。

- [ ] **Step 1: 失敗するテストを書く**

`snotra-core/src/indexer.rs` の `mod tests` へ追加する。

```rust
    /// **キャッシュヒットの起動は走査しない。** #1001 の受け入れの本体である。
    ///
    /// 「走査していない」を、時間ではなく**結果**で測る——キャッシュを保存した後で
    /// 走査対象へファイルを 1 つ足し、それが返る材料に**現れない**ことを見る。
    /// 走査が 1 回でも走れば現れるので、時計や環境に依存せず決定論的である。
    #[test]
    fn a_cache_hit_startup_does_not_scan() {
        let dir = temp_dir("cache_hit_no_scan");
        let scan_root = temp_dir("cache_hit_no_scan_root");
        std::fs::write(scan_root.join("first.txt"), b"x").expect("write");

        let scan = vec![ScanPath {
            path: scan_root.display().to_string(),
            extensions: vec![".txt".into()],
            include_folders: false,
        }];

        // 1 回目: cache-miss → 走査して保存する。
        let first = load_or_scan_with_stats_in(&dir, &scan, false);
        assert!(!first.stats.cache_hit, "1 回目は cache-miss であること");
        assert_eq!(first.material.tree().len(), 1);

        // キャッシュを書いた後で対象を増やす。
        std::fs::write(scan_root.join("second.txt"), b"y").expect("write");

        // 2 回目: cache-hit → 走査しないので、増えたファイルは見えない。
        let second = load_or_scan_with_stats_in(&dir, &scan, false);
        assert!(second.stats.cache_hit, "2 回目は cache-hit であること");
        assert_eq!(second.stats.scan_ms, 0, "cache-hit で走査時間が立ってはならない");
        assert_eq!(
            second.material.tree().len(),
            1,
            "cache-hit の起動が走査している（増えたファイルが見えてしまった）"
        );
    }
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra-core a_cache_hit_startup_does_not_scan`
Expected: FAIL（`cannot find function load_or_scan_with_stats_in`）

- [ ] **Step 3: `load_or_scan_with_stats_in` を切り出す**

現在の `load_or_scan_with_stats`（`indexer.rs:629`）の本体を `dir: &Path` を取る形へ移し、内部の `load_cache(current_hash)` を `load_cache_in(dir, current_hash)`、`save_cache_sorted(...)` を `save_cache_sorted_in(dir, ...)` へ差し替える。既存の公開関数はその薄い包みにする。

```rust
/// `load_or_scan_with_stats` と同じ手順を `dir` 注入で行う（統合テスト用）。
///
/// **製品の入口（`Config::config_dir()` を解決する側）でテストを書かないこと。**
/// 実 `%APPDATA%\Snotra` を読み書きし、テスト実行が実運用のデータを動かす（#1013）。
pub fn load_or_scan_with_stats_in(
    dir: &Path,
    scan: &[ScanPath],
    show_hidden_system: bool,
) -> LoadOrScanResult {
    // （現在の load_or_scan_with_stats の本体をここへ移す）
}

pub fn load_or_scan_with_stats(scan: &[ScanPath], show_hidden_system: bool) -> LoadOrScanResult {
    match Config::config_dir() {
        Some(dir) => load_or_scan_with_stats_in(&dir, scan, show_hidden_system),
        // config dir が解決できない環境では保存できないが、索引は建てられる。
        None => /* 既存の「保存先が無い」経路と同じ扱いにする */,
    }
}
```

`None` 側の扱いは、既存の `save_cache_sorted`（`indexer.rs:861`）が `Config::config_dir()` を解決できないときに `IndexMaterial::from_tree(IndexTree::build(entries))` へ落ちているのと同じ方針に揃える。

- [ ] **Step 4: 通ることを確認し、変異で落ちることを確かめる**

Run: `cargo test -p snotra-core a_cache_hit_startup_does_not_scan`
Expected: PASS

続けて**変異を当てる**。cache-hit 枝の `return LoadOrScanResult { ... }` の直前へ `let _ = scan_all(scan, show_hidden_system);` を 1 行入れる……のでは `material` が変わらないので落ちない。**落ちる変異を当てること**——cache-hit 枝の材料を `IndexMaterial::from_tree(IndexTree::build(scan_all(scan, show_hidden_system)))` へ差し替え、テストが「増えたファイルが見えてしまった」で落ちることを目で見る。確認したら戻す。

**この差は重要である。** この検知器が守るのは「走査の副作用」ではなく「**cache-hit の材料がキャッシュ由来であること**」だと分かる。守れない退行（結果に影響しない無駄な走査）が在ることを、テストの doc へ「残る死角」として書く。

- [ ] **Step 5: 検証を実行してコミット**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo doc --workspace --no-deps --document-private-items
git add snotra-core/src/indexer.rs
git commit -m "test(core): キャッシュヒットの起動が走査しないことを結果で測る"
```

---

### Task 7: 設定タブに最終構築日時を表示する

**Files:**
- Modify: `snotra-settings/Cargo.toml`（`chrono` を足す）
- Modify: `snotra-settings/src/i18n.rs`（`TrKey` を 2 個・ja / en の両方）
- Modify: `snotra-settings/src/tabs/index.rs`（整形関数と表示の 1 行）

**Interfaces:**
- Consumes: `snotra_core::indexer::index_built_at_in(dir) -> Option<u64>`（Task 1）・`snotra_core::config::Config::config_dir()`（既存）
- Produces: なし（末端の表示）

**ユーザーの指定**: 「設定タブからいつ更新したのかだけ見えればいい」。**ボタンも `/s` への誘導文も置かない。** 検索 0 件時の表示・status 行への常時表示も置かない（ADR の「受容する残余」に記録済み）。

- [ ] **Step 1: 失敗するテストを書く**

`snotra-settings/src/tabs/index.rs` の末尾へ `mod tests` を追加する（無ければ新設）。**整形は `DateTime<Local>` を引数に取る純関数にする**——`Local::now()` を内部で呼ぶと時間帯と実行時刻でテストが揺れる。`snotra-core/src/instant.rs` の `format_date` が同じ形をしている。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn built_at_is_rendered_as_a_local_datetime() {
        let dt = chrono::Local.with_ymd_and_hms(2026, 8, 4, 9, 12, 0).unwrap();
        assert_eq!(format_built_at(&dt), "2026-08-04 09:12");
    }

    /// **不在は「未構築」へ倒す。** `index.bin` が無い・読めない・壊れているを
    /// 区別しない——ユーザーにとってはどれも「まだ構築していない」と同じである。
    #[test]
    fn an_absent_index_renders_as_not_built() {
        let tr = Tr::new(snotra_core::config::Language::Ja);
        assert_eq!(built_at_text(None, &tr), tr.t(TrKey::LabelIndexNotBuilt));
    }
}
```

`Tr::new` の実際のコンストラクタ名と `Language` の指定方法は `snotra-settings/src/i18n.rs` と既存のタブのテストに合わせること。

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra-settings built_at`
Expected: FAIL（`format_built_at` / `built_at_text` / `TrKey::LabelIndexNotBuilt` が無い）

- [ ] **Step 3: `chrono` を足す**

`snotra-settings/Cargo.toml` の `[dependencies]` へ、`snotra-core/Cargo.toml` と**同じ版・同じ feature 指定**で追加する。

```toml
chrono = { version = "0.4", default-features = false, features = ["clock", "std"] }
```

- [ ] **Step 4: `TrKey` を 2 個足す**

`snotra-settings/src/i18n.rs` の `TrKey` enum へ追加し、ja / en の両方の match アームを埋める（片方だけだと網羅性で落ちる）。

```rust
    LabelIndexLastBuilt,
    LabelIndexNotBuilt,
```

ja:
```rust
        TrKey::LabelIndexLastBuilt => "最終構築:",
        TrKey::LabelIndexNotBuilt => "未構築",
```

en:
```rust
        TrKey::LabelIndexLastBuilt => "Last built:",
        TrKey::LabelIndexNotBuilt => "Not built",
```

- [ ] **Step 5: 整形関数と表示を実装する**

`snotra-settings/src/tabs/index.rs` へ追加する。

```rust
/// UNIX 秒をローカル時刻の「YYYY-MM-DD HH:MM」へ整形する。
///
/// **`Local::now()` を内部で呼ばない。** 時間帯と実行時刻でテストが揺れるため、
/// 変換済みの `DateTime<Local>` を受け取る（`snotra_core::instant` の `format_date`
/// と同じ形）。秒は落とす——ユーザーが知りたいのは「いつ更新したか」であって、
/// 秒の精度は要らない。
fn format_built_at(dt: &chrono::DateTime<chrono::Local>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// 表示する文字列を決める。**不在・読めない・壊れているを区別しない**——
/// ユーザーにとってはどれも「まだ構築していない」と同じである。
fn built_at_text(built_at: Option<u64>, tr: &Tr) -> String {
    use chrono::TimeZone;
    let Some(secs) = built_at else {
        return tr.t(TrKey::LabelIndexNotBuilt).to_string();
    };
    match chrono::Local.timestamp_opt(secs as i64, 0).single() {
        Some(dt) => format_built_at(&dt),
        // 範囲外の値（壊れたファイル）も「未構築」へ倒す。
        None => tr.t(TrKey::LabelIndexNotBuilt).to_string(),
    }
}
```

`ui()` の中、`style::section_heading(ui, tr.t(TrKey::HeadingScanTargets));` の**前**へ 1 行置く（スキャンパスの一覧より上に、索引そのものの状態を示す）。

```rust
        // **索引がいつのものかを示すだけである。** 再構築のボタンは置かない——
        // 設定アプリは別プロセスで、本体との通信路は config.toml と config_watcher
        // しかない（ボタンは通信路の新設を要する・ADR-rescan-explicit-only）。
        let built_at = snotra_core::config::Config::config_dir()
            .and_then(|dir| snotra_core::indexer::index_built_at_in(&dir));
        style::hint(
            ui,
            &format!(
                "{} {}",
                tr.t(TrKey::LabelIndexLastBuilt),
                built_at_text(built_at, tr)
            ),
        );
```

`style::hint` の実際のシグネチャ（`&str` か `impl Into<String>` か）を確認して合わせること。

- [ ] **Step 6: 通ることを確認する**

Run: `cargo test -p snotra-settings`
Expected: PASS

- [ ] **Step 7: 検証を実行してコミット**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-settings
cargo doc --workspace --no-deps --document-private-items
git add snotra-settings/
git commit -m "feat(settings): インデックスタブに最終構築日時を出す"
```

---

### Task 8: 実機ゲート

**Files:** なし（測るだけ）。記録は PR 本文へ書く

**背景**: ここまでのテストはどれも「走査が起きないこと」を**結果**で測っており、**実機で 22〜30 秒の CPU 張り付きが本当に消えたか**は測っていない。#1001 の実機トレースが使った手（`Get-CimInstance Win32_Process` の `TotalProcessorTime` と `Threads.Count`）で確かめる。

**`Get-Process` を使わないこと。** PowerShell 7 の `Get-Process` は `ReadOperationCount` を持たず、空値が 0 として計算に混ざる（#1001 の Gotcha）。使う信号は CPU 時間とスレッド数である。

- [ ] **Step 1: 実バイナリをビルドする**

```bash
cargo build -p snotra
```
**`cargo test` では `target/debug/snotra.exe` は更新されない。** 古いバイナリを測ると、検査は成功したまま変更前の挙動を測る（#835）。

- [ ] **Step 2: スモークを走らせる**

```bash
npm run test:powershell
npm run smoke:startup
npm run smoke:egui
```
Expected: すべて PASS。落ちたら trace イベント名・hotkey の前提が壊れていないか確認する（`docs/build-commands.md` カテゴリ C）

- [ ] **Step 3: キャッシュヒットの起動で CPU が張り付かないことを測る**

`index.bin` が在る状態（cache-hit）で本体を起動し、起動から 40 秒間、2 秒おきに次を記録する。

```powershell
Get-CimInstance Win32_Process -Filter "Name='snotra.exe'" |
  Select-Object ProcessId, @{n='CPU_s';e={$_.UserModeTime/1e7 + $_.KernelModeTime/1e7}}, ThreadCount
```

Expected: **CPU 増分が経過秒に追従しない**（#1001 の実機トレースでは 1 コアを 22 秒使い切り、CPU 増分 ≒ 経過秒だった）。`index.bin` の mtime が起動によって動かないこと。

- [ ] **Step 4: `/s` が働くことを確かめる**

ランチャーで `/s` を打つ。Expected: 「インデックス構築中...」が出て、完了後に `index.bin` の mtime が動き、アイコンが再抽出される（結果リストのアイコンが一度消えてから戻る）。

- [ ] **Step 5: 設定タブの表示を確かめる**

設定アプリを開き、インデックスタブに「最終構築: <日時>」が Step 4 の再構築時刻と一致して出ることを目で見る。`index.bin` を退避した状態で開き直し、「未構築」が出ることも確かめる（**確かめたら退避したファイルを戻す**）。

- [ ] **Step 6: 形式昇格を実機で確かめる**

`index.bin` を退避し、旧版（v4 か v6）の `index.bin` を用意できるなら置いて起動する。Expected: 起動後に `index.bin` が現行版になっている（`binfmt::peek_version` 相当を確認する使い捨てスクリプト、または `cargo test -p snotra-core load_cache_upgrades` の緑で代替する）。**旧版の実ファイルが用意できないなら、この Step はユニットテストで代替したことを PR 本文に明記する**——測っていないものを測ったと書かない。

- [ ] **Step 7: 記録を残す**

Step 3〜6 で観測した値と、代替した項目を PR 本文へ書く。**秒数は PR 本文にのみ書き、恒久文書へは書かない。**

---

## Self-Review

**1. 仕様カバレッジ**（設計書の各節 → タスク）

| 設計書 | タスク |
|---|---|
| §2.1 `snotra-core` の撤去 | Task 5 |
| §2.2 `src-tauri` の撤去 | Task 4 |
| §2.3 死ぬ検知器 | Task 4 Step 4・Task 5 Step 6 |
| §3.1 形式昇格の移設 | Task 2 |
| §3.2 アイコン無効化の移設 | Task 3 |
| §4.1 `index_built_at_in` | Task 1 |
| §4.2 設定タブの表示 | Task 7 |
| §5 文書の同期（9 件） | Task 2 Step 6・Task 3 Step 6・Task 4 Step 7・Task 5 Step 10-12 |
| §6 検知器（5 本） | Task 1 Step 5,7・Task 2 Step 1・Task 3 Step 1・Task 6 Step 1 |
| §7 受容する残余 | ADR に記録済み（コード変更なし） |

**2. 順序の依存**: Task 2・3（移設）は Task 4・5（撤去）より**前**でなければならない。逆にすると、移設先が無い区間で責務が落ちる。Task 1 は Task 7 の前。Task 6 は Task 5 の後（`load_or_scan_with_stats` の形が確定してから切り出す）。

**3. 型の一貫性**: `index_built_at_in(&Path) -> Option<u64>` は Task 1 で定義し Task 7 で消費する。`peek_first_field_from_bytes(&[u8], [u8; 4]) -> Option<T>` は Task 1 の中で定義と消費が閉じる。`load_or_scan_with_stats_in(&Path, &[ScanPath], bool) -> LoadOrScanResult` は Task 6 の中で閉じる。

**4. 既知の未確定（実装時に現物へ合わせる）**: `snotra-core/src/indexer.rs` のテストが使う一時ディレクトリのヘルパー名、`snotra-settings` の `Tr` のコンストラクタ名、`style::hint` のシグネチャ、`snotra-core/src/lib.rs` の `rescan_log` の宣言（`mod` か `pub mod` か）。いずれも**該当ファイルを開けば 1 行で分かる**もので、判断を要する分岐ではない。

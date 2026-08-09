# `scan_all` の `seen` を根の重なりに条件づける 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `indexer::scan_all` が重複排除の `HashSet` を建てるのを「走査の根どうしが重なるとき」だけに限り、根が `C:\` 一本の運用点で 312,625 件ぶんの正規化キー確保を消す。

**Architecture:** 走査の根の集合に対して入れ子関係を事前判定する述語を足し、`seen` を `Option<HashSet<String>>` にする。木の走査は同じディレクトリを二度読まないので、1 回の `scan_all` の中で同じ正規化キーが二度現れる経路は根が入れ子のとき以外に無い。判定は根の数（一桁）に対する全ペア走査で、額に影響しない。

**Tech Stack:** Rust / `snotra-core` crate / `cargo test` / 計測は `tests/memory_footprint.rs` の計数アロケータ。

**設計書:** `docs/superpowers/specs/2026-08-09-scan-all-seen-conditional-design.md`（本計画は §番号でそこを参照する）
**issue:** #1002 / **ブランチ:** `perf/scan-all-seen-conditional`（作成済み・spec の 2 コミットが載っている）

## Global Constraints

- **タスクの順序を入れ替えてはならない。** Task 1（計器）は Task 3（実装）より**前**にコミットする。A 側標本は実装前にしか取れない
- 受け入れ判定は**確保回数**であり壁時計ではない（#1002 の規約・反復 9 の先例）。`PERFORMANCE.md` の見出しに壁時計を載せない
- **A/B で走査結果を突き合わせない。** 対象は生きた `C:\` 全体で churn が挟まる（設計書 §4）。ゲートは「1 回の走査の中で正規化キーの重複が 0 件」
- `cargo test -p snotra-core` は PostToolUse hook が自動で走らせる（**沈黙 = 合格**）。ただし **doc コメントを追加・変更したら `cargo doc --workspace --no-deps --document-private-items` を手で走らせる**（intra-doc link 切れは CI でしか発火しない・`.claude/rules/comments.md`）
- 計器の実行コマンド（`docs/build-commands.md` が SSOT）:
  `cargo test --release -p snotra-core --test memory_footprint -- --ignored --nocapture --test-threads=1`
  **`--test-threads=1` は外せない**（計数アロケータがプロセス大域のため）

---

## File Structure

| ファイル | 役割 | 変更 |
|---|---|---|
| `snotra-core/tests/memory_footprint.rs` | 索引の常駐・区間コストの実測ハーネス | Phase A の末尾に `scan_all` 区間を足す（Task 1） |
| `snotra-core/src/indexer.rs` | 走査・重複排除・索引キャッシュ | 述語 2 つを新設（Task 2）・`seen` を `Option` 化（Task 3） |
| `PERFORMANCE.md` | 実測値の正本 | 節を追加（Task 4） |
| `snotra-core/CLAUDE.md` | モジュール固有の不変条件 | 「indexer.rs の背景再スキャン」へ 1 行（Task 4） |

`SPEC.md` は変更しない（挙動不変。設計書 §6 の分岐に入った場合のみ再判定）。

---

## Task 1: `scan_all` 区間の計器を新設し、A 側標本を取る

**Files:**
- Modify: `snotra-core/tests/memory_footprint.rs`（`measure_real_index_footprint` の末尾 + 新しい private fn）

**Interfaces:**
- Consumes: `indexer::scan_all(&[ScanPath], bool) -> Vec<AppEntry>`（`pub`）/ `indexer::normalize_entry_key(&str) -> String`（`pub`）/ 同ファイル内の `snap()` / `reset_peak()` / `report(label, before, after, n)`
- Produces: 標準出力の A 側標本（確保回数・peak・件数・重複件数）。後続タスクはこの数値を `PERFORMANCE.md` へ書く

- [ ] **Step 1: 計器の本体を書く**

`snotra-core/tests/memory_footprint.rs` の `report_cache_bytes` 関数定義の**直前**に、次の関数を足す。

```rust
/// 背景再スキャンが毎起動踏む `scan_all` 区間を測る。
///
/// **この区間はどのフェーズ計測にも現れない。** Phase A のロードはキャッシュヒット枝ゆえ
/// `scan_ms` が 0 で、`rescan_task` は呼び出し側が `drop` する——`cache_load_ms` と
/// `total_ms` の間に居た全エントリ複製が見えなかったのと同じ形である
/// （`snotra-core/CLAUDE.md`「indexer.rs の背景再スキャン」）。
///
/// **返り値は entries へ混ぜない。** 実走査はファイルシステムの churn で実行ごとにぶれ、
/// 混ぜると常駐がバイト単位で再現しなくなる。ここで測るのは区間のコストである
/// （PATH スキャンの区間と同じ規約）。
///
/// **受け入れゲートは「正規化キーの重複件数 0」である。** A/B で走査結果を突き合わせては
/// ならない——A と B は別プロセス・別時刻の走査であり、間に temp・ログ・キャッシュの churn が
/// 必ず挟まる。重複件数は 1 回の走査の中で閉じるので churn の影響を受けない
/// （設計書 `docs/superpowers/specs/2026-08-09-scan-all-seen-conditional-design.md` §4）。
///
/// **この区間は実走査そのものゆえ、Phase A の実行時間が大きく伸びる。** 所要はファイルシステムの
/// キャッシュの温度で数倍にぶれるので、ここに秒数を書かない（書けば測り直すたびに腐る）。
fn report_scan_all_cost(config: &Config) {
    reset_peak();
    let t0 = snap();
    let start = std::time::Instant::now();
    let scanned = indexer::scan_all(&config.paths.scan, config.search.show_hidden_system);
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    let t1 = snap();

    let n = scanned.len();
    report("scan_all（背景再スキャン経路・常駐外）", t0, t1, n);
    println!("  壁時計: scan_all {ms:.0} ms（{n} 件・実行ごとに数十%ぶれる）");

    // **検算は区間の外で行う。** この走査自体が n 件の `String` を確保するので、
    // `t1` より前に置くと測りたい額へ自分の額が混ざる。
    let mut keys = std::collections::HashSet::with_capacity(n);
    let mut dups: Vec<&str> = Vec::new();
    for e in &scanned {
        if !keys.insert(indexer::normalize_entry_key(&e.target_path)) {
            dups.push(&e.target_path);
        }
    }
    println!(
        "  正規化キーの重複: {} 件（0 が受け入れ条件——0 なら `seen` を建てない走査は、\
         建てた走査と件数も順序も一致する）",
        dups.len()
    );
    for p in dups.iter().take(20) {
        println!("    重複: {p}");
    }
    if dups.len() > 20 {
        println!("    …ほか {} 件", dups.len() - 20);
    }
}
```

- [ ] **Step 2: 呼び出しを Phase A の末尾へ足す**

`measure_real_index_footprint` の最終行 `report_cache_bytes(n);` の**直後**（関数の閉じ括弧の直前）に足す。

```rust
    // **最後に置く。** 上の常駐・残留の測定より前に置くと、実走査の一時確保が peak を汚す。
    report_scan_all_cost(&config);
```

- [ ] **Step 3: コンパイルを通す**

Run: `cargo test --release -p snotra-core --test memory_footprint --no-run`
Expected: 成功（警告なし）。`Config` / `indexer` は同ファイル上部で既に import 済み。

- [ ] **Step 4: A 側標本を取る**

Run: `cargo test --release -p snotra-core --test memory_footprint -- --ignored --nocapture --test-threads=1`
Expected: Phase A の末尾に次の 3 行が出る（所要はファイルシステムのキャッシュの温度で
数倍にぶれる——秒数は書かない）。

```
  scan_all（背景再スキャン経路・常駐外）  live +xx.xx MiB  peak xx.xx MiB  blocks +xxxxxx  allocs   xxxxxx
  壁時計: scan_all xxxxx ms（312xxx 件・実行ごとに数十%ぶれる）
  正規化キーの重複: 0 件（0 が受け入れ条件——…）
```

**この 4 つの数字（allocs / peak / 件数 / 重複件数）を控える。** Task 4 で `PERFORMANCE.md` へ書く A 側である。**重複件数が非 0 なら、そこで止まって報告する**（設計書 §6——case-sensitive ディレクトリの同居が実在したということ。仕様変更として扱い直しになる）。

- [ ] **Step 5: コミット**

```bash
git add snotra-core/tests/memory_footprint.rs
git commit -F <path>   # メッセージ本文は複数行なのでファイル経由で渡す
```

メッセージ:

```
test(core): scan_all 区間の計器を Phase A へ足す（#1002）

背景再スキャンが毎起動踏む区間はどのフェーズ計測にも現れない。Phase A の
ロードはキャッシュヒット枝ゆえ scan_ms が 0 で、rescan_task は drop される。
PATH スキャンと同じ「常駐外・返り値は捨てる」形で区間を足す。

受け入れゲートは正規化キーの重複件数 0 である。A/B で走査結果を突き合わせて
はならない——対象は生きた C:\ 全体で、A と B の間に churn が必ず挟まる。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

## Task 2: 根の重なりを判定する述語

**Files:**
- Modify: `snotra-core/src/indexer.rs`（`scan_all` の直前に fn 2 つ / `mod tests` にテスト 6 本）

**Interfaces:**
- Consumes: `crate::config::normalize_scan_path_key(&str) -> String`（`pub(crate)`・`config.rs:530`）/ `crate::config::ScanPath`
- Produces: `fn roots_overlap(scan_paths: &[ScanPath]) -> bool`（private・Task 3 が `scan_all` から呼ぶ）/ `fn is_ancestor_or_same(a: &str, b: &str) -> bool`（private）

- [ ] **Step 1: 失敗するテストを書く**

`snotra-core/src/indexer.rs` の `mod tests` の中、`temp_dir` ヘルパーの定義より後ろに足す。

```rust
    // ---- roots_overlap tests ----

    /// 述語のテスト用に最小の `ScanPath` を作る。拡張子と `include_folders` は
    /// **判定に関与しない**（設計書 §2.2 の過剰近似）。
    fn root(path: &str) -> ScanPath {
        ScanPath {
            path: path.to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        }
    }

    #[test]
    fn roots_overlap_detects_drive_root_ancestor() {
        assert!(roots_overlap(&[root("C:\\"), root("C:\\Tools")]));
    }

    /// **境界の 2 枝を 1 本にまとめると、ここが落ちる。** `c:\tools` は `c:\toolsextra` の
    /// 接頭辞だが、次の 1 バイトが `\` ではないので入れ子ではない。
    #[test]
    fn roots_overlap_ignores_sibling_sharing_a_prefix() {
        assert!(!roots_overlap(&[root("C:\\Tools"), root("C:\\ToolsExtra")]));
    }

    #[test]
    fn roots_overlap_is_order_independent() {
        assert!(roots_overlap(&[root("C:\\Tools"), root("C:\\")]));
    }

    /// **完全一致も重なりとして拾う。** `scan_all` は `dedup_scan_paths` を通さない配列も
    /// 受け取る（`src-tauri` の `icon_pipeline_cost_probe`）。
    #[test]
    fn roots_overlap_detects_exact_duplicates_after_normalization() {
        assert!(roots_overlap(&[root("C:\\Tools"), root("c:/tools/")]));
    }

    #[test]
    fn roots_overlap_false_for_single_root() {
        assert!(!roots_overlap(&[root("C:\\")]));
    }

    #[test]
    fn roots_overlap_false_across_drives() {
        assert!(!roots_overlap(&[root("C:\\"), root("D:\\Apps")]));
    }

    #[test]
    fn roots_overlap_false_for_empty() {
        assert!(!roots_overlap(&[]));
    }
```

- [ ] **Step 2: テストが落ちることを確認**

Run: `cargo test -p snotra-core roots_overlap`
Expected: FAIL — `cannot find function 'roots_overlap' in this scope`（コンパイルエラー）

- [ ] **Step 3: 述語を実装する**

`snotra-core/src/indexer.rs` の `pub fn scan_all` の**直前**に足す。

```rust
/// 2 つの**正規化済み**の根について、`a` が `b` の祖先か同一か。
///
/// **境界の 2 枝を 1 本にまとめてはならない。** [`crate::config::normalize_scan_path_key`] は
/// ドライブ根だけ末尾 `\` を残す（`c:\` に対し `c:\tools`）。ドライブ根にも境界チェックを
/// 課すと `c:\\tools` を探して偽になり、非ドライブ根から外すと `c:\tools` が `c:\toolsextra`
/// を入れ子だと誤判定する。
// `allow`: 呼び出し点は Task 3 で `scan_all` へ入る。それまでは lib ターゲットから
// 到達しないため `-D warnings` 下で `dead_code` が落とす。**Task 3 で必ず外す。**
#[allow(dead_code)]
fn is_ancestor_or_same(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    b.len() > a.len()
        && b.starts_with(a)
        && (a.ends_with('\\') || b.as_bytes()[a.len()] == b'\\')
}

/// 走査の根どうしが重なるか。**重なるときだけ [`scan_all`] は重複排除の `seen` を建てる。**
///
/// 木の走査は同じディレクトリを二度読まないので、1 回の `scan_all` の中で同じ正規化キーが
/// 二度現れる経路は**根が入れ子になっているとき以外に無い**。junction / reparse point で
/// 別名になった経路は文字列が異なるので `seen` は元から捕まえない。
///
/// **判定は入れ子だけを見る**（拡張子集合の交差・`include_folders` の両立は見ない）。額は
/// 増えないのに判定の面積だけが広がるための過剰近似である。
///
/// **完全一致も重なりとして拾う。** 起動経路は `apply_migrations` → `normalize_scan_paths`
/// を通るので同一キーの根は残らないが、`scan_all` は dedup を通さない配列も受け取る
/// （`src-tauri` の `icon_pipeline_cost_probe` が `Config::default_scan_paths()` を直接渡す）。
/// 述語を config 側の dedup の性質へ依存させないための冗長である。
///
/// 根は一桁ゆえ全ペア走査で無料である。
// `allow`: 上の [`is_ancestor_or_same`] と同じ理由。**Task 3 で必ず外す。**
#[allow(dead_code)]
fn roots_overlap(scan_paths: &[ScanPath]) -> bool {
    let keys: Vec<String> = scan_paths
        .iter()
        .map(|sp| crate::config::normalize_scan_path_key(&sp.path))
        .collect();
    keys.iter().enumerate().any(|(i, a)| {
        keys[i + 1..]
            .iter()
            .any(|b| is_ancestor_or_same(a, b) || is_ancestor_or_same(b, a))
    })
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test -p snotra-core roots_overlap`
Expected: PASS（7 本すべて）

- [ ] **Step 5: clippy が通ることを確認**

Run: `cargo clippy -p snotra-core --all-targets -- -D warnings`
Expected: 成功。**`dead_code` で落ちるなら Step 3 の `#[allow(dead_code)]` が抜けている**——この 2 つは Task 3 で呼び出し点が入るまで lib ターゲットから到達しない。

- [ ] **Step 6: doc の検査**

Run: `cargo doc --workspace --no-deps --document-private-items`
Expected: 成功（intra-doc link `[scan_all]` / `[crate::config::normalize_scan_path_key]` が解決する）

- [ ] **Step 7: コミット**

```bash
git add snotra-core/src/indexer.rs
git commit -F <path>
```

メッセージ:

```
feat(core): 走査の根どうしの重なりを判定する述語を足す（#1002）

木の走査は同じディレクトリを二度読まないので、1 回の scan_all の中で同じ
正規化キーが二度現れる経路は根が入れ子のとき以外に無い。判定は入れ子だけを
見る（拡張子交差・include_folders は見ない過剰近似）。

境界の 2 枝は分ける。normalize_scan_path_key はドライブ根だけ末尾 \ を残す
ため、まとめると c:\ が偽になるか c:\tools が c:\toolsextra を拾う。

完全一致も重なりとして拾う。scan_all は dedup を通さない配列も受け取る
（icon_pipeline_cost_probe が default_scan_paths を直接渡す）。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

## Task 3: `seen` を `Option` にして述語へ繋ぐ

**Files:**
- Modify: `snotra-core/src/indexer.rs`（`scan_all` / `scan_directory_with_extensions` / 新しい private fn 1 つ / 既存テスト 5 箇所の呼び出し / 新規テスト 1 本）

**Interfaces:**
- Consumes: `roots_overlap(&[ScanPath]) -> bool`（Task 2）/ `normalize_entry_key(&str) -> String`
- Produces: `fn accept_entry(seen: &mut Option<HashSet<String>>, path: &str) -> bool`（private）。`scan_directory_with_extensions` の第 6 引数が `&mut Option<std::collections::HashSet<String>>` に変わる

- [ ] **Step 1: 入れ子の根での回帰テストを書く**

`snotra-core/src/indexer.rs` の `mod tests` の中、Task 2 で足した述語テストの直後に足す。**このテストは現行実装でも通る**（`seen` が常に在るため）——Step 5 の変異注入が本当の Red である。

```rust
    /// **入れ子の根では重複排除が要る。** `dedup_scan_paths` は完全一致マージのみゆえ、
    /// `X` と `X\sub` は両方とも残る（設計書 §1）。
    #[test]
    fn scan_all_dedups_when_roots_are_nested() {
        let dir = temp_dir("nested_roots");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).expect("create sub dir");
        fs::write(sub.join("tool.exe"), b"x").expect("write fixture");

        let scan = vec![
            ScanPath {
                path: dir.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
            ScanPath {
                path: sub.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
        ];
        let entries = scan_all(&scan, true);

        assert_eq!(
            entries.len(),
            1,
            "入れ子の根で同じファイルが二度入っている（重複排除が効いていない）"
        );

        let _ = fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: テストが通ることを確認（ベースライン）**

Run: `cargo test -p snotra-core scan_all_dedups_when_roots_are_nested`
Expected: PASS（現行実装は無条件に `seen` を建てるため）

- [ ] **Step 3: `seen` を `Option` にする**

3 箇所を書き換える。

(a) `scan_all`（`indexer.rs:117`）:

```rust
pub fn scan_all(scan_paths: &[ScanPath], show_hidden_system: bool) -> Vec<AppEntry> {
    let mut entries = Vec::new();
    // **重ならない根では `seen` を建てない**（根拠は [`roots_overlap`] の doc）。現運用点
    // （根が `C:\` 一本）では全エントリぶんの正規化 + `HashSet` 挿入がまるごと消える。
    let mut seen: Option<std::collections::HashSet<String>> =
        roots_overlap(scan_paths).then(std::collections::HashSet::new);

    for sp in scan_paths {
        let ext_set = build_extension_list(&sp.extensions);
        scan_directory_with_extensions(
            Path::new(&sp.path),
            &ext_set,
            sp.include_folders,
            show_hidden_system,
            &mut entries,
            &mut seen,
        );
    }

    entries
}
```

Task 2 で付けた `#[allow(dead_code)]` を**ここで外す**。

(b) 採否のヘルパーを `scan_directory_with_extensions` の直前に足す:

```rust
/// `seen` を建てている走査では既出キーを落とし、建てていない走査では素通しする。
///
/// **`None` のとき [`normalize_entry_key`] を呼ばないことがこの関数の要点である**——
/// 1 件ごとの `String` 確保がここで消える。呼び出し側は `!name.is_empty() &&` の**後ろ**に
/// 置くこと（現行の短絡評価を保つ。名前が空のエントリのキーを `seen` へ入れない）。
fn accept_entry(seen: &mut Option<std::collections::HashSet<String>>, path: &str) -> bool {
    match seen {
        Some(set) => set.insert(normalize_entry_key(path)),
        None => true,
    }
}
```

(c) `scan_directory_with_extensions`（`indexer.rs:137`）のシグネチャと 2 つの採用点:

```rust
fn scan_directory_with_extensions(
    dir: &Path,
    extensions: &[String],
    include_folders: bool,
    show_hidden_system: bool,
    entries: &mut Vec<AppEntry>,
    seen: &mut Option<std::collections::HashSet<String>>,
) {
```

フォルダ側（現行の `let key = …; if seen.insert(key) {` を置き換える）:

```rust
                if !name.is_empty() {
                    let path_str = path.to_string_lossy();
                    if accept_entry(seen, path_str.as_ref()) {
                        entries.push(AppEntry {
                            name,
                            target_path: path_str.into_owned(),
                            is_folder: true,
                        });
                    }
                }
```

ファイル側（現行の `let key = …; if !name.is_empty() && seen.insert(key) {` を置き換える）:

```rust
                let path_str = path.to_string_lossy();
                if !name.is_empty() && accept_entry(seen, path_str.as_ref()) {
                    entries.push(AppEntry {
                        name,
                        target_path: path_str.into_owned(),
                        is_folder: false,
                    });
                }
```

再帰呼び出し（`scan_directory_with_extensions(&path, …, entries, seen)`）は**そのままでよい**——`&mut Option<_>` は自動で再借用される。

- [ ] **Step 4: 既存テストの呼び出しを直す**

`scan_directory_with_extensions` を直接呼ぶテストが 5 箇所ある。**行番号で辿らない**（Task 2 で行がずれている）——次で位置を出す。

```bash
grep -n "scan_directory_with_extensions" snotra-core/src/indexer.rs
```

いずれも次の形なので、

```rust
        let mut seen = std::collections::HashSet::new();
```

を次へ置き換える。

```rust
        let mut seen = Some(std::collections::HashSet::new());
```

Run: `cargo test -p snotra-core`
Expected: PASS（全テスト）

- [ ] **Step 5: 変異注入 —— 検知器が本当に発火することを実測する**

`roots_overlap` の本体を一時的に潰す。

```rust
fn roots_overlap(_scan_paths: &[ScanPath]) -> bool {
    false // ← 変異。この行だけを一時的に置く
}
```

Run: `cargo test -p snotra-core scan_all_dedups_when_roots_are_nested`
Expected: **FAIL** — `入れ子の根で同じファイルが二度入っている（重複排除が効いていない）` / `left: 2, right: 1`

**落ちなかったら、そこで止まって報告する。** 検知器が対象を捕まえていないということであり、先へ進んではならない（反復 8 で 3 本中 1 本が落ちなかった教訓）。

- [ ] **Step 6: 変異を戻し、通ることを確認**

Step 3 の `roots_overlap` へ戻す。

Run: `cargo test -p snotra-core`
Expected: PASS（全テスト）

- [ ] **Step 7: doc の検査**

Run: `cargo doc --workspace --no-deps --document-private-items`
Expected: 成功（`[roots_overlap]` / `[normalize_entry_key]` が解決する）

- [ ] **Step 8: コミット**

```bash
git add snotra-core/src/indexer.rs
git commit -F <path>
```

メッセージ:

```
perf(core): scan_all の seen を根の重なりに条件づける（#1002）

seen は全ヒットの正規化キー（1 件ごとに String 確保）を積むが、代金に見合う
のは走査の根どうしが重なりうるときだけで、根が C:\ 一本の運用点では純粋な
費用である。roots_overlap が偽なら建てない。

判定の粒度は「入れ子が 1 つでもあれば全根で seen」に置いた——重なる構成では
現行と 1 バイトも変わらず、挙動不変の論証がそこで閉じる。

accept_entry は None のとき normalize_entry_key を呼ばない。呼び出しは
!name.is_empty() の後ろに置き、現行の短絡評価を保つ。

検知器（scan_all_dedups_when_roots_are_nested）は roots_overlap を false へ
潰すと落ちることを実測した。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

## Task 4: 判定を根ごとの `check` / `record` へ細かくする

> **このタスクは実測を受けた設計改訂である**（設計書 §1 の訂正・§2.2 の書き直し）。Task 3 までの
> 「`scan_all` 単位で建てる／建てない」判定は、実 config が 4 根で `C:\` が他 3 根の祖先だった
> ため**実運用点で削減 0** だった（allocs 5,991,749 → 5,991,979・peak 変化なし）。**判定は
> 正しく、前提が偽だった。**

**Files:**
- Modify: `snotra-core/src/indexer.rs`（`roots_overlap` を `root_roles` へ置換 / `Dedup`・`RootRole` を新設 / `scan_all`・`scan_directory_with_extensions` を書き換え / テストの追加と書き換え）

**Interfaces:**
- Consumes: `is_ancestor_or_same(&str, &str) -> bool`（Task 2・そのまま残す）/ `normalize_entry_key_into(&mut String, &str)`（`indexer.rs` の既存 `pub` 関数）
- Produces: `struct RootRole { check: bool, record: bool }` / `fn root_roles(&[ScanPath]) -> Vec<RootRole>` / `struct Dedup { set, buf, role }` と `Dedup::accept(&mut self, path: &str) -> bool`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の中、Task 2 が足した `roots_overlap` のテスト群と置き換える形で足す。**`root(path)` ヘルパーは Task 2 が定義済みなので再定義しない。**

```rust
    // ---- root_roles tests ----

    /// **積むのは「後続の根と重なる」側である。** 重複が起きるのは先に入ったエントリが
    /// 後の走査で再び現れるときだけなので、向きはこの 1 通りしかない。
    #[test]
    fn root_roles_records_on_the_earlier_root_and_checks_on_the_later() {
        let roles = root_roles(&[root("C:\\X"), root("C:\\X\\sub")]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (true, false));
    }

    /// **順序が逆でも役割が入れ替わるだけで、重複排除は成立する。**
    #[test]
    fn root_roles_follow_the_order_not_the_depth() {
        let roles = root_roles(&[root("C:\\X\\sub"), root("C:\\X")]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (true, false));
    }

    /// 実運用点の形（最大の根が最後に来る）。**ここで `C:\` が「照合のみ」になることが
    /// この設計の全部である**——積まないので 30 万件ぶんの `String` 確保が消える。
    #[test]
    fn root_roles_over_the_real_shape_leave_the_largest_root_inert() {
        let roles = root_roles(&[
            root("C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs"),
            root("C:\\Users\\Eoh\\Desktop"),
            root("C:\\"),
        ]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (false, true));
        assert_eq!(
            (roles[2].check, roles[2].record),
            (true, false),
            "最大の根が積む側に回ると削減が消える"
        );
    }

    #[test]
    fn root_roles_are_all_inert_when_nothing_overlaps() {
        let roles = root_roles(&[root("C:\\A"), root("D:\\B")]);
        assert!(roles.iter().all(|r| !r.check && !r.record));
    }

    #[test]
    fn root_roles_treat_exact_duplicates_as_overlap() {
        let roles = root_roles(&[root("C:\\Tools"), root("c:/tools/")]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (true, false));
    }

    /// **境界の 2 枝を 1 本にまとめると、ここが落ちる**（`c:\tools` は `c:\toolsextra` の
    /// 接頭辞だが、次の 1 バイトが `\` ではないので入れ子ではない）。
    #[test]
    fn root_roles_ignore_siblings_sharing_a_prefix() {
        let roles = root_roles(&[root("C:\\Tools"), root("C:\\ToolsExtra")]);
        assert!(roles.iter().all(|r| !r.check && !r.record));
    }

    #[test]
    fn root_roles_empty_for_no_paths() {
        assert!(root_roles(&[]).is_empty());
    }
```

さらに、走査の挙動を**両方向で**固定するテストを足す（Task 3 の `scan_all_dedups_when_roots_are_nested` は `[X, X\sub]` の順しか見ていない）。

```rust
    /// **子の根が先に来る順序でも重複が出ない。** 役割が入れ替わるだけで成立することを、
    /// 述語の単体テストではなく走査の結果で固定する。
    #[test]
    fn scan_all_dedups_when_the_child_root_comes_first() {
        let dir = temp_dir("nested_roots_child_first");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).expect("create sub dir");
        fs::write(sub.join("tool.exe"), b"x").expect("write fixture");

        let scan = vec![
            ScanPath {
                path: sub.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
            ScanPath {
                path: dir.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
        ];
        let entries = scan_all(&scan, true);

        assert_eq!(
            entries.len(),
            1,
            "子の根が先に来る順序で同じファイルが二度入っている"
        );

        let _ = fs::remove_dir_all(&dir);
    }
```

**Task 2 が足した `roots_overlap` のテスト 7 本は削除する**（`roots_overlap` 自体を消すため）。境界の検査は上の `root_roles_ignore_siblings_sharing_a_prefix` と `root_roles_treat_exact_duplicates_as_overlap` が引き継ぐ。

- [ ] **Step 2: テストが落ちることを確認**

Run: `cargo test -p snotra-core root_roles`
Expected: FAIL — `cannot find function 'root_roles' in this scope`（コンパイルエラー）

- [ ] **Step 3: `root_roles` を実装し、`roots_overlap` を置き換える**

Task 2 が入れた `roots_overlap` を**削除**し、同じ位置へ次を置く。`is_ancestor_or_same` はそのまま残す。

```rust
/// 2 つの正規化済みの根が重なるか（どちらが祖先でもよい）。
fn roots_overlap_pair(a: &str, b: &str) -> bool {
    is_ancestor_or_same(a, b) || is_ancestor_or_same(b, a)
}

/// 走査中の根の役割。**重複排除に払う代金を根ごとに決める。**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootRole {
    /// 先行する根と重なる → 既出かを照合する。
    check: bool,
    /// 後続の根と重なる → 自分のキーを積む。
    record: bool,
}

/// 根ごとに [`RootRole`] を決める。
///
/// **積むのは「後続の根と重なる」側だけである。** 木の走査は同じディレクトリを二度読まない
/// ので、あるエントリが二度現れるのは「根 `i` に入ったものが、後続の根 `j > i` の走査でも
/// 現れる」ときに限る。ゆえに根 `i` のキーを保持する必要があるのは後続に重なる根があるとき
/// だけで、**最後の根は（先行と重なっていても）照合するだけでよい**。
///
/// **額は根の順序に依存する。** 実運用点では最大の根 `C:\` が最後に来るため、その 30 万件が
/// 丸ごと「照合のみ」になる。**これは判定の欠陥ではない**——順序に対して述語は正しく、額だけが
/// 構成に依存する。**順序を並べ替えて額を取りに行ってはならない**: 返り値の順序が変わり、
/// `entries_digest` がずれて毎起動 `index.bin` を書き直す（検知器は
/// `sorted_comparison_ignores_enumeration_order`）。
///
/// **完全一致も重なりとして拾う。** `scan_all` は `dedup_scan_paths` を通さない配列も受け取る
/// （`src-tauri` の `icon_pipeline_cost_probe` が `Config::default_scan_paths()` を直接渡す）。
///
/// 根は一桁ゆえ全ペア走査で無料である。
fn root_roles(scan_paths: &[ScanPath]) -> Vec<RootRole> {
    let keys: Vec<String> = scan_paths
        .iter()
        .map(|sp| crate::config::normalize_scan_path_key(&sp.path))
        .collect();
    (0..keys.len())
        .map(|i| RootRole {
            check: keys[..i].iter().any(|h| roots_overlap_pair(h, &keys[i])),
            record: keys[i + 1..]
                .iter()
                .any(|j| roots_overlap_pair(&keys[i], j)),
        })
        .collect()
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test -p snotra-core root_roles`
Expected: PASS（7 本）

- [ ] **Step 5: `Dedup` 型を実装し、`accept_entry` を置き換える**

Task 3 が入れた `accept_entry` を**削除**し、同じ位置へ次を置く。

```rust
/// 走査中の重複排除の状態。**集合・バッファ・根の役割を 1 つに束ねる**——別々の引数で
/// 並べると、再帰の呼び出し点で組を崩せてしまう。
struct Dedup {
    /// 重なる根が 1 つも無ければ `None`（この走査は重複排除を必要としない）。
    set: Option<std::collections::HashSet<String>>,
    /// 照合だけの根で使い回す正規化キーのバッファ。**確保を走査あたり 1 回に抑える。**
    buf: String,
    /// いま走査している根の役割。[`scan_all`] のループが根ごとに差し替える。
    role: RootRole,
}

impl Dedup {
    /// このエントリを採用してよいか。
    ///
    /// **`record` が偽の根で `normalize_entry_key` を呼ばないことが本設計の全部である**
    /// ——実運用点では最大の根がこちらへ回り、30 万件ぶんの `String` 確保が消える。
    /// 照合は [`normalize_entry_key_into`] で 1 本のバッファへ詰め直し、`HashSet<String>` を
    /// `&str` で引く（`Borrow<str>`）。**記録側と照合側が同じ関数を通ることがバイト一致の
    /// 根拠である**——別実装を書き起こしてはならない。
    fn accept(&mut self, path: &str) -> bool {
        let Some(set) = self.set.as_mut() else {
            return true;
        };
        match (self.role.check, self.role.record) {
            // 積む根は insert が照合を兼ねる（既出なら false が返る）。
            (_, true) => set.insert(normalize_entry_key(path)),
            (true, false) => {
                normalize_entry_key_into(&mut self.buf, path);
                !set.contains(self.buf.as_str())
            }
            (false, false) => true,
        }
    }
}
```

- [ ] **Step 6: `scan_all` と `scan_directory_with_extensions` を繋ぎ替える**

```rust
pub fn scan_all(scan_paths: &[ScanPath], show_hidden_system: bool) -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let roles = root_roles(scan_paths);
    // **どの根も集合に触れないなら建てない**（根拠は [`root_roles`] の doc）。
    let needs_set = roles.iter().any(|r| r.check || r.record);
    let mut dedup = Dedup {
        set: needs_set.then(std::collections::HashSet::new),
        buf: String::new(),
        role: RootRole {
            check: false,
            record: false,
        },
    };

    for (sp, role) in scan_paths.iter().zip(&roles) {
        dedup.role = *role;
        let ext_set = build_extension_list(&sp.extensions);
        scan_directory_with_extensions(
            Path::new(&sp.path),
            &ext_set,
            sp.include_folders,
            show_hidden_system,
            &mut entries,
            &mut dedup,
        );
    }

    entries
}
```

`scan_directory_with_extensions` の第 6 引数を `dedup: &mut Dedup` にし、2 つの採用点を次へ置き換える（**`!name.is_empty() &&` の後ろに置く短絡評価を保つ**）。

フォルダ側:

```rust
                if !name.is_empty() {
                    let path_str = path.to_string_lossy();
                    if dedup.accept(path_str.as_ref()) {
                        entries.push(AppEntry {
                            name,
                            target_path: path_str.into_owned(),
                            is_folder: true,
                        });
                    }
                }
```

ファイル側:

```rust
                let path_str = path.to_string_lossy();
                if !name.is_empty() && dedup.accept(path_str.as_ref()) {
                    entries.push(AppEntry {
                        name,
                        target_path: path_str.into_owned(),
                        is_folder: false,
                    });
                }
```

再帰呼び出しは `dedup` をそのまま渡す（自動再借用）。

- [ ] **Step 7: 既存テストの呼び出しを直す**

`scan_directory_with_extensions` を直接呼ぶテストが 5 箇所ある。**行番号で辿らない**——次で位置を出す。

```bash
grep -n "scan_directory_with_extensions" snotra-core/src/indexer.rs
```

Task 3 が入れた `let mut seen = Some(std::collections::HashSet::new());` を次へ置き換え、呼び出しの引数も `&mut dedup` にする。

```rust
        let mut dedup = Dedup {
            set: Some(std::collections::HashSet::new()),
            buf: String::new(),
            role: RootRole {
                check: false,
                record: true,
            },
        };
```

**`record: true` を使う**——これらのテストは単一ディレクトリの走査で重複排除の挙動を見ており、従来の `insert` する形と等価にするため。

Run: `cargo test -p snotra-core`
Expected: PASS（全テスト）

- [ ] **Step 8: 変異注入 —— 検知器が本当に発火することを実測する**

**2 つの変異を順に入れ、それぞれで落ちることを確かめる。** 片方だけでは向きの誤りを捕まえられない。

変異 A（積む側を潰す）:

```rust
            record: false, // ← 変異。`root_roles` の record を常に false にする
```

Run: `cargo test -p snotra-core scan_all_dedups`
Expected: **FAIL** — `scan_all_dedups_when_roots_are_nested` と `scan_all_dedups_when_the_child_root_comes_first` の両方が `left: 2, right: 1` で落ちる

変異 B（照合側を潰す）:

```rust
            check: false, // ← 変異。`root_roles` の check を常に false にする
```

Run: `cargo test -p snotra-core scan_all_dedups`
Expected: **FAIL**（同上）

**どちらかが落ちなかったら、そこで止まって報告する。** 検知器が対象を捕まえていないということであり、先へ進んではならない。

- [ ] **Step 9: 変異を戻し、全体が通ることを確認**

Run: `cargo test -p snotra-core`
Expected: PASS（全テスト）

- [ ] **Step 10: doc の検査**

Run: `cargo doc --workspace --no-deps --document-private-items`
Expected: 成功

- [ ] **Step 11: コミット**

```bash
git add snotra-core/src/indexer.rs
git commit -F <path>
```

メッセージ:

```
perf(core): 重複排除の代金を根ごとに決める（#1002）

実 config は 4 根で C:\ が他 3 根の祖先だった。「重なるなら建てる」の粒度
では判定が真になり、実運用点の削減が 0 だった（allocs 5,991,749 →
5,991,979・peak 変化なし）。判定は正しく、前提が偽だった。

木の走査は同じディレクトリを二度読まないので、あるエントリが二度現れるのは
「根 i に入ったものが後続の根 j > i の走査でも現れる」ときに限る。ゆえに
積むのは後続と重なる根だけでよく、最後の根は照合するだけでよい。実運用点で
は最大の根 C:\ が最後に来るため、その 30 万件が丸ごと照合のみになる。

照合側は normalize_entry_key_into で 1 本のバッファへ詰め直し、HashSet を
&str で引く（確保 0）。記録側と照合側が同じ関数を通ることがバイト一致の
根拠である。

額は根の順序に依存するが、順序を並べ替えて額を取りに行ってはならない——
返り値の順序が変わり entries_digest がずれる。

検知器は 2 つの変異（record を常に false / check を常に false）で
それぞれ落ちることを実測した。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

## Task 5: B 側標本を取り、文書へ落とす

**Files:**
- Modify: `PERFORMANCE.md`（節を追加）
- Modify: `snotra-core/CLAUDE.md`（「indexer.rs の背景再スキャン」節へ 1 行）

**Interfaces:**
- Consumes: Task 1 の A 側標本（allocs / peak / 件数 / 重複件数）と、本タスクで取る B 側標本

- [ ] **Step 1: B 側標本を取る**

Run: `cargo test --release -p snotra-core --test memory_footprint -- --ignored --nocapture --test-threads=1`
Expected: `scan_all（背景再スキャン経路・常駐外）` の `allocs` が A 側から大きく落ち、`正規化キーの重複: 0 件` が出る。

**重複件数が非 0 なら、そこで止まって報告する**（設計書 §6）。

- [ ] **Step 2: `PERFORMANCE.md` へ節を追加**

「採用: PATH スキャンの問いを反転（確保 314,395 → 2,066・反復 9）」の節と同じ様式で、**見出しに壁時計を載せず**に書く。

```markdown
### 採用: `scan_all` の `seen` を根の重なりに条件づける（確保 xxx,xxx → x,xxx・#1002）

`indexer::scan_all` は重複排除のため全ヒットの正規化キー（1 件ごとに `String` を確保）を
`HashSet` へ積んでいた。**この `seen` が代金に見合うのは走査の根どうしが重なりうるときだけ**
で、根が `C:\` 一本の運用点では純粋な費用である。

木の走査は同じディレクトリを二度読まないので、1 回の `scan_all` の中で同じ正規化キーが
二度現れる経路は**根が入れ子のとき以外に無い**（`dedup_scan_paths` は完全一致マージのみ
ゆえ入れ子の根は表現可能であり、`seen` を素で消す案は成立しない）。

| 指標 | before | after |
|---|---:|---:|
| **確保回数** | （A 側） | （B 側） |
| 区間 peak | （A 側） | （B 側） |
| エントリ件数 | （A 側） | （B 側・churn で数十件ぶれる） |
| 正規化キーの重複 | — | **0 件** |

**壁時計は指標にならない。** 区間全体が実走査そのもの（数万 ms）で、消える正規化 + 挿入は
その 1% 未満に埋もれる。反復 9（PATH スキャン）と同型だが、あちらは区間全体が 186 ms で
削減が -71% として現れた。**削るべきは瞬間の常駐であり、背景再スキャンは低優先度スレッド
から走るので壁時計は元から問題ではない。**

**受け入れは「1 回の走査の中で正規化キーの重複が 0 件」で判定した。** A/B で走査結果を
突き合わせてはならない——対象は生きた `C:\` 全体であり、A と B の間に temp・ログ・キャッシュ
の churn が必ず挟まる。重複が 0 なら、旧実装は何も落としていない＝件数も順序も一致する
ことが論理的に閉じる。

**残余**: NTFS の case-sensitive ディレクトリで `Foo.exe` と `foo.exe` が同居していれば
単一根でも重複が出うる。実運用点では **0 件**であった（測って 0 だった、という記録である）。
```

**（A 側）（B 側）の欄には Task 1 / Step 1 で控えた実数を入れる。** プレースホルダのまま
コミットしてはならない。

- [ ] **Step 3: `snotra-core/CLAUDE.md` へ不変条件を 1 行**

「indexer.rs の背景再スキャン」節の箇条書きへ足す。

```markdown
- **`scan_all` の `seen` は根どうしが重なるときだけ建てる**（`roots_overlap`）。木の走査は同じディレクトリを二度読まないので、1 回の走査の中で同じ正規化キーが二度現れる経路は**根が入れ子のとき以外に無い**——`dedup_scan_paths` は完全一致マージのみゆえ入れ子の根は表現可能であり、**素で消すことはできない**。検知器は `scan_all_dedups_when_roots_are_nested`（`roots_overlap` を `false` へ潰すと落ちることを実測済み）。**削減そのものの退行は挙動テストでは捕まらない**——`seen` を建てても結果は同じなので、捕まえるのは `tests/memory_footprint.rs` の確保回数だけである
```

- [ ] **Step 4: ガバナンス検査**

Run: `npm run governance:check`
Expected: 全検査 passed

- [ ] **Step 5: コミット**

```bash
git add PERFORMANCE.md snotra-core/CLAUDE.md
git commit -F <path>
```

メッセージ:

```
docs: scan_all の seen 条件づけの実測を記録する（#1002）

確保回数と区間 peak の A/B、および受け入れゲート（正規化キーの重複 0 件）の
実測を PERFORMANCE.md へ。壁時計は指標にならない——区間全体が実走査そのもの
で、消える正規化 + 挿入はその 1% 未満に埋もれる。

削減そのものの退行は挙動テストでは原理的に捕まらない（seen を建てても結果は
同じ）ことを snotra-core/CLAUDE.md に明記した。捕まえるのは計器だけである。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

## 検知器の射程（受容する残余）

**「`seen` を建てない」ことの検知器は挙動テストでは原理的に作れない。** `seen` を建てても
走査結果は同じなので、`roots_overlap` を常に `true` へ潰す変異は**どのテストも落とさない**。

- 機構で守られるのは片側だけである: `roots_overlap` を常に `false` へ潰す変異は
  `scan_all_dedups_when_roots_are_nested` が捕まえる（Task 3 / Step 5 で実測する）
- 反対側（削減が消える退行）を捕まえるのは `tests/memory_footprint.rs` の確保回数だけで、
  これは `#[ignore]` の手動計測である。**CI は守らない**

これは受容する残余である。Task 4 / Step 3 で `snotra-core/CLAUDE.md` へ明記する。

## PR

4 タスクすべてを 1 PR にまとめる。**`gh pr create` の前に `git push -u origin HEAD` を打つ**
（未 push だと `pre-bash` hook が空 PR として拒む）。PR 本文には issue の closing keyword
（`Closes #1002`）を入れ、マージは `/merge-pr` の手順で行う。

# 背景再スキャンの結果をセッションへ適用する 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 背景再スキャンが `Changed` を返したとき、走っているセッションの索引をその場で差し替える（再起動を待たない）。**走査の回数は 1 回のまま**にする。

**Architecture:** 再スキャンが既に手に持っていて捨てている材料（`save_cache_sorted_in` の戻り値）を `IndexMaterial` として運び、`src-tauri` が索引を建てて `apply_prebuilt_index` で差し替える。索引を建てる手順は `indexing.rs` の drain ループと**同じ 1 つの関数**を通す。

**Tech Stack:** Rust 2024 / `snotra-core::{indexer, engine}` / Tauri managed state

**設計の正本:** `docs/superpowers/specs/2026-08-10-rescan-applies-its-result-design.md`

## Global Constraints

- **走査を 2 回にしない。** `start_index_build` を呼んではならない——その drain ループは `rebuild_and_save`（= `scan_all`）を通る。#1001 の当の代金が倍になる
- **索引を建てる手順を写さない。** PATH マージを含む手順は 1 つの関数に閉じ、drain ループと再スキャンの適用が同じそこを通る。写すと片方だけ PATH マージを忘れ、**PATH のコマンドが検索から消えるのに検索結果は出る**（気づく手段が無い）
- **`complete_index_drain` を使わない。** 台帳へ「現在の `IndexInputs` を満たした」と宣言する操作であり、起動時の config で走査した再スキャンにその資格は無い。使うのは `apply_prebuilt_index`
- **stale なら差し替えない。** config が変わって本式ビルドが動く（動いている）ので譲る
- **重い構築はロックの外。** ロック内に入れてよいのは差し替えの一瞬だけ
- **通知を出さない。** `indexing-started` / `indexing-complete` を飛ばさない（トレイが操作不能になる）
- **材料を運ぶのは `Changed` のときだけ。** 形式の昇格だけなら中身は同一で、差し替えても得るものが無い
- `cargo clippy --workspace --all-targets -- -D warnings` が通ること。コミットは各タスク末、`main` へ直接コミットしない（本作業のブランチは `feat/rescan-applies-its-result`）

---

## File Structure

| ファイル | 責務 | タスク |
|---|---|---|
| `snotra-core/src/indexer.rs` | `RescanRun` の定義・材料を運ぶ・偽コメントの修正 | 1 |
| `src-tauri/src/indexing.rs` | 索引を建てる手順を 1 関数へ切り出す（drain と共有） | 2 |
| `src-tauri/src/main.rs` | 再スキャンの適用（stale ガード + 差し替え）・構造の検知器 | 3 |
| `SPEC.md` / `snotra-core/CLAUDE.md` / `src-tauri/CLAUDE.md` | 文書同期 | 5 |

---

### Task 1: 材料を運ぶ（snotra-core）

**Files:**
- Modify: `snotra-core/src/indexer.rs`
- Test: 同ファイルの `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `IndexMaterial::derived(tree, masks)`（`pub(crate)`・既存）、`save_cache_sorted_in`（既存）
- Produces:
  - `pub struct RescanRun { pub outcome: RescanOutcome, pub material: Option<IndexMaterial> }`
  - `BackgroundRescanTask::run(self) -> RescanRun`（返り値の型が変わる）
  - `fn try_background_rescan_in(..., cached_len: usize) -> RescanRun`（引数は不変・返り値の型だけ変わる）

- [ ] **Step 1: 失敗するテストを書く**

`indexer.rs` の `mod tests` に足す。**`temp_dir` / `INDEX_LOCK_TEST_GUARD` / `rescan_log_lines` は既存**のものを使う。

```rust
    /// **`Changed` のとき材料が返る。** これが無いと呼び出し側は索引を建てられず、
    /// 走っているセッションの索引は最後まで古いままになる（索引が 1 起動ぶん遅れる）。
    /// 変異: 材料を捨てる実装（現状への回帰）でこの検査が落ちる。
    #[test]
    fn background_rescan_returns_material_when_entries_changed() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_material_changed");
        let scan: Vec<ScanPath> = Vec::new();
        let hash = compute_config_hash(&scan, false);
        save_cache_sorted_in(&dir, Vec::new(), hash);

        // 空の走査結果に対し、キャッシュ側の digest をわざと食い違わせて Changed にする。
        let run = try_background_rescan_in(
            &dir,
            &scan,
            false,
            hash,
            entries_digest(&[]) ^ 1,
            current_index_generation(),
            INDEX_CACHE_VERSION,
            0,
        );
        assert_eq!(run.outcome, RescanOutcome::Changed);
        assert!(
            run.material.is_some(),
            "Changed なら索引の材料が返ること（捨てると 1 起動ぶん遅れる）"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// **`Unchanged` では材料を返さない。** 中身が同じなのに差し替えれば、索引 1 本ぶんの
    /// 常駐を無駄に積むだけである。変異: 常に材料を返す実装で落ちる。
    #[test]
    fn background_rescan_returns_no_material_when_unchanged() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_material_unchanged");
        let scan: Vec<ScanPath> = Vec::new();
        let hash = compute_config_hash(&scan, false);
        save_cache_sorted_in(&dir, Vec::new(), hash);

        let run = try_background_rescan_in(
            &dir,
            &scan,
            false,
            hash,
            entries_digest(&[]),
            current_index_generation(),
            INDEX_CACHE_VERSION,
            0,
        );
        assert_eq!(run.outcome, RescanOutcome::Unchanged);
        assert!(run.material.is_none(), "Unchanged で材料を返してはならない");

        let _ = fs::remove_dir_all(&dir);
    }

    /// **`Skipped` でも材料を返さない**（走査すらしていない）。
    #[test]
    fn background_rescan_returns_no_material_when_skipped() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_material_skipped");
        let held = INDEX_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let run = try_background_rescan_in(
            &dir,
            &[],
            false,
            0,
            0,
            current_index_generation(),
            INDEX_CACHE_VERSION,
            0,
        );
        drop(held);
        assert_eq!(run.outcome, RescanOutcome::Skipped);
        assert!(run.material.is_none(), "Skipped で材料を返してはならない");

        let _ = fs::remove_dir_all(&dir);
    }
```

さらに**既存**のテスト `background_rescan_upgrades_stale_format_when_entries_are_unchanged` に 1 行足す（fixture を複製しないため）:

```rust
        assert!(
            outcome.material.is_none(),
            "形式の昇格だけなら中身は同一——材料を返してはならない"
        );
```

- [ ] **Step 2: テストが落ちることを確認する**

Run: `cargo test -p snotra-core background_rescan_returns_material`
Expected: **コンパイルエラー**（`RescanRun` が無く、`try_background_rescan_in` は `RescanOutcome` を返す）

- [ ] **Step 3: `RescanRun` を定義する**

`RescanOutcome` の enum 定義の**直後**に置く:

```rust
/// 背景再スキャンの結果。**結末と、索引を建てる材料の組**である。
///
/// **材料が載るのは `Changed` のときだけである。** 形式の昇格だけ（`format_upgrade`）の
/// ときは中身が同一で、差し替えても得るものが無い（索引 1 本ぶんの常駐を無駄に積む）。
///
/// **`RescanOutcome` 自身に材料を持たせない。** 持たせると `Copy` と `PartialEq` を失い、
/// 結末だけを見る既存の検知器が一斉に書き換えになる。
pub struct RescanRun {
    pub outcome: RescanOutcome,
    /// `Changed` のときの索引の材料。**呼び出し側がこれで索引を建てて差し替える。**
    /// 走査はここまでで 1 回きりであり、呼び出し側が建て直す必要は無い
    /// ——建て直す実装（`start_index_build` を呼ぶ形）は全走査を 2 回にする。
    pub material: Option<IndexMaterial>,
}
```

- [ ] **Step 4: `try_background_rescan_in` を材料つきにする**

戻り値の型を `RescanRun` に変え、`let mut rec = ...` の直後に材料の受け皿を足す:

```rust
    let mut rec = rescan_log::RescanRecord::default();
    let mut material = None;
```

保存の箇所を置き換える（**コメントも差し替える。現状のコメントは事実でない**）:

```rust
            let t = Instant::now();
            // **保存が返した木と `CachedMasks` を、そのまま索引の材料にする。**
            // かつてここは捨てており（「呼び出し側が再構築を kick する」と書いてあったが、
            // その kick は存在しなかった）、`index.bin` は新しいのに走っているセッションの
            // 索引は最後まで古いままだった——**索引が常に 1 起動ぶん遅れていた**。
            // 建て直すのではなく運ぶので、**走査は 1 回のままである**。
            let (tree, masks) = save_cache_sorted_in(dir, scanned, config_hash);
            rec.save = Some(t.elapsed());
            // **中身が変わったときだけ運ぶ**（昇格だけなら中身は同一）。
            if changed {
                material = Some(IndexMaterial::derived(tree, masks));
            }
```

末尾を組に変える:

```rust
    RescanRun { outcome, material }
```

- [ ] **Step 5: `run()` と `try_background_rescan` の型を通す**

`try_background_rescan` の戻り値を `RescanRun` に、`BackgroundRescanTask::run` の戻り値を `RescanRun` に変える。`run` の doc を実際に起きることへ直す:

```rust
    /// 再スキャンを実行し、**結末と索引の材料の組**を返す。`Changed` のときは呼び出し側が
    /// アイコンキャッシュを無効化し、返った材料で索引を建てて差し替える。
    pub fn run(self) -> RescanRun {
```

- [ ] **Step 6: 既存の呼び出し点を機械的に直す**

Run: `grep -n "try_background_rescan_in(\|task.run()\|\.run()" snotra-core/src/indexer.rs`

結末を比べているテストは `run.outcome` を見る形へ変える。**アサーションの意味は変えない**——変える必要が出たら、それは挙動を変えてしまった合図として報告する。

- [ ] **Step 7: テストが通ることを確認する**

Run: `cargo test -p snotra-core --lib`
Expected: PASS（既存の再スキャンテストが全部通り、新規 3 本 + 追記 1 行が加わる）

- [ ] **Step 8: lint・整形・doc**

Run: `cargo fmt --all` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo doc --workspace --no-deps --document-private-items`

- [ ] **Step 9: コミット**

コミットメッセージは Write ツールで一時ファイルへ書き `git commit -F <path>`（**bash の HEREDOC は Windows で壊れる**）。件名: `feat(core): 背景再スキャンが索引の材料を返す（走査は 1 回のまま）`

---

### Task 2: 索引を建てる手順を 1 関数へ切り出す（挙動不変）

**Files:**
- Modify: `src-tauri/src/indexing.rs`

**Interfaces:**
- Consumes: `indexer::IndexMaterial`・`IndexInputs`・`PrebuiltIndex`（すべて既存）
- Produces: `pub(crate) fn build_index_from_material(material: indexer::IndexMaterial, inputs: &IndexInputs) -> PrebuiltIndex`

- [ ] **Step 1: 関数を切り出す**

`drain_index` の**上**に置く:

```rust
/// 材料から索引を建てる。**PATH エントリのマージを含む。**
///
/// **drain ループと背景再スキャンの適用が同じここを通ることが、両者が一致することの
/// 根拠である。** 手順を写すと、片方だけ PATH マージを忘れる欠陥が沈黙で起きる
/// ——PATH のコマンドが検索から消えるが、検索結果自体は出るので気づく手段が無い
/// （`normalize_entry_key_into` と同じ理屈）。
pub(crate) fn build_index_from_material(
    mut material: indexer::IndexMaterial,
    inputs: &IndexInputs,
) -> PrebuiltIndex {
    // **木とマスクは組のまま持つ**ので、片方だけ伸ばす形はここでは書けない
    // （正本は `IndexMaterial` の doc）。
    if inputs.include_path_env {
        let path_entries = indexer::scan_path_env(material.tree(), inputs.show_hidden_system);
        material.extend_with_path_entries(path_entries);
    }
    // **ここで分岐しない。** 派生データの有無で建て方が分かれるのは
    // `SearchEngine::from_material` の 1 か所だけである。
    PrebuiltIndex::from_material(material, inputs.migemo_enabled)
}
```

ファイル先頭の `use` に `snotra_core::engine::{IndexInputs, PrebuiltIndex};` を足す（既存の import と重複しないよう確認する）。

- [ ] **Step 2: `drain_index` を通す**

PATH マージと `PrebuiltIndex::from_material` の行を置き換える:

```rust
        let material = indexer::rebuild_and_save(&inputs.scan, inputs.show_hidden_system);
        let new_index = build_index_from_material(material, &inputs);
```

`let mut material` の `mut` は不要になるので落とす。**移した 2 つのコメント**（PATH マージ・分岐しない）は新しい関数側にあるので、ここには残さない（写しを作らない）。

- [ ] **Step 3: 挙動が変わっていないことを確認する**

Run: `cargo test -p snotra` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo fmt --all -- --check`
Expected: PASS。**このタスクは純粋な切り出しであり、既存テストの書き換えは 1 行も要らない。** 要るなら挙動を変えてしまった合図として報告する。

- [ ] **Step 4: コミット**

件名: `refactor(tauri): 索引を建てる手順を 1 関数へ切り出す（drain と共有する準備）`

---

### Task 3: 再スキャンの結果を差し替える

**Files:**
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/main.rs` の `#[cfg(test)] mod tests`（**無ければ新設する**）

**Interfaces:**
- Consumes: Task 1 の `RescanRun`、Task 2 の `build_index_from_material`
- Produces: `fn apply_rescanned_index(app: &AppHandle, material: indexer::IndexMaterial)`

- [ ] **Step 1: 適用を書く**

`setup_background_rescan` の spawn 本体を置き換える:

```rust
            .spawn(move || {
                indexer::lower_current_thread_priority();
                let run = task.run();
                if run.outcome != indexer::RescanOutcome::Changed {
                    return;
                }
                if let Some(icons) = handle_for_rescan.try_state::<IconCacheState>() {
                    icon::invalidate_icon_cache(&icons);
                }
                if let Some(material) = run.material {
                    apply_rescanned_index(&handle_for_rescan, material);
                }
            });
```

`setup_background_rescan` の**下**に足す:

```rust
/// 再スキャンが返した材料で、走っているセッションの索引を差し替える。
///
/// **`start_index_build` を呼んではならない。** その drain ループは `rebuild_and_save` を
/// 通り `scan_all` をもう一度走らせる——走査が 2 回になり、#1001 の当の代金が倍になる。
/// 材料は再スキャンが既に持っており、建て直す必要は無い。
///
/// **重い構築（`build_index_from_material`）はロックの外で行う。** ロックの中に入れて
/// よいのは差し替えの一瞬だけである。
fn apply_rescanned_index(app: &AppHandle, material: indexer::IndexMaterial) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    // **stale なら譲る。** config が変わって本式ビルドが動いている（あるいはこれから
    // 動く）ということであり、起動時の config で走査したこちらに資格は無い。
    let inputs = {
        let Ok(engine) = state.engine.lock() else {
            return;
        };
        if engine.is_index_stale() {
            return;
        }
        IndexInputs::from_config(engine.config())
    };

    let index = indexing::build_index_from_material(material, &inputs);

    let Ok(mut engine) = state.engine.lock() else {
        return;
    };
    // **建てている間に config が変わりうる。** 窓は閉じないが、読み直せば大半は捕まる。
    // 取り逃しても走っている本式ビルドが後から上書きするので収束する。
    if engine.is_index_stale() {
        return;
    }
    // **`complete_index_drain` を使ってはならない**——あれは「このビルドが現在の
    // `IndexInputs` を満たした」と台帳へ宣言する操作で、起動時の config で走査した
    // こちらにその資格は無い。宣言すれば config 変更で立った stale を誤って落とし、
    // 本来走るべき再構築が走らなくなる。
    engine.apply_prebuilt_index(index);
}
```

`use` に `snotra_core::engine::IndexInputs;` を足す（既存と重複しないか確認する）。

- [ ] **Step 2: 「走査を 2 回にしない」を構造で固定する検知器を書く**

`main.rs` に `#[cfg(test)] mod tests` が無ければ末尾に新設し、次を置く:

```rust
#[cfg(test)]
mod tests {
    /// **走査を 2 回にしないことを、時間ではなく構造で固定する。**
    ///
    /// `start_index_build` の drain ループは `rebuild_and_save`（= `scan_all`）を通るので、
    /// 再スキャンの適用からそれを呼ぶと全走査が 2 回になる（#1001 の当の代金が倍）。
    /// 速さは環境で揺れるが、**呼んでいるかどうかは揺れない**。
    ///
    /// 母集団はこのファイルのソーステキストそのものである（`startup.rs` の
    /// `count_matches_the_enum_declaration` と同じ手）。
    #[test]
    fn rescan_application_does_not_kick_a_full_rebuild() {
        let src = include_str!("main.rs");
        let after = src
            .split_once("fn apply_rescanned_index(")
            .expect("apply_rescanned_index が見つからない（改名したらこの検査も直す）")
            .1;
        let body = after.split_once("\nfn ").map_or(after, |(b, _)| b);
        assert!(
            !body.contains("start_index_build"),
            "再スキャンの適用から start_index_build を呼んでいる（全走査が 2 回になる）"
        );
    }
}
```

- [ ] **Step 3: 検知器が変異で落ちることを実測する**

`apply_rescanned_index` の中へ一時的に `let _ = indexing::start_index_build(app);` を足し、
Run: `cargo test -p snotra rescan_application_does_not_kick`
Expected: **FAIL**。確認したら足した行を戻し、再度 PASS になることを確認する。**この往復を報告に書く**（落ちることを見ていない検知器は検知器ではない）。

- [ ] **Step 4: 検証**

Run: `cargo test -p snotra` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo fmt --all -- --check` → `cargo doc --workspace --no-deps --document-private-items`

- [ ] **Step 5: `/race-check` を当てる**

スレッドをまたぐ共有状態の差し替えを足したので、`/race-check` を実行する。指摘は報告に含める。

- [ ] **Step 6: コミット**

件名: `feat(tauri): 再スキャンの結果で走行中の索引を差し替える（#1001）`

---

### Task 4: 実機ゲート（再起動せずに見つかることを確かめる）

**新設した検証経路は、報告の前にその経路自体を走らせる。** 単体テストは「材料が返る」までしか見ておらず、**実際に検索へ出るか**は見ていない。

**Files:** 変更なし（観測のみ）

- [ ] **Step 1: release をビルドする**

Run: `cargo build --release -p snotra`

- [ ] **Step 2: 目印を置いてから起動する**

```powershell
Set-Content 'C:/tmp/snotra-swap-marker-zzz.txt' 'swap gate'
Remove-Item "$env:APPDATA/Snotra/rescan-log.jsonl" -ErrorAction SilentlyContinue
$p = Start-Process 'C:/workspace/Snotra/target/release/snotra.exe' -PassThru
```

- [ ] **Step 3: 再スキャンの完走を待つ**

`%APPDATA%/Snotra/rescan-log.jsonl` に `"ev":"end"` かつ `"outcome":"changed"` の行が現れるまで待つ（**時間で決め打ちしない**——環境で数倍ぶれる）。

- [ ] **Step 4: 再起動せずに検索して出ることを確かめる**

`Ctrl+K` でウィンドウを出し、`snotra-swap-marker-zzz` と入力して**結果に出ること**を確認する。カテゴリ D（目視）に当たるので、人が見るか、打鍵注入 + 窓キャプチャで実施する（`docs/build-commands.md`「エージェントが目視項目を自分で実施するとき」）。

**これが本タスクの全部である。** ここが赤なら差し替えは効いていない。

- [ ] **Step 5: 後片付けと記録**

プロセスを停止し、目印を削除する。**観測の出力（`rescan-log.jsonl` の該当行と、検索結果の確認）を PR 本文へ貼る。** 「確かめた」と書くのではなく出力を貼る。

---

### Task 5: 文書同期と PR

**Files:**
- Modify: `SPEC.md`（§3.3）
- Modify: `snotra-core/CLAUDE.md`（「indexer.rs の背景再スキャン」）
- Modify: `src-tauri/CLAUDE.md`（`main.rs` / `indexing.rs` の該当記述）

- [ ] **Step 1: `SPEC.md` §3.3 を同期する（仕様変更）**

「通常起動時はハイブリッド方式」の子項目を次の形にする:

```markdown
  - 差分があればキャッシュを更新し、**実行中のセッションの索引もその場で差し替える**（次回起動を待たない）
```

さらに 1 項目足す:

```markdown
  - 設定変更による再構築が予定されている場合、差分スキャンの結果は差し替えず本式の再構築に譲る
```

- [ ] **Step 2: `snotra-core/CLAUDE.md` を直す**

「indexer.rs の背景再スキャン」節の「`RescanOutcome::Changed` ならアイコンキャッシュを無効化する」を、**材料を返して呼び出し側が索引を差し替えること**を含む記述へ改める。**タスクが抱えるのは digest だけ**という既存の記述は生きているので消さない（材料は結果として返るものであって、抱えているものではない）。

- [ ] **Step 3: `src-tauri/CLAUDE.md` を直す**

`main.rs` / `indexing.rs` の記述に、索引を建てる手順が 1 関数に閉じていること（drain と再スキャンの適用が共有すること）と、差し替えが `apply_prebuilt_index` であって台帳を claim しないことを足す。

- [ ] **Step 4: governance:check**

Run: `npm run governance:check`
Expected: 全検査 passed

- [ ] **Step 5: コミット**

件名: `docs: 再スキャンの結果適用を SPEC §3.3 とモジュール索引へ同期`

- [ ] **Step 6: PR を作る**

**`gh pr create` の前に push する**（未 push＝空 PR は hook が拒む）。**鎖に `cd` を含めない。**

```bash
git push -u origin HEAD
```

PR 本文には Task 4 の観測結果を貼り、`Refs #1001` を書く。**`Closes #1001` にしない**——本 issue は間引きと SPEC §3.3 の「差分スキャン」の乖離まで含んでおり、この反復はそこを閉じない。

---

## Self-Review

**1. Spec coverage**

| spec の節 | 実装するタスク |
|---|---|
| §2.1 走査は 1 回のまま（材料を運ぶ） | Task 1 Step 4 |
| §2.2 運ぶ形（`Changed` のときだけ） | Task 1 Step 3・Step 4、検知器は Step 1 |
| §2.3 drain と同じ 1 関数を通す | Task 2 |
| §2.4 `apply_prebuilt_index`・stale なら譲る | Task 3 Step 1 |
| §2.5 通知を出さない | Task 3 Step 1（`indexing-started` を書かない） |
| §3 受容する残余 | Task 1・3 の doc コメント |
| §4 検証（検知器・実機ゲート） | Task 1 Step 1、Task 3 Step 2〜3、Task 4 |
| §5 文書更新 | Task 5 |
| §6 却下した案 | Task 1・3 の doc コメントが理由を持つ |

**2. Placeholder scan**: TBD / TODO / 「適切に処理する」の類は無し。全コードステップに実コードが入っている。

**3. Type consistency**: `RescanRun { outcome, material }` のフィールド名は Task 1 の定義と Task 3 の使用で一致。`build_index_from_material(material, &inputs)` の引数順は Task 2 の定義と Task 3 の呼び出しで一致。`IndexInputs::from_config(engine.config())` は既存 API（`engine.rs` の `pub fn config`）。

**4. 実装者向けの注意**

- **`material` は closure の外で宣言し、closure から `&mut` で書く。** `rec` が既に同じ形なので、借用は同じ理屈で通る
- **`main.rs` に `#[cfg(test)] mod tests` が無い可能性がある。** 無ければ新設する（`include_str!("main.rs")` は同ファイル内から読める）
- **`state.engine.lock()` の poison は `let Ok(..) else { return }` で握り潰す。** ここは best-effort の適用経路であり、計器と同じく製品を落とさない（release は `panic = "abort"`）
- **PATH マージの一致は検知器ではなく構造で担保している**（1 関数を共有する）。テストで縛ろうとすると `scan_path_env` が実 PATH を読むため flaky になる。**受容する残余として PR 本文に書くこと**

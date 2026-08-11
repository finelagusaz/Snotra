# 検索の worker 化 実装計画（#1004）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** パスクエリの打鍵中に `Engine::search` がフレームを 20 ms 止める状態を解消し、検索をプロセス寿命の worker へ出して seq 照合で採り込む。

**Architecture:** 2 PR。PR 1 でフレーム所要時間と打鍵→採り込みの計器を入れ、A 側ベースラインを採り、検知器 H6 が**実運用点で赤いこと**を実測する。PR 2 で単一 worker + 最新クエリ勝ちへ移し、同じ H6 を緑へ倒す。設計の正本は `docs/superpowers/specs/2026-08-10-search-worker-design.md`。

**Tech Stack:** Rust（`src-tauri`・egui/softbuffer・`std::sync::mpsc`）/ PowerShell 7（smoke・Pester）

## Global Constraints

- **`main` へ直接コミット・プッシュしない。** PR 1 は現ブランチ `perf/search-worker`、PR 2 は `perf/search-worker-async` を新規に切る
- **`cargo test -p snotra --lib` は常に失敗する**（`src-tauri` は `[lib]` を持たない）。テストは `cargo test -p snotra`
- **PostToolUse hook が `.rs` の編集ごとに fmt / clippy / test を自動実行する。沈黙は合格である**。手で再実行しない
- **doc コメント（`///` / `//!`）を足したら `cargo doc --workspace --no-deps --document-private-items` を手で走らせる**（intra-doc link 切れは hook が沈黙し CI でのみ落ちる）
- **新しく書くコメントは「1 段落 1 行」——文の途中で物理改行を入れない**（`docs/comment-guidelines.md`「日本語の折返し」。表・箇条書き・コードフェンスは対象外）。行が長くなって構わない——rustfmt はコメントを折り返さないので長い行がそのまま正しい形である。**折返しは rustdoc の描画を壊さず grep だけを壊す**ので気づかれない（2026-08-08 の実測で 5 件、うち 1 件は識別子を分断して grep が 0 件を返した）。**本計画のコード例はすべてこの形で書いてあるので、逐語で写せば規約を満たす**。**適用は `.rs` である**——PowerShell（`.ps1` / `.psm1`）のコメントは既存ファイルの作法（折返しあり）へ合わせること。同ガイドラインは `.rs` の編集で配送され、模範例も Rust であり、そこへ厳格適用すると既存と不揃いになる
- **`.rs` ファイルを新規追加したら `src-tauri/CLAUDE.md`「モジュール構成」の該当リストへファイル名行を足し、`npm run governance:check` を走らせる**
- **検知器（H6 / H7）を足したら、故障注入で実際に発火することを一度測る**（`.claude/rules/safety-nets.md`）。稼働中のガードは弱めず、複製に変異を当てる
- `-D warnings` 下では未使用の新 API が `dead_code` で落ちる。**新しい型・関数の導入と呼び出し点の移行は同じタスクに束ねる**

### trace と判定器について、実装前に頭へ入れておく事実（2026-08-10 に実測）

これを知らずに書くと必ず踏む:

- **trace 行の封筒は `{seq, ts_ms, event, data}`** である（`src-tauri/src/trace.rs`）。`seq` は**プロセス全体の連番**であり、本計画が導入する検索の識別子とは別物。**payload 側の名前は `dispatch_seq` にする**——`seq` にすると判定器が封筒の連番を読む罠になる
- 判定器の公開面は **`Test-SnotraTraceInvariants -Events <psobject[]> -Sections <hashtable[]> -DroppedLineCount <int>`** である。`Invoke-SnotraTraceJudgement` は内部。**smoke（`smoke-egui.ps1`）も Pester（`SnotraTraceInvariants.Tests.ps1`）も前者を呼ぶ**
- **判定器は実 trace に当たっている。** `smoke-egui.ps1` が冒頭でモジュールを Import し、`Get-SnotraTraceInvariantNames` を回して違反を集計する（`-Sections @()` で呼ぶので区間帰属は擬似区間 0 になる）
- **返り値は `Sections` / `Overall` / `Counts` / `Violations` / `Unjudgeable` / `Observed` / `DroppedLineCount` / `JudgeFailed`。** `Skips` や `Passes` という配列は**無い**。SKIP は「PASS も FAIL も記録されなかった」ことで決まり、判定不能の理由は `Unjudgeable` へ積む
- プロパティの読みは **`Get-SnotraTraceProperty -InputObject $x -Name '<name>'`**（`-Object` ではない）。StrictMode 下で欠落プロパティへ直接触ると例外になるため、必ずこれを通す
- **`smoke:egui` は CI で走る**（`.github/workflows/e2e.yml` の `smoke-egui` job）
- Pester の実行は **`npm run test:powershell`**

---

# PR 1 — 計器と A 側ベースライン

## Task 1: フレーム計時の純粋核

**Files:**
- Modify: `src-tauri/src/egui_shell/layout.rs`（`Debouncer` の隣・同ファイルの `#[cfg(test)] mod tests` にテストを足す）

**Interfaces:**
- Produces: `layout::FrameTimer` — `FrameTimer::default()` / `fn begin(&mut self, now: Instant) -> Option<Duration>`（前フレームの開始からの間隔。初回は `None`）

- [ ] **Step 1: 失敗するテストを書く**

`layout.rs` の `mod tests` へ足す:

```rust
#[test]
fn frame_timer_reports_interval_from_previous_begin() {
    let base = Instant::now();
    let mut t = FrameTimer::default();
    assert_eq!(t.begin(base), None, "初回は比較元が無い");
    assert_eq!(
        t.begin(base + Duration::from_millis(50)),
        Some(Duration::from_millis(50)),
        "2 フレーム目は前回 begin からの間隔"
    );
    assert_eq!(
        t.begin(base + Duration::from_millis(70)),
        Some(Duration::from_millis(20)),
        "間隔は直前の begin 基準（初回基準ではない）"
    );
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra frame_timer_reports_interval`
Expected: FAIL（`cannot find type FrameTimer`）

- [ ] **Step 3: 最小実装**

```rust
/// フレームの開始時刻を 1 つだけ持ち、前フレームからの間隔を返す（#1004 PR 1）。
/// **間隔は合否ではなく内訳である**——判定に使うのはフレームの所要時間の側で、このランタイムはイベント駆動ゆえ健全でも間隔は debounce 幅・打鍵間隔まで開く（`docs/superpowers/specs/2026-08-10-search-worker-design.md` の §3.3 が正本）。
#[derive(Default)]
pub struct FrameTimer {
    last_began: Option<Instant>,
}

impl FrameTimer {
    pub fn begin(&mut self, now: Instant) -> Option<Duration> {
        let interval = self.last_began.map(|prev| now.duration_since(prev));
        self.last_began = Some(now);
        interval
    }
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p snotra frame_timer_reports_interval`
Expected: PASS

- [ ] **Step 5: コミット**

```
git add src-tauri/src/egui_shell/layout.rs
git commit -m "feat(egui): #1004 フレーム間隔を測る FrameTimer の純粋核"
```

---

## Task 2: `egui_frame` を吐く配線

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（`fn update`）
- Modify: `src-tauri/src/egui_shell/mod.rs`（`layout::` の re-export 行へ `FrameTimer` を足す）

**Interfaces:**
- Consumes: `layout::FrameTimer`（Task 1）
- Produces: trace イベント `egui_frame`（`data.update_us` / `data.interval_us`）

- [ ] **Step 1: view の struct へフィールドを足す**

`view.rs` の main 窓 view の struct（`controller` / `applied_font_family` を持つ型）へ:

```rust
    /// フレーム所要と間隔の計器（#1004 PR 1）。`SNOTRA_TRACE` 無効時も進めてよい（`Instant` 差だけ）。
    frame_timer: crate::egui_shell::FrameTimer,
```

`Self { ... }` を組む箇所へ `frame_timer: Default::default(),` を足す。

- [ ] **Step 2: update の冒頭で計時を始める**

`fn update` の `let app = self.controller.app().clone();` の**直後**へ:

```rust
        let frame_started = Instant::now();
        let frame_interval = self.frame_timer.begin(frame_started);
```

- [ ] **Step 3: update の末尾で吐く**

`fn update` の**最終行**（`applied_background` を更新する末尾ブロックより後）へ:

```rust
        crate::trace::trace(
            "egui_frame",
            serde_json::json!({
                "update_us": frame_started.elapsed().as_micros() as u64,
                "interval_us": frame_interval.map(|d| d.as_micros() as u64),
            }),
        );
```

`interval_us` は初回フレームで `null` になる。**判定側はそれを読まない**（H6 が見るのは `update_us` だけ）。

- [ ] **Step 4: 実機で trace が出ることを確認する**

Run: `cargo build -p snotra`

Run（PowerShell）: `$env:SNOTRA_TRACE='1'; cargo run -p snotra 2>&1 | Select-String 'egui_frame' | Select-Object -First 5`
ホットキーで窓を出し、数文字打ってから終了。`update_us` を持つ行が並ぶことを確認する。**`update_us` が 0 ばかりなら計時の位置が間違っている**（末尾ブロックより前で吐いている）。

- [ ] **Step 5: コミット**

```
git add src-tauri/src/egui_shell/view.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(egui): #1004 フレームの所要と間隔を egui_frame で出す"
```

---

## Task 3: 打鍵→採り込みの計器（`SearchDispatch`）

**この型は PR 2 でも使う。** PR 1 では同期経路で「振って即座に採る」、PR 2 では worker の遅着を裁く。**同じ器が改修の前後で当たること**が #1000 の要求である。

**Files:**
- Create: `src-tauri/src/egui_shell/search_dispatch.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod search_dispatch;` と re-export）
- Modify: `src-tauri/src/egui_shell/launcher_controller.rs`（struct フィールド・`run_search_with` の `QueryIntent::Plain` 枝）
- Modify: `src-tauri/CLAUDE.md`（「モジュール構成」の `egui_shell/` リスト）

**Interfaces:**
- Produces:
  - `SearchDispatch::default()`
  - `fn issue(&mut self, key_at: Instant, now: Instant) -> u64`
  - `fn accept(&mut self, seq: u64, now: Instant) -> Option<Settled>`
  - `fn invalidate(&mut self)`
  - `fn pending_seq(&self) -> u64`
  - `struct Settled { pub seq: u64, pub since_key: Duration, pub since_dispatch: Duration }`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/egui_shell/search_dispatch.rs` を作り、テストだけ書く:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_latest_seq() {
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        let first = d.issue(base, base + Duration::from_millis(50));
        let second = d.issue(
            base + Duration::from_millis(60),
            base + Duration::from_millis(110),
        );
        assert!(second > first, "seq は単調に増える");
        assert!(
            d.accept(first, base + Duration::from_millis(120)).is_none(),
            "追い越された結果は採らない"
        );
        let settled = d
            .accept(second, base + Duration::from_millis(130))
            .expect("最新の seq は採る");
        assert_eq!(settled.since_key, Duration::from_millis(70), "打鍵起点");
        assert_eq!(
            settled.since_dispatch,
            Duration::from_millis(20),
            "dispatch 起点"
        );
    }

    #[test]
    fn accept_is_once_per_issue() {
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        let seq = d.issue(base, base);
        assert!(d.accept(seq, base).is_some());
        assert!(
            d.accept(seq, base).is_none(),
            "同じ結果を二度採らない（採り込みは行の差し替えと一対一）"
        );
    }

    #[test]
    fn invalidate_drops_in_flight() {
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        let seq = d.issue(base, base);
        d.invalidate();
        assert!(
            d.accept(seq, base).is_none(),
            "同期で行を差し替えたら in-flight は必ず古い（spec §4.5）"
        );
    }
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra accepts_only_the_latest_seq`
Expected: FAIL（`file not found for module` か `cannot find type SearchDispatch`）

- [ ] **Step 3: 最小実装**

同ファイルの先頭へ（テストより上）:

```rust
//! 検索 dispatch の同一性を測る純粋核（#1004）。
//!
//! **`SearchState::rows_generation` とは別の量である**——世代は「行が差し替わったか」、ここの seq は「どの要求か」を指す。#699 の世代は `set_results` が持ったままにする。
//!
//! **PR 1（同期）と PR 2（worker）で同じ型を使う**——計器が改修の前後で同じ区間を測ることが受け入れの条件である（#1000 の「同じ器を当てられること」）。

use std::time::{Duration, Instant};

/// 採り込みが成立したときの経過。**打鍵起点と dispatch 起点の両方を持つ**——打鍵起点には 50 ms の trailing debounce 待ちが必ず入るため、片方では worker 往復の費用を読めない。
pub struct Settled {
    pub seq: u64,
    pub since_key: Duration,
    pub since_dispatch: Duration,
}

struct Pending {
    seq: u64,
    key_at: Instant,
    dispatched_at: Instant,
}

#[derive(Default)]
pub struct SearchDispatch {
    next_seq: u64,
    pending: Option<Pending>,
}

impl SearchDispatch {
    /// 新しい要求へ seq を振る。前の要求は破棄される（最新クエリ勝ち）。
    pub fn issue(&mut self, key_at: Instant, now: Instant) -> u64 {
        self.next_seq += 1;
        self.pending = Some(Pending {
            seq: self.next_seq,
            key_at,
            dispatched_at: now,
        });
        self.next_seq
    }

    /// 結果が届いたときに呼ぶ。**現 pending と一致するときだけ `Some`** を返し、pending を消す。
    pub fn accept(&mut self, seq: u64, now: Instant) -> Option<Settled> {
        match &self.pending {
            Some(p) if p.seq == seq => {}
            _ => return None,
        }
        let pending = self.pending.take()?;
        Some(Settled {
            seq,
            since_key: now.duration_since(pending.key_at),
            since_dispatch: now.duration_since(pending.dispatched_at),
        })
    }

    /// in-flight を失効させる。**同期で `set_results` を呼ぶ出所は必ずここを通す**（spec §4.5）。
    pub fn invalidate(&mut self) {
        self.pending = None;
    }

    /// 現在 in-flight の seq（無ければ 0）。判定器が「失効した結果を採っていないか」を読む材料である。
    pub fn pending_seq(&self) -> u64 {
        self.pending.as_ref().map_or(0, |p| p.seq)
    }
}
```

- [ ] **Step 4: モジュール登録**

`mod.rs` の `mod` 宣言群へ `mod search_dispatch;`、re-export へ `pub(crate) use search_dispatch::SearchDispatch;` を足す（`Settled` は `launcher_controller` が `let` で受けるだけなので re-export 不要）。

- [ ] **Step 5: 通ることを確認する**

Run: `cargo test -p snotra search_dispatch`
Expected: 3 テストとも PASS

- [ ] **Step 6: 同期経路へ配線して `egui_search:settled` を出す**

`launcher_controller.rs` の `LauncherController` struct へ:

```rust
    /// 検索要求の同一性（#1004）。PR 1 では同期経路の計器、PR 2 では worker 結果の裁定に使う。
    dispatch: crate::egui_shell::SearchDispatch,
```

`new()` の初期化へ `dispatch: Default::default(),` を足す。`run_search_with` の `QueryIntent::Plain` 枝で、`engine.search` を呼ぶブロックと `set_results` を次で置き換える:

```rust
                        let search_started = std::time::Instant::now();
                        let (results, index_entries) = {
                            let state = match self.app_handle.try_state::<crate::AppState>() {
                                Some(s) => s,
                                None => return,
                            };
                            let mut engine = state.engine.lock().unwrap();
                            // 索引件数は H6 のゲート材料である（判定が意味を持つ規模かを判定器が知る）。**既に lock を握っている区間で取る**——このために lock を増やさない。
                            let n = engine.entry_count();
                            (engine.search(&query), n)
                        }; // lock 解放
                        let seq = self.dispatch.issue(self.last_input_at, search_started);
                        self.state.set_results(results);
                        if let Some(settled) = self.dispatch.accept(seq, Instant::now()) {
                            crate::trace::trace(
                                "egui_search:settled",
                                serde_json::json!({
                                    "dispatch_seq": settled.seq,
                                    "pending_seq": self.dispatch.pending_seq(),
                                    "index_entries": index_entries,
                                    "since_key_us": settled.since_key.as_micros() as u64,
                                    "since_dispatch_us": settled.since_dispatch.as_micros() as u64,
                                }),
                            );
                        }
```

**既存の `egui_search:dispatch` の trace はそのまま残す**（`Engine::search` 区間だけの内訳として役に立つ）。

**フィールド名は `dispatch_seq` である。`seq` にしてはならない**——trace の封筒が同名のプロセス連番を持つ。

- [ ] **Step 7: モジュール索引を更新して governance:check**

`src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` の括弧内ファイル列挙へ `search_dispatch.rs` を足し、ファイル別索引の箇条書きへ:

```
  - `search_dispatch.rs` — 検索 dispatch の同一性の純粋核（責務は `//!`）
```

Run: `npm run governance:check`
Expected: 全検査 passed

- [ ] **Step 8: doc を検算してコミット**

Run: `cargo doc --workspace --no-deps --document-private-items`

```
git add src-tauri/src/egui_shell/search_dispatch.rs src-tauri/src/egui_shell/mod.rs src-tauri/src/egui_shell/launcher_controller.rs src-tauri/CLAUDE.md
git commit -m "feat(egui): #1004 検索 dispatch の seq と egui_search:settled を出す"
```

---

## Task 4: smoke へパスクエリの打鍵を注入する

**マーカーを新設しない。** 区間は既存の trace で切れる——`egui_input:changed`（`launcher_controller.rs` が打鍵ごとに出す）から `egui_search:settled` までが「打鍵から結果が出るまで」であり、アイドルのフレームを含まない。判定器はこの 2 イベントで区間を作る（Task 5）。

**Files:**
- Modify: `scripts/smoke-egui.ps1`（既存の 1 文字クエリ注入の直後）

- [ ] **Step 1: 既存の注入の作法を読む**

Run: `grep -n "queryVk" -B 15 -A 15 scripts/smoke-egui.ps1`
Expected: 送出 → 観測 → 取りこぼし時に一度だけ再注入、という形が読める。**その形へ合わせる。**

- [ ] **Step 2: パスクエリを注入する**

既存の 1 文字クエリ注入の直後へ:

```powershell
    # --- パスクエリ打鍵（#1004）----------------------------------------------
    # `c:\` は has_path_sep が真になり incremental cache が無効化される＝全件走査の経路。
    # 打鍵から結果までのフレームが予算を超えないことを H6 が判定する（区間は
    # egui_input:changed → egui_search:settled で切れるのでマーカーは要らない）。
    #
    # **ガードは既存の作法に合わせる。** このスクリプトは「失敗が既に在るときはキーを
    # 注入しない」規律を持ち、1 文字クエリ注入も Escape 注入も同じ条件で守られている
    # ——窓が出ていない状態で打鍵すると、キーが他のアプリへ飛ぶためである。
    if ($failures.Count -eq 0 -and $resultsChecked) {
      # 先行の 1 文字クエリが入力欄に残っているので消してから打つ（既存ブロックの
      # Backspace と同じ作法。消さないとクエリが "zc:\" になる）。
      Send-SnotraKey -VirtualKey $VK_BACK
      Start-Sleep -Milliseconds 50
      Send-SnotraKey -VirtualKey $VK_BACK -Up
      Start-Sleep -Milliseconds 50
      Send-SnotraKey -VirtualKey 0x43            # c
      Send-SnotraKey -VirtualKey 0x43 -Up
      Start-Sleep -Milliseconds 120              # debounce(50ms) の trailing を跨がせる
      Send-SnotraKeyChord -VirtualKeys @(0x10, 0xBA)   # Shift + ; = :
      Start-Sleep -Milliseconds 120
      Send-SnotraKey -VirtualKey 0xDC            # \
      Send-SnotraKey -VirtualKey 0xDC -Up
      Start-Sleep -Milliseconds 300              # 全件走査 + 採り込みの完了を待つ

      # **打鍵が入ったことを観測する。** 固定 sleep だけで済ませると、3 キーのどれかが
      # 落ちても $failures が増えず沈黙する。とくに `\` が落ちると has_path_sep が偽のまま
      # incremental cache の経路に落ち、**全件走査を一度も叩かずに緑を返す**。既存 3 ブロック
      # （hotkey / 1 文字クエリ / Escape）が例外なく観測を持つのと同じ理由である。
      $pathTyped = Wait-SnotraTraceCondition -Path $errPath -TimeoutMs $ObserveTimeoutMs `
        -Description "パスクエリ 3 文字の入力" `
        -Predicate { $_.event -eq 'egui_input:changed' -and $_.data.after_chars -eq 3 }.GetNewClosure()
      if ($null -eq $pathTyped) {
        $failures += "path query 'c:\' not observed as 3 chars within ${ObserveTimeoutMs}ms"
      }
    }
```

**`Send-SnotraKey` の down / up の間には既存ブロックと同じく 50 ms を挟むこと**（既存の hotkey・1 文字クエリ・Escape はいずれもそうしている）。

**注入は show が落ち着いてから打つこと。** 2026-08-10 の実測（release・smoke）では、**show 直後の 5 フレームが 13〜23 ms、hide 直後の 1 フレームが 55 ms** かかっている（定常フレームは 64〜139 µs）。これらが打鍵区間へ混入すると H6 は worker 化の後も赤いままになる。既存の 1 文字クエリ注入の**後ろ**へ置くこの位置なら、初期化のフレームは既に過ぎている——**位置を前へ動かさないこと。**

**`Send-SnotraKeyChord` が Shift 付き単発に使えるか**は Step 1 で読んだ既存の使い方で確かめる。使えないなら `Send-SnotraKey -VirtualKey 0x10`（Shift down）→ `0xBA` down/up → `0x10 -Up` に展開する。

- [ ] **Step 3: 走らせて trace に区間が現れることを確認する**

Run: `npm run smoke:egui`
Expected: 成功し、trace に `egui_input:changed` → `egui_frame`（複数）→ `egui_search:settled` の並びが現れる

Run: 出力ログから `index_entries` の値を読む
**この値を控える**——Task 5 の閾値を決める材料であり、CI の索引規模がここで分かる。

- [ ] **Step 4: コミット**

```
git add scripts/smoke-egui.ps1
git commit -m "test(smoke): #1004 パスクエリを打鍵注入する"
```

---

## Task 5: 取り下げ — 不変条件 H6 は置かない

> **⚠️ このタスクは PR 1 の実測により取り下げた。以下の Step は実装しないこと。** 正本は `docs/superpowers/specs/2026-08-10-search-worker-design.md` の §3.3。理由は 2 つある:
>
> 1. **trace の書き込みが 1 本あたり約 10 ms かかり、フレーム時間の計器を汚染する。** 実測では、実質的な処理が無い区間（`issue` と `set_results` しか挟まない）でも trace 間で 12 ms 空いた。一方 `Engine::search` 自身は `egui_search:dispatch` の実測で 7〜162 µs しかない。**予算 16.7 ms は trace 2 本で超える**ので、絶対値での合否判定が成立しない
> 2. **smoke は常に 1 件の索引を seed し、実運用点では走らせられない**（`index_entries` の実測は 1。scan 対象を上書きする引数が無い）。索引件数のゲートを置けば H6 はどこでも永久に SKIP になり、#930 が戒めた「発火しえない検出器」になる
>
> **受け入れ 2 は Task 6 の実運用点 A/B 実測で示す。** smoke に残す不変条件は H7（Task 10）だけで、H7 は seq の大小だけを見るので索引規模にも trace I/O にも依存しない。
>
> **番号は詰めない**——Task 6 以降を ledger と brief が番号で参照しているため、この節は取り下げの記録として残す。**以下は取り下げた設計の記録である。**

**Files:**（実施しない）
- Modify: `scripts/lib/SnotraTraceInvariants.psm1`
- Modify: `scripts/lib/SnotraTraceInvariants.Tests.ps1`

**Interfaces:**
- Consumes: `egui_frame.data.update_us`（Task 2）・`egui_search:settled.data.index_entries`（Task 3）・`egui_input:changed`（既存）
- Produces: 不変条件 `H6`

- [ ] **Step 1: 既存テストの書き方を読む**

Run: `sed -n '1,60p' scripts/lib/SnotraTraceInvariants.Tests.ps1`
Expected: 合成イベントの作り方（`seq` / `event` / `data` を持つオブジェクトの形）と `$script:OneSection` が読める。**その形をそのまま使う。**

- [ ] **Step 2: 失敗するテストを書く**

**Step 1 で読んだ合成イベントの作り方に合わせて**、次の 3 例を書く（下は形の指針であり、ヘルパ名は既存に合わせる）:

```powershell
Describe 'H6 — 打鍵から結果までのフレーム予算' {
    It '打鍵区間に予算超過のフレームがあれば違反' {
        $events = @(
            (New-TraceEvent 1 'egui_input:changed' @{ scope = 'search' })
            (New-TraceEvent 2 'egui_frame' @{ update_us = 21445 })
            (New-TraceEvent 3 'egui_search:settled' @{ index_entries = 312377 })
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection
        $r.Overall['H6'] | Should -Be 'FAIL'
        @($r.Violations | Where-Object { $_.Invariant -eq 'H6' }).Count | Should -Be 1
    }

    It '予算内なら合格' {
        $events = @(
            (New-TraceEvent 1 'egui_input:changed' @{ scope = 'search' })
            (New-TraceEvent 2 'egui_frame' @{ update_us = 4200 })
            (New-TraceEvent 3 'egui_search:settled' @{ index_entries = 312377 })
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection
        $r.Overall['H6'] | Should -Be 'PASS'
    }

    It '索引が小さい環境では判定せず SKIP（沈黙で合格にしない）' {
        $events = @(
            (New-TraceEvent 1 'egui_input:changed' @{ scope = 'search' })
            (New-TraceEvent 2 'egui_frame' @{ update_us = 900 })
            (New-TraceEvent 3 'egui_search:settled' @{ index_entries = 1200 })
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection
        $r.Overall['H6'] | Should -Be 'SKIP'
        @($r.Unjudgeable | Where-Object { $_.Invariant -eq 'H6' }).Count | Should -BeGreaterThan 0
    }
}
```

`New-TraceEvent` に相当するヘルパが既存に無ければ、既存テストが使っている生成形をそのまま書き下す。

- [ ] **Step 3: 落ちることを確認する**

Run: `npm run test:powershell`
Expected: H6 の 3 テストが FAIL

- [ ] **Step 4: 実装**

モジュール冒頭のテーブルへ 1 行:

```
# | H6 | 打鍵から結果までの `egui_frame` の `update_us` がフレーム予算を超えたら異常 | 検索がフレームの中で走っている（#1004） |
```

`$script:Invariants` へ `'H6'` を足す（**この一覧への追加を忘れると集計と exit code から黙って落ちる**）。イベント名の定数を冒頭へ:

```powershell
$script:EventFrame = 'egui_frame'
$script:EventInputChanged = 'egui_input:changed'
$script:EventSearchSettled = 'egui_search:settled'

# H6 が意味を持つ最小の索引規模。**20 ms は 312,377 件で出た数字であり**、小さい索引では
# 走査が速く A 側でも緑になりうる（= 直ったのではなく測れていない・#930 の型）。
# 実測値の正本は PERFORMANCE.md。
$script:H6MinIndexEntries = 100000

# フレーム予算。リフレッシュレートを trace から知る術が無いので 60Hz を下限として使う
# ——**高リフレッシュレート機では緩い判定になる**（受容する残余。緩い側へ倒すのは
# 誤検出で smoke を赤くしないため）。
#
# **この閾値は実測で裏付けてある**（2026-08-10・release・smoke）: 定常フレームは
# 64〜139 us（0.1 ms 未満）で、検索は 20 ms。正常時は 2 桁下・異常時は 1 桁上に
# 分離するので、16.7 ms はどちらからも遠い。**ただし show 直後の 5 フレームは
# 13〜23 ms・hide 直後の 1 フレームは 55 ms あり、打鍵区間へ混入すれば誤爆する**
# ——区間を egui_input:changed から egui_search:settled に限るのはそのためである。
$script:H6FrameBudgetUs = 16700
```

判定本体（H1 / H4 / H5 の分岐と同じ 1 パスの中）へ:

```powershell
                # --- H6 ---
                # **測るのは所要時間であって間隔ではない**——このランタイムはイベント駆動で、
                # 健全でも間隔は debounce 幅（50ms）・打鍵間隔（100〜200ms）まで開く。
                # 間隔で判定すると worker 化後も永久に赤いままになる（spec の §3.3）。
                if ($name -eq $script:EventInputChanged) {
                    $inKeystroke = $true
                    $keystrokeFrames = @()
                } elseif ($name -eq $script:EventFrame -and $inKeystroke) {
                    $us = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $event.Raw.data -Name 'update_us')
                    if ($null -ne $us) { $keystrokeFrames += $us }
                } elseif ($name -eq $script:EventSearchSettled) {
                    $inKeystroke = $false
                    $entries = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $event.Raw.data -Name 'index_entries')
                    if ($null -eq $entries) {
                        $unjudgeable += @{ Invariant = 'H6'; Seq = $event.Seq; SectionId = $sectionId; Reason = 'index_entries が読めない' }
                    } elseif ($entries -lt $script:H6MinIndexEntries) {
                        $unjudgeable += @{ Invariant = 'H6'; Seq = $event.Seq; SectionId = $sectionId; Reason = "索引 $entries 件は判定閾値 $($script:H6MinIndexEntries) 件に満たない" }
                    } else {
                        $over = @($keystrokeFrames | Where-Object { $_ -gt $script:H6FrameBudgetUs })
                        if ($over.Count -gt 0) {
                            $violations += [pscustomobject]@{
                                Invariant = 'H6'
                                SectionId = $sectionId
                                Detail    = "打鍵区間に予算超過が $($over.Count) 枚（最大 $(($over | Measure-Object -Maximum).Maximum) us > $($script:H6FrameBudgetUs) us）"
                            }
                        } else {
                            Add-SnotraTracePass -PassCount $passCount -Invariant 'H6' -SectionId $sectionId
                        }
                    }
                    $keystrokeFrames = @()
                }
```

`$inKeystroke = $false` と `$keystrokeFrames = @()` を、状態機械の変数を初期化している場所（`$mainState` / `$resultsShown` の隣）へ足す。

**`$event.Raw.data` の読み方は Step 1 で読んだ既存の payload 読み出しに合わせること**——判定器が `Raw` を保持する形と、`data` がネストしている事実（trace 行は `{seq, ts_ms, event, data}`）の両方に依る。

- [ ] **Step 5: 通ることを確認する**

Run: `npm run test:powershell`
Expected: H6 の 3 テストと既存テストが全 PASS（**`$script:Invariants` への追加を忘れるとソース走査テストが落ちる**）

- [ ] **Step 6: 故障注入で発火を実測する**

**稼働中の判定器を弱めない——複製に変異を当てる。**

```powershell
$tmp = Join-Path $env:TEMP "h6-probe"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
Copy-Item scripts/lib/SnotraTraceInvariants.psm1 $tmp/
# 複製側の閾値だけを 0 にする（実運用点でない環境でも判定させる）
(Get-Content $tmp/SnotraTraceInvariants.psm1) -replace 'H6MinIndexEntries = 100000', 'H6MinIndexEntries = 0' |
    Set-Content $tmp/SnotraTraceInvariants.psm1
Import-Module $tmp/SnotraTraceInvariants.psm1 -Force
# Task 4 で採った実 trace を食わせる
```

**予算超過のフレームが実在する trace で違反が出ること**を確認する。**変異が本来の回帰と同じ姿か**も確かめる——ここでの回帰の姿は「打鍵中に 16.7 ms を超えるフレームが出る」であり、A 側の実 trace にそのまま在る（閾値を下げたのは判定を**走らせる**ためで、違反の条件を弱めてはいない）。

確認後、`Remove-Item -Recurse -Force $tmp` で複製を消す。

- [ ] **Step 7: コミット**

```
git add scripts/lib/SnotraTraceInvariants.psm1 scripts/lib/SnotraTraceInvariants.Tests.ps1
git commit -m "test(smoke): #1004 H6（打鍵区間の予算超過フレーム）を足す"
```

---

## Task 6: A 側ベースラインの採取と記録

**このタスクは実運用点でしか成立しない。** 索引は `[[paths.scan]] path = 'C:\'`（312k 規模）。

**Files:**
- Modify: `PERFORMANCE.md`

- [ ] **Step 1: release ビルドで 3 標本採る**

Run: `cargo build -p snotra --release`
Run（3 回）: `$env:SNOTRA_TRACE='1'; ./target/release/snotra.exe 2>&1 | Tee-Object -FilePath "$env:TEMP/snotra-a-1.log"`（2 回目・3 回目はファイル名を変える）
各回、ホットキー → `c:\` を打鍵 → Escape → 終了。

- [ ] **Step 2: 打鍵区間の `update_us` を集計する**

各ログの `egui_frame` から、`egui_input:changed` と `egui_search:settled` に挟まれた区間の `update_us` の p50 / max を出す。

- [ ] **Step 3: 検索がフレームを占有していることを内訳で確かめる**

同じログから、打鍵直後のフレームについて次の 2 つを並べる:

- そのフレームの `update_us`
- 同じ打鍵の `egui_search:settled` の `since_dispatch_us` と、`egui_search:dispatch` の `elapsed_us`

**実運用点では `elapsed_us` が 20 ms 前後になるはずである**（`PERFORMANCE.md`「パスクエリのフレームコスト」の p50 と整合する）。**そこが µs 桁なら索引が実運用点でない**——`[[paths.scan]]` を確認する。

**trace の書き込みが 1 本あたり約 10 ms 乗ることを念頭に読むこと**（Task 5 の取り下げ理由）。`update_us` の絶対値は汚染されているが、**A/B の両側へ等しく乗るので差分は読める**。実運用点では検索の 20 ms が汚染を上回る。

- [ ] **Step 4: PERFORMANCE.md へ記録する**

「パスクエリのフレームコスト」の直後へ、日付・release・標本数・p50 / max を既存の表と同じ粒度で書く。**最小値へ畳まない。**

書くべきこと:
- 「A 側」であることと、B 側は PR 2 で**同じ器・同日・同条件**から採ること
- **`update_us` には trace の書き込み（1 本あたり約 10 ms）が含まれる**こと。読む人が絶対値を実挙動と誤読しないための但し書きである
- 比較に使う列は `update_us`（A/B 差分）と `elapsed_us`（検索そのもの）の 2 つであること

- [ ] **Step 5: コミットして PR を作る**

```
git add PERFORMANCE.md
git commit -m "docs: #1004 打鍵中のフレーム所要の A 側ベースライン"
git push -u origin HEAD
```

PR 本文へ: **「H6 は取り下げた（理由は spec の §3.3）。受け入れ 2 は PR 2 で B 側を採って A/B で示す」**と明記する。

---

# PR 2 — 単一 worker + seq 照合

**ブランチ:** PR 1 のマージ後に `git checkout main && git pull && git checkout -b perf/search-worker-async`

## Task 7: worker の骨格

> **⚠️ Task 7 と Task 8 は 1 つの作業へ統合した（2026-08-11）。** 分けたのは計画の誤りである——`-D warnings` 下で `search_worker.rs` の 4 項目（`SearchRequest` / `SearchMsg` / `coalesce` / `spawn_search_worker`）がすべて `dead_code` で落ちる。**素の bin ビルドでは `#[cfg(test)] mod tests` ごとコンパイルから除外される**ので、テストが `coalesce` を呼んでいても救えない（`--all-targets` は両方をビルドし、素の方で落ちる）。re-export を足しても「未使用 import」が増えるだけで根本は消えない（実測）。
>
> **これは Global Constraints が既に禁じていた形である**——「新しい型・関数の導入と呼び出し点の移行は同じタスクに束ねる」。**`#[allow(dead_code)]` で逃げてはならない**: 一時的な黙らせは、Task 8 で呼び出しが入ったあとも残り、将来その API が本当に呼ばれなくなったときの検出を永久に潰す。
>
> **実施の形**: Task 7 と Task 8 の Step を通しで行い、**1 コミットにまとめる**。成否の指標は `cargo clippy --workspace --all-targets -- -D warnings` が通ることである。

**Files:**
- Create: `src-tauri/src/egui_shell/search_worker.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`
- Modify: `src-tauri/CLAUDE.md`（モジュール索引）

**Interfaces:**
- Produces:
  - `struct SearchRequest { pub seq: u64, pub query: String }`
  - `enum SearchMsg { Done { seq: u64, results: Vec<SearchResult>, index_entries: usize } }`
  - `fn spawn_search_worker(app: tauri::AppHandle) -> (Sender<SearchRequest>, Receiver<SearchMsg>)`
  - `fn coalesce(first: SearchRequest, rest: impl Iterator<Item = SearchRequest>) -> SearchRequest`

- [ ] **Step 1: coalescing の失敗するテストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn req(seq: u64, q: &str) -> SearchRequest {
        SearchRequest {
            seq,
            query: q.to_string(),
        }
    }

    #[test]
    fn coalesce_keeps_only_the_last_request() {
        let picked = coalesce(req(1, "c"), vec![req(2, "c:"), req(3, "c:\\")].into_iter());
        assert_eq!(picked.seq, 3, "溜まった要求は最後だけ走らせる");
        assert_eq!(picked.query, "c:\\");
    }

    #[test]
    fn coalesce_of_single_request_is_itself() {
        let picked = coalesce(req(7, "abc"), std::iter::empty());
        assert_eq!(picked.seq, 7);
    }
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra coalesce_keeps_only`
Expected: FAIL

- [ ] **Step 3: 実装**

```rust
//! 検索を実行する worker（#1004）。**プロセス寿命の 1 本**であり、要求は最新だけを走らせる。
//!
//! **都度 spawn を採らない理由**は `spawn_folder_load` の doc と対である——あちらの per-nav spawn は dead UNC の hang を隔離するための選択で、`engine.search` には転移しない（hang しない代わりに必ず共有 Mutex を要求する）。打鍵ごとに spawn すると、捨てるとわかっている結果のために lock と CPU を払う。
//!
//! **`egui::Context` を持たない**——長寿命 worker が Context clone を握ると `RepaintScheduler` の Arc が窓の `Destroyed` を越えて生き、停止を妨げる（#671 PR D）。起床は `wake_main` を使う。

use std::sync::mpsc::{Receiver, Sender, channel};

use snotra_core::ui_types::SearchResult;
use tauri::Manager;

pub struct SearchRequest {
    pub seq: u64,
    pub query: String,
}

pub enum SearchMsg {
    Done {
        seq: u64,
        results: Vec<SearchResult>,
        /// H6 のゲート材料である。**engine lock を握っている区間で取る**（lock を増やさない）。
        index_entries: usize,
    },
}

/// 溜まった要求から最後の 1 つを選ぶ（最新クエリ勝ち）。
pub fn coalesce(first: SearchRequest, rest: impl Iterator<Item = SearchRequest>) -> SearchRequest {
    rest.fold(first, |_, next| next)
}

/// worker を 1 本立てる。`Sender` が drop されると `recv` が Err を返しループが終わる（join はしない・best-effort）。
pub fn spawn_search_worker(app: tauri::AppHandle) -> (Sender<SearchRequest>, Receiver<SearchMsg>) {
    let (req_tx, req_rx) = channel::<SearchRequest>();
    let (msg_tx, msg_rx) = channel::<SearchMsg>();
    std::thread::spawn(move || {
        while let Ok(first) = req_rx.recv() {
            // recv で 1 つ取った後、溜まっている分を吸って最後だけ採用する。
            let picked = coalesce(first, req_rx.try_iter());
            let Some(state) = app.try_state::<crate::AppState>() else {
                return;
            };
            let (results, index_entries) = {
                let mut engine = state.engine.lock().unwrap();
                let n = engine.entry_count();
                (engine.search(&picked.query), n)
            }; // lock 解放
            if msg_tx
                .send(SearchMsg::Done {
                    seq: picked.seq,
                    results,
                    index_entries,
                })
                .is_err()
            {
                return; // 受け手が消えた（プロセス終了）
            }
            crate::egui_shell::wake_main(&app);
        }
    });
    (req_tx, msg_rx)
}
```

**`wake_main` が `mod.rs` から re-export されていなければ**（`grep -n "wake_main" src-tauri/src/egui_shell/mod.rs` で確認）、`window_coordinator::wake_main` を直接呼ぶか re-export を足す。

- [ ] **Step 4: モジュール登録と索引更新**

`mod.rs` へ `mod search_worker;` と re-export。`src-tauri/CLAUDE.md`「モジュール構成」へ:

```
  - `search_worker.rs` — 検索を実行する単一 worker（責務は `//!`）
```

- [ ] **Step 5: 通ることを確認する**

Run: `cargo test -p snotra coalesce`
Expected: PASS

Run: `npm run governance:check`
Expected: 全検査 passed

- [ ] **Step 6: doc 検算とコミット**

Run: `cargo doc --workspace --no-deps --document-private-items`

```
git add src-tauri/src/egui_shell/search_worker.rs src-tauri/src/egui_shell/mod.rs src-tauri/CLAUDE.md
git commit -m "feat(egui): #1004 検索 worker と最新クエリ勝ちの coalescing"
```

---

## Task 8: Plain 枝を worker へ移し、結果を drain する

**Files:**
- Modify: `src-tauri/src/egui_shell/launcher_controller.rs`
- Modify: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Consumes: `spawn_search_worker` / `SearchRequest` / `SearchMsg`（Task 7）・`SearchDispatch`（Task 3）
- Produces: `LauncherController::drain_search(&mut self)`

- [ ] **Step 1: struct とコンストラクタ**

```rust
    search_tx: Sender<crate::egui_shell::SearchRequest>,
    search_rx: Receiver<crate::egui_shell::SearchMsg>,
```

`new()` の冒頭で `let (search_tx, search_rx) = crate::egui_shell::spawn_search_worker(app_handle.clone());` を作り `Self { ... }` へ足す。

- [ ] **Step 2: Plain 枝を発行だけにする**

Task 3 Step 6 で書いた同期ブロック全体を次で置き換える:

```rust
                    QueryIntent::Plain => {
                        if self.state.query().trim().is_empty() || self.indexing() {
                            // 空クエリと構築中は**同期でクリアする**（worker を経由させると消した文字が 1 フレーム残る）。同期で差し替える以上、in-flight は失効させる（spec の §4.5）。
                            self.dispatch.invalidate();
                            self.state.set_results(Vec::new());
                            return;
                        }
                        let query = self.state.query().to_string();
                        let seq = self.dispatch.issue(self.last_input_at, Instant::now());
                        // 送信失敗（worker が死んでいる）は無視する——次の打鍵で再送され、表示は前の行を保ったままになる。
                        let _ = self
                            .search_tx
                            .send(crate::egui_shell::SearchRequest { seq, query });
                        // **結果が届くまで前の行を保つ**（folder cache 未着枝と同じ扱い）。
                    }
```

**`egui_search:dispatch` の trace はこの枝から消える**（`Engine::search` をここで呼ばなくなるため）。worker 側へは移さない——`egui_search:settled` の `since_dispatch_us` が同じ区間をより正確に語る。

- [ ] **Step 3: drain を書く**

```rust
    /// worker の結果を採り込む（#1004）。**seq が現 pending と一致するときだけ行を差し替える**——追い越された結果は捨てる。世代は `set_results` が進める（#699 は無傷）。
    pub(super) fn drain_search(&mut self) {
        while let Ok(crate::egui_shell::SearchMsg::Done {
            seq,
            results,
            index_entries,
        }) = self.search_rx.try_recv()
        {
            let now = Instant::now();
            let Some(settled) = self.dispatch.accept(seq, now) else {
                crate::trace::trace(
                    "egui_search:dropped",
                    serde_json::json!({
                        "dispatch_seq": seq,
                        "pending_seq": self.dispatch.pending_seq(),
                    }),
                );
                continue;
            };
            self.state.set_results(results);
            crate::trace::trace(
                "egui_search:settled",
                serde_json::json!({
                    "dispatch_seq": settled.seq,
                    "pending_seq": self.dispatch.pending_seq(),
                    "index_entries": index_entries,
                    "since_key_us": settled.since_key.as_micros() as u64,
                    "since_dispatch_us": settled.since_dispatch.as_micros() as u64,
                }),
            );
        }
    }
```

- [ ] **Step 4: view から呼ぶ**

`view.rs` の `fn update` 内、`self.controller.poll_search_debounce(&ctx);` の**直前**へ:

```rust
        self.controller.drain_search();
```

**行の差し替えはクリック消費より前でなければならない**（#699）。`poll_search_debounce` より前に置くのは、同じフレームで trailing 発火が新しい要求を出す前に、届いた結果を採るためである。

- [ ] **Step 5: ビルドと実機確認**

Run: `cargo build -p snotra`
Run: `$env:SNOTRA_TRACE='1'; cargo run -p snotra` → `c:\` を打鍵

Expected: 結果が出る。`egui_search:settled` が `since_dispatch_us` 付きで現れる。**`egui_frame` の `update_us` が打鍵中も予算内に収まる。**

- [ ] **Step 6: コミット**

```
git add src-tauri/src/egui_shell/launcher_controller.rs src-tauri/src/egui_shell/view.rs
git commit -m "feat(egui): #1004 Plain 検索を worker へ出し seq 照合で採り込む"
```

---

## Task 9: 同期で行を差し替える出所すべてを失効させる

**この規則を列挙で守らない。** 「同期で `set_results` を呼ぶ出所は、同じ場所で `invalidate()` を呼ぶ」（spec の §4.5）。

**Files:**
- Modify: `src-tauri/src/egui_shell/launcher_controller.rs`

- [ ] **Step 1: 出所を数え上げる**

Run: `grep -n "set_results" src-tauri/src/egui_shell/launcher_controller.rs`

**grep が返した全件を「同期で差し替える／そうでない」へ分類すること。ここに列挙を書かない**——2026-08-11 に実際に起きたとおり、**列挙を書けばそれが上限として読まれ、漏れが漏れのまま通る**（当初この行は 6 件を数えていたが、実際の grep は 9 件返し、`start_launch`（launching 開始時のクリア）と `clear_search`（クエリを空にする単一チョークポイント）が落ちていた。前者は「launching 中は results 窓が hide される」という doc 記載の不変条件への違反経路である）。

**列挙で守ると腐るからこのタスクを不変条件の形にしたのであり、その手順に列挙を書いては本末転倒である。** 判断の基準は 1 つ:「その `set_results` は worker の結果を採り込むものか、それとも同期で行を差し替えるものか」。前者（`drain_search` の中）だけが対象外で、**それ以外はすべて対象である**。`SearchState::reset()` を通る `consume_reset_pending` も含む。

- [ ] **Step 2: 規則を縛るテストを書く**

`search_dispatch.rs` の `mod tests` へ:

```rust
#[test]
fn stale_result_is_dropped_after_synchronous_replacement() {
    let base = Instant::now();
    let mut d = SearchDispatch::default();
    // `c:\u` を打って worker が走り出した
    let in_flight = d.issue(base, base);
    // クエリを空にした → 同期でクリアした出所が invalidate を呼ぶ
    d.invalidate();
    // worker の結果が遅れて届く
    assert!(
        d.accept(in_flight, base + Duration::from_millis(20))
            .is_none(),
        "空クエリの下に古い行が生え直してはならない"
    );
}
```

Run: `cargo test -p snotra stale_result_is_dropped`
Expected: **PASS する**（Task 3 の `invalidate` が既に正しいため）。**このテストは型の契約を確かめるものであり、規則の本体は次の Step である**——呼び出し側が `invalidate()` を呼ばなければこのテストは緑のまま事故が起きる。**H7（Task 10）がその残りを smoke で捕まえる。**

- [ ] **Step 3: 各出所へ `invalidate()` を置く**

Step 1 で数えた各 `set_results` の直前へ `self.dispatch.invalidate();`。`consume_reset_pending` の `self.state.reset();` の隣へも:

```rust
            self.dispatch.invalidate(); // hide を跨いだ in-flight は show 後の行を汚さない
```

- [ ] **Step 4: flush-on-Enter の同期を残す**

`on_enter` の flush 経路を、`run_search_with` を呼ばず**その場で同期実行する**形へ変える（spec の §4.7）:

```rust
        if crate::egui_shell::should_flush_on_enter(
            self.state.view_kind(),
            is_plain,
            self.search_debounce.is_armed(),
        ) {
            self.search_debounce.cancel();
            // #1004: Enter は最終クエリの結果をその場で要求するため、worker の往復を待てない（待つ設計は Enter 二度押し・Escape・hide の in-flight を全部抱える）。
            // Enter は 1 回きりで、ユーザーは結果を待っている——ここの同期は正当である。
            let query = self.state.query().to_string();
            let searched = if query.trim().is_empty() || self.indexing() {
                None
            } else {
                self.app_handle.try_state::<crate::AppState>().map(|state| {
                    let mut engine = state.engine.lock().unwrap();
                    engine.search(&query)
                })
            };
            // **どちらの枝でも同期で行を差し替える**——空クエリ・indexing 中にクリアを落とすと、古い行が残ったまま直後の `activate_or_execute` がそれを起動する（`run_search_with` の Plain 早期 return が旧実装で担っていた処置である）。
            self.dispatch.invalidate();
            self.state.set_results(searched.unwrap_or_default());
        }
```

**分岐を `Option` へ畳むのは意図的である。** 当初の計画は `if !query.trim().is_empty() && !self.indexing() { … }` の中だけで `invalidate` + `set_results` を呼ぶ形だったが、**ガードが偽のとき何もしない**という欠陥を持っていた（2026-08-11 のレビューで発見）。旧実装では `on_enter` が `run_search_with` を経由し、その Plain 早期 return が空クエリ・indexing 中を同期クリアしていた——**書き換えでその処置が落ちた**。症状は「クエリを空にして 50 ms 以内に Enter すると、古い行が残ったままそれが起動される」である。`Option` へ畳めば「どちらの枝でも `invalidate` + `set_results` を通る」ことが構造で保証され、**片方だけ直す将来の変更**に耐える。

- [ ] **Step 5: 検証**

Run: `cargo test -p snotra`
Expected: 全 PASS

Run: `cargo run -p snotra` → `c:\` を素早く打って**打ち終わりの 50 ms 以内に Enter**
Expected: 最終クエリの結果が起動される（leading 時点の古い結果ではない）

- [ ] **Step 6: コミット**

```
git add src-tauri/src/egui_shell/launcher_controller.rs src-tauri/src/egui_shell/search_dispatch.rs
git commit -m "fix(egui): #1004 同期で行を差し替える出所は in-flight を失効させる"
```

---

## Task 10: 不変条件 H7 — 失効した結果の採り込み

**Files:**
- Modify: `scripts/lib/SnotraTraceInvariants.psm1`
- Modify: `scripts/lib/SnotraTraceInvariants.Tests.ps1`

- [ ] **Step 1: 失敗するテストを書く**

Task 5 Step 1 で読んだ合成イベントの形に合わせて:

```powershell
Describe 'H7 — 失効した検索結果の採り込み' {
    It 'dispatch_seq が pending より小さい settled は違反' {
        $events = @(
            (New-TraceEvent 1 'egui_search:settled' @{ dispatch_seq = 3; pending_seq = 5; index_entries = 312377 })
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection
        $r.Overall['H7'] | Should -Be 'FAIL'
    }

    It '最新の結果を採るのは正常（pending_seq = 0 は pending 無し）' {
        $events = @(
            (New-TraceEvent 1 'egui_search:settled' @{ dispatch_seq = 5; pending_seq = 0; index_entries = 312377 })
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection
        $r.Overall['H7'] | Should -Be 'PASS'
    }
}
```

**空列での SKIP を測るテストは書かない**——判定器の既定が SKIP であることは既存の `Get-SnotraTraceInvariantNames` の Describe が全不変条件について既に測っている。

- [ ] **Step 2: 落ちることを確認する**

Run: `npm run test:powershell`
Expected: H7 の 2 テストが FAIL

- [ ] **Step 3: 実装**

冒頭テーブルへ:

```
# | H7 | `egui_search:settled` が `dispatch_seq < pending_seq` で現れたら異常 | 失効した検索結果が行を汚す（#1004） |
```

`$script:Invariants` へ `'H7'`。H6 の `EventSearchSettled` 分岐（Task 5）の中へ続けて:

```powershell
                    # --- H7 ---
                    # 採り込み時点の pending より古い seq が採られたら、失効の規則が破れている。
                    # `pending_seq = 0` は「pending 無し」＝この結果が最新だったことを意味する。
                    # **`data` も `Get-SnotraTraceProperty` の 2 段経由で読む。直接ドット参照しない。**
                    $data = Get-SnotraTraceProperty -InputObject $event.Raw -Name 'data'
                    $dispatchSeq = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $data -Name 'dispatch_seq')
                    $pendingSeq = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $data -Name 'pending_seq')
                    if ($null -eq $dispatchSeq -or $null -eq $pendingSeq) {
                        $unjudgeable += @{ Invariant = 'H7'; Seq = $event.Seq; SectionId = $sectionId; Reason = 'dispatch_seq / pending_seq が読めない' }
                    } elseif ($pendingSeq -ne 0 -and $dispatchSeq -lt $pendingSeq) {
                        $violations += @{
                            Invariant = 'H7'
                            Seq       = $event.Seq
                            SectionId = $sectionId
                            Message   = "失効した結果を採った: dispatch_seq=$dispatchSeq < pending=$pendingSeq"
                        }
                    } else {
                        Add-SnotraTracePass -PassCount $passCount -Invariant 'H7' -SectionId $sectionId
                    }
```

**形は既存の H1 / H4 / H5 に合わせること**（`hashtable` + `Message` / `Reason`、`Seq` を持つ）。2026-08-11 の実装で当初 `[pscustomobject]` + `Detail` + `$event.Raw.data` の直接ドット参照を指定していたが、**実測で否定された**: StrictMode 下では `data` を持たないイベント 1 件で例外が飛び、**判定器全体が `JudgeFailed=true` になって H1/H4/H5 まで道連れで SKIP する**。モジュール冒頭の doc が「trace 行のスキーマがドリフトしても判定器が落ちないように、プロパティの読みは必ずここを通す」と書いているのはこのためであり、**「判定不能を PASS へ化けさせない」という要石を最悪の形で破る**。

**H7 は索引規模のゲートを持たない**——失効判定の破れは索引の大きさと無関係に現れる。

- [ ] **Step 4: 通ることを確認する**

Run: `npm run test:powershell`
Expected: 全 PASS

- [ ] **Step 5: 故障注入で発火を実測する**

**複製に変異を当てる**——ここでは Rust 側を一時的に壊す:

`drain_search` の `let Some(settled) = self.dispatch.accept(seq, now) else { ... continue; };` を、常に採り込む形へ書き換える。smoke を回して H7 が違反を出すことを確認したら、**必ず戻す**:

```
git checkout -- src-tauri/src/egui_shell/launcher_controller.rs
```

**この変異は本来の回帰と同じ姿である**——「失効判定を忘れた実装」が spec の §4.5 が防ごうとしている姿そのものである。

- [ ] **Step 6: コミット**

```
git add scripts/lib/SnotraTraceInvariants.psm1 scripts/lib/SnotraTraceInvariants.Tests.ps1
git commit -m "test(smoke): #1004 H7（失効した検索結果の採り込み）を足す"
```

---

## Task 11: B 側を測って A/B で示す

**Files:**
- Modify: `PERFORMANCE.md`

- [ ] **Step 1: smoke で H7 が PASS することを確認する**

Run: `npm run smoke:egui`（**smoke 既定のプロファイルでよい**——H7 は seq の大小だけを見るので索引規模に依存しない）
Expected: **H7 が PASS。** SKIP なら `egui_search:settled` が 1 件も出ていない——`drain_search` の呼び出し位置（Task 8 Step 4）を疑う。**H6 は存在しない**（Task 5 で取り下げた）。

- [ ] **Step 2: 実運用点で B 側を 3 標本採る**

PR 1 Task 6 Step 1〜3 と**同じ手順・同日・同条件**で採る。**日をまたいで A / B を比べない**（`PERFORMANCE.md`「warm frame は日をまたいで比較しない」）。

- [ ] **Step 3: PERFORMANCE.md へ B 側を書き、A/B で示す**

A 側の隣へ同じ粒度で。**比べる列は 2 つである**:

1. **打鍵直後のフレームの `update_us`** ——ここから検索の 20 ms が消えたことが本題である。**trace の書き込み（1 本あたり約 10 ms）は A/B 両側へ等しく乗るので、差分として読む**
2. **`egui_search:dispatch` の `elapsed_us`** ——A 側では Plain 枝から出る。**B 側ではこの trace は Plain 枝から消える**（worker へ移ったため）ので、B 側の対応値は「フレームの外で走った」ことの記録として `since_dispatch_us` を併記する

**`since_key_us` は増える**（worker 往復が乗る）——それは設計どおりであり退行ではない。**そう明記する。**

- [ ] **Step 4: コミット**

```
git add PERFORMANCE.md
git commit -m "docs: #1004 worker 化後の B 側計測"
```

---

## Task 12: 仕上げ — race-check と SPEC 同期

- [ ] **Step 1: `/race-check` を走らせる**

`AGENTS.md`「条件別チェック」の worker spawn・channel・フレーム drain の行に逐語で該当する。**指摘へ fix-forward を当てたら、同じ枠組みを修正差分にも再実行してから閉じる。**

- [ ] **Step 2: SPEC.md の同期要否を判定する**

Run: `grep -n "検索" SPEC.md | head -40`

**判定は 2 つの参照で決まる**（`AGENTS.md`「開発ワークフロー」）: 当該挙動の記述があるか、それに**合わせる**のか**変える**のか。検索の**結果**は変わらず、変わるのは反映のタイミングである。「同期で反映する」と読める記述があれば仕様変更として同期する。**無ければ「変更なし」と判断した根拠（読んだ節）を PR 本文へ書く。**

- [ ] **Step 3: `cargo doc` と governance:check**

Run: `cargo doc --workspace --no-deps --document-private-items`
Run: `npm run governance:check`

- [ ] **Step 4: PR を作る**

```
git push -u origin HEAD
```

PR 本文へ:
- A 側 / B 側の数値と、`since_key_us` が増えるのは設計どおりである旨
- **受容する残余 3 件**（lock 競合は残る・走り出した走査は止まらない・木の索引化は別 issue）を spec から引く
- `Closes #1004`

---

## 自己レビュー（この計画を書いた後の検算）

- **spec カバレッジ**: §2 → 2 PR 構成。§3.2 → Task 2・3。§3.3 → Task 5 Step 4 の判定形（`update_us` を見る）。§3.4 → Task 5 の件数ゲートと Task 6 Step 4。§4.1 → Task 7。§4.2 → Task 7 の `wake_main` と `//!`。§4.3 → Task 3（`rows_generation` に触らない）。§4.4 → Task 8 Step 2。§4.5 → Task 9・Task 10。§4.6 → debounce に触るタスクを置かないことが実装である。§4.7 → Task 9 Step 4。§4.8 → Task 7 Step 3 の `is_err()` 復帰。§5 → Task 12 Step 4。§6 → Task 12 Step 1〜2
- **型の一貫性**: `SearchDispatch` の `issue` / `accept` / `invalidate` / `pending_seq` は Task 3 定義・Task 8/9 使用。`SearchRequest` / `SearchMsg`（`index_entries` を含む）は Task 7 定義・Task 8 使用。`FrameTimer::begin` は Task 1 定義・Task 2 使用
- **trace の payload 名**: `dispatch_seq` / `pending_seq` / `index_entries` / `since_key_us` / `since_dispatch_us` / `update_us` / `interval_us`。**封筒の `seq` と衝突しない**ことを Task 3・7・8 と Task 5・10 の判定側で一致させた
- **PR 1 で載せた `index_entries` を PR 2 の worker も載せる**——H6 のゲートが PR をまたいで同じ材料を読む

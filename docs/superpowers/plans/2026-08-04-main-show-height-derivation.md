# show 経路が「これから描く高さ」を導出する 実装計画（#755 / #801）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** show 経路が窓を畳む先を「バー高固定」から「そのフレームで実際に描かれる高さ」へ変え、#755（再表示後に伸びず案内が切り取られる）と #801（show で高さが縮んでから伸びる）を同時に消す。

**Architecture:** status 行の有無を返す述語を `notify.rs`（`overlay_kind` の隣）に置き、毎フレーム側（`view.rs`）と show 側（`window_coordinator.rs`）の両方がそれを呼ぶ。show は reset-on-show 後の状態を 3 つのリテラル引数で渡す。あわせて reset-on-show で main のサイズ memo も初期値へ戻し、導出がずれても固着せず 1 フレームで直る形（fail-safe）にする。

**Tech Stack:** Rust（Tauri v2 / egui / softbuffer）・PowerShell（smoke）

設計の正本は `docs/superpowers/specs/2026-08-04-main-show-height-derivation-design.md`。**却下した案とその根拠はそちらにある**——この計画はそれを前提に手順だけを書く。

## Global Constraints

- **決定 2（show の実高導出）と決定 3（memo リセット）を別コミットに割らない。** 決定 3 単独では #801 を「毎回」へ悪化させる。Task 4 が両方を 1 コミットで入れる
- **`main_window_height`（`layout.rs`）を変更しない。** 既存の純粋核テストに手を入れない
- **`layout.rs` へ新しいモジュール間依存を作らない。** 同ファイルは `use std::time::Duration` 以外の依存を持たない
- **show の操作順序は不変**——高さを決める → 位置を決める → show。畳む先の値だけが変わる
- `SPEC.md` を書くときは**実装シンボル名を書かない観測文**にする（#902 の降格後の文体）
- コミットは feature ブランチ `fix/755-801-show-height-derivation` 上で行う（`main` へ直接コミットしない）

## File Structure

| ファイル | 責務 | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/notify.rs` | 通知 primitive の純粋核。status 行の優先ラダー（`overlay_kind`）の所在 | **述語 `status_row_present` を追加**（Task 1） |
| `src-tauri/src/egui_shell/mod.rs` | `egui_shell` の re-export | `status_row_present` を re-export（Task 1） |
| `src-tauri/src/egui_shell/view.rs` | main 窓の 1 フレーム | `has_status` を述語経由へ（Task 2）／ reset-on-show で memo を戻す（Task 4） |
| `src-tauri/src/egui_shell/window_coordinator.rs` | 窓を駆動する責務。show の collapse の所在 | 読み口 2 本を追加し、collapse を実高へ（Task 4） |
| `scripts/lib/SnotraSmoke.psm1` | smoke の共有配管 | `Start-SnotraProcess` に `-ExtraVariables` を追加（Task 3） |
| `scripts/smoke-egui.ps1` | egui の自動回帰 smoke | **toast ありの 2 回目 show シナリオを追加**（Task 3） |
| `SPEC.md` / `src-tauri/CLAUDE.md` / `docs/adr/ADR-show-path-derives-drawn-height.md` | 意図の層 | 同期（Task 5） |

**タスク順の理由:** 検出器（Task 3）を修正（Task 4）より前に置く。**Task 3 の完了時点で smoke は赤**であり、それが「この検査は実際にこのバグを捕まえる」ことの証拠になる。Task 4 で緑になる。

---

### Task 1: status 行の述語を純粋核へ置く

**Files:**
- Modify: `src-tauri/src/egui_shell/notify.rs`（`overlay_kind` の直後・現在 43 行付近）
- Modify: `src-tauri/src/egui_shell/mod.rs:18`（re-export 行）
- Test: `src-tauri/src/egui_shell/notify.rs`（同ファイル末尾の `#[cfg(test)] mod tests`）

**Interfaces:**
- Consumes: 既存の `overlay_kind(indexing: bool, launching: bool, has_notice: bool) -> Option<OverlayKind>`
- Produces: `pub fn status_row_present(indexing: bool, results_view: bool, launching: bool, has_notice: bool) -> bool` — Task 2 と Task 4 が呼ぶ

- [ ] **Step 1: 失敗するテストを書く**

`notify.rs` の `#[cfg(test)] mod tests` に追加する:

```rust
    #[test]
    fn status_row_absent_when_no_source_is_active() {
        assert!(!status_row_present(false, true, false, false));
    }

    #[test]
    fn indexing_row_requires_results_view() {
        assert!(
            status_row_present(true, true, false, false),
            "Results ビューで indexing 中なら案内の行が出る"
        );
        assert!(
            !status_row_present(true, false, false, false),
            "tool / folder 段では indexing 案内を出さない（view.rs の連言と同じ）"
        );
    }

    #[test]
    fn launching_and_notice_do_not_depend_on_results_view() {
        assert!(status_row_present(false, false, true, false));
        assert!(status_row_present(false, false, false, true));
    }
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p snotra status_row`
Expected: FAIL（`cannot find function status_row_present`）

- [ ] **Step 3: 最小の実装を書く**

`overlay_kind` の直後に追加する:

```rust
/// status 行が 1 本出るか。**優先順そのものは `overlay_kind` が正本**であり、この述語は
/// 「行が出るか」だけを返す（`layout::main_window_height` の `status_height` の材料）。
///
/// **`results_view` を独立の引数に取るのは show 経路のためである。** `show_egui_main` は
/// フレームの外で「最初のフレームが描く高さ」を導く必要があり、reset-on-show 後の値を
/// リテラルで渡す（`window_coordinator.rs` の呼び出し点）。毎フレーム側は実際の値を渡す。
///
/// **`layout.rs` へ置いてはならない**——同ファイルは `std::time::Duration` 以外の依存を
/// 1 つも持たない自己完結した純粋核であり、この述語の意味論は高さの算術ではなく overlay 側である。
pub fn status_row_present(
    indexing: bool,
    results_view: bool,
    launching: bool,
    has_notice: bool,
) -> bool {
    overlay_kind(indexing && results_view, launching, has_notice).is_some()
}
```

`mod.rs:18` の re-export へ足す:

```rust
pub(crate) use notify::{NOTICE_HOTKEY, OverlayKind, overlay_kind, status_row_present};
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p snotra status_row`
Expected: PASS（3 件）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/egui_shell/notify.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(egui): status 行の有無を返す述語を overlay_kind の隣へ置く"
```

---

### Task 2: 毎フレーム側をその述語経由にする

挙動は変わらない。**show 側と同じ関数を通す**ことが目的である。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs:659`（`let has_status = overlay_text.is_some();`）

**Interfaces:**
- Consumes: Task 1 の `status_row_present`
- Produces: なし（内部の書き換え）

- [ ] **Step 1: 呼び出し点を差し替える**

`view.rs:659` の 1 行を置き換える。**`overlay_text` の構築（636-651 行）はそのまま残す**——文言の導出はそちらが持つ:

```rust
        // **`overlay_text.is_some()` と同値である**（同じ 4 つの入力を同じ `overlay_kind` へ
        // 通すため）。それでも述語を経由するのは、**show 経路が同じ関数を呼ぶ**からである
        // （`window_coordinator::show_egui_main`）。2 か所が同じ述語を通ることが、
        // 「畳む高さ = 描く高さ」を保つ機構である（#755 / #801）。
        let has_status = crate::egui_shell::status_row_present(
            self.controller.indexing(),
            self.controller.state().view_kind() == ViewKind::Results,
            self.controller.is_launching(),
            self.controller.notice_message().is_some(),
        );
```

- [ ] **Step 2: ビルドと既存テストで挙動不変を確認する**

Run: `cargo test -p snotra` と `cargo clippy -p snotra --all-targets -- -D warnings`
Expected: PASS（新しい失敗が無いこと）

- [ ] **Step 3: コミット**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "refactor(egui): 毎フレームの status 行判定を共有述語経由にする"
```

---

### Task 3: 検出器を先に置く（この時点で赤になる）

**toast ありで 2 回目の show を測る**シナリオを smoke へ足す。**既定プロファイル（toast なし）の高さ断言は #755 も #801 も捕まえない**——toast も status も無い状態では main は常にバー高で、そこは一度も壊れていない。

**Files:**
- Modify: `scripts/lib/SnotraSmoke.psm1`（`Start-SnotraProcess` に `-ExtraVariables` を追加）
- Modify: `scripts/smoke-egui.ps1`（既存 `finally` の直後・`$generated` の検査より前へシナリオ 2 を追加）

**Interfaces:**
- Consumes: `New-SnotraVerificationProfile` / `Wait-SnotraTraceEvent` / `Send-SnotraKeyChord` / `Wait-SnotraWindow`（すべて既存の export）
- Produces: `$failures` への追加項目（既存の失敗チャネル）

- [ ] **Step 1: モジュールに env の受け口を足す**

`SnotraSmoke.psm1` の `Start-SnotraProcess` の `param` へ 1 つ足し、`$variables` へ合流させる:

```powershell
        [switch]$Trace,
        # 呼び出し側が足す env（`SNOTRA_EGUI_FAKE_UPDATE` 等の視覚スモーク用ハッチ）。
        # 名前の検証は `Invoke-SnotraEnvironment` が行う（ここでは合流だけ）。
        [hashtable]$ExtraVariables = @{}
    )

    $variables = @{ SNOTRA_CONFIG_DIR = $ConfigDir }
    if ($Trace) { $variables.SNOTRA_TRACE = '1' }
    foreach ($k in $ExtraVariables.Keys) { $variables[$k] = $ExtraVariables[$k] }
```

- [ ] **Step 2: シナリオ 2 を smoke へ書く**

`smoke-egui.ps1` の既存 `finally` ブロックの**直後**（`$generated` を数える行より前）へ追加する:

```powershell
# ---- シナリオ 2: toast ありで 2 回目の show の高さが保たれるか（#755 / #801）----
#
# **既定プロファイル（toast なし）の高さ断言は両 issue を 1 件も捕まえない**——toast も status も
# 無い状態では main は常にバー高であり、そこは一度も壊れていない。捕まえるには「toast あり」かつ
# 「2 回目の show」が要る: #755 は 2 回目で 43px へ固着し、#801 は 1 フレームの伸びとして出る。
# **両者は 1 回の show では排他である**（2026-08-04 実測・#878 のコメント）。
#
# `SNOTRA_EGUI_FAKE_UPDATE` は起動時に読まれるためシナリオ 1 に相乗りできず、起動が 1 回増える。
#
# 高さは **DWM の実表示矩形**（`DWMWA_EXTENDED_FRAME_BOUNDS` = 属性 9）で読む。`GetWindowRect` は
# 不可視のリサイズ枠を含み、2 行の高さが 1 行の 2 倍にならない（実測: 118 / 64 に対し DWM は 110 / 56）。
$toastProfileDir = Join-Path $PSScriptRoot '..\target\smoke-egui\profile-toast'
# **font_size / bar_padding を明示 seed する**——既定値が変わってもこの検査の期待値が黙って動かない。
$toastProfile = New-SnotraVerificationProfile -ProfileDir $toastProfileDir -ShowIcons $false `
  -AdditionalSections "[visual]`r`nfont_size = 15`r`nbar_padding = 28"
$expectedBarLogical = 15 + 28   # layout::Metrics::from_config: bar_height = font_size + bar_padding
$toastErrPath = Join-Path $toastProfileDir 'stderr.log'
Remove-Item -LiteralPath $toastErrPath -Force -ErrorAction SilentlyContinue

function Get-MainWindowDwmSize {
  param([IntPtr]$Handle)
  $r = New-Object SnotraSmokeInterop.Native+RECT
  if ([SnotraSmokeInterop.Native]::DwmGetWindowAttribute($Handle, 9, [ref]$r, 16) -ne 0) {
    throw 'DwmGetWindowAttribute(EXTENDED_FRAME_BOUNDS) に失敗しました。'
  }
  [pscustomobject]@{ W = $r.Right - $r.Left; H = $r.Bottom - $r.Top }
}

# **証拠出力へ渡す起点は自前で取る**——`$launchedAt` はシナリオ 1 の起動時刻であり、
# ここで使うと「起動からの経過」が数十秒ずれて手掛かりを誤らせる。
$toastLaunchedAt = Get-Date
$toastProc = Start-SnotraProcess -ConfigDir $toastProfile.FullPath -FilePath $ExePath -Trace `
  -StandardErrorPath $toastErrPath -ExtraVariables @{ SNOTRA_EGUI_FAKE_UPDATE = '1' }
try {
  $hk2 = Wait-SnotraTraceEvent -Path $toastErrPath -EventName 'hotkey:registered' -TimeoutMs $StartupObserveTimeoutMs
  if ($null -eq $hk2) { throw 'シナリオ 2: hotkey:registered を観測できませんでした' }
  $vks2 = @($hk2.data.vks | ForEach-Object { [byte][int]$_ })

  foreach ($round in 1..2) {
    Send-SnotraKeyChord -VirtualKeys $vks2
    if ($null -eq (Wait-SnotraTraceEvent -Path $toastErrPath -EventName 'egui_show:done' -TimeoutMs $ObserveTimeoutMs)) {
      throw "シナリオ 2: $round 回目の egui_show:done を観測できませんでした"
    }
    $hwnd2 = Wait-SnotraWindow -Title 'Snotra' -Process $toastProc -TimeoutMs 5000

    # **1 秒サンプリングする**——1 点だけ見ると #801 の「伸びる」を伸びた後の値で見て緑になる。
    $heights = @()
    $sw2 = [System.Diagnostics.Stopwatch]::StartNew()
    $dwmW = 0
    while ($sw2.ElapsedMilliseconds -lt 1000) {
      $b = Get-MainWindowDwmSize -Handle $hwnd2
      $heights += $b.H
      $dwmW = $b.W
      Start-Sleep -Milliseconds 100
    }
    $sw2.Stop()
    $minH = ($heights | Measure-Object -Minimum).Minimum
    $maxH = ($heights | Measure-Object -Maximum).Maximum

    # 論理 px への換算は **config が幅を固定していることを較正点にする**（DPI API を別に読まない）。
    # seed の window_width は 600（New-SnotraVerificationProfile の既定）。
    $scale = $dwmW / 600.0
    $barPx = $expectedBarLogical * $scale

    # **片側の断言にする**——DWM 矩形には環境依存の数 px のずれが乗る（実測 +2px）。
    # 判別したいのは「バー 1 行（43 論理 px）」と「バー + toast（86 論理 px）」で、
    # 差は 43 論理 px ある。1.5 倍を閾値に置けば、ずれに影響されず両者を分けられる。
    if ($minH -lt 1.5 * $barPx) {
      $failures += ("toast ありの show #{0}: 窓が toast 行を含む高さになっていない（DWM 高さ {1}px / バー 1 行 ≒ {2:N0}px・#755）" -f $round, $minH, $barPx)
    }
    if ($minH -ne $maxH) {
      $failures += ("toast ありの show #{0}: 表示中に高さが動いた（{1}px → {2}px・#801 の 1 フレームの伸び）" -f $round, $minH, $maxH)
    }
    Write-Host ("toast ありの show #{0}: DWM {1}x{2}（min {3} / max {4}・バー 1 行 ≒ {5:N0}px）" -f `
      $round, $dwmW, $heights[0], $minH, $maxH, $barPx)

    if ($round -lt 2) {
      Send-SnotraKey -VirtualKey 27
      Send-SnotraKey -VirtualKey 27 -Up
      if ($null -eq (Wait-SnotraTraceEvent -Path $toastErrPath -EventName 'egui_hide:done' -TimeoutMs $ObserveTimeoutMs)) {
        throw "シナリオ 2: $round 回目の egui_hide:done を観測できませんでした"
      }
      Start-Sleep -Milliseconds 400
    }
  }
} catch {
  Show-FailureEvidence -Path $toastErrPath -Proc $toastProc -Context "シナリオ 2 throw: $($_.Exception.Message)" `
    -LaunchedAt $toastLaunchedAt -PostMortemWaitMs $PostMortemWaitMs
  throw
} finally {
  if (-not $toastProc.HasExited) { Stop-Process -Id $toastProc.Id -Force }
}
```

- [ ] **Step 3: 赤になることを確認する（このタスクの受け入れ条件）**

Run: `cargo build --release -p snotra` の後に `npm run smoke:egui`
Expected: **FAIL**。`toast ありの show #2: 窓が toast 行を含む高さになっていない（DWM 高さ 56px / バー 1 行 ≒ 54px・#755）` が出る。

**赤にならなければ止まること。** 検出器が現状のバグを捕まえないなら、修正後に緑になっても何も保証しない。

- [ ] **Step 4: コミット**

```bash
git add scripts/lib/SnotraSmoke.psm1 scripts/smoke-egui.ps1
git commit -m "test(smoke): toast ありの 2 回目 show で高さが保たれるかを検査する（現状は赤）"
```

---

### Task 4: show が実高を導出し、reset-on-show が memo も戻す（修正本体）

**この 2 つを 1 コミットで入れる。** memo リセット単独では #801 を「毎回」へ悪化させる。

**Files:**
- Modify: `src-tauri/src/egui_shell/window_coordinator.rs`（`read_window_width` の隣に読み口 2 本 / `show_egui_main` の `#[cfg(windows)]` collapse ブロック）
- Modify: `src-tauri/src/egui_shell/view.rs`（reset-on-show の消費ブロック）

**Interfaces:**
- Consumes: Task 1 の `status_row_present`、既存の `read_metrics` / `read_window_width` / `layout::main_window_height`
- Produces: なし（driver 内部）

- [ ] **Step 1: 読み口 2 本を足す**

`window_coordinator.rs` の `read_window_width` の直後へ:

```rust
/// index 構築中か（show 経路が status 行の有無を導くために読む）。正本は `AppState.indexing`。
/// 毎フレーム側は `launcher_controller` の同名メソッドが同じフラグを読む。
fn read_indexing(app: &tauri::AppHandle) -> bool {
    app.try_state::<crate::AppState>()
        .map(|s| s.indexing.load(Ordering::Relaxed))
        .unwrap_or(false)
}

/// updater toast の行が出るか（show 経路が高さを導くために読む）。正本は `UpdaterUiState`。
/// **reset-on-show はこれを触らない**——ゆえに hide を跨いで残り、show 後の最初のフレームでも
/// 同じ値になる（`launcher_controller` の reset 消費のコメントが明記している）。
fn read_toast_present(app: &tauri::AppHandle) -> bool {
    app.try_state::<super::UpdaterUiState>()
        .map(|st| st.0.lock().unwrap().toast().is_some())
        .unwrap_or(false)
}
```

- [ ] **Step 2: collapse を実高へ差し替える**

`show_egui_main` の `#[cfg(windows)]` ブロック（`let bar_h = ...` と `set_size` の 2 行）を置き換える。**冒頭のコメント（幅を config から読む理由）はそのまま残す**:

```rust
        // 畳む先は「そのフレームで実際に描かれる高さ」である（#755 / #801）。かつては
        // バー高固定で、最初のフレームが status / toast の分だけ書き直していた——その
        // 食い違いが、伸びる（#801）か固着する（#755）かのどちらかとして必ず現れた。
        // **両者は 1 回の show では排他であり、同じ食い違いの 2 分岐である**。
        let m = read_metrics(app);
        // **3 つのリテラルが reset-on-show への依存である。** 最初のフレームは reset 後の
        // 状態を描くので、`launching` と一時通知は消えており、view は Results 段に戻っている。
        // 前提が変わったら `status_row_present` の呼び出し点を grep すればここへ来る。
        let status = crate::egui_shell::status_row_present(
            read_indexing(app),
            /* results_view */ true,
            /* launching    */ false,
            /* has_notice   */ false,
        );
        let height = layout::main_window_height(
            m.bar_height,
            status.then_some(m.toast_height),
            read_toast_present(app).then_some(m.toast_height),
        );
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
```

- [ ] **Step 3: reset-on-show で main の memo も戻す**

`view.rs` の `if self.controller.consume_reset_pending() {` ブロック内、`results.reset_size_guard()` の呼び出しの**後**へ:

```rust
            // **main 窓のサイズ memo も初期値へ戻す**（results と対称・#755）。show 経路は
            // OS のサイズを直接書き、この memo を更新しない。戻さないと「memo == 導出値」の
            // 一致で補正が握り潰され、**導出がずれた瞬間に固着する**。
            //
            // 戻すことの代価は show ごとの同値 `set_size` 1 回だけである（show 経路が既に
            // 同じ高さを設定しているため見た目は変わらない）。得るのは fail-safe である
            // ——導出がずれても 1 フレームで実際に描く高さへ直る。
            //
            // **この 1 手を単独で入れてはならない**: show 経路が実高を導出しない状態でこれを
            // 入れると、補正が必ず撃たれて #801 が全ての show で起きる（実測で確認済み）。
            self.last_set_width = 0.0;
            self.last_set_height = 0.0;
```

- [ ] **Step 4: ビルドと単体テスト**

Run: `cargo clippy -p snotra --all-targets -- -D warnings` と `cargo test -p snotra`
Expected: PASS

- [ ] **Step 5: 検出器が緑になることを確認する**

Run: `cargo build --release -p snotra` の後に `npm run smoke:egui`
Expected: PASS。`toast ありの show #1` と `show #2` の両方で DWM 高さがバー 1 行の 1.5 倍以上、かつ min == max。

- [ ] **Step 6: カテゴリ D（実機の目視）**

`docs/build-commands.md` カテゴリ D の手順に従う。加えて本件固有の確認:

```powershell
$env:SNOTRA_EGUI_FAKE_UPDATE = "1"; cargo run -p snotra
```

- ホットキーで表示 → **最初からトーストが見えている**（43px で現れて伸びない）
- Escape → もう一度ホットキー → **2 回目もトーストが見えている**（fix 前は切り取られていた）
- 3 回目も同じ

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/egui_shell/window_coordinator.rs src-tauri/src/egui_shell/view.rs
git commit -m "fix(egui): show が「これから描く高さ」を導出し、reset-on-show が main の memo も戻す"
```

---

### Task 5: 意図の層を同期する

挙動が変わるので `SPEC.md` の同期は**仕様変更として**行う。

**Files:**
- Modify: `SPEC.md` §20.3（トースト UI・1121-1122 行付近）
- Modify: `src-tauri/CLAUDE.md`「モジュール構成」の `window_coordinator.rs` の項 と「実装パターン」の show の順序制約の項
- Modify: `src-tauri/src/egui_shell/view.rs`（デルタガード近傍の「意図的な 2 導出」コメント・870-873 行付近）
- Create: `docs/adr/ADR-show-path-derives-drawn-height.md`

**Interfaces:** なし

- [ ] **Step 1: `SPEC.md` §20.3 を直す**

「show 時はバー高への collapse 後に toast 分へ拡張する（1 フレームの高さスナップを受容）」を、**実装シンボル名を書かない観測文**で置き換える:

```
  加算する（#532 SU5・#646 決定 2）。show 時は表示直後に描かれる行の分まで含めた高さへ
  揃えてから位置を決める（高さのスナップは起きない・#755 / #801）
```

§4.7 の幅側の記述（「show 時は幅も設定値へ揃えてから位置を決める…#824」）と**同じ 1 つの規則**として読めることを確認する。

- [ ] **Step 2: `src-tauri/CLAUDE.md` を直す**

「モジュール構成」の `window_coordinator.rs` の項で「**main のサイズは 2 か所に分かれる**——show 経路の bar_height collapse はここ、毎フレームの動的高さは `view.rs`（ADR-results-presentation-two-stage の意図的な 2 導出）」を、**2 か所が同じ述語を共有する**形へ書き換える:

```
**main のサイズは 2 か所で設定する**——show 経路（ここ）と毎フレーム（`view.rs`）。
**両者は同じ高さを導く**: status 行の有無は `status_row_present` を、積算は
`main_window_height` を共有する。show 側は reset-on-show 後の状態をリテラルで渡す
（畳む高さと描く高さが食い違うと、伸びる〔#801〕か固着する〔#755〕のどちらかが必ず出る）
```

「実装パターン」の show の順序制約の項は、**順序は不変**のまま畳む先だけを直す:

```
- **show の操作順序制約（`egui_shell::show_egui_main`）**: 高さを決める（最初のフレームが
  描く高さ）→ `position_on_target_monitor` → `show()` の順。位置計算はウィンドウサイズを
  OS から読み戻してクランプするため、サイズは位置より前に確定していなければならない
```

- [ ] **Step 3: `view.rs` のコメントを直す**

デルタガード直前（870-873 行付近）のコメントを置き換える。**`size_delta_exceeds` が判定式の正本であること・memo を窓の所有型へ寄せないこと・ADR 却下 1 の引用は残す**（却下 1 の主張〔`main_size` を results の導出へ入れない〕は今も真である）:

```rust
        // 判定式の正本は `layout::size_delta_exceeds`（#749）。results 側と**式だけを共有し、
        // memo は共有しない**（ADR-results-presentation-two-stage 却下 1: `main_size` を results の
        // 導出へ入れない）。**高さの導出そのものは show 経路と共有する**——`status_row_present` と
        // `main_window_height` の 2 本を両者が通る（#755 / #801）。共有するのは導出であって memo ではない。
```

- [ ] **Step 4: 新しい ADR を書く**

`docs/adr/ADR-show-path-derives-drawn-height.md` を作る。**`ADR-results-presentation-two-stage` は編集しない**（ADR は凍結された歴史・`ADR-adr-frozen-history`）。内容は spec の §2 と、却下 6 が依存していた前提:

- 旧 ADR の却下 6 は「毎フレームの導出をそのまま使えば結果件数で伸びた高さでクランプする」と読んで統合を禁じた
- 実際に共有するのは `main_window_height`（バー + status + toast のみで結果件数に伸びない）であり、**実表示の高さでクランプするほうが正しい**
- 同じ ADR の却下 3 が残した教訓（禁止を恒久文書へ書く前に、その禁止が依存している前提を明示できるか確かめよ）の適用例である
- 却下した案: memo だけを直す（実測で #801 を普遍化）／継ぎ目 2 まで踏み込む（フレームに閉じない消費者を巻き込む・#738 / #760 へ）／show 側にインラインで書く（前提が grep に掛からない）／既定プロファイルへ高さ断言を足す（捕まえるはずのバグを 1 件も捕まえない）

- [ ] **Step 5: ガバナンス検査**

Run: `npm run governance:check`
Expected: 全検査 passed

- [ ] **Step 6: コミット**

```bash
git add SPEC.md src-tauri/CLAUDE.md src-tauri/src/egui_shell/view.rs docs/adr/ADR-show-path-derives-drawn-height.md
git commit -m "docs: show が描く高さを導出する形へ意図の層を同期する（#755 / #801）"
```

---

## 完了時の確認

- [ ] `cargo test -p snotra` / `cargo clippy -p snotra --all-targets -- -D warnings` が緑
- [ ] `npm run smoke:egui`（シナリオ 2 を含む）が緑
- [ ] `npm run smoke:startup` が緑
- [ ] `npm run governance:check` が緑
- [ ] カテゴリ D の目視: toast ありで 3 回連続の show がすべてトースト込みの高さ・伸びなし
- [ ] PR 本文の closing keyword に #755 と #801 を入れる。**#878 は閉じない**（継ぎ目 2 / 4 が残る）——代わりに #878 へ「継ぎ目 3 が閉じ、継ぎ目 1 は fail-safe 化された」とコメントする

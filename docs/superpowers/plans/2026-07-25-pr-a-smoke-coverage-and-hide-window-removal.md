# PR A: results 窓の smoke 被覆 + `RuntimeFrame::hide_window()` 削除 + 文書訂正 — 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** results 窓の show / hide を `smoke:egui` の自動被覆に載せ、`visible` を false に固着させうる未使用 API を削除し、実装と食い違う 6 箇所のコメントを訂正する。

**Architecture:** results 窓の可視性は現在**自動テストに一切かかっていない**（`smoke-egui.ps1` は Alt+Q と Escape のみを注入し文字を打たないため、results は全行程で一度も表示されない）。本 PR は後続 PR（A′ / B / C / D）が触る経路に先に網を張る。trace は `ResultsWindow` newtype（PR A′）が吸収する 3 関数の**内側ではなく呼び出し側**に置き、A′ で書き直しにならないようにする。

**Tech Stack:** Rust（`src-tauri` = crate `snotra`, `snotra-egui-runtime`）/ PowerShell 7（`scripts/smoke-egui.ps1`）/ Tauri v2 / egui + softbuffer / Windows

## Global Constraints

- **`main` へ直接コミットしない。** 本計画は feature ブランチ上で実行する（`git branch --show-current` が `main` でないことを Task 1 冒頭で確認する）。
- **bash の HEREDOC を使わない。** 複数行テキストは Write ツールか PowerShell here-string（閉じ `'@` は行頭）。
- **パス区切りは `/`。** `\` はエスケープが壊れやすい。
- **`/tmp` へ書かない。** `$env:TEMP` 配下を使う。
- trace のイベント名は `egui_results:show` / `egui_results:hide` で固定する（`smoke-egui.ps1` の `Wait-TraceEvent` が `"event":"<名前>"` を `-SimpleMatch` で探すため、名前の揺れは即 fail になる）。
- trace は**要求レベル**であり遷移レベルではない。同じ状態でも出る。smoke は presence のみを assert する。
- 本 PR は `snotra-egui-runtime` の `EguiWindow.visible` / `render()` の早期 return / `Focused(true)` arm / `hidden_window_is_not_painted` テストに**触らない**（spec §7 残余 2・3。「hidden 中は `update()` が走らない」の機構が未測定であるため、`visible` の除去は別判断とする）。
- 検証は `docs/build-commands.md` のカテゴリ A（clippy + 変更 crate の test）に加え、**カテゴリ C（`smoke:egui`）が必須**。`.claude/rules/src-tauri.md` のとおり post-edit hook はカテゴリ A しか走らせないため、**本 PR で「沈黙 = 合格」は成立しない**。

## 参照 spec

`docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md`（決定 6 = `hide_window()` 削除 / 決定 7 = trace の置き場 / §5 = 文書訂正）

## File Structure

| ファイル | 変更 | 責務 |
|---|---|---|
| `scripts/smoke-egui.ps1` | Modify | results 被覆の追加（scan seed・文字注入・2 trace の観測）。判定と skip 報告の SSOT |
| `src-tauri/src/egui_shell/view.rs` | Modify: `drive_results_window`（694-734） | results の show / hide 要求時に trace を出す |
| `src-tauri/src/egui_shell/mod.rs` | Modify: `hide_egui_main`（436-461）ほかコメント 4 箇所 | 外部 hide 経路の trace + 全称主張の限定 |
| `snotra-egui-runtime/src/runtime.rs` | Modify: `RuntimeFrame`（23-41）・`apply_frame_commands`（352-364）・`render()` の frame 初期化 | 未使用の hide 経路を API ごと削除 |
| `src-tauri/src/egui_shell/results_view.rs` | Modify: `//!`（6-9） | 削除された API を指す家訓の撤去 |
| `src-tauri/src/platform/mod.rs` | Modify: 268-273 | 削除済み関数を根拠にした stale コメントの書き直し |
| `src-tauri/src/main.rs` | Modify: 287 | 全称主張の限定 |

## 実行前に知っておくべき事実（実測済み）

1. **`smoke-egui.ps1` は CI でも走る。** `.github/workflows/e2e.yml:67` が `npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig` を実行する。CI の seed 済み config は `[paths] scan = []`（インデックス対象なし）ゆえ、**文字を打っても結果はゼロで results 窓は出ない**。したがって scan の seed も本 PR の範囲に含める。
2. **`-SeedConfig` は config が既に存在するとき seed しない**（既存 config を決して上書きしない・現行 30-52 行）。開発機では通常 config が存在するため、seed が走らない場合は results 検査を **skip し、skip したことを出力する**（黙って被覆を落とさない）。
3. **Escape 1 回で hide する。** `SearchState::on_escape`（`src-tauri/src/egui_shell/search_state.rs:308-326`）は `tool` と `folder` のみを見る。plain results ビューでは**クエリが非空でも** `EscapeOutcome::Hide` を返す。したがって文字を打った後も既存の Escape ステップはそのまま使える。
4. **`ScanPath` の TOML 形状**（`snotra-core/src/config.rs:446-452`）:

```toml
[[paths.scan]]
path = "..."
extensions = [".exe"]
include_folders = false
```

5. **`crate::trace_main(event: &str, data: serde_json::Value)`** は `view.rs` / `mod.rs` の双方から既に使われている（例: `mod.rs:460` の `egui_hide:done`、`view.rs:440` の `egui_slash`）。追加 import は不要。
6. **`RuntimeFrame::hide_window()` の製品コード呼び出し元はゼロ。** `snotra-egui-mvp/src/main.rs:185` は `close_window()` のみを使う。`view.rs:1006` は `drag_window()` のみ。

---

### Task 1: smoke に results 被覆を足す（Red）

**Files:**
- Modify: `scripts/smoke-egui.ps1`

**Interfaces:**
- Produces: trace イベント名 `egui_results:show` / `egui_results:hide` への依存。Task 2 がこの名前で emit する。
- Produces: 新パラメータ `-ResultsQuery <string>`（既定 `""`）。seed 済みでない環境で results 検査を明示的に有効化するための開発者向け入口。

- [ ] **Step 1: ブランチが `main` でないことを確認する**

Run: `git branch --show-current`
Expected: `main` 以外（`main` なら `git checkout -b feat/pr-a-results-smoke-coverage` を実行してから続行）

- [ ] **Step 2: `param()` ブロックに `-ResultsQuery` を追加する**

`scripts/smoke-egui.ps1` の `param()`（1-10 行）の `[string]$HotkeyVks = "18,81"` の直後に、閉じ括弧の前へ次を足す（直前の行に `,` を付けること）:

```powershell
  ,
  # results 窓の被覆に使う検索クエリ（1 文字想定）。既定 "" のとき:
  #   -SeedConfig で実際に seed できた場合のみ "z"（seed した zsnotrasmoke.exe に一致）を使う。
  #   seed しなかった場合（既存 config あり / -SeedConfig なし）は results 検査を skip する。
  # 既存 config を持つ開発機で検査したいときは、その索引に一致する文字を明示的に渡す。
  [string]$ResultsQuery = ""
```

- [ ] **Step 3: seed 済みかどうかを追跡し、scan 対象を作る**

現行の `if ($SeedConfig) { ... }` ブロック（27-53 行）を次で置き換える。`$seededNow` は「このプロセスが実際に config を書いたか」を表す。

```powershell
$seededNow = $false
if ($SeedConfig) {
  $cfgDir = Join-Path $env:APPDATA "Snotra"
  $cfgPath = Join-Path $cfgDir "config.toml"
  if (-not (Test-Path $cfgPath)) {
    New-Item -ItemType Directory -Force -Path $cfgDir | Out-Null
    # results 窓の被覆用に、索引に必ず 1 件載るダミーを置く（中身は問わない——indexer は
    # 拡張子だけで判定する: snotra-core/src/indexer.rs の matches_extension）。
    # 名前は既存の索引と衝突しにくい接頭辞にし、-ResultsQuery 既定の "z" で引けるようにする。
    $scanDir = Join-Path $env:TEMP "snotra_smoke_scan"
    New-Item -ItemType Directory -Force -Path $scanDir | Out-Null
    $dummy = Join-Path $scanDir "zsnotrasmoke.exe"
    if (-not (Test-Path $dummy)) { New-Item -ItemType File -Path $dummy | Out-Null }
    $scanDirToml = $scanDir -replace '\\', '/'
    # 最小の有効 TOML。[hotkey]/[appearance]/[paths] は #[serde(default)] 無しの必須セクションで、
    # 空 TOML は parse 失敗し「破損復旧」経路（stderr 診断 + config.toml.bak 退避 + 復旧バルーン）を
    # 毎回踏んでしまう（PR #659 レビューで検出）。値は config.rs の既定と同一
    # （hotkey Alt+Q = 本スクリプト既定の -HotkeyVks 18,81 と一致）。
    # scan は上の 1 ファイルだけを対象にする（索引は 1 件・ビルドは即座に終わる）。
    # **`scan = []` と `[[paths.scan]]` を併記してはならない**——同一キーの再定義で TOML の
    # parse が落ち、config 破損復旧経路へ落ちる。`[paths]` を空ヘッダで置き、その下に
    # array-of-tables を続ける（`PathsConfig.scan` は #[serde(default)] ゆえ空でも可）。
    $seedToml = @"
[hotkey]
modifier = "Alt"
key = "Q"

[appearance]
window_width = 600

[paths]

[[paths.scan]]
path = "$scanDirToml"
extensions = [".exe"]
include_folders = false
"@
    Set-Content -Path $cfgPath -Value $seedToml -Encoding utf8
    $seededNow = $true
    Write-Host "Seeded minimal config: $cfgPath (scan: $scanDir)"
  } else {
    Write-Host "Config already exists, seed skipped: $cfgPath"
  }
}

# results 検査に使うクエリを決める。空文字なら検査を skip する。
if ([string]::IsNullOrEmpty($ResultsQuery) -and $seededNow) {
  $ResultsQuery = "z"
}
```

**注意**: `@"..."@`（展開あり here-string）を使う。`$scanDirToml` を埋めるため。閉じ `"@` は必ず行頭に置くこと。

- [ ] **Step 4: 文字注入用のヘルパー定数を足す**

`$VK_ESCAPE = 0x1B`（61 行）の直後に足す:

```powershell
# 1 文字クエリの VK は英字のみを想定（VK_A..VK_Z = 0x41..0x5A = ASCII 大文字と同値）。
function Get-LetterVk {
  param([string]$Ch)
  $u = $Ch.ToUpperInvariant()
  if ($u.Length -ne 1 -or $u[0] -lt 'A' -or $u[0] -gt 'Z') {
    throw "ResultsQuery must be a single A-Z letter, got: '$Ch'"
  }
  return [byte][int][char]$u[0]
}
```

- [ ] **Step 5: results の show 検査を挿入する**

`if (-not $shown) { ... }` ブロック（136-138 行）と、その次の WebView2 増分チェックの**間**に挿入する:

```powershell
  # results 窓の被覆（#671/#673 サイクル PR A）。索引内容を制御できるときだけ実行する。
  $resultsChecked = $false
  if ($failures.Count -eq 0 -and -not [string]::IsNullOrEmpty($ResultsQuery)) {
    $resultsChecked = $true
    $queryVk = Get-LetterVk $ResultsQuery
    # 索引構築中は plain 検索が抑止される（SPEC §4.7）。起動直後の負荷で 1 回目の打鍵が
    # 抑止側に落ちることがあるため、hotkey 注入と同じく一度だけ再注入する。
    $resultsShown = $false
    foreach ($attempt in 1..2) {
      Send-Key $queryVk
      Start-Sleep -Milliseconds 50
      Send-Key $queryVk -Up
      if (Wait-TraceEvent -Path $errPath -EventName "egui_results:show" -TimeoutMs $ObserveTimeoutMs) {
        $resultsShown = $true
        break
      }
    }
    if (-not $resultsShown) {
      $failures += "egui_results:show not observed within ${ObserveTimeoutMs}ms x2 after typing '$ResultsQuery'"
    }
  }
```

- [ ] **Step 6: results の hide 検査を Escape ステップに足す**

現行の Escape ブロック（146-155 行）の `egui_hide:done` 検査の直後、同じ `if ($failures.Count -eq 0)` の中へ足す:

```powershell
    # main の hide は hide_egui_main が results も同時に隠す（#646 PR2 決定 6）。
    # show 側を検査したときだけ対で検査する（対称ペア・/symmetric-check）。
    if ($resultsChecked -and
        -not (Wait-TraceEvent -Path $errPath -EventName "egui_results:hide" -TimeoutMs $ObserveTimeoutMs)) {
      $failures += "egui_results:hide not observed within ${ObserveTimeoutMs}ms after Escape"
    }
```

- [ ] **Step 7: skip したことを必ず出力する（silent cap を作らない）**

最終の成功メッセージ（177 行付近の `Write-Host "egui smoke passed..."`）を次で置き換える:

```powershell
Write-Host ""
if ($resultsChecked) {
  Write-Host "egui smoke passed (show/hide + results show/hide observed, webview delta 0)." -ForegroundColor Green
} else {
  Write-Host "egui smoke passed (show/hide observed, webview delta 0)." -ForegroundColor Green
  Write-Host "NOTE: results window coverage was SKIPPED (no controlled index). Pass -SeedConfig on a machine without %APPDATA%/Snotra/config.toml, or pass -ResultsQuery <letter> matching your index." -ForegroundColor Yellow
}
```

**注意**: `$resultsChecked` は `try` ブロック内で定義されるため、`finally` の後で参照できるようスクリプト先頭（`$failures = @()` の直前）に `$resultsChecked = $false` を置いておくこと。Step 5 の `$resultsChecked = $false` はそのまま残してよい（再代入は無害）。

- [ ] **Step 8: release ビルドを用意する**

Run: `cargo build --release -p snotra`
Expected: 成功（`target/release/snotra.exe` が生成される）

- [ ] **Step 9: Red を確認する**

**このステップは既存の config を持たない状態を要求する。** 現在の `%APPDATA%/Snotra/config.toml` を退避してから実行し、検査後に戻す。

Run:
```powershell
$cfg = Join-Path $env:APPDATA "Snotra/config.toml"
if (Test-Path $cfg) { Move-Item $cfg "$cfg.smokebak" -Force }
npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig
```

Expected: **FAIL** — `egui smoke failed:` に続けて
`egui_results:show not observed within 8000ms x2 after typing 'z'`
（trace がまだ存在しないため。`egui_show:done` と webview delta は通る）

退避した config を戻す:
```powershell
$cfg = Join-Path $env:APPDATA "Snotra/config.toml"
Remove-Item $cfg -Force -ErrorAction SilentlyContinue
if (Test-Path "$cfg.smokebak") { Move-Item "$cfg.smokebak" $cfg -Force }
```

- [ ] **Step 10: コミットしない**

Task 1 は Red の状態で終える。Task 2 の Green と合わせて 1 コミットにする（赤いスクリプトを単独でコミットすると、その commit で CI の smoke job が落ちるため）。

---

### Task 2: results の show / hide に trace を足す（Green）

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs:709-733`（`drive_results_window`）
- Modify: `src-tauri/src/egui_shell/mod.rs:446-450`（`hide_egui_main` の results hide）

**Interfaces:**
- Consumes: Task 1 が固定した trace 名 `egui_results:show` / `egui_results:hide`
- Produces: なし（後続タスクは本タスクの出力に依存しない）

- [ ] **Step 1: `drive_results_window` の hide 側に trace を足す**

`src-tauri/src/egui_shell/view.rs` の現行:

```rust
        let visible = show_results && res_h > 0.0;
        if !visible {
            if self.last_results_visible {
                crate::egui_shell::hide_results(&results);
                self.last_results_visible = false;
            }
            return;
        }
```

を次で置き換える:

```rust
        let visible = show_results && res_h > 0.0;
        if !visible {
            if self.last_results_visible {
                crate::egui_shell::hide_results(&results);
                // trace は 3 つの生 Win32 関数の**内側ではなく呼び出し側**に置く（spec 決定 7）——
                // PR A′ がその 3 関数を ResultsWindow の method へ移すため、内側に書くと
                // smoke が「1 PR 後に消える関数」から出る event 名を pin することになる。
                crate::trace_main("egui_results:hide", serde_json::json!({ "from": "drive" }));
                self.last_results_visible = false;
            }
            return;
        }
```

- [ ] **Step 2: `drive_results_window` の show 側に trace を足す**

同ファイルの現行:

```rust
        if !self.last_results_visible {
            // フォーカスを奪わない表示（tauri show() は SW_SHOW で活性化する・#646 PR2）。
            crate::egui_shell::show_results_no_activate(&results);
            self.last_results_visible = true;
        }
```

を次で置き換える:

```rust
        if !self.last_results_visible {
            // フォーカスを奪わない表示（tauri show() は SW_SHOW で活性化する・#646 PR2）。
            crate::egui_shell::show_results_no_activate(&results);
            // 置き場の理由は上の hide 側コメントと同じ（spec 決定 7）。
            crate::trace_main("egui_results:show", serde_json::json!({ "rows": count }));
            self.last_results_visible = true;
        }
```

- [ ] **Step 3: `hide_egui_main` の外部 hide 経路に trace を足す**

`src-tauri/src/egui_shell/mod.rs` の現行:

```rust
    if let Some(results) = app.get_window("results") {
        hide_results(&results);
    }
```

を次で置き換える:

```rust
    if let Some(results) = app.get_window("results") {
        hide_results(&results);
        // 呼び出し側に置く（spec 決定 7）。results の hide は 2 経路あり
        // （ここと view.rs の drive_results_window）、trace は要求レベルゆえ
        // 既に隠れていても出る——smoke は presence のみを assert する。
        crate::trace_main("egui_results:hide", serde_json::json!({ "from": "hide_main" }));
    }
```

- [ ] **Step 4: clippy と該当 crate のテストを走らせる**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS（警告ゼロ）

Run: `cargo test -p snotra`
Expected: PASS

- [ ] **Step 5: release を再ビルドする**

Run: `cargo build --release -p snotra`
Expected: 成功

- [ ] **Step 6: Green を確認する**

Task 1 Step 9 と同じ手順（config 退避 → smoke → 復元）で実行する。

Run:
```powershell
$cfg = Join-Path $env:APPDATA "Snotra/config.toml"
if (Test-Path $cfg) { Move-Item $cfg "$cfg.smokebak" -Force }
npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig
```

Expected: **PASS** — `egui smoke passed (show/hide + results show/hide observed, webview delta 0).`

復元:
```powershell
$cfg = Join-Path $env:APPDATA "Snotra/config.toml"
Remove-Item $cfg -Force -ErrorAction SilentlyContinue
if (Test-Path "$cfg.smokebak") { Move-Item "$cfg.smokebak" $cfg -Force }
```

- [ ] **Step 7: skip 経路も壊れていないことを確認する**

Run（config を退避せず、`-SeedConfig` も付けない）: `npm run smoke:egui -- -ExePath target/release/snotra.exe`
Expected: PASS、かつ黄色の `NOTE: results window coverage was SKIPPED ...` が出る

- [ ] **Step 8: コミット**

```bash
git add scripts/smoke-egui.ps1 src-tauri/src/egui_shell/view.rs src-tauri/src/egui_shell/mod.rs
git commit -m "test(smoke): #671 results 窓の show/hide を smoke の自動被覆に載せる"
```

---

### Task 3: `RuntimeFrame::hide_window()` を削除する

**Files:**
- Modify: `snotra-egui-runtime/src/runtime.rs:23-41`（`RuntimeFrame`）
- Modify: `snotra-egui-runtime/src/runtime.rs:352-364`（`apply_frame_commands`）
- Modify: `snotra-egui-runtime/src/runtime.rs:286-295`（`render()` 内の `RuntimeFrame` 初期化）
- Modify: `src-tauri/src/egui_shell/results_view.rs:1-9`（`//!`）

**Interfaces:**
- Consumes: なし
- Produces: `RuntimeFrame` の public API が `close_window()` / `drag_window()` の 2 つになる

**削除の根拠（実測）**: `hide_window()` の製品コード呼び出し元はゼロ。`results` は `focusable(false)` ゆえ `Focused(true)` が原理的に来ず、`visible` が false に固着すると復帰経路が無い。その第 3 の writer を API ごと消す（spec 決定 6）。

**スコープ外（意図的）**: `EguiWindow.visible` フィールド本体、`render()` の `if !visible { return }`、`Focused(true)` arm、`hidden_window_is_not_painted` テストには**触らない**。`visible` は本タスク後に「書き込みが `Focused(true)` の 1 箇所だけ」になるが、除去の可否は「hidden 中に `update()` が走らない」機構の測定に依存する（spec §7 残余 2・3）。**`Focused(true)` arm は `on_window_event` 経由で repaint を起こす現行の唯一の起動源でもあるため、`visible` の代入が no-op であることを理由に arm ごと消してはならない**（spec §2.5）。

- [ ] **Step 1: 呼び出し元がゼロであることを自分で確認する**

Run: `rg -n "hide_window\(" --glob '!docs/**'`
Expected: 3 件のみ — `snotra-egui-runtime/src/runtime.rs:34`（定義）、`src-tauri/src/egui_shell/results_view.rs:6`（禁止コメント）、`RETROSPECTIVE.md`（記録）。**製品コードの呼び出しがあれば STOP**（前提が崩れているため計画へ戻る）

- [ ] **Step 2: `RuntimeFrame` から `hide_requested` と `hide_window()` を消す**

`snotra-egui-runtime/src/runtime.rs` の現行:

```rust
pub struct RuntimeFrame {
    close_requested: bool,
    hide_requested: bool,
    drag_requested: bool,
}

impl RuntimeFrame {
    pub fn close_window(&mut self) {
        self.close_requested = true;
    }

    pub fn hide_window(&mut self) {
        self.hide_requested = true;
    }

    pub fn drag_window(&mut self) {
        self.drag_requested = true;
    }
}
```

を次で置き換える:

```rust
pub struct RuntimeFrame {
    close_requested: bool,
    drag_requested: bool,
}

impl RuntimeFrame {
    pub fn close_window(&mut self) {
        self.close_requested = true;
    }

    pub fn drag_window(&mut self) {
        self.drag_requested = true;
    }
}
```

**`hide_window()` を再導入してはならない。** 従属窓（`focusable(false)`）は `Focused(true)` が原理的に来ないため、`visible = false` を書く経路を view に与えると永久非描画へ固着する。hide は窓の外部（`egui_shell::hide_egui_main` / main の `drive_results_window`）が所有する。

- [ ] **Step 3: `apply_frame_commands` から hide 分岐を消す**

現行:

```rust
    fn apply_frame_commands(&mut self, frame: RuntimeFrame) -> Result<(), RuntimeError> {
        if frame.drag_requested {
            self.window.start_dragging()?;
        }
        if frame.hide_requested {
            self.window.hide()?;
            self.visible = false; // 不変条件⑥: hide 要求を出したフレームから非表示扱いにする。
        }
        if frame.close_requested {
            self.window.close()?;
        }
        Ok(())
    }
```

を次で置き換える:

```rust
    fn apply_frame_commands(&mut self, frame: RuntimeFrame) -> Result<(), RuntimeError> {
        if frame.drag_requested {
            self.window.start_dragging()?;
        }
        if frame.close_requested {
            self.window.close()?;
        }
        Ok(())
    }
```

- [ ] **Step 4: `render()` 内の `RuntimeFrame` 初期化から `hide_requested` を消す**

`render()`（`runtime.rs:288-292`）の現行:

```rust
        let mut frame = RuntimeFrame {
            close_requested: false,
            hide_requested: false,
            drag_requested: false,
        };
```

を次で置き換える:

```rust
        let mut frame = RuntimeFrame {
            close_requested: false,
            drag_requested: false,
        };
```

- [ ] **Step 5: `results_view.rs` の禁止コメントを撤去する**

`src-tauri/src/egui_shell/results_view.rs` の冒頭 `//!` から次の 4 行（6-9 行）を削除する:

```
//! **禁止: この view で `frame.hide_window()` を呼ばない** — results は focusable(false) で
//! `Focused(true)` が永遠に来ないため、runtime の visible フラグが false に固着し永久非描画
//! になる（復帰経路は Focused(true) のみ・runtime.rs）。hide は必ず外部（`hide_egui_main` /
//! main の drive）の `window.hide()` で行う。
```

直前の空行 `//!`（5 行）も併せて削除し、`//!` ブロックが「窓の可視性・サイズ・位置の driver は main 側（hidden 窓は update() が走らないため）。」で終わるようにする。

**代わりに削除の事実を残す**（次の 1 行を `//!` の末尾に足す）:

```
//! hide は外部（`hide_egui_main` / main の `drive_results_window`）が所有する。runtime に
//! view 側から hide する API は無い（`RuntimeFrame::hide_window` は #671 サイクル PR A で削除）。
```

- [ ] **Step 6: 両 crate の検査を走らせる**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS。**`visible` が「一度も false にならないフィールド」になるが、`Focused(true)` arm が書き込むため dead_code 警告は出ない**（出た場合はスコープ外の除去に踏み込まず、警告内容を報告して STOP）

Run: `cargo test -p snotra-egui-runtime`
Expected: PASS（`hidden_window_is_not_painted` はローカル定義の述語を検査する恒真テストゆえ、本変更では落ちない。spec §7 残余 3）

Run: `cargo test -p snotra`
Expected: PASS

Run: `cargo test -p snotra-egui-mvp`
Expected: PASS（`snotra-egui-mvp/src/main.rs:185` は `close_window()` のみを使うため影響なし）

- [ ] **Step 7: smoke で回帰が無いことを確認する**

Run: `cargo build --release -p snotra`
Expected: 成功

Run: `npm run smoke:egui -- -ExePath target/release/snotra.exe`
Expected: PASS（skip の NOTE 付き。results 被覆は Task 2 で確認済みゆえここでは skip 経路で足りる）

- [ ] **Step 8: コミット**

```bash
git add snotra-egui-runtime/src/runtime.rs src-tauri/src/egui_shell/results_view.rs
git commit -m "refactor(runtime): #671 未使用の RuntimeFrame::hide_window を削除する"
```

---

### Task 4: 実装と食い違うコメント 6 箇所を訂正する

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs:357`, `:434`, `:454`, `:488`
- Modify: `src-tauri/src/main.rs:287`
- Modify: `src-tauri/src/platform/mod.rs:268-273`

**Interfaces:**
- Consumes: なし
- Produces: なし（文書のみ）

**根拠**: spec §5。`hide_egui_main` が「全 hide の唯一の副作用所有点」という主張は #646 PR2 以降**偽**である（`view.rs:712` の results hide が経由しない）。また削除済み関数 `show_main_and_emit` への参照が 2 箇所残る。

- [ ] **Step 1: 訂正対象を自分で数え直す**

Run: `rg -n "全 hide|show_main_and_emit" src-tauri/src`
Expected: **7 件** — `egui_shell/mod.rs:344`（`show_egui_main` doc の「全 hide は外部化ゆえ」。**これは訂正しない**・Step 6 参照）、`:357`、`:434`、`:454`、`:488`、`main.rs:287`、`platform/mod.rs:270`。うち**訂正対象は 6 件**（`mod.rs:344` を除く全件）。**件数が違えば実態に合わせて全件見る**（この Step の数字ではなく grep の出力が SSOT）

- [ ] **Step 2: `hide_egui_main` の doc を限定する**

`src-tauri/src/egui_shell/mod.rs` の現行:

```rust
/// egui 経路の hide。全 hide の唯一の副作用所有点（codex #7）。外部 window.hide() のみで
/// runtime.visible を false にしない（空白窓回避・codex #4）。
```

を次で置き換える:

```rust
/// egui 経路の hide。**main の** hide の唯一の副作用所有点（codex #7）——世代 bump・位置保存・
/// main_visible・working set trim はここにしか無い。**results の hide はここを通らない経路が
/// ある**（`view.rs` の `drive_results_window`）ため、両窓を合わせた合流点ではない（#646 PR2
/// 以降・全称主張の訂正は #671 サイクル PR A）。
/// 外部 window.hide() のみで runtime.visible を false にしない（空白窓回避・codex #4）。
```

- [ ] **Step 3: trim の呼び出し元コメントを限定する**

同ファイルの現行:

```rust
    // hide 後に working set を trim する（全 hide 経路の合流点＝ここが唯一の呼び出し元・#532 SU6.5）。
```

を次で置き換える:

```rust
    // hide 後に working set を trim する（**main の** hide 経路の合流点＝ここが唯一の呼び出し元・
    // #532 SU6.5）。results 単独 hide（view.rs の drive）では main が可視のままゆえ trim しないのが正しい。
```

- [ ] **Step 4: `register_hide_listener` の doc を限定する**

同ファイルの現行:

```rust
/// view からの `egui-hide-requested` を受け、hide_egui_main を実行する（全 hide の合流点・codex #7）。
```

を次で置き換える:

```rust
/// view からの `egui-hide-requested` を受け、hide_egui_main を実行する（**main の** hide の
/// 合流点・codex #7）。
```

- [ ] **Step 5: `main.rs` のコメントを限定する**

`src-tauri/src/main.rs` の現行:

```rust
            // view→emit→listener の合流点。全 hide を hide_egui_main の 1 経路に集約（codex #7）。
```

を次で置き換える:

```rust
            // view→emit→listener の合流点。**main の** hide を hide_egui_main の 1 経路に集約（codex #7）。
```

- [ ] **Step 6: `show_egui_main` doc の削除済み関数参照を言い換える**

`src-tauri/src/egui_shell/mod.rs:357` の現行:

```rust
    // 高さリセット → 位置 → show の順（SU2 の show_main_and_emit と同じ制約）。
```

を次で置き換える（`show_main_and_emit` は #532 SU7 で削除済み）:

```rust
    // 高さリセット → 位置 → show の順（旧 WebView2 経路から引き継いだ順序制約）。
```

`mod.rs:344` の `show_egui_main` doc にある「全 hide は外部化ゆえ」は**そのままでよい**——ここは runtime.visible の話であり、Task 3 で `hide_window()` を消した後は文字どおり真である。

- [ ] **Step 7: `platform/mod.rs` の stale コメントを書き直す**

現行（268-273 行）:

```rust
            PlatformCommand::TurnOffIme(hwnd_raw) => {
                // Known: this command is dispatched from the platform thread after
                // show_main_and_emit() calls show()/set_focus() on the main thread.
                // A narrow timing window exists where the window receives focus before
                // IME is disabled. Mitigated by passing HWND directly to avoid an extra
                // lookup. Residual race is theoretical and not observed in practice.
```

を次で置き換える。**名前が古いだけでなく、述べている機構が現行と逆である**——`show_egui_main` は IME オフを focus 同期の**後**に置くことを意図的な順序制約としている（`mod.rs` の「focus 同期より後に置く」コメント）:

```rust
            PlatformCommand::TurnOffIme(hwnd_raw) => {
                // 送信元は egui_shell::show_egui_main（旧 show_main_and_emit は #532 SU7 で削除）。
                // 呼び出し順は set_focus() → SendMessageTimeoutW によるフォーカス同期待ち →
                // 本コマンド送信であり、「focus より先に IME を切る」ことは意図的に避けている
                // （前に置くと IME オフが対象窓に効かない）。HWND を直接渡すのは再 lookup を
                // 避けるため。
```

- [ ] **Step 8: 検査を走らせる**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

Run: `cargo test -p snotra`
Expected: PASS

Run: `npm run governance:check`
Expected: `governance:check — G1..G10 passed`

- [ ] **Step 9: 訂正漏れが無いことを確認する**

Run: `rg -n "全 hide の唯一|全 hide の合流点|全 hide を hide_egui_main|show_main_and_emit" src-tauri/src`
Expected: 0 件

- [ ] **Step 10: コミット**

```bash
git add src-tauri/src/egui_shell/mod.rs src-tauri/src/main.rs src-tauri/src/platform/mod.rs
git commit -m "docs: #671 hide 経路の全称主張を限定し削除済み関数への参照を解消する"
```

---

## 完了時の検証（PR 作成前・スキップ不可）

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — PASS
- [ ] `cargo test -p snotra` / `-p snotra-egui-runtime` / `-p snotra-egui-mvp` — PASS
- [ ] `npm run governance:check` — G1..G10 passed
- [ ] `npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig`（config を退避した状態）— **results 被覆込みで PASS**
- [ ] `npm run smoke:startup -- -ExePath target/release/snotra.exe` — PASS
- [ ] `git push -u origin HEAD` してから `gh pr create`（PreToolUse hook が push 先行を要求する。`&&` で繋ぐのは可）
- [ ] PR 本文の closing keyword を確認する。**本 PR は #671 を close しない**（PR A は #671 の一部でしかない）。`gh pr view <PR> --json closingIssuesReferences` が空であることをマージ直前に確認する

## 報告に含めること

- 追加/更新したテスト名: `smoke-egui.ps1` の results 被覆（`egui_results:show` / `egui_results:hide` の観測）
- 検証した不変条件:
  - results 窓は 1 文字入力で表示され、Escape で main と同時に隠れる（自動被覆・従来ゼロ）
  - view から `visible` を false に落とす経路が存在しない（`RuntimeFrame::hide_window` の削除により構造的に）
  - `hide_egui_main` は main の hide の副作用所有点であり、results の hide 経路はそこを通らないものがある（コメントの訂正で明文化）
- skip した検証と理由: 開発機（既存 config あり）での results 被覆は skip される。CI（`e2e.yml` の smoke-egui job・`-SeedConfig`）が fresh runner で被覆する

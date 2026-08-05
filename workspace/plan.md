# キャレット検査の機序の再設計 — 実装計画

> **実装者へ:** タスク単位で進めること。ステップは `- [ ]` で追跡する。
> **コミットは各タスクの末尾で行うが、チェックボックスにはしていない**——このリポジトリは
> 作業項目にコミット以降（コミット・push・PR 作成・マージ）を置くことを禁じている
> （`.claude/skills/start-issue/SKILL.md`。実施前にチェックすれば未実行の行為を完了として
> 主張し `gh pr create` のガードを解除し、実施後にチェックすればそのチェックを含む
> コミットが未チェックのまま残る・#922 で前者を踏んだ）。

**Goal:** キャレットの断言を egui_kittest（実コードの並びを縛る）へ移し、実機配管の Pester 検査を「起動後の最初のフレームで入力欄が打鍵を受け取れるか」1 点へ縮める。

**Architecture:** 設計書 `docs/superpowers/specs/2026-08-05-caret-test-mechanism-design.md` が正本。3 層（L1 状態遷移 / L2 キャレット / L3 実プロセス）のうち、L1 は既存の単体検査が守る。L2 を `view.rs` から `&mut Ui` だけを取る関数へ切り出して kittest で駆動し、L3 は `egui_input:focus_state` の観測 1 件へ縮める。あわせて、縮めても残る単一インスタンス衝突（実測 3/30）を塞ぐ。

**Tech Stack:** Rust（egui 0.35 / egui_kittest 0.35）、PowerShell 7 + Pester 6。

## Global Constraints

- `egui = "=0.35.0"`（ピン止め）。`egui_kittest` は `"0.35"` を使う——`snotra-settings/Cargo.toml:23` と同じ指定に揃える
- `cargo clippy --workspace --all-targets -- -D warnings` が green であること。**未使用の新 API は `dead_code` で落ちる**ため、新設と呼び出し点の移行は同じタスクに束ねる
- PowerShell スクリプトは `Set-StrictMode -Version Latest` の下で動く（`SnotraSmoke.psm1:3`）。存在しないメンバへのアクセスは実行時エラーになる
- **`finally` から throw しない**——元の例外を覆い隠すため
- コメントは `docs/comment-guidelines.md` の様式に従う

---

## File Structure

| ファイル | 責務 | 変更 |
|---|---|---|
| `snotra-egui-runtime/src/env.rs` | trace ハッチの env 述語（空文字を未設定として扱う唯一の場所） | **新規** |
| `snotra-egui-runtime/src/lib.rs` | モジュール宣言 | `mod env;` を追加 |
| `snotra-egui-runtime/src/{input,renderer,repaint,runtime,windows_ime}.rs` | 各 trace ハッチ | `var_os(...).is_some()` 7 箇所を `env::trace_hatch_enabled` へ |
| `snotra-egui-runtime/CLAUDE.md` | モジュール索引 | `env.rs` の行を追加 |
| `src-tauri/src/egui_shell/view.rs` | 検索入力欄の widget 合成を切り出し、kittest 検査を持つ | 切り出し + `mod tests` へ追加 |
| `src-tauri/Cargo.toml` | dev 依存 | `[dev-dependencies]` 節を新設し `egui_kittest` |
| `scripts/lib/SnotraSmoke.psm1` | プロセス停止の終了待ち | `Stop-SnotraProcessAndWait` を新設 |
| `scripts/lib/SnotraSmoke.Tests.ps1` | 実機配管の縮小・衝突の検出 | 2 つの `finally` / キャレット It / `AfterAll` / 警告ノイズ |
| `scripts/smoke-egui.ps1` | 冗長になる固定待ち | `Start-Sleep 300`（`139`）を削除 |
| `PERFORMANCE.md` | 計器の自称正本 | 欠けている 2 名前 + 受理値 1 文 |
| `docs/build-commands.md` | 検証コマンドの正本 | 実機配管の記述を実態へ |

---

## Task 1: プロセスの終了を待つ共有ヘルパ

単一インスタンス衝突（実測 3/30）を塞ぐ。**縮小後も残る故障**であり、L2/L3 の再設計とは独立に効く。

**Files:**
- Modify: `scripts/lib/SnotraSmoke.psm1`（`Resolve-SnotraExistingProcess` は `383`、`Stop` 分岐は `398-400`、`Export-ModuleMember` は `847`）
- Test: `scripts/lib/SnotraSmoke.Tests.ps1`（`Describe 'Resolve-SnotraExistingProcess'` は `233`）

**Interfaces:**
- Produces: `Stop-SnotraProcessAndWait -Process <object> [-TimeoutMs <int>] [-Quiet]` → `[bool]`（終了を確認できたら `$true`）

**設計上の制約（実測済み・逸脱すると壊れる）**

- **引数に `[System.Diagnostics.Process]` の型を付けてはならない。** 既存 fixture（`Tests.ps1:243-248`）は `Id` だけを持つ `[pscustomobject]` で方針分岐を固定しており、型を付けると**パラメータ束縛で例外**になる（実測: `Cannot create object of type "System.Diagnostics.Process". "Id" is a ReadOnly property.`）
- 型を外すだけでは足りない。`Set-StrictMode -Version Latest` 下では失敗が最初のメンバアクセスへ移る（実測: `The property 'HasExited' cannot be found on this object.`）。**fixture 側に `HasExited` と `WaitForExit` を足す**
- `psm1:399` にだけ `-ErrorAction` が無い。ヘルパへ畳むと `Policy Stop` が今まで上げていたエラーが黙るので、**`-Quiet` で明示的に切り替える**

- [x] **Step 1: 失敗するテストを書く**

`Tests.ps1` の `Describe 'Resolve-SnotraExistingProcess'`（`233`）の直前へ新しい `Describe` を足す。

```powershell
Describe 'Stop-SnotraProcessAndWait（#872 単一インスタンス衝突）' {
    BeforeAll {
        function New-FakeProcess {
            param([bool]$HasExited = $false, [bool]$WaitResult = $true, [int]$Id = 123)
            $fake = [pscustomobject]@{ Id = $Id }
            $fake | Add-Member -MemberType NoteProperty -Name HasExited -Value $HasExited
            $fake | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value ([scriptblock]::Create("param(`$ms) `$$WaitResult"))
            return $fake
        }
    }

    It '$null は何もせず $true を返す' {
        Stop-SnotraProcessAndWait -Process $null | Should -BeTrue
    }

    It '既に終了しているプロセスは kill しない' {
        Mock -ModuleName SnotraSmoke Stop-Process {}
        Stop-SnotraProcessAndWait -Process (New-FakeProcess -HasExited $true) | Should -BeTrue
        Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 0
    }

    It '生存しているプロセスを kill して終了を待つ' {
        Mock -ModuleName SnotraSmoke Stop-Process {}
        Stop-SnotraProcessAndWait -Process (New-FakeProcess) | Should -BeTrue
        Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 1 -ParameterFilter { $Id -eq 123 -and $Force }
    }

    It '期限内に終了しなければ throw せず $false を返す（finally から呼ぶため）' {
        Mock -ModuleName SnotraSmoke Stop-Process {}
        $result = $null
        { $result = Stop-SnotraProcessAndWait -Process (New-FakeProcess -WaitResult $false) `
            -TimeoutMs 10 -WarningAction SilentlyContinue } | Should -Not -Throw
        $result | Should -BeFalse
    }

    It 'WaitForExit が例外を投げても throw せず $false を返す（他人のプロセスのアクセス拒否）' {
        Mock -ModuleName SnotraSmoke Stop-Process {}
        $fake = [pscustomobject]@{ Id = 456 }
        $fake | Add-Member -MemberType NoteProperty -Name HasExited -Value $false
        $fake | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($ms) throw 'Access is denied' }
        $result = $null
        { $result = Stop-SnotraProcessAndWait -Process $fake -WarningAction SilentlyContinue } |
            Should -Not -Throw
        $result | Should -BeFalse
    }
}
```

- [x] **Step 2: 落ちることを確認する**

実行: `npm run test:powershell`
期待: 5 件が `CommandNotFoundException: Stop-SnotraProcessAndWait` で FAIL

- [x] **Step 3: ヘルパを実装する**

`SnotraSmoke.psm1` の `Resolve-SnotraExistingProcess`（`383`）の直前へ置く。

```powershell
<#
.SYNOPSIS
プロセスを停止し、**終了を待つ**（#872 単一インスタンス衝突）。

.DESCRIPTION
`Stop-Process -Force` は制御を即返す。`tauri_plugin_single_instance` が登録されているため、
先発がまだ生きたまま後発を起動すると、後発は先発へ通知して即終了する——`smoke-egui.ps1` が
**#755/#801 是正 B** として同じ機序を既に解いており、機序の正本はそちらのコメントである。

**throw しない。** 呼び出し点が `finally` を含み、`finally` からの throw は元の例外を覆い隠す。
終了を確認できなかったことは戻り値と警告で表し、**赤にする責務は呼び出し側が持つ**
（`Describe '実機配管'` の `AfterAll` と、次の It の `Resolve-SnotraExistingProcess -Policy Reject`）。

**引数に型を付けない。** 既存の単体検査は `Id` だけを持つ偽オブジェクトで方針分岐を固定して
おり、`[System.Diagnostics.Process]` を要求すると束縛で落ちる（実測）。
#>
function Stop-SnotraProcessAndWait {
    [CmdletBinding()]
    [OutputType([bool])]
    param(
        [Parameter(Mandatory)]
        [AllowNull()]
        $Process,
        [int]$TimeoutMs = 5000,
        # `Stop-Process` 自体のエラーを黙らせる。**既定は黙らせない**——`Policy Stop` は
        # #853 以来 `-ErrorAction` 無しで、アクセス拒否を呼び出し側へ上げていた（psm1:399）。
        [switch]$Quiet
    )

    if ($null -eq $Process) { return $true }
    if ($Process.HasExited) { return $true }

    if ($Quiet) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    } else {
        Stop-Process -Id $Process.Id -Force
    }

    try {
        if ($Process.WaitForExit($TimeoutMs)) { return $true }
    } catch {
        Write-Warning "pid=$($Process.Id) の終了待ちに失敗しました: $($_.Exception.Message)"
        return $false
    }
    Write-Warning "pid=$($Process.Id) が ${TimeoutMs}ms 以内に終了しませんでした（single-instance 衝突の恐れ）。"
    return $false
}
```

`Export-ModuleMember -Function @(`（`847`）の一覧へ `'Stop-SnotraProcessAndWait'` を足す。

- [x] **Step 4: 通ることを確認する**

実行: `npm run test:powershell`
期待: 新設 5 件が PASS

- [x] **Step 5: `Policy Stop` を経由させる**

`psm1:398-400` を置き換える。**`-Quiet` は付けない**（既存のエラーチャネルを保つ）。

```powershell
    foreach ($process in $existing) {
        # **終了を待つ**（#872）。待たずに返すと、呼び出し側が直後に起動する本体が
        # single-instance で先発へ通知して即終了し、trace を 1 行も書かないまま
        # 待ちが予算を使い切る（機序の正本は `smoke-egui.ps1` の #755/#801 是正 B）。
        [void](Stop-SnotraProcessAndWait -Process $process)
    }
```

- [x] **Step 6: 呼び出し側 2 箇所を移行する**

`Tests.ps1:380-382` と `Tests.ps1:501-503` の `if (...) { Stop-Process ... }` を、それぞれ次へ置き換える。**`-Quiet` を付ける**（従来 `-ErrorAction SilentlyContinue` だったため）。

```powershell
            [void](Stop-SnotraProcessAndWait -Process $proc -Quiet)
```

- [x] **Step 7: 衝突の検出を exit code の層へ上げる**

`Describe '実機配管'`（`344`）の末尾へ `AfterAll` を足す。**`finally` から throw しない設計のままで、検出だけを合否へ載せる。**

```powershell
    # **待ちきれなかった生き残りを、ここで赤にする**（#872）。各 It の `finally` は
    # `Write-Warning` しか出せない（`finally` からの throw は元の例外を覆い隠す）ため、
    # 検出点が無いと Pester 実行全体から生きた snotra.exe が漏れても誰も見ない。
    # `AfterAll` からの throw は It の例外を覆い隠さないので、ここが正しい層である。
    AfterAll {
        $leaked = @(Get-Process -Name 'snotra' -ErrorAction SilentlyContinue)
        if ($leaked.Count -gt 0) {
            $ids = $leaked.Id -join ', '
            $leaked | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
            throw "実機配管の後に snotra が残っています（pid=$ids）。終了待ちが効いていません。"
        }
    }
```

- [x] **Step 8: 遅着警告のノイズ源を止める**

`Tests.ps1:186` の It（`予算が尽きていても条件を必ず 1 度は評価する`）は `-TimeoutMs 0` で走るため、`予算 0ms を過ぎた評価で成立しました` を**毎回 1 件**出す。`SnotraSmoke.psm1:574` の本物の遅着警告と同一文言で、grep で区別できない（実測: 全 30 反復に出ていた）。

同じファイルの `:206` と `:224` に既にある書き方に揃え、その It の `Wait-SnotraTraceCondition` 呼び出しへ `-WarningAction SilentlyContinue` を足す。

- [x] **Step 9: 全体が通ることを確認する**

実行: `npm run test:powershell`
期待: 全件 PASS（既存 55 件 + 新設 5 件）

- [x] **Step 10: 故障注入で「効いている」ことを 1 度実測する**

`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」。**稼働中のガードは弱めない——複製に変異を当てる。**

`SnotraSmoke.psm1` を一時ディレクトリへコピーし、複製側の `Stop-SnotraProcessAndWait` から `WaitForExit` の行を消す。その複製を import して `実機配管` の 2 つの It を連続実行し、`Tests.ps1:441` の `Reject` が throw する（＝直した機序を意図的に再現できる）ことを確かめる。結果を本ファイル末尾の「実測ログ」へ書く。

**実施時の変更**: 変異は「待ちを外す」ではなく「**kill も待ちも行わない**」にした。前者はローカルの速さでは衝突が確率的にしか起きず、注入が空振りしうる。後者は「直前の It が生きたプロセスを残す」状況を決定的に作るので、**検出器（`Reject` と `AfterAll`）が鳴るかを測る**という目的にはこちらが適う。複製一式（`scripts/lib/*`）を temp へ写し、`Tests.ps1` が `$PSScriptRoot` から module を読む形をそのまま使った。

- [x] **Step 11（計画外・実装中に判明）: 既存の `Policy Stop` の fixture を更新する**

`Tests.ps1` の `Stop 方針では列挙した既存プロセスだけを停止する` は `Id` だけの偽オブジェクトを `Get-Process` の mock に返させていた。Step 5 で `Policy Stop` がヘルパを通るようになり、ヘルパは `HasExited` / `WaitForExit` を読むため、`Set-StrictMode -Version Latest` の下でメンバアクセスに落ちる。fixture へ両メンバを足し、It 名も `…停止し、終了を待つ` へ改めた。**この作業は計画の「設計上の制約」で予見していたが、作業項目としては書き落としていた。**

**コミット**: `fix(pester): 実機配管のプロセス停止に終了待ちを入れ、単一インスタンス衝突を止める (#872)`

---

## Task 2: 検索入力欄の widget 合成を切り出す（挙動不変）

kittest から駆動できる継ぎ目を作る。**このタスクでは挙動を変えない。**

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（`move_text_cursor_to_end` は `179`、切り出す区画は `578`〜`671` の `egui::Frame::new()...inner`）

**Interfaces:**
- Produces:
  ```rust
  pub(crate) struct SearchInputParams {
      pub(crate) input_id: egui::Id,
      pub(crate) restored_search: bool,
      pub(crate) window_focused: bool,
      pub(crate) input_editable: bool,
      pub(crate) inset: f32,
      pub(crate) field_height: f32,
      pub(crate) font: egui::FontId,
      pub(crate) text_color: egui::Color32,
  }

  pub(crate) fn search_input_ui(
      ui: &mut egui::Ui,
      buf: &mut String,
      params: &SearchInputParams,
      hint: impl FnOnce(&mut egui::Ui) -> String,
  ) -> egui::Response
  ```

**なぜこの形か**

- **`self.controller` に触れない。** `response.changed()` → `on_input_changed` は呼び出し側に残す。controller は `tauri::AppHandle` を持ち、単体検査では構築できない
- **`RuntimeFrame` に触れない。** 切り出す区画に `frame` の依存が 1 つも無いことを実測済み（`view.rs` が frame を使うのは `311` / `382` / `1046` の 3 箇所だけ）
- **hint はクロージャで受ける。** `HintPlan::Folder` の分岐は `ui.available_width()` と `ui.painter()` を**内側の Frame の中で**読む。文字列を先に計算して渡すと `available_width` が変わる。クロージャなら呼ばれる位置が同じなので挙動が変わらない

- [x] **Step 1: 関数を追加する**

`move_text_cursor_to_end`（`179`）の直後へ置く。中身は現在の `578`〜`671` の逐語移動である。

```rust
/// 検索入力欄の widget 合成（**controller にも `RuntimeFrame` にも触らない**）。
///
/// **3 つの順序がこの関数の内容そのものである**——キャレットの末尾同期（#840）と focus の
/// 要求（#872/#936）は、どちらも `TextEdit` の**構築前**でなければ同一フレームの文字イベントに
/// 効かない。`view.rs` の `mod tests` の kittest がこの並びを実コードごと縛る。
pub(crate) struct SearchInputParams {
    pub(crate) input_id: egui::Id,
    pub(crate) restored_search: bool,
    pub(crate) window_focused: bool,
    pub(crate) input_editable: bool,
    pub(crate) inset: f32,
    pub(crate) field_height: f32,
    pub(crate) font: egui::FontId,
    pub(crate) text_color: egui::Color32,
}

pub(crate) fn search_input_ui(
    ui: &mut egui::Ui,
    buf: &mut String,
    params: &SearchInputParams,
    hint: impl FnOnce(&mut egui::Ui) -> String,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    egui::Frame::new()
        .inner_margin(egui::Margin::same(params.inset.round() as i8))
        .show(ui, |ui| {
            if params.restored_search {
                move_text_cursor_to_end(&ctx, params.input_id, buf);
            }
            if params.window_focused
                && params.input_editable
                && !ctx.memory(|m| m.has_focus(params.input_id))
            {
                ctx.memory_mut(|m| m.request_focus(params.input_id));
            }
            let hint_text = hint(ui);
            ui.add_sized(
                egui::vec2(ui.available_width(), params.field_height),
                egui::TextEdit::singleline(buf)
                    .id(params.input_id)
                    .interactive(params.input_editable)
                    .font(params.font.clone())
                    .text_color(params.text_color)
                    .hint_text(egui::RichText::new(hint_text).font(params.font.clone())),
            )
        })
        .inner
}
```

- [x] **Step 2: 呼び出し側を置き換える**

`view.rs` の `578`〜`671` を次で置き換える。`input_id` は関数の外で作る（`focus_state` の trace が使う）。

```rust
        let input_id = ui.make_persistent_id("search_input");
        let params = SearchInputParams {
            input_id,
            restored_search,
            window_focused: pre.focused,
            input_editable,
            inset,
            field_height,
            font: bar_font.clone(),
            text_color: bar_theme.name_color,
        };
        let response = search_input_ui(ui, &mut buf, &params, |ui| match hint_plan {
            HintPlan::Tool => crate::egui_shell::ui_strings::tool_select_hint(l).to_string(),
            HintPlan::Search => crate::egui_shell::ui_strings::search_hint(l).to_string(),
            HintPlan::Folder(dir) if !buf_is_empty => {
                crate::egui_shell::ui_strings::folder_hint(l, dir)
            }
            HintPlan::Folder(dir) => {
                let avail = (ui.available_width() - TEXT_EDIT_HINT_H_MARGIN).max(0.0);
                let shown = crate::egui_shell::layout::fit_middle_by_measure(dir, avail, |cand| {
                    let text = crate::egui_shell::ui_strings::folder_hint(l, cand);
                    ui.painter()
                        .layout_no_wrap(text, bar_font.clone(), bar_theme.name_color)
                        .size()
                        .x
                });
                crate::egui_shell::ui_strings::folder_hint(l, &shown)
            }
        });
```

**注意**: クロージャは `buf` を借用できない（`search_input_ui` が `&mut buf` を取るため）。`HintPlan::Folder(dir) if !buf.is_empty()` のガードは、**呼び出し前に** `let buf_is_empty = buf.is_empty();` を計算して使う。

- [x] **Step 3: 挙動不変を確認する**

実行: `cargo clippy --workspace --all-targets -- -D warnings` と `cargo test -p snotra`
期待: green。**既存の `focus_requested_before_text_edit_applies_same_frame_input`（`1134`）も通ること**

- [x] **Step 4: 目視で回帰が無いことを見る**

実行: `npm run smoke:egui`
期待: green（`egui_show:done` → `egui_results:show` → `egui_hide:done`）

**実施時の変更（Task 6 Step 5 へ寄せた）**: `smoke-egui.ps1` の既定は `target/release/snotra.exe` で、手元のそれは **8/4 10:06 製＝本サイクルの変更も #938 も含まない**。そのまま走らせると「対象を含まない自明な green」になる（`AGENTS.md`「検証コマンドは『観測形が対象を含むか』まで測る」）。**release の作り直しは Rust の変更が出揃う Task 6 Step 5 で 1 度だけ行い、そこで smoke を実行する。** Task 2 の挙動不変は `cargo test -p snotra`（206 passed・#840 と #938 の検査を含む）が受け持つ。

**コミット**: `refactor(egui): 検索入力欄の widget 合成を frame 非依存の関数へ切り出す (#872)`

---

## Task 3: kittest でキャレットの並びを実コードごと縛る（L2 の格上げ）

#938 が受容した残余——「単体テストが縛るのは egui の意味論であって `update()` の並びではない」——を閉じる。

**Files:**
- Modify: `src-tauri/Cargo.toml`（`[dev-dependencies]` 節を新設）
- Modify: `src-tauri/src/egui_shell/view.rs`（`mod tests`）

**Interfaces:**
- Consumes: Task 2 の `search_input_ui` / `SearchInputParams`

- [x] **Step 1: dev 依存を足す**

`src-tauri/Cargo.toml` へ（`snotra-settings/Cargo.toml:23` と同じ指定）:

```toml
[dev-dependencies]
egui_kittest = { version = "0.35", default-features = false }
```

- [x] **Step 2: 失敗するテストを書く**

`view.rs` の `mod tests` へ足す。**両方向で固定する**——並びを戻したときに落ちることまで見る。

```rust
    use egui_kittest::Harness;

    /// kittest の state。**復元フラグをフレームごとに切り替える**ために buf と束ねる。
    struct CaretState {
        buf: String,
        restored: bool,
        window_focused: bool,
    }

    fn caret_harness(id: egui::Id, focused: bool) -> Harness<'static, CaretState> {
        Harness::new_ui_state(
            move |ui, st: &mut CaretState| {
                let params = SearchInputParams {
                    input_id: id,
                    restored_search: st.restored,
                    window_focused: st.window_focused,
                    input_editable: true,
                    inset: 0.0,
                    field_height: 20.0,
                    font: egui::FontId::proportional(12.0),
                    text_color: egui::Color32::WHITE,
                };
                let _ = search_input_ui(ui, &mut st.buf, &params, |_| String::new());
            },
            CaretState {
                buf: "alpha".to_owned(),
                restored: false,
                window_focused: focused,
            },
        )
    }

    /// 復元フレームで、**同一フレームに載っていた**文字が末尾へ入る（#840/#872）。
    ///
    /// **`step()` を使う（`run()` ではない）。** `run()` は再描画要求が尽きるまで複数フレーム
    /// 回すため、文字が 2 フレーム目で入っても通ってしまい、この検査の主題（同一フレーム）が
    /// 骨抜きになる。`step()` は「キューされた各イベントにつき 1 フレーム、イベントが無ければ
    /// 1 フレーム」である（`egui_kittest-0.35` の `Harness::step` doc・一次資料で確認済み）。
    #[test]
    fn restored_frame_appends_same_frame_input_at_end() {
        let id = egui::Id::new("search_input");
        let mut harness = caret_harness(id, true);
        // 復元より前に 2 フレーム回して、focus と TextEdit の state を確立する
        // （`move_text_cursor_to_end` は `TextEdit::load_state` が None の間は何もしない）。
        harness.step();
        harness.step();

        // 復元フレーム: restored=true と文字を**同じフレーム**へ載せる。
        // 文字は本体と同じ経路で渡す（runtime は WM_CHAR / IME 確定を Ime(Commit) にする）。
        harness.state_mut().restored = true;
        harness
            .input_mut()
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Commit("z".to_owned())));
        harness.step();

        assert_eq!(
            harness.state().buf.as_str(),
            "alphaz",
            "復元フレームに載った打鍵は復元クエリの末尾へ入る"
        );
    }

    /// focus を要求しなければ同じ文字が捨てられる（この検査が本当に focus を見ている証拠）。
    #[test]
    fn without_focus_request_the_same_input_is_dropped() {
        let id = egui::Id::new("search_input");
        let mut harness = caret_harness(id, false); // focus 要求の条件を落とす
        harness.step();
        harness.step();

        harness.state_mut().restored = true;
        harness
            .input_mut()
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Commit("z".to_owned())));
        harness.step();

        assert_eq!(
            harness.state().buf.as_str(),
            "alpha",
            "焦点が無ければ文字は入らない"
        );
    }
```

- [x] **Step 3: 落ちることを確認する**

実行: `cargo test -p snotra restored_frame_appends -- --nocapture`
期待: `restored_frame_appends_same_frame_input_at_end` が **`zalpha`（キャレットが先頭のまま）で FAIL** する見込み——ただしこれは Task 2 が既に正しい並びを持っているため、**実際には最初から PASS しうる**。その場合は Step 5 の故障注入が「この検査が本当に discriminate するか」の唯一の証拠になるので、**Step 5 を省略しない**。

`egui_kittest` 0.35 の API はレジストリの一次資料で確認済み（`new_ui_state` / `step` / `input_mut` / `state` / `state_mut` がすべて実在）。`Harness<'static, State>` の形は `snotra-settings/src/app.rs:818` と同じ。

- [x] **Step 4: 通ることを確認する**

実行: `cargo test -p snotra`
期待: 2 件とも PASS

- [x] **Step 5: 故障注入で検出力を実測する**

`search_input_ui` の中で `move_text_cursor_to_end` の呼び出しを `TextEdit` の**後ろ**へ動かす。`restored_frame_appends_same_frame_input_at_end` が落ちることを確かめてから戻す。結果を「実測ログ」へ書く。

**実施時の発見（このステップを省いていたら気づけなかった）**: **1 回目の注入は落ちなかった。** focus 直後の egui のキャレットは既に末尾に在り、`move_text_cursor_to_end` が no-op になるため、呼び出しを後ろへ動かしても結果が変わらない。つまり**初版の検査はキャレットの並びを縛れていなかった**（縛れていたのは focus だけで、それは対照検査が既に見ている）。実経路の `restored_search` は**バッファ全体が置き換わった**フレームであり、残るキャレットは古い（短い）テキストの位置を指す——復元フレームの前に `TextEditState` でキャレットを先頭へ置く 1 段を足して、その状態を作った。2 回目の注入は `left: "zalpha" / right: "alphaz"` で落ちた。

**コミット**: `test(egui): キャレットと focus の並びを kittest で実コードごと縛る (#872/#936)`

---

## Task 4: 実機配管を focus_state 1 点へ縮める（L3）

**Files:**
- Modify: `scripts/lib/SnotraSmoke.Tests.ps1`（キャレットの It は `386`〜`515`）

**縮小の内容**

```
現行  Resolve → Start → WaitWindow → index.bin 待ち → SetForeground
      → A/L/P/H/A → 待ち(5s) → Right → A/A → 待ち(5s) → Escape → z → 待ち(5s)

縮小  Resolve → Start → WaitWindow → SetForeground → focus_state を待つ → has_focus == true
```

- **前面化は残す。** focus 要求は `pre.focused` に条件づけられており、前面が取れなければ `has_focus` は真にならない。#890 以降 30 反復で前面化の失敗は 0 件
- **`index.bin` と `[config]` の検査は落とす。** 直前の It（`345`）が同じ断言を持つ
- 打鍵を注入しないので `$pressKey` と `$waitForInputChange` は不要になる

- [x] **Step 1: It を書き換える**

`386` の It 全体を次で置き換える。名前も内容に合わせる。

```powershell
    It '起動後の最初のフレームで入力欄が打鍵を受け取れる状態になっている' {
        # **この It が守るのは L3（実プロセス層）だけである。** キャレットの断言は
        # `view.rs` の kittest が実コードごと縛る（#872 の機序再設計）。ここに打鍵を
        # 注入しないのは、注入と 3 段の待ちが 7 か月の間欠失敗の構造的前提そのもの
        # だったからである（#872 本文の要素 1・2）。
        #
        # `egui_input:focus_state` は #938 が**この回帰の検出器として**置いたもので、
        # 偽に戻れば起動直後の打鍵が再び捨てられている（機序の正本は `view.rs` の
        # 当該コメント）。
        $profile = Join-Path $TestDrive 'caret-profile'
        $stderr = Join-Path $TestDrive 'caret.err'
        $created = New-SnotraVerificationProfile -ProfileDir $profile -ShowIcons $false `
            -GeneralSection @'
show_on_startup = true
auto_hide_on_focus_lost = false
'@
        $proc = $null
        try {
            Resolve-SnotraExistingProcess -Policy Reject
            $proc = Start-SnotraProcess -ConfigDir $created.FullPath -Trace `
                -FilePath $env:SNOTRA_PESTER_EXE -StandardErrorPath $stderr
            $hwnd = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs 30000
            Set-SnotraForegroundWindow -Handle $hwnd | Should -BeTrue

            $focus = Wait-SnotraTraceCondition -Path $stderr -TimeoutMs 30000 -PollMs 100 `
                -AbortIfExited $proc -Description 'egui_input:focus_state（最初のフレーム）' `
                -Predicate { $_.event -eq 'egui_input:focus_state' }
            $focus | Should -Not -BeNullOrEmpty
            $focus.data.has_focus | Should -BeTrue
        } catch {
            Write-Host '--- caret integration stderr trace ---'
            if (Test-Path -LiteralPath $stderr) {
                @(Get-Content -LiteralPath $stderr) | ForEach-Object { Write-Host $_ }
            } else {
                Write-Host "(stderr file not found: $stderr)"
            }
            Write-Host '--- end caret integration stderr trace ---'
            throw
        } finally {
            [void](Stop-SnotraProcessAndWait -Process $proc -Quiet)
        }
    }
```

**予算を 30,000ms にする理由**: 待ちは 1 つだけになり、順序依存が無い。実測でフレーム不回転は 24.3 秒（1/30）まで観測されている——**予算を広げても隠れる退行は無い**（この待ちは「フレームが 1 度でも回ったか」しか見ないため、遅さそのものが判定に混ざらない）。

- [x] **Step 2: 撤去した env フックを消す**

`SNOTRA_PESTER_FAILURE_GRACE_MS` と `SNOTRA_PESTER_TRACE_DIR` の分岐は、打鍵の遅着を測るための足場だった。**縮小版には待ちが 1 つしか無く、遅着と喪失を分ける問いも消えた**ので、上の書き換えで一緒に落ちている。`scripts/repro-pester-flake.ps1` の `.NOTES` が撤去対象として名指ししているので、**そちらの撤去は #872 / #936 を閉じるときに一括で行う**（このタスクでは触らない）。

- [x] **Step 3: 通ることを確認する**

実行: `cargo build -p snotra` の後 `npm run test:powershell`
期待: 全件 PASS。**この It の所要が現行の 16〜24 秒から数秒へ落ちること**を目視で確認する

- [x] **Step 4: 故障注入で検出力を実測する**

`view.rs` の focus 要求の条件を落とす（`params.window_focused &&` を `false &&` にする）。この It が `has_focus` = false で落ちることを確かめてから戻す。結果を「実測ログ」へ書く。

**コミット**: `test(pester): 実機配管を focus_state 1 点へ縮め、打鍵注入と 3 段の待ちを外す (#872)`

---

## Task 5: 空文字の env が trace ハッチを点灯させるのを止める

測定ハーネスが `SNOTRA_EGUI_INPUT_TRACE` を空文字で漏らし、**2 反復目以降の全測定が計器つきで走っていた**（実測 26/27）。読み手側を直す。

**Files:**
- Create: `snotra-egui-runtime/src/env.rs`
- Modify: `snotra-egui-runtime/src/lib.rs`（`mod env;`）
- Modify: `snotra-egui-runtime/src/{input,renderer,repaint,runtime,windows_ime}.rs`（7 箇所）
- Modify: `snotra-egui-runtime/CLAUDE.md`（モジュール索引）

**Interfaces:**
- Produces: `pub(crate) fn trace_hatch_enabled(name: &str) -> bool`

**厳しい許可リスト（`src-tauri/src/trace.rs` の `env_flag`）へ寄せない理由**

- PowerShell 側の読み手（`SnotraSmoke.psm1:664`）は緩いまま残る。`=0` では現在**両者 ON で一貫**しているのに、許可リストへ寄せると PS が真・Rust が偽の**新しい食い違い**が生まれる
- `renderer.rs:76` は `paint()` の中＝毎フレーム。`env_flag` は `var` + `trim().to_ascii_lowercase()` で ON 時の割り当てが 1→2 に増える。直後のコメントが「計器が測定対象を汚さない」ことを設計意図として明記している
- 実バグは**空文字ちょうど**である。手本は `snotra-core/src/config.rs` の `config_dir_from`（`var_os` + `!is_empty()`・rustdoc に理由あり）

- [x] **Step 1: 失敗するテストを書く**

`snotra-egui-runtime/src/env.rs` を新規作成する。

```rust
//! trace ハッチ（`SNOTRA_EGUI_*_TRACE`）の env 述語。
//!
//! **空文字を「未設定」として扱う唯一の場所である。** `var_os(..).is_some()` は `Some("")` を
//! 真と読むため、値を消したつもりの空文字で計器が点く。PowerShell の
//! `[Environment]::SetEnvironmentVariable($name, $null, 'Process')` は変数を消さず**空文字で
//! 作る**ので、この経路は実際に踏まれた（#872: 測定ハーネスが 26/27 反復を計器つきにしていた）。
//!
//! **`src-tauri/src/trace.rs` の `env_flag`（`1|true|yes|on` の許可リスト）へは寄せない。**
//! こちらのハッチは PowerShell 側にも緩い読み手（`scripts/lib/SnotraSmoke.psm1` の
//! `Send-SnotraKey`）が居り、許可リストにすると `=0` 系で新しい食い違いが生まれる。
//! 同じ「空文字は未設定」の判断は `snotra-core/src/config.rs` の `config_dir_from` にもある。

/// 判定核（env を読まないので並列テストから安全に、網羅的に測れる）。
///
/// 判定核を分ける形は `snotra-core` の `config_dir_from` と同じ流儀である——edition 2024 では
/// `std::env::set_var` が `unsafe` であり、env を触るテストは並列実行とも噛み合わない。
fn is_enabled(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

/// trace ハッチが有効か。**空文字は未設定として扱う。**
pub(crate) fn trace_hatch_enabled(name: &str) -> bool {
    is_enabled(std::env::var_os(name).as_deref())
}

#[cfg(test)]
mod tests {
    use super::is_enabled;
    use std::ffi::OsStr;

    #[test]
    fn empty_value_is_treated_as_unset() {
        assert!(!is_enabled(None), "未設定は偽");
        assert!(!is_enabled(Some(OsStr::new(""))), "空文字は偽（#872 の実バグ）");
    }

    #[test]
    fn any_non_empty_value_enables() {
        for v in ["1", "0", "true", "false", "on", "verbose"] {
            assert!(is_enabled(Some(OsStr::new(v))), "{v} は真（許可リストにしない）");
        }
    }
}
```

- [x] **Step 2: 落ちることを確認する**

実行: `cargo test -p snotra-egui-runtime env::`
期待: `mod env` が宣言されていないため**コンパイルエラー**

- [x] **Step 3: モジュールを宣言し、7 箇所を移行する**

`lib.rs` へ `mod env;` を足す。次の 7 箇所を `crate::env::trace_hatch_enabled("<名前>")` へ置き換える。**`input.rs` の `OnceLock` によるキャッシュは残す**（述語だけ差し替える）。

| ファイル:行 | env 名 |
|---|---|
| `input.rs:34` | `SNOTRA_EGUI_INPUT_TRACE` |
| `renderer.rs:76` | `SNOTRA_EGUI_PAINT_TRACE` |
| `repaint.rs:197` | `SNOTRA_EGUI_WAKE_TRACE` |
| `runtime.rs:279` | `SNOTRA_EGUI_WAKE_TRACE` |
| `runtime.rs:456` | `SNOTRA_EGUI_REPAINT_TRACE` |
| `windows_ime.rs:100` | `SNOTRA_EGUI_IME_TRACE` |
| `windows_ime.rs:209` | `SNOTRA_EGUI_IME_TRACE` |

`snotra-egui-runtime/CLAUDE.md` の「モジュール構成」へ 1 行足す:

```
- `env.rs`: trace ハッチの env 述語（空文字を未設定として扱う唯一の場所・#872）
```

- [x] **Step 4: 通ることを確認する**

実行: `cargo clippy --workspace --all-targets -- -D warnings` と `cargo test -p snotra-egui-runtime`
期待: green。**`var_os(...).is_some()` が 0 件になったことを `grep -rn "var_os(" snotra-egui-runtime/src` で確認する**

- [x] **Step 5: 空文字を直接注入して両方向を実測する**

`.claude/rules/safety-nets.md`「**故障注入は回復機構ごと巻き戻して行う**」——PowerShell 側の復元を直さない以上、空文字は依然として作られうる。それを直接注入して測る。

```powershell
$env:SNOTRA_EGUI_INPUT_TRACE = ''
# 本体を起動し、stderr に SNOTRA_EGUI_INPUT 行が 0 件であることを確認
$env:SNOTRA_EGUI_INPUT_TRACE = '1'
# 同じく起動し、行が出ることを確認（両方向）
```

結果を「実測ログ」へ書く。

**コミット**: `fix(egui): 空文字の env が trace ハッチを点灯させるのを止める (#872)`

---

## Task 6: 記録を実態へ合わせる

**Files:**
- Modify: `PERFORMANCE.md`（計器の一覧は `250-253`）
- Modify: `docs/build-commands.md`（実機配管の記述は `178`、`smoke-egui.ps1` の関連は `180`）
- Modify: `scripts/smoke-egui.ps1`（`139` の `Start-Sleep 300`）

- [x] **Step 1: `PERFORMANCE.md` の計器一覧を直す**

この節は「**このリストが計器の正本である**」と自称しながら、5 名前のうち 3 つしか載せていない（`SNOTRA_EGUI_INPUT_TRACE` と `SNOTRA_EGUI_IME_TRACE` が欠落）。受理値にも触れていない。2 名前を足し、次の 1 文を置く。

```
**値は空でなければ何でもよい（空文字は「未設定」として扱う・#872）。** `SNOTRA_TRACE` だけは
別の意味論（`1|true|yes|on` のみ・`src-tauri/src/trace.rs`）である。
```

- [x] **Step 2: `smoke-egui.ps1` の冗長な固定待ちを消す**

`139` の `Start-Sleep -Milliseconds 300` は `Policy Stop` に対する事実上の待ちだった（#853 以前からの逐語の持ち越し）。Task 1 で関数の内側に本物の待ちが入るため、説明のつかない固定遅延として残る。削除し、`138` のコメントへ「終了待ちは `Resolve-SnotraExistingProcess` が持つ（#872）」を足す。

- [x] **Step 3: `docs/build-commands.md` を実態へ合わせる**

`178` の実機配管の説明から「フォルダ復帰後の次打鍵が復元クエリの末尾へ入ることを統合検査する（#840・#843）」を、次の趣旨へ書き換える（**面積を増やさず既存文を置き換える**）。

> 起動後の最初のフレームで入力欄が打鍵を受け取れる状態になっていることを実機で検査する（#872）。キャレットの断言は `src-tauri/src/egui_shell/view.rs` の kittest が実コードごと縛る。

- [x] **Step 4: ガバナンス検査**

実行: `npm run governance:check`
期待: green（検査 18 件）

- [x] **Step 5: 全体の検証**

実行: `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` / `npm run test:powershell` / `npm run smoke:egui`
期待: すべて green

**コミット**: `docs: 計器の正本と実機配管の記述を実態へ合わせる (#872)`

---

## 実測ログ（実装中に埋める）

| 何を測ったか | タスク | 結果 |
|---|---|---|
| 待ちを外した複製で衝突が再現するか | 1-10 | **再現した（両方向）。** 変異（kill も待ちもしない複製）→ キャレット It が `RuntimeException: Snotra が既に起動しています（pid=20544）` で 77ms で失敗（**CI の iter-008/021/029 と逐語一致**）、加えて新設 `AfterAll` が `実機配管の後に snotra が残っています` で発火。修正版 → 統合 2/2 pass・`AfterAll` 沈黙 |
| `move_text_cursor_to_end` を後ろへ動かすと kittest が落ちるか | 3-5 | **1 回目は落ちなかった**（focus 直後のキャレットが既に末尾で no-op ゆえ、検査が縛れていなかった）。復元フレームの前にキャレットを先頭へ置く 1 段を足して再注入 → **`left: "zalpha" / right: "alphaz"` で落ちた**。戻して 208 passed |
| focus 要求を落とすと縮小した実機配管が落ちるか | 4-4 | **落ちた。** focus 要求の条件を反転した debug ビルドで `has_focus:false` が 5 フレームとも記録され `Expected $true, but got $false`。復元後 76 passed。**所要は 2.56s → 491ms**（CI runner では従来 16〜24 秒） |
| 空文字/`1` の両方向で計器が切り替わるか | 5-5 | **両方向 OK。** 空文字 → `SNOTRA_EGUI_INPUT` 行 **0**、`1` → **13**（`[trace]` は両方 7 行でアプリは正常動作）。PowerShell 側の復元は直していないので、空文字は依然として作られうる状態での注入である |

---

## 未確定（実装前に潰す）

- [x] **U-A** — `Set-SnotraForegroundWindow` を外せるか。**残す判断で確定**（設計書）。focus 要求が `pre.focused` に条件づけられており、アプリの `set_focus()` だけで OS が前面を渡すかは未実測。外す試みは follow-up
- [x] **U-B** — フレーム不回転（実測 1/30）。**縮小版でも残ることを受容**。ただし断言の失敗ではなく 1 回の待ちの時間切れとして現れる
- [x] **U-C** — `egui_kittest` 0.35 の API。**解決**（レジストリの一次資料で確認）。`new_ui_state` / `step` / `input_mut` / `state` / `state_mut` はすべて実在する。**`run()` ではなく `step()` を使う**——`run()` は再描画要求が尽きるまで複数フレーム回すため、「同一フレーム」という検査の主題が骨抜きになる（`step` の doc: *Run a frame for each queued event (or a single frame if there are no events)*）

## PR 本文のチェックリストへ転記するもの（CI が要る／散文では蒸発する）

**ここでは意図的にチェックボックスを使わない。** この計画が所有しない作業をチェックボックスで
置くと、plan.md に未チェックの `- [ ]` が恒久的に残り、`gh pr create` のガード（#749）を
永久にブロックする。PR を立てるときに本文へ転記すること。

1. `rust-check` の log に `Stop-SnotraProcessAndWait` の警告が実際に出るか（`finally` 内の `Write-Warning` の扱いは未実測。CI の実測は PR が在って初めて行える）
2. #872 へコメント: 残余の再分類（衝突 / 遅着 / フレーム不回転）・計器汚染・機序の再設計
3. #786 へコメント: `smoke-startup.ps1` の 5 回ループが同型（`Policy Stop` + 固定 120ms）である候補機序
4. #872 / #936 を閉じるときに `repro-pester-flake.ps1` 一式を撤去（撤去対象の正本は同ファイルの `.NOTES`）

## 受容する残余（名指し）

- **終了が 5,000ms 以内に収まる範囲のシャットダウン退行は吸収され、検査は緑のまま通る。** 完全な漏れは `AfterAll` が捕まえるが、遅くなったこと自体の読み手は置いていない
- **`SnotraSmoke.psm1:664` の PowerShell 側の読み手は緩いまま残る。** 空文字では両者とも偽で一致するので、実バグの経路は塞がる

---

## code-reviewer の指摘への対応（Step 4b・実装後）

**Critical 2 件はいずれも「本サイクルの中心的主張が立たない」もので、自分で実測して確定させてから直した。**

- [x] **C1 — 縮めた実機配管が、自ら名乗る回帰を検出しなかった。** `Wait-SnotraTraceCondition` は一致の**最後**を返す（`psm1` の `Select-Object -Last 1`）。#938 の回帰は「frame 1 だけ偽・frame 2 以降は真」で、`focus_state` は show ごと 5 行出るので、**回帰した実装でも最後の行は真＝ PASS**（合成 trace で実測: seq=3 / True）。Task 4 Step 4 の注入は 5 行とも偽にする**本来の回帰より強い変異**だったため素通りしていた。**修正**: `Read-SnotraTraceSnapshot` で `window_focused` が真の**最初の**行に断言し、その部分集合が空でないことを別に断言する（省くと主語ゼロで自明に緑）。合成 trace 3 条件で検算（回帰 → FAIL / 正常 → PASS / 主語ゼロ → FAIL）
- [x] **C2 — kittest も focus の並びを縛れていなかった。** 判定フレームより前に `Harness` が既にフレームを走らせており、focus 要求が `!has_focus` ガードで走らない状態で測っていた（注入しても通ることを実測）。**修正**: `kittest_first_frame_requests_focus_before_text_edit` を追加し、判定フレームの直前に `surrender_focus` で焦点を手放す。再注入で `left: "alpha" / right: "alpha"` で落ちることを確認
- [x] **H1 — `AfterAll` の `Get-Process -Name snotra` がグローバルだった。** 開発者の実インスタンスを予告なく Force kill し、しかも「終了待ちが効いていません」という誤った診断で赤くする。`.Path` が検査対象の実行ファイルと一致するものだけに絞った
- [x] **M1 — `repro-pester-flake.ps1` の env 2 つが読み手を失ったまま「効く」と書かれていた。** Task 4 で読み手が消えた事実を doc へ書いた
- [x] **M3 — 旧テストの doc が `input_id` を作った直後のコメントを正本として指していた。** 切り出しでその位置が動いたため `search_input_ui` の中を指すよう直し、受容残余が #872 で閉じたことも書いた
- H2（落ちた断言の棚卸し）と M2（`smoke-startup.ps1` の同型）は**指摘どおりで対応不要**——前者は判断の裏取り、後者は範囲外として #786 へ送出済み

**得られた規律**（設計書の「検証」節へ恒久記録した）: **故障注入は、本来の回帰より強い変異にしてはならない。** 同じ型の誤りをこのサイクルで 3 回踏んだ。注入が赤くなったことは、検査が当の回帰を捕まえる証拠にならない。

## 人間レビュー

- [x] 承認済み — 2026-08-05 / 問い: "`workspace/plan.md` をご確認のうえ、注釈を加えるか承認してください。**承認前は実装へ渡しません**（承認後に workspace をコミットします）。" / 回答: "承認、/implement で進めて"

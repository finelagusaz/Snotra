# code-review: `chore/872-residual-triage`（実装レビュー）

レビュー対象: コミット `70b5166`（`main..HEAD` の 7 コミット）＋ 未コミットの Task 6 分（作業ツリー）。
作業ツリーの取得時刻: 2026-08-05 14:49 JST。指紋（sha256 先頭 16 桁）:
`SnotraSmoke.Tests.ps1` = `80369938cd7b888c` / `view.rs` = `eb9c9b21774aa05f` / `smoke-egui.ps1` = `297a069214fa5c1b`。

> **注意（並行編集）**: レビュー中に `scripts/smoke-egui.ps1` が変更された（mtime 14:41:27・
> `Stop-SnotraProcessAndWait` との意図的な非 DRY を説明する 5 行のコメント追加）。差分は取り直して
> 上の指紋の状態で読んでいるが、`AGENTS.md`「サブエージェント委譲と worktree」の
> 「検査対象を変更しながら検査を走らせない」に反する状況だったことを記録する。

## 私が実際に実行した検証

| コマンド | 結果 |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | green（exit 0） |
| `cargo test -p snotra` | **208 passed / 0 failed**（新設 kittest 2 件を含む） |
| `npm run governance:check` | green（検査 18 件・ADR 31 本の名前検査を含む） |
| `grep -rn "var_os(" snotra-egui-runtime/src` | **`env.rs` の 1 件のみ**＝移行は完全。`env.rs` の「唯一の場所」と ADR の「7 箇所」は成立 |
| `grep -n "SNOTRA_EGUI\|focus_state" SPEC.md` | 0 件 → Phase 2e は **SPEC 対象外** |
| `Wait-SnotraTraceCondition` の返り値の実測（下記 C1） | **最後の一致を返す**ことを偽 trace で確認 |

Pester（`npm run test:powershell`）は実行していない——実機配管が前面を奪ってユーザーのデスクトップを
占有するため。実行していないことを明記する。

---

## Critical

### C1. 縮めた実機配管は、自ら「回帰検出器」と名乗る #938 の回帰を検出しない

**場所**: `scripts/lib/SnotraSmoke.Tests.ps1:467-518`（新 It）／ 機序は `scripts/lib/SnotraSmoke.psm1:635`

**壊れた不変条件**: 「`egui_input:focus_state` が偽に戻れば起動直後の打鍵が再び捨てられている」——
この含意が成り立つのは**最初のフレームの行**についてだけなのに、検査は最後の行を読んでいる。

`Wait-SnotraTraceCondition` は一致した事象のうち **`Select-Object -Last 1`** を返す（`psm1:635`）。
一方 #938 の欠陥の現れ方は `view.rs:234-238` が実測として記している通り
**「frame 1 が `has_focus=false`、frame 2 から真」**である。`focus_state` は show ごとに 5 フレーム分
出る（`view.rs:99/114/419/740`）ので、回帰した実装の trace は

```
seq1 has_focus=false   ← 唯一の証拠
seq2..seq5 has_focus=true
```

となり、検査が読むのは `seq5` = true → **PASS**。実測（`SnotraSmoke.psm1` を import し、上記の形の
偽 trace を与えた）:

```
returned seq = 3 / has_focus = True
```

Task 4 Step 4 の故障注入がこれを見逃したのは、変異が `params.window_focused &&` → `false &&`
＝**5 フレームとも偽**という、本来の回帰より強い形だったためである（実測ログの「has_focus:false が
5 フレームとも記録され」がその証拠）。**注入が強すぎると、検出力の穴はそのまま通過する。**

**修正案**（`.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」に沿う形）:

```powershell
# 待つのは従来どおり（1 件出れば十分）。
$focus = Wait-SnotraTraceCondition -Path $stderr -TimeoutMs 30000 -PollMs 100 `
    -AbortIfExited $proc -Description 'egui_input:focus_state（最初のフレーム）' `
    -Predicate { $_.event -eq 'egui_input:focus_state' }
$focus | Should -Not -BeNullOrEmpty

# **断言は「窓が OS focus を得た最初のフレーム」に対して行う**——これが view.rs の条件
#（window_focused && input_editable && !has_focus を TextEdit 構築前に評価する）と同じ主語である。
$events = @((Read-SnotraTraceSnapshot -Path $stderr).Events |
    Where-Object { $_.event -eq 'egui_input:focus_state' -and $_.data.window_focused })
$events.Count | Should -BeGreaterThan 0   # ← **空集合で vacuous に通るのを塞ぐ**
$events[0].data.has_focus | Should -BeTrue
```

`Count -gt 0` の 1 行を省いてはならない。`focus_state` の予算は 5 フレームで、show の外では
再武装しない（`view.rs:419`）ため、OS focus が 6 フレーム目以降に来ると主語が空になり、
`ForEach-Object` で全件に断言する形は**対象ゼロで自明に緑**になる（→ ⚠️W1 も参照）。

### C2. kittest も同じ半分しか縛れていない疑いが濃い（focus の並びは縛れていない）

**場所**: `src-tauri/src/egui_shell/view.rs:1221-1281`（`caret_harness` /
`kittest_restored_frame_appends_same_frame_input_at_end`）

**根拠となる不変条件**: `search_input_ui` の focus 要求は
`!ctx.memory(|m| m.has_focus(params.input_id))` でガードされている（`view.rs:247-252`）。
`caret_harness` は復元フレームの**前に `step()` を 2 回**回す（`view.rs:1259-1260`）ので、
3 フレーム目の時点で widget は既に focus を持ち、**focus 要求そのものが走らない**。

ゆえに focus 要求ブロックを `ui.add_sized(...)` の**後ろ**へ動かしても、frame 1 で遅れて要求 →
frame 2 で focus 確立 → frame 3（復元 + `Ime(Commit)`）は焦点つきで走るため、
結果は `"alphaz"` のまま＝**テストは通る**と読める。これは Task 3 Step 5 の 1 回目の注入が
空振りしたのと**同じ機序**（暖機フレームが初回だけの欠陥を隠す）であり、計画は
`move_text_cursor_to_end` の側だけ再注入して、`request_focus` の側は一度も注入していない。

裏づけ（一次証拠・いずれも本差分の中）:

- `view.rs:234-238`: 「frame 1 が `has_focus=false`、frame 2 から真。**再 show では `Memory` に
  残るので初回だけ**」——欠陥は焦点が未確立のフレームにしか現れない
- 旧検査 `focus_requested_before_text_edit_applies_same_frame_input`（`view.rs:1195/1208`）は
  並びごとに**新しい `egui::Context`** を使っている。暖機した Context では差が出ないからである

**ゆえに、C1 と合わせると #938 の再導入は L2（kittest）でも L3（実機配管）でも捕まらない。**
`view.rs:203-213` の doc が言う「`move_text_cursor_to_end` と `request_focus` の**どちらか**が
`TextEdit` の後ろへ動けば落ちる」は、後者について成り立たない主張である（`AGENTS.md`
「全称表現は前提条件とセットで書く」）。

**確認方法（実装者が 1 回だけ走らせる）**: `search_input_ui` の focus 要求ブロック
（`view.rs:247-252`）を `ui.add_sized(...)` の後ろへ移し、`cargo test -p snotra kittest_`。
落ちなければ本項は確定である。

**修正案**: focus の並びだけを縛る検査を 1 本足す——**暖機フレームを持たない harness**
（`window_focused: true` / `restored: false`）で、最初の `step()` に `Ime(Commit)` を載せ、
文字が入ることを断言する。**期待文字列は推測せず 1 度測ってから書くこと**（新規 `TextEdit` の
初期キャレット位置は `"alphaz"` とは限らない・`AGENTS.md`「判定の中核は自分で測る」）。
その検査は、focus 要求を後ろへ動かした注入で落ちることまで確かめて初めて意味を持つ。

---

## High

### H1. `AfterAll` の `Get-Process -Name snotra` はグローバルで、開発者の実インスタンスを巻き込む

**場所**: `scripts/lib/SnotraSmoke.Tests.ps1:534-542`

2 つの害がある:

1. **開発機で動いているユーザー自身の Snotra を、予告なく `Stop-Process -Force` する。**
   従来この Describe が持っていた破壊的操作は「自分が起動したプロセスの kill」だけだった
2. **診断が誤りになる。** 先行インスタンスがある実行では、最初の It の
   `Resolve-SnotraExistingProcess -Policy Reject`（`:435`）が先に throw して赤になるのに、
   `AfterAll` はそれを「実機配管の後に snotra が残っています（…）。**終了待ちが効いていません**」
   と報告する。読み手は無実の `Stop-SnotraProcessAndWait` を疑いに行く

**修正案**: Describe が起動した pid を `$script:` スコープに集めて突き合わせる。最小でも
`$_.Path -eq $env:SNOTRA_PESTER_EXE` で絞り、文言を「この Describe が起動したプロセスが
残っています」に改める。

### H2. 検出力の喪失の棚卸し（旧検査が見て新検査が見ないもの）

依頼にあった列挙。「別の検査が見ている」か「受容する残余」かの判定つき。

| 落ちた断言 | 受け皿 | 判定 |
|---|---|---|
| Escape で `restore_query` / `restore_results` / `restore_selected` が戻る（L1） | `search_state.rs` の単体検査 | **別の検査が見ている**（設計書の分類どおり） |
| 復元フレームの同一フレーム打鍵が末尾へ入る（L2・キャレット） | `view.rs` の kittest | **見ている**（`move_text_cursor_to_end` の並びについては注入で実測済み） |
| 同上（L2・focus の並び） | — | **C2 のとおり、どこも見ていない可能性が高い** |
| 起動直後の最初のフレームで打鍵を受け取れる（L3） | 縮小した実機配管 | **C1 のとおり、最後のフレームを見ており主題を外している** |
| OS の打鍵がアプリへ届く配線（`SendInput` → tao → egui） | `smoke-egui.ps1` | **別の検査が見ている（CI 発火まで確認）**。`e2e.yml` は `pull_request` かつ `paths` に `src-tauri/**` / `scripts/lib/**` / `scripts/smoke-egui.ps1` を含むので、本 PR では確実に走る |
| `index.bin` が意図したプロファイルへ生成される | 直前の It（`:451-457`） | **成立**（同 It が `Test-Path $indexPath` の断言を持つことを実測） |
| `[config] ` 不在（seed が読めた肯定的証拠） | 直前の It（`:446`） | **成立**（同 It が `Should -Be 0` を持つ）。ただし**両 It は別プロファイルを使う**ので、caret-profile 側で `SNOTRA_CONFIG_DIR` が効かなかった場合の直接の受け皿は消えた。実害は小さい（実 config なら `show_on_startup` 既定 false で `Wait-SnotraWindow` が時間切れ）が、**失敗の理由は読みにくくなる**——受容する残余として名指しするのが正しい |
| 打鍵から `egui_input:changed` までの遅着 / 喪失の切り分け | — | **受容する残余**（設計書が明示。ただし ⚠️W3・M1 を参照） |

---

## Medium

### M1. 読み手を失った env フックが、まだ「効く」と書かれたまま動いている

**場所**: `scripts/repro-pester-flake.ps1:9-10, 24-30, 122-127` / `.github/workflows/pester-flake-repro.yml:20-27`

Task 4 が `SNOTRA_PESTER_TRACE_DIR` と `SNOTRA_PESTER_FAILURE_GRACE_MS` の読み手（Tests.ps1 の
`catch` / `finally`）を消したため、この 2 つは**設定されるが誰も読まない**。にもかかわらず:

- `repro-pester-flake.ps1` の `.DESCRIPTION` は「**成功した反復の trace も残す**」「失敗時の猶予
  （…）**唯一の実験**」と機能として謳い続ける
- workflow は `failure_grace_ms` の既定を `15000` にして渡し続ける（説明文つき）

計画は撤去を「#872 / #936 を閉じるとき」へ送った（PR 本文チェックリスト 4）が、**撤去までの間、
道具の自己記述が嘘になる**。`CLAUDE.md`「チーム憲章」の「記録への信頼で動く」に照らすと、
1 行の追記（「読み手は #872 で撤去済み。この 2 つは現在 no-op」）は今すべき仕事である。

### M2. 対称パスの片側だけ「説明のつかない固定遅延」を消した

**場所**: `scripts/smoke-startup.ps1:89-93`（`Stop-Process -Force` の直後に `Start-Sleep 120`）

`smoke-egui.ps1:139` の `Start-Sleep 300` を「待たない kill に対する事実上の待ち」として消した
判断は正しいが、**同型の 120ms が `smoke-startup.ps1` の 5 回ループに残る**。ヘルパは既に
export 済み（`psm1:908`）で 1 行で寄せられる。計画は「#786 へコメント」（PR 本文 3）として
コードは触らない判断をしており、判断としては妥当だが、**非対称が残ることは記録に必要**。
少なくとも当該行に「#872 のヘルパへ寄せていない理由 / 追跡先 #786」を 1 行置くのが
この repo の作法に合う（レビュー中に `smoke-egui.ps1:465-469` へ入った「意図的に別である」の
追記と、まさに同じ扱い）。

### M3. 切り出しで無効化したコメントのポインタ

**場所**: `src-tauri/src/egui_shell/view.rs:1182-1184`

> ⚠️ **これが縛るのは egui の意味論であって `update()` の並びではない。** …位置の理由は
> `input_id` を作った直後のコメントが正本

`input_id` の生成は `view.rs:666` へ移り、その直後にあるのは `buf_is_empty` の説明である。
焦点の並びの理由は `search_input_ui`（`:231-246`）へ移った。**正本を名指しするポインタが
外れている**——この repo はコメントを正本として扱うので、`search_input_ui` の doc を指すよう
書き換えるべき。あわせて同 doc の ⚠️（受容残余）も、kittest 追加後の実態（C2）に合わせた
書き換えが要る。

---

## Low

- **L1. `bar_font` の clone が 1 → 3 に増えた**（`view.rs:677` の `params.font`、`:263` の `.font()`、
  `:267` の `.hint_text(...).font()`）。切り出し前は clone 1 回 + move 1 回だった。
  `FontId { size, family }` で本 view は `FontId::proportional` しか使わないため clone は確保を
  伴わない memcpy であり、**毎フレームでも実害はない**。折り畳むなら `SearchInputParams` を
  `&egui::FontId` で持つ形もあるが、KISS の観点で現状で良いと判断する
- **L2. `egui::Context` の clone が 1 フレームあたり 1 回増えた**（`view.rs:456` の update() 側と
  `:220` の `search_input_ui` 側）。Arc の増減のみ。`src-tauri/CLAUDE.md`「モジュール構成」が
  禁じるのは **managed state への保持**であって関数内の一時 clone は対象外——不変条件違反ではない
- **L3. `hint` closure の `let hint: String = match { … }; hint`**（`view.rs:696-721`）は冗長。
  型注釈があるため clippy は黙る（実測 green）。逐語移動の結果なので現状維持でよい
- **L4. `env.rs` のテストが `" "`（空白 1 文字）を真側に含めている**のは良い設計判断。
  「空でなければ真」という規約の境界を、規約の言葉どおりに固定している

---

## ⚠️（確信の持てない所見・必ず目を通してほしい）

- **W1. 新 It は「アプリ自身の `set_focus()` が最初の 5 フレーム以内に OS focus をもたらす」ことに
  依存している可能性が高い。** PowerShell の `Set-SnotraForegroundWindow` は `Wait-SnotraWindow` の
  **後**に走る（`:497-498`）が、アプリは `show_on_startup = true` で窓を出した直後からフレームを
  回す。`focus_state` の予算は 5 フレームで show の外では再武装しない（`view.rs:419`）ので、
  **5 行すべてが PowerShell の前面化より前に出ている**公算が大きい。だとすると:
  - 設計書の「前面化は残す（前面が取れなければ真にならない）」は、条件としては正しいが
    **実際に真をもたらしている主体の説明としては外れている**（U-A は実は測れる位置にある）
  - 逆に runner が重く、アプリ自身の focus 取得が 5 フレームに間に合わない回では、
    **回帰が無いのに確定的に赤**になる新しい故障様態が生まれる。30,000ms の予算はこれに効かない
    （行はもう出ないので、待っても現れない）

  測り方: 1 度 Pester を走らせ、`caret.err` の `focus_state` 5 行の `window_focused` と、
  PowerShell が前面化した時刻を突き合わせる。C1 の修正（`window_focused` が真の最初の行を見る +
  空集合の禁止）は、この不確かさに対しても正しい向きに効く
- **W2. `-Skip:$sessionLocked` で Describe ごと skip された実行で `AfterAll` が走るかを実測していない。**
  Pester 6 が skip 時に `AfterAll` を実行するなら、セッションロック中の環境でも H1 の
  「他人の snotra を kill する」が起きる
- **W3. `egui_input:changed` は Pester 側の消費者を失った**（emit は
  `src-tauri/src/egui_shell/launcher_controller.rs`）。今後は誰も読まない計器として残る。
  意図的に残すならその旨を emit 点のコメントへ。#872/#936 のクローズ時に撤去候補へ入れるのが自然
- **W4. `Stop-SnotraProcessAndWait` の「待ちきれなかった」経路は、実プロセスに対して一度も
  走っていない。** 単体検査 5 件は偽オブジェクトで全分岐を測っており設計としては正しい
  （`Process.WaitForExit(int)` が `bool` を返すという前提も正しい）。ただし統合 It が通るのは
  常に「素直に終了する」側だけである。計画の PR 本文チェックリスト 1（CI ログに警告が実際に
  出るか）はこの残余をちょうど埋める項目なので**必ず実施すること**
- **W5. `AfterAll` の `throw` が Pester の `FailedCount` に載ることを実測していない。**
  設計の要（「`Write-Warning` は合否に影響しない」ので層を上げた）がここに懸かっている。
  故障注入ログには「新設 `AfterAll` が … で発火」とあるが、**発火した = 赤くなった**とは限らない。
  `run-pester.ps1` の exit code まで見た記録があるなら、この ⚠️ は消えてよい
- **W6. 並行編集**（冒頭の注意）。`smoke-egui.ps1` は私が最初に差分を取った後に変更された。
  追加分（`:465-469`「共有ヘルパと意図的に別である」）自体は**良い追記**で、M2 と同じ性格の
  非対称に説明を与えている——`smoke-startup.ps1` にも同じ扱いが要る、という M2 の指摘は残る

---

## Phase 別の照合記録

### 2a. `workspace/plan.md` の不変条件との照合

| 不変条件 | 実装箇所 | 判定 |
|---|---|---|
| 引数に `[System.Diagnostics.Process]` の型を付けない | `psm1:404-407` | 守られている（`[AllowNull()]` + 型なし） |
| fixture に `HasExited` / `WaitForExit` を足す | `Tests.ps1:245-259, 318-323` | 守られている（`Policy Stop` の fixture も Step 11 で追随） |
| `finally` から throw しない | `psm1:416-432`（`return $false` のみ） | 守られている |
| `-Quiet` は `Policy Stop` に付けない / 呼び出し側 2 箇所には付ける | `psm1:456` / `Tests.ps1:463, 519` | 守られている |
| `search_input_ui` は controller にも `RuntimeFrame` にも触らない | `view.rs:214-271` | 守られている |
| hint はクロージャで受ける（`available_width` の読み位置を変えない） | `view.rs:253` | 守られている |
| `input.rs` の `OnceLock` キャッシュは残す | `input.rs:29-39` | 守られている |
| `var_os(...).is_some()` が 0 件 | grep 実測（`env.rs` の 1 件のみ） | 守られている |
| kittest は `run()` ではなく `step()` | `view.rs:1259-1271` | 守られている |
| `egui_kittest` の版は `snotra-settings` と同じ指定 | `src-tauri/Cargo.toml:54-55` | 守られている |

### 2b. 対称コードパス

| 変更パス | 対称の相手 | 判定 |
|---|---|---|
| `Resolve-SnotraExistingProcess` の `Stop` 分岐に終了待ち | `smoke-egui.ps1` のシナリオ 1→2 の待ち（`:461-473`） | **不要**。意味論が違う（あちらは前提条件ゆえ throw する）。追記されたコメントが理由を明記しており妥当 |
| 同上 | `smoke-startup.ps1:89-93` の 5 回ループ | **要検討 → M2**（同型。ヘルパは使える状態にある） |
| 同上 | `visual-check-colors.ps1:299` / `bench-startup.ps1:80,105` / `measure-memory-stages.ps1:97,120` | **不要**。いずれも「停止直後に次のインスタンスを起動して trace を待つ」構造ではない（`measure-memory-stages.ps1:97` は測定の前処理、`:120` は最終後始末、`bench-startup.ps1` は測定器で本サイクルの射程外） |
| `Start-Sleep 300` の削除（smoke-egui） | `smoke-startup.ps1:93` の `Start-Sleep 120` | **要検討 → M2** |
| `focus_state` を読む側（Pester） | 書く側（`view.rs:740-752`・5 フレーム分） | 読み手は 1 件で成立＝**この非対称が C1 の実体である** |

### 2c. DRY / 関数カバレッジ

- `search_input_ui` の呼び出し点は 1（`view.rs:680`）+ テスト 1（`:1241`）。同等の合成を
  手書きしている箇所は無い（`make_persistent_id("search_input")` は `:666` の 1 件のみ・grep 実測）
- `request_focus` の製品コードでの呼び出しは `view.rs:251` の 1 件のみ（残りはテスト）。
  「2 か所で要求しない」（#700 の規範）は守られている
- `Stop-SnotraProcessAndWait` を使わず同じ処理を手書きしている箇所 → M2 の 1 件
- `trace_hatch_enabled` を使わず `var_os` で書いている箇所 → 0 件（grep 実測）

### 2d. リソースライフサイクル

| リソース | 生成 | 破棄 | 判定 |
|---|---|---|---|
| Pester の実プロセス（It 1） | `Tests.ps1:437` | `:463`（`finally` + 終了待ち） | **ペア成立**（今回むしろ強化された） |
| Pester の実プロセス（It 2） | `:495` | `:519` | ペア成立 |
| 取りこぼし | — | `AfterAll`（`:534`） | ペア成立だが **H1 の巻き添えあり** |
| `capture.Bitmap` | `:441` | `:460` | ペア成立（変更なし） |
| `OnceLock<bool>`（`input.rs`） | プロセス生存期間 | — | 意図どおり（doc が非対称の理由を明記） |

### 2e. SPEC.md 同期

| 差分が変える挙動 | SPEC 該当節 | 判定 |
|---|---|---|
| trace ハッチの受理値（空文字を未設定に） | 記述なし（`SPEC.md` に `SNOTRA_EGUI` は 0 件・grep 実測） | **SPEC 対象外**。正本は `PERFORMANCE.md` の計器一覧で、Task 6 Step 1 が同期済み |
| 検索入力欄の合成の切り出し | §18.5 / §11 に関わるが**挙動は不変** | SPEC 対象外（該当コメントは逐語で移動済み） |
| 実機配管の検査内容 | 記述なし | SPEC 対象外。`docs/build-commands.md` の該当 bullet が正本で、Task 6 Step 3 が同期済み |

### 2f. 「変更不要」判断の再評価

**再評価: `Set-SnotraForegroundWindow` を残す判断（U-A）**

- ケース A: アプリ自身の `set_focus()` が最初の 5 フレーム内に OS focus を得る → 前面化は
  **判定に寄与していない** → 問題: 設計書の説明が実態とずれる（⚠️W1）
- ケース B: 前面化が focus をもたらす → 想定どおり。ただし前面化はフレーム 1〜5 より後に走るため、
  **`focus_state` の 5 行が既に尽きている**なら真の行が 1 つも出ない → 問題: 回帰なしの赤（⚠️W1）
- ケース C: 前面化に失敗（#890 以降 30 反復で 0 件）→ `Should -BeTrue` で即赤。想定どおり
- **結論: 変更必要ではないが、判定の主語を「`window_focused` が真の最初のフレーム」へ
  明示する（C1 の修正）ことで、A/B いずれでも意味の通る検査になる**

**再評価: `[config]` 不在検査を落とした判断**

- ケース A: caret-profile へ `SNOTRA_CONFIG_DIR` が効かない → 実 config を読む →
  `show_on_startup` 既定 false → `Wait-SnotraWindow` 時間切れで赤。**検出はされる**（理由は不明瞭）
- ケース B: 実 config も `show_on_startup = true` の開発機 → 窓は出て `focus_state` も出るので
  **緑になりうる**。ただし It 1 が同じ実行で `[config]` を見ているため、env 経路そのものの破れは
  同じ Pester 実行の中で捕まる
- **結論: 変更不要を確認（受容する残余として H2 の表に名指し済み）**

### Phase 3. パフォーマンス

`view.rs` の毎フレーム経路に触れるため実施。

- 計算量の変化なし（逐語移動）
- 毎フレームの追加確保: `FontId` の clone +2、`Context` の clone +1、`SearchInputParams` 1 個
  （すべてスタック / Arc 増分のみ・ヒープ確保なし）→ **実害なし**（L1 / L2）
- `buf.is_empty()` の前倒し（`view.rs:669`）は評価時点が `Frame::show` の前へ動くが、その間に
  `buf` を変える経路は無い（`move_text_cursor_to_end` はキャレット state のみ、`request_focus` は
  memory のみ）→ **挙動不変**
- `ui.available_width()` の読み位置は変わらない（`add_sized` の引数 `:255` ／ hint 内 `:709`。
  どちらも内側 `Frame` の `ui` のまま）→ **挙動不変**
- 1 フレーム 1 回の live-read 規約（`src-tauri/CLAUDE.md`「モジュール構成」）に違反なし
  ——`params` は `visual` snapshot 由来の値を運ぶだけで config を読み直していない
- `env::trace_hatch_enabled` は `renderer.rs:76`（毎フレーム）で `var_os` を呼ぶが、
  これは**変更前と同じ回数**である（キャッシュの非対称は `input.rs` の doc が明記しており妥当）

---

## まとめ（優先順）

1. **C1 / C2 を先に潰す。** 今の形では #872/#936 の再発を L2 も L3 も捕まえない見込みで、
   「機序を再設計した」という本サイクルの主張そのものが立たない
2. **H1**（`AfterAll` の射程）は開発者の実インスタンスを壊すので、マージ前に直す
3. M1〜M3 は 1 行〜数行の記録の是正
4. ⚠️ は全件目を通し、W4 / W5 は PR 本文チェックリストへ（CI でしか測れない）

### 追記: C2 の修正案の細部（暖機なし harness だけでは不十分）

暖機フレームをまったく持たない harness には別の穴がある——`TextEdit::load_state` が `None` の間は
`move_text_cursor_to_end` が no-op で、キャレット位置は新規 `TextEdit` に egui が与える既定に
依存する。**識別に必要な形はもう少し狭い**: 判定フレームの時点で「widget の state は在るが
focus は無い」状態を作ること。構成できる——`window_focused: false` で 2 フレーム暖機し
（widget は描かれ state は保存されるが focus は取らない）、次のフレームで `window_focused` を
true に切り替えて同じフレームに `Ime(Commit)` を載せる。この形なら focus 要求が `TextEdit` の
前か後かで結果が分かれる。**単純な「暖機なし」だけを作ると両方の並びで通ってしまい、
C2 の指摘自体が誤りだったと誤読されうる。**

### 追記: W5 の訂正（設計への疑いではなく記録の欠落）

W5 は「層の選択が未検証」ではない。計画の実測ログ（Task 1 Step 10）は、注入時に `Reject` の
throw と `AfterAll` の throw の**両方が出たこと**を記録している。欠けているのはその先——
**その throw が `FailedCount` に載ったか（`run-pester.ps1` の exit code がどうだったか）の記録**
である。W5 はこの記録の欠落として読むこと。

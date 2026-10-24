# 調査: #1274 smoke-egui に focus 獲得時の押しっぱなしキーを足し、変異 2 を検知する

## issue の要約

`snotra-egui-runtime/src/input.rs` の `on_keyboard_event` は、文字（`typed_text_event(pressed, event.text_with_all_modifiers())`・`input.rs:411`）を、`on_window_event` の `KeyboardInput` 腕が `admit_key` で早期 return した**後**（`input.rs:293-313`）にしか積まない。この順序を崩して文字の配送を `admit_key` より前へ動かす退行（変異 2）は、focus 獲得時に押されていたキーの文字を検索欄へ入れる。これを赤にする検査が今は無い。

- `admit_key` の単体テストは判定そのものを縛るが、呼び出しの順序は縛らない
- `KeyEvent` は crate 外から組み立てられないので、`on_keyboard_event` を単体テストで通せない（#1268 と同じ制約）
- smoke-egui には、focus の移り変わりをまたいでキーを押し続けるシナリオが無い（`scripts/smoke-egui.ps1:432-433` が「捕まえないもの」として明記）

issue のやること: 文字キーを押したまま本体へ focus を移し、文字が入らないことを断言する。不在の断言は沈黙で通るので、打鍵が届いたことを正の効果で確かめる。変異 2 を注入して赤になることを確かめる。

## 関連ファイル・シンボル（grep で実在を確認）

- `snotra-egui-runtime/src/input.rs`
  - `admit_key`（`:101`）: release は常に通し `held` から外す／合成 press は `held` へ入れて落とす／非合成 press は `held` に無ければ通す
  - `on_window_event` の `KeyboardInput` 腕（`:287-314`）: `admit_key` が偽なら `drop_key` を input trace に残して return
  - `on_keyboard_event`（`:350`）: `Key` イベントを積み、末尾（`:411`）で文字を積む
  - `Focused` 腕（`:221-238`）: `Focused(false)` で `held_since_focus_gain` を全消去
  - `input_trace` / `input_trace_enabled`（`:44`・`:69`）: env `SNOTRA_EGUI_INPUT_TRACE` で立つ別系統の計器。行は `SNOTRA_EGUI_INPUT <kind> ts_ms=… …` で、`[trace]` の JSON 行ではない
  - テスト doc（`:742-744`）: Ctrl+H が `KeyEvent.text` の取り違えを捕まえる旨（#1268）
- `scripts/smoke-egui.ps1`
  - シナリオ 1: 起動 → hotkey で show → 1 文字クエリで results show → パスクエリ `c:\` → Ctrl+H（`:420-457`）→ Escape で hide（`:465-480`）→ H1 などの不変条件判定（`:492-529`）
  - シナリオ 1 の観測は `Wait-SnotraTraceEvent` を `-MinMatchCount` なしで呼ぶ＝**1 件でも現れた時点で成立する**（`:288`・`:367`・`:471`・`:478`。返すのはその周回で一致したうちの最後の 1 件・`SnotraSmoke.psm1:696`）
  - シナリオ 2（`:558-738`）は `Get-SnotraTraceEventCount` + `-MinMatchCount (基準 + 1)` で「打鍵の後に増えた 1 件」を待つ
  - 成功時の文言（`:783`）
- `scripts/lib/SnotraSmoke.psm1`
  - `Send-SnotraKey`（`:763`・`keybd_event` を 1 回呼ぶ）、`Send-SnotraKeyChord`（`:787`・押下順に押し逆順に離す）
  - `Start-SnotraProcess` の `-ExtraVariables`（`:338`）
- `src-tauri/src/egui_shell/lifecycle.rs` の `plan_hotkey`（`:15`）: 表示中かつ `hotkey_toggle` なら `HideNow`、それ以外は Alt 押下中なら `ShowAfterAltRelease`
- `snotra-core/src/config/schema.rs` の `default_hotkey_toggle`（`:73`）: 既定は真。smoke の seed は上書きしない
- `src-tauri/src/egui_shell/launcher_controller/frame_stages.rs` の `consume_reset_pending`（`:41`）: show 後の最初のフレームで `state.reset()`（入力欄を空に戻す）
- `src-tauri/src/egui_shell/view.rs`（`:343-348`）: `window_focused && input_editable` かつ入力欄が focus を持たなければ、構築前に `request_focus`
- `.github/workflows/smoke.yml`（`:108`）: CI がビルドするのは `snotra` だけで、`snotra-settings` は作らない
- `docs/adr/ADR-synthetic-key-press-suppression.md`: `keybd_event` で Escape を 1 回だけ注入して合成 press（`synthetic=true`）を実測した。`Z` を 4 秒保持しても `repeat=true` は 0 件

## 再利用できる既存パターン

- **打鍵の効果を到達の証拠にする**（#1268 の Ctrl+H）: `egui_input:changed` の `before_chars` / `after_chars` を区間で全件数え、期待の並び（`[3->2]`）と完全一致を断言する。届かなければ「観測なし」で赤、余分な変化も赤
- **増分で待つ**（シナリオ 2）: 打鍵の前に件数を数え、`件数 + 1` を待つ
- **区間を seq で切る**: `seq` は全 trace を貫く単一のカウンタなので、show の `seq` より後の事象だけを数えられる

## 技術的制約

- 合成 press は `WM_SETFOCUS` の瞬間に `GetAsyncKeyState` が押下中と答えるキーについて tao が作る（`admit_key` の doc・`tao-0.37.1/.../keyboard.rs:103-107`）。`keybd_event` の押下はこれに映る（ADR の実測。ただしローカル機での実測で、CI runner では未確認）
- `keybd_event` はオートリピートを生まない。押しっぱなしの間に追加の `WM_KEYDOWN` は来ない
- **押しっぱなしのキーは、本体が隠れている間は前面の別アプリへ届く**。CI runner なら無害だが、ローカル実行では利用者の前面アプリへ文字が入る
- シナリオ 1 の既存の観測は 1 件でも現れれば成立するので、既存の Escape 検査より**前**に show/hide を足すと、既存の断言が新しい事象で満たされて弱まる
- input trace（`SNOTRA_EGUI_INPUT_TRACE`）を読むスクリプトは今は無い。`drop_key` を証拠に使うなら env の指定と行の読み取りが新たに要る。runner では stderr 1 行が 17〜56ms かかる（`input.rs:146-147`）ので、全行を出すと系の時間が変わる

## focus の移し方の候補

| 案 | 中身 | 費用 |
|---|---|---|
| (A) 設定ウィンドウを閉じて戻す | #927 の再現経路そのもの | CI で `snotra-settings` のビルドと GUI 起動が要る（`smoke.yml` は今それを作らない） |
| (B) 別ウィンドウから `SetForegroundWindow` | 補助ウィンドウを立てて本体へ移す | 前面化の制限（foreground lock）に当たりうる。補助ウィンドウの所有と破棄が増える |
| (C) **文字キーを押したまま hotkey で隠して出し直す** | 本体が前面のときに文字キーを押し、そのまま hotkey で hide → hotkey で show。show の `WM_SETFOCUS` で合成 press が作られる | 新しいプロセス・ウィンドウ・ビルドが要らない。押下を始めるのは本体が前面のときなので、隠れている間の hotkey 以外は外へ漏れない |

(C) は issue に無い候補で、`smoke-egui.ps1:432-433` の「hotkey を離してから show するので押下中のキーが残らない」の裏返しである。

## 正の効果の取り方（(C) を前提に）

show 後に、押しっぱなしの文字キーへ**もう 1 回 down を送り**（非合成の press）、それから up、最後にふつうの打鍵を 1 回送る。区間（その show の `seq` より後）の `egui_input:changed` を全件数える。

| 状況 | 合成 press | 2 回目の down | ふつうの打鍵 | 区間の変化 |
|---|---|---|---|---|
| 正常 | `held` へ入り落ちる | `held` にあるので落ちる | 入る | `[0->1]` |
| 合成 press が起きない（環境の前提が崩れた） | なし | `held` が空なので入る | 入る | `[0->1, 1->2]` |
| 変異 2（文字を `admit_key` より前へ） | 文字が入る（届けば） | 文字が入る | 入る | 3 件（合成の文字が focus 前で落ちれば 2 件） |

- 正常が緑になるには合成 press が起きていることが要る。前提が崩れれば**赤に倒れる**（沈黙の緑にはならない）
- 変異 2 は 2 回目の down だけでも赤になる
- 打鍵が 1 つも届かなければ「観測なし」で赤

これなら input trace を読まずに済む（`drop_key` は診断として残すだけでよい）。

## 未解決の疑問

1. 文字キーを押したまま Alt+Q を打って、`RegisterHotKey` の hotkey が発火するか（非修飾キーの押下が判定に影響しないか）
2. 2 回目の `keybd_event` down が、非合成の press として tao から届くか（`is_synthetic=false`）
3. show 直後の `state.reset()` が `egui_input:changed` を出さないか（出ると区間の起点がずれる）
4. CI runner でも `keybd_event` の押下が `GetAsyncKeyState` に映り、合成 press が作られるか
5. 新しい区間で `egui_results:show` などが増えることが、後段の判定（H1 など）に影響しないか——H1 判定は新しい手順より前に終わるので影響しないはずだが、`Read-SnotraTraceSnapshot` を後で呼ぶ箇所が無いか

## 敵対的調査（`workspace/adversarial-1274.txt`）の所見と採否

壊せなかった項目: 争点 1〜6 すべて（合成 press の発生・2 回目の down が非合成で届き落ちる・reset が `egui_input:changed` を出さない・3 状況の分かれ方・H1 より後に置けば既存の断言を弱めない・押下中でも hotkey が発火する）と、測定環境の 2 点（seed の `hotkey_toggle` は既定の真・`egui_input:changed` は変異 2 が触る経路の下流）。

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| `Wait-SnotraTraceEvent` は「最初の 1 件」ではなく、成立した周回の最後の 1 件を返す | 壊せた（不正確） | **採る**。`SnotraSmoke.psm1:696` で自分で確かめ、本文を直した。「前に足すと既存の断言が弱まる」の結論は変わらない |
| 手順は「show → Z → hide → show」の 4 手になる（Escape の後は隠れている）。調査に書いていない | 壊せた（欠落） | **採る**。計画の手順に入れる |
| Z の down と up の間で失敗すると、Z が OS 上で押されたまま残る | 壊せた（欠落） | **採る**。新しい手順を try/finally で包み、finally で必ず Z を up する |
| 変異 2 の件数は「3 件か 2 件」だけでなく、同じフレームに積まれれば `[0->2, 2->3]` もありうる | 壊せた（不正確） | **採る**。断言は「`[0->1]` と完全一致」だけにし、件数の場合分けは失敗文言に使わない |
| 偽の緑: 2 回目の down が届かず、かつ合成の文字が `egui_show:done` より前に `egui_input:changed` を出すと、変異 2 が `[0->1]` に見える | ⚠️ | **採る（機序は自分で裁定）**。区間の起点を `egui_show:done` ではなく、**その前の hide の `egui_hide:done` の seq** にする。hide から show までの間に入力欄が変わる経路は無い（`egui_input:changed` の出所は `view.rs:874` の 1 つだけ）ので、合成の文字がどの時刻に出ても区間に入る |
| CI runner で `keybd_event` の押下が `GetAsyncKeyState` に映るかは未確認。映らなければ赤に倒れる（沈黙の緑にはならない）が、CI が恒常的に赤になりうる | ⚠️ | **採る**。ローカルの実測で前提を確かめたうえで、CI での確認は PR 本文のチェックリストへ送る（PR があって初めて測れる） |
| 押下中でも hotkey が発火するのは Win32 の仕様からの推論で、未実測 | ⚠️ | **採る**。計画の未確定に入れ、ローカルで実測する |
| 新しい手順の後は、成功文言（`:783`）・「捕まえないもの」のコメント（`:432-433`）・H1 ブロックの「最後まで hidden で終わる」のコメント（`:490-491`）を直す必要がある | ⚠️ | **採る**。前 2 つは直す。H1 のコメントは、新しい手順も Escape で隠して終えるので真のまま残る（実装時に読み直す） |

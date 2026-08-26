# 調査: #1173 キーボードによる窓移動と可視中クランプの相互作用

## issue の要約

`clamp_main_into_work_area` は `view.rs` が**ポインタ非押下のフレームすべて**で呼ぶ（`!ui.input(|i| i.pointer.any_down())`）。マウスドラッグはこの条件で除外されるが、キーボード移動（`Alt+Space` → `M` → 矢印 → `Enter`）にはポインタ押下が伴わない。**モーダル移動ループ中に egui のフレームが回るなら、毎フレーム引き戻されて `ADR-main-window-clamp-on-pointer-release` 却下 2 の「モニター間移動の封鎖」がキーボード経路で現実になる。** 回らないならマウスと同じ「確定後に 1 度戻る」で問題なし。**どちらであるかを測っていない**というのが issue の中身であり、これは #909（#738）以降ずっと在る性質で #878 / PR #1171 が持ち込んだものではない。

期待する決着は 2 つ——「問題なし」なら ADR へ**キーボード移動を名指しで**受容残余として記録して閉じる／「封鎖」なら `any_down()` に代わる「移動中である」の判定を設計する（ADR 却下 1 の wndproc サブクラス化の却下理由を測り直すことから始める）。

## 関連ファイル・シンボル（実在を確認済み）

| 場所 | 何を持つか |
|---|---|
| `src-tauri/src/egui_shell/view.rs:1279-1281` | クランプの**唯一の発火条件** `if !ui.input(\|i\| i.pointer.any_down())` と `clamp_main_into_work_area` 呼び出し |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `clamp_main_into_work_area` の実体・`point_monitor_work_area` 基準・`read_bar_anchor` |
| `src-tauri/src/egui_shell/mod.rs:347-359` | main 窓の builder（`decorations(false)` / `resizable(false)` / `skip_taskbar(true)` / `always_on_top(true)` / `visible(false)`） |
| `SPEC.md` §8.2「表示中の作業領域への復帰（#738）」 | 「ドラッグしている間は拘束しない」の正本。**キーボード移動は名指されていない** |
| `docs/adr/ADR-main-window-clamp-on-pointer-release.md` | 却下 1（wndproc サブクラス）／却下 2（毎フレームクランプ＝封鎖の機序）／却下 5（backstop の実測却下と**再測手順**） |
| `docs/adr/ADR-injected-arrow-key-physical-identity.md` | `Send-SnotraKey` の `bScan=0` と `physical=Numpad2` を欠陥として扱わない決定。**矢印注入はこの ADR を読んでから組む** |
| `scripts/lib/SnotraSmoke.psm1` | `Start-SnotraProcess` / `Wait-SnotraWindow` / `Set-SnotraForegroundWindow` / `Send-SnotraKey` / `Get-SnotraWindowCapture`。**画面ロック検出（#866）に守られる唯一の経路** |
| `docs/build-commands.md:112-122`「エージェントが目視項目を自分で実施するとき」 | 自己実施の作法。**行 120 が本 issue に直撃する**（下記の制約 3） |

## 一次証拠（tao 0.35.3 のソースを読んだ結果・`~/.cargo/registry/src/*/tao-0.35.3/src/platform_impl/windows/`）

**以下はすべて「ソースの読み」であって対象そのものの測定ではない。実測で裁定する。**

1. **`decorations:false` でも `WS_SYSMENU` は残る。** `window_state.rs:244` が `style |= WS_CAPTION | WS_CLIPSIBLINGS | WS_SYSMENU` を立て、`:271-272` が装飾なしのとき `style &= !WS_CAPTION` しか落とさない。→ システムメニュー自体は窓に在る
2. **しかし `WM_SYSCHAR` が DefWindowProc へ届かない見込みがある。** `keyboard.rs:111-113` は `WM_SYSKEYDOWN` を `ProcResult::DefSubclassProc`（＝ DefWindowProc へ通す）にする一方、`:162-178` の `WM_CHAR | WM_SYSCHAR` 枝は `event_info` が `Some` のとき `ProcResult::Value(LRESULT(0))` で**握り潰す**。`Alt+Space` のシステムメニューは DefWindowProc(`WM_SYSCHAR`) 経由で開くため、**開かないと予測される**
3. **`skip_taskbar(true)` ゆえタスクバー経由のシステムメニューという入口も無い**（`mod.rs:352`）
4. **tao はモーダルループ中に描画イベントを流す機構を持つ。** `event_loop.rs:1028-1044` が `WM_ENTERSIZEMOVE` / `WM_EXITSIZEMOVE` で `MARKER_IN_SIZE_MOVE` を出し入れし、`:2329-2355` のスレッドターゲット `WM_PAINT` が「nested win32 event loop」を明示的に扱って `flush_paint_messages` → `redraw_events_cleared` を回す。**言えるのは「フレームが回りうる機構が在る」までで、実際に回るかは repaint 要求（`Moved` → `request_repaint`）次第である**
5. **`MARKER_IN_SIZE_MOVE` は tao の内部フラグで公開 API に出ていない**（`grep -rn MARKER_IN_SIZE_MOVE` は `event_loop.rs` 3 箇所と `window_state.rs` の定義のみ・アクセサ無し）。tao は `GetGUIThreadInfo` を使っていない（`grep` 0 件）

## 再利用できる既存パターン

- **ADR 却下 5 の再測手順がそのまま骨組みになる**——「治具を書いて打鍵/マウスを注入し、押下したままの窓矩形と離して 0.5s 後の窓矩形を `GetWindowRect` で比べる。押下中に窓が引き戻されていれば封鎖が起きている」。本 issue はマウス押下をキーボード移動モードへ差し替えるだけである
- **`SNOTRA_CONFIG_DIR` で使い捨てプロファイル**（`docs/build-commands.md:124-136`）。実 config・実 index に触れずに測れる
- 位置の判定は `GetWindowRect` でよい（`docs/build-commands.md:118`。高さの判定にだけ使ってはならない）。作業領域は `MonitorFromWindow` + `GetMonitorInfoW` の `rcWork`

## 技術的制約

1. **開発機は単一モニターである**（実測・**DPI-aware で測り直した値**: `rcMonitor = 0,0,1920,1080` / `rcWork = 0,0,1920,1020`。物理ピクセル）。
   - **⚠️ 最初にここへ書いた `1536x864` / `0,0,1536,816` は誤りだった。** `[System.Windows.Forms.Screen]::AllScreens` を DPI awareness を通さない PowerShell から呼び、Win32 が仮想化した論理値を「実測」として書いていた。`SetProcessDpiAwarenessContext(-4)` を通した `MonitorFromPoint` + `GetMonitorInfoW` で自分で測り直して訂正した（この機体は 125% スケーリング）。
   - **この罠は `scripts/lib/SnotraSmoke.psm1:175-187`（`Get-SnotraWindowDpi` の doc）が名指しで警告している**——「2026-08-16 に実測した 125% の機体では、`GetDeviceCaps(LOGPIXELSX)` が 96 を、`System.Windows.Forms.Screen` が物理 1920x1080 を 1536x864 と報告した」。**同じ機体で同型を再発させた。** 治具は `Initialize-SnotraDpiAwareness` を先に通すこと（`GetWindowRect` は物理座標を返すので、作業領域と土俵を揃えないと絶対値の判定が破綻する）。
   - **却下 2 が記述した「モニター間移動の封鎖」そのものはこの機体で再現できない。** 単一モニターで測れるのは上位の判別子——「**移動モード中に、バー矩形を作業領域の外へ出せるか**」である。出せない／出しても即座に引き戻されるなら毎フレームクランプが働いており、封鎖の前提が成立する
2. **実 config の hotkey は `Ctrl+K`**（`%APPDATA%/Snotra/config.toml`）。`Alt+Space` と衝突しない。`follow_cursor_monitor = true`・`window_width = 300`
3. **モーダルループ中の値を単発観測で判定してはならない**（`docs/build-commands.md:120`・実測済み）——合成マウス移動への追従が不安定で、同一手順・同一バイナリでも窓 top が 956 と 1050 の間で揺れた。**実装の有無を切り替える対照実験だけが差を示す。** 本 issue では「クランプ行を無効化した対照ビルド」との差を見る形になる
4. **`Send-SnotraKey` は `bScan=0`** で撃つため矢印が `physical=Numpad2` として届く。egui 側では `ArrowDown` として実るので下流に影響しないが（`ADR-injected-arrow-key-physical-identity`）、**DefWindowProc のモーダル移動ループが scancode / 拡張ビットを見るかは別問題であり、注入した矢印が移動モードを駆動する保証は無い**
5. **`SnotraSmoke.psm1` にマウス注入（`SetCursorPos` + `mouse_event`）は無い**（`docs/build-commands.md:116` が明記）。自前 P/Invoke を足すと画面ロック検出（#866）の外へ出る

## 測定結果（2026-08-26・release ビルド・単一モニター `rcWork = 0,0,1920,1020`）

生の観測は `workspace/measurement-1173-treatment.txt`（クランプ有効）と `workspace/measurement-1173-control.txt`（クランプ行を落としたローカル未コミットパッチ）。治具は `workspace/measure-1173.ps1`。

### Q1: `Alt+Space` はシステムメニューを開く — **予測は外れた**

**開く。** `FindWindowW('#32768')` が visible なポップアップを返し、続く `M` → 矢印キーで窓が実際に動いた。**一次証拠 2（tao `keyboard.rs` の `WM_SYSCHAR` 握り潰しから「開かない」と予測）は偽である。** ソース読みは代理であって対象そのものの測定ではない、という留保がそのまま効いた。**予測の機序がどこで破れたかは特定していない**（`event_info.is_none()` の早期 return 経路か、`WM_SYSKEYDOWN` に対する DefWindowProc の別の扱いか）——issue の決着に要らないので測っていない。

### Q2: モーダル移動ループ中もフレームは回り、クランプは発火する — **封鎖の前提が成立**

**対照の差が決定的である。**

| | 移動中に到達する上限 | 移動中に外へ出たまま留まれるか | 確定後 |
|---|---|---|---|
| **クランプ有効**（treatment） | **`bottom = 1020` = `rcWork.bottom` ちょうど** | **留まれない**——反復 2〜5 では一度 1132 まで出るが、移動ループの中にいるまま 1020 へ引き戻され、以後 80 回の押下すべてで 1020 のまま | 1020（＝作業領域の内側） |
| **クランプ無効**（control） | `bottom = 1133/1134`（カーソルが画面下端で止まる位置） | **留まれる**——200 回の押下すべてで 1134 のまま | 1106（作業領域の**外**） |

- **treatment が止まる値が `rcWork.bottom` ちょうどであること**が、止めているのがカーソルの限界ではなく `WorkArea::clamp` であることを示す（control の上限 1133/1134 はカーソル限界であり、作業領域とは無関係な値である）
- **treatment の反復 2〜5 で、`Enter` を撃つ前に 1132 → 1020 の引き戻しが起きている**——確定時ではなく**移動ループの最中に**クランプが走っている直接の証拠である
- **引き戻しは毎フレーム即座ではなく、モーダルループの `SetWindowPos` との競り合いである**（1132 の状態が 60 回ぶんの押下にわたって観測された）。それでも**外に留まることはできない**

**⚠️ 却下 2 が記述した「モニター間移動の封鎖」そのものは、この単一モニター機では測っていない。** 測ったのは**その機序**（ポインタ非押下のモーダル移動ループ中にクランプが毎フレーム発火する）であり、封鎖は機序 + 却下 2 が既に実測した算術からの**演繹**である。多モニター機での確認は残っている。

## 未解決の疑問（測って潰す）

- **Q1（ゲート 1）: `Alt+Space` はこの窓でシステムメニューを開くか。** 一次証拠 2 は「開かない」を予測するが、ソース読みは代理である。開かないなら issue が想定した二択の**外の第 3 の決着**になる——ただし結論は「`Alt+Space` 経路では移動モードへ入れない」に留め、SC_MOVE の他の入口（`WM_SYSCOMMAND SC_MOVE` の直接送出・外部ツール）を名指して射程を書く。**issue の本質は「ポインタ非押下のモーダル移動ループ」であり `Alt+Space` は入口の一つにすぎない**
- **Q2（ゲート 2）: モーダル移動ループ中に egui のフレームが回り、クランプが発火するか。** Q1 が開かないなら `SendMessage(WM_SYSCOMMAND, SC_MOVE, 0)` を直接撃って同じループへ入れて測る（入口の可否と、ループ中の挙動は別の問い）
- **Q3: 封鎖だった場合の代替判定。** 候補は 2 つで、どちらも**要検証**として置くだけにする——(a) `GetGUIThreadInfo` の `GUI_INMOVESIZE`（自スレッドが move/size モーダルループ中かを OS へ問える。**wndproc サブクラス化が不要**なので ADR 却下 1 の理由に当たらない可能性がある）、(b) ADR 却下 1 の wndproc サブクラス化の却下理由を測り直す。**(a) が成立するなら (b) へ行く必要が無い**
- **Q4: 対照ビルドの作り方。** 制約 3 が要求する「実装の有無を切り替える対照」を、製品コードへ試験専用の注入を入れずに作れるか（`ADR-no-test-only-injection-in-product-code` が在る）。ローカルの一時パッチで足りるか要確認

## 敵対的調査（Step 3b）の反映

サブエージェント 1 体（`general-purpose` / `sonnet`）に (a)〜(g) の 7 命題を渡し、壊せた項目と壊せなかった項目の両方を宣言させた。全文は `workspace/adversarial-1173.txt`。

| 命題 | 判定 | 採否と理由 |
|---|---|---|
| (a) `decorations:false` でも `WS_SYSMENU` が残る | 壊せなかった（高） | 維持。`window_state.rs:307` の追加根拠（装飾なしで落ちるのは `WS_CAPTION\|WS_THICKFRAME` だけ）を**採る** |
| (b) `WM_SYSCHAR` 握り潰しで `Alt+Space` が開かない見込み | 壊せなかった（中 ⚠️） | 維持。`TranslateMessage` の実在（`event_loop.rs:266/405/411/815`）と `PeekMessage` による `event_info` 持ち越し（`keyboard.rs:127-149`）を**採る**——機序が 1 段深く裏づいた。**ただし依然として見込みであり、Q1 の実測が必要**という位置づけは変えない |
| (c) クランプ発火条件は `view.rs:1279` だけ | 壊せなかった（高） | 維持。呼び出し 1 箇所を独立に数え上げた結果を**採る** |
| (d) モーダルループ中にフレームが回りうる機構が在る | 壊せなかった（中 ⚠️） | 維持。**追加で採る**: `WM_SYSCOMMAND` はほぼ全 variant で `ProcResult::DefWindowProc` へ落ちる（`event_loop.rs:1273` 付近）——Q2 のフォールバック（`SC_MOVE` を直接撃つ）が tao 側で塞がれていないことの裏づけ |
| (e) 単一モニターの実測値と代替判別子 | **壊せた** | **採る（訂正済み・上の制約 1）。** 判別子の方針は生き残るが、**引用した絶対座標が誤りだった**。指摘は自分で `SetProcessDpiAwarenessContext(-4)` を通して測り直して裁定した（`0,0,1920,1020`） |
| (f) 実 config は `Ctrl+K` 等 | 壊せなかった（高） | 維持 |
| (g) `GUI_INMOVESIZE` は却下 1 の理由に当たらない | 壊せなかった（中 ⚠️） | 維持。**ただし「別プロセスからの実呼び出し・権限面は未検証」を採り、Q3 の候補 (a) に「要検証」を明示する**（本体プロセス内から自スレッドを問う形なら権限は問題にならないはずだが、それも未測定である） |

**測定環境への追加の疑いも採った。**

- `Send-SnotraKeyChord`（`SnotraSmoke.psm1:787-803`）が既にあり、`Alt+Space`（VK `0x12`, `0x20`）を押下順 → 逆順解放で撃てる。**自前 P/Invoke を足さずに Q1 を測れる**——治具はこれを使う
- **ビルド構成を計画で指定していなかった。** repaint 頻度が debug / release で変わりうる。**release で測る**ことを計画へ明記する（実運用に近い側で測り、debug は補助）

**採らなかった機序は無い。** (e) は所見・機序とも自分の再測で一致した。

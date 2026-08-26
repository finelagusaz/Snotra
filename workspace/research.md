# 調査: results 窓の表示後に注入した打鍵が届かなくなる（#999）

**この文書は #999 サイクルの一次調査であり、次サイクルでは再利用しない。**

## issue の要約

#996（アイコンキャッシュ剪定の撤去の検討・CLOSED）の実機測定中に、`egui_results:show`（rows=200）の直後から
注入した打鍵が 1 つも届かなくなる状態へ 6 回連続で当たった。trace は同地点で止まり（13〜18 行）、
`/q` へ到達できずスクリプトは強制終了へ落ちる。プロセスは生きており、前面窓は main のままだった。

**#999 が求めるのは判定であって修正ではない**——製品の欠陥だった場合は別 issue を立てると issue 自身が定めている。
やることは 4 つ:

1. `SNOTRA_EGUI_INPUT_TRACE` を立てて Down キーが本体へ届いているかを測る（(A) 配送が止まる / (B) そもそも届いていない の切り分け）
2. (A) だった場合の沈黙の契機の特定
3. `smoke:egui` / `check:colors` が同じ条件へ入りうるかの判定
4. 製品の欠陥か注入経路固有かの結論

## 計器: 1 回の実行が配送〜適用の各層を同時に割る

`SNOTRA_EGUI_INPUT_TRACE` は既に実装されており、**コード変更なしで (A)/(B) より細かく割れる**。
行の種類と出所（すべて grep 実測）。

> **訂正（2026-08-26）**: 当初この表は 4 行で、**`push_key` を落としていた**。
> **その 1 行が「打鍵が egui へ実ったか」を答える最も内側の計器であり、
> 落としたまま機序を書いたせいで §「実測」の結論を 1 度誤った。**
> 計器の一覧は `input.rs` の `input_trace(` 呼び出しを grep して作ること——散文から写さない。

母集団は `grep -oE 'input_trace\(\s*"[a-z_]+"' snotra-egui-runtime/src/{input,runtime}.rs` の 6 種**すべて**である
（`drop_key` / `push_key` / `push_text` / `rx_key` / `rx_text` / `take`）。

| 行 | 出所 | 何を言うか |
|---|---|---|
| `SNOTRA_SMOKE_INJECT` | `scripts/lib/SnotraSmoke.psm1:780-784`（`Send-SnotraKey`） | PowerShell が `keybd_event` を**呼んだ**時刻（成否は返らない） |
| `rx_key` | `runtime.rs:215-227` | tao が窓イベントとして**配送した**。`window_id` / `state` / `physical` / `synthetic` つき |
| `rx_text` | `runtime.rs:228-231` | `ReceivedImeText` が届いた（WM_CHAR 側）。`chars=` |
| `drop_key` | `input.rs:293-302` | 配送されたが `admit_key` が**抑止した**。`reason=held_since_focus_gain` |
| `push_key` | `input.rs:346-356` | **egui へ積んだか**。`mapped=` が `active_key.is_some()`＝「打鍵が実ったか」を答える**最も内側の層** |
| `push_text` | `input.rs:311-318` | 文字として積んだか。`committed=` |
| `take` | `input.rs:149-166` | egui へフレームを渡した。`frame=` / `events=` / `focused=`。100ms の心拍に間引くが、**入力を積んだフレームは必ず出る** |

### 判定表（実行前に確定させる）

| 観測 | 結論 |
|---|---|
| `take` の心拍が途切れる | フレーム不回転。(A) のうち「イベントループが回っていない」 |
| **心拍は出るが `ts_ms` の間隔が数秒へ伸びている**、**かつ同じ区間に入力か再描画要求が在る** | **フレームが重い**（下の H3′）。二値では「正常」に落ちる第 4 の状態 |
| 心拍あり・間隔も正常・`rx_key` 無し | issue の **(B)**。`keybd_event` の打鍵がそもそもアプリへ配送されていない |
| `rx_key` あり・`drop_key` あり | #927 の `held_since_focus_gain` による抑止（後述） |
| `rx_key` あり・`drop_key` 無し・下流の trace 無し | egui／アプリ層。`take` の `events=` が 0 か否かでさらに割れる |

**心拍の有無を二値で読んではならない**（敵対枠 3b の所見 2・採用）。`TAKE_TRACE_HEARTBEAT` は
「前回の `take` から 100ms 以上経っていたら次の `take` で 1 行出す」でしかなく、**フレームの実周期を
測る計器ではない**——1 フレームが数秒かかっても心拍は「途切れていない」ように見える。
ゆえに**判定は行の有無ではなく `ts_ms` の階差**で行う。

**ただし階差だけでは重さを名乗れない**（2026-08-26 実測で自分が踏んだ）。`-PostShowDelayMs 800` の
待ちの区間で階差 **490ms** が出たが、これは**入力も再描画要求も無いので `take` が呼ばれていない**だけである。
**「階差が伸びる → フレームが重い」は偽陽性を持つ**——重さの証拠にするには、
同じ区間に入力か再描画要求が在ることを併せて示すこと。

**`take` の行は 2 つの窓が混ざる。** `InputState` は窓ごとに 1 つで（`runtime.rs:363,389`）、
`take` は `window_id` を出さず `frame=` は各窓の独立カウンタである（自分で実測）。
2 本の単調増加列が交互に現れる形になるので、**`frame=` の非単調を「巻き戻り」と読まない。**

**`take` の `focused=` も同時に読む**——issue が測った `GetForegroundWindow`（OS の前面）と、tao が
`Focused` イベントで持つ内部状態は別物であり、後者だけが `admit_key` の消去点を駆動する。

## 仮説と、その仮説自身への反証材料

### H1: `held_since_focus_gain` の持ち越し（#927 機構）

`input.rs:99-115` の `admit_key` は、**synthetic な press を無条件に `held` へ入れ、release まで press を渡さない**。
消去点は `Focused(false)` の 1 か所だけである（`input.rs:218-227`。`Focused(true)` では消さない——合成 press が
`Focused(true)` より先に届くため）。doc 自身が危険を名指ししている:

> 抑止したキーの release が届かない経路で抑止が持ち越されると、以後 Escape が永久に効かなくなる

症状（以後すべての打鍵が沈黙・プロセスは生存・前面は main）はこの記述と形が一致する。

**この仮説への反証（敵対枠 3b の所見 1 で強化・採用）**。当初は「注入の down/up が 40ms で閉じるので
窓が狭い」というタイミングの議論を置いていたが、**構造的な理由 2 つの方が強い**（いずれも自分で
一次証拠を確認した）:

1. **`admit_key` の release 分岐は `is_synthetic` を問わず無条件に `held.remove` して `true` を返す**
   （`input.rs:102-105`）。ゆえに H1 が「以後すべての打鍵が沈黙し続ける」を説明するには
   **release すら届かないこと**を仮定せねばならず、それは H1 固有の機序ではなく H2／(B) へ退化する。
2. **#999 の再現手順には focus 往復（`Focused(false)` → `Focused(true)`）の発生源が無い。**
   results 窓は `focusable(false)` + `SW_SHOWNOACTIVATE` で `Focused` を一度も受けず（U-3）、
   設定窓も開いていない。往復が起きなければ `held_since_focus_gain` は空のままで、H1 は**一度も発火しない**。

⚠️ 2 は不在の主張（全称否定）ゆえ確信は中である。**偽になる形**: アイコン抽出スレッドの
COM/シェル問い合わせが一時窓を作る、Explorer のオーバーレイハンドラが前面を触る、等で
main が focus を落とす経路が在れば H1 は復権する。

**結論は変わらない——H1 は `drop_key` 行の有無で 1 発で決着する。** 根拠が「タイミングが狭い」から
「発火源が無く、release は無条件に通る」という構造的理由へ置き換わっただけである。

### H2: フレーム不回転（caret spec の U-B）

`docs/superpowers/specs/2026-08-05-caret-test-mechanism-design.md` が **30 反復中 1 件**の
「24.3 秒 1 フレームも回らない・`take` 行が 0 件」を記録し、その機序を **U-B として射程外**と宣言している。
#999 の署名（全キー沈黙・プロセス生存・前面不変）はこれと一致する。**候補であって結論ではない。**

### H3: アイコン抽出の負荷（issue が挙げた候補）

**issue の書き方は既に一部が偽である**（実測）:

- 抽出は**メインスレッドで走らない**——`results_view.rs:199-205` の `spawn_icon_load` が `std::thread::spawn` する
- **200 行ぶんは積まれない**——`request_icons_for_results`（`results_view.rs:164-192`）が受け取る `rows` は
  `layout::icon_prefetch_range` で絞った viewport 範囲であり、結果全件ではない（同関数の doc が明記）
- `icon_pending` による in-flight 除外が thread pileup を防いでいる

### H3′: アイコンの**適用**はイベントループスレッドで走る（敵対枠 3b の所見 2・採用）

H3 の反論は**抽出コスト**だけを否定していて、**適用コスト**を見落としていた。`results_view.rs:574-594` の
`update()` は毎フレーム `while let Ok(msg) = self.icon_rx.try_recv()` で完了メッセージを**同期的に**
drain し、届いた件数ぶん `icon_ctx.load_texture(...)` を呼ぶ（自分で実読）。**worker は `ColorImage` を
送るだけで `load_texture` は呼ばない**とコメントが明記しており、テクスチャ生成はイベントループスレッドである。
drain は上限を持たない `while let` なので、`C:\WinSxS` 級の重い問い合わせが**バーストで完了**すると
1 フレームぶんの drain が長時間化しうる。

この状態の署名は **「`rx_key` は届く・心拍も出る・だが `take` の `ts_ms` 階差が数秒」** であり、
H2（0 フレーム）とも (B) とも違う。⚠️ drain が実際に何 ms 食うかは未計測——静的読解では定量化できない。

worker が撃つ `egui_ctx.request_repaint()` の頻度もこの経路に載るが、いずれも**フレームが回っている**ことを
含意するので H2 とは両立しない。`take` の**階差**がこの 3 つを分ける。

## 既存の検証への影響（issue の「未評価」を一段進める）

**`smoke:egui` は `egui_results:show` の後に打鍵を注入している**（`scripts/smoke-egui.ps1` 実測）:

- `367` で `egui_results:show` を観測
- `388-401` で BackSpace → `c` → Shift+`;` → `\` を注入
- `426-428` で Escape を注入

つまり **`smoke:egui` は「results 表示後に打鍵する」という条件そのものへ入っている**。
違うのは索引の規模（検証用プロファイルは 1〜3 件）だけである。

`check:colors` は `visual-check-colors.ps1:295-296` で 1 文字クエリを 1 回注入するのみ。

**false green にはならない**（issue の記述どおり）——`smoke:egui` の失敗はすべて `$failures` への追加＝ exit≠0 であり、
観測タイムアウトが検査項目になっている。ゆえに影響評価の問いは「壊れるか」ではなく
**「機序が行数・アイコン抽出に依存するか、それとも規模に依らない機構（H1/H2）か」**である。前者なら免れており、
後者なら間欠 flake としてもう出ているはずである。

## 測定環境の制約（実行前に潰す）

1. **#996 の測定ハーネスはリポジトリに無い。** `workspace/` は空で、最後に `workspace/` を触ったコミットは
   ee8dcb12（#1059 サイクル）。`scripts/` に icons.bin を扱う本番スクリプトは無い
   （`grep -rln icons.bin scripts/` は governance の test fixture 1 件のみ）。**再現ハーネスは書き直しになる。**
2. **env は空文字で渡してはならない。** `snotra-egui-runtime/src/env.rs` の `trace_hatch_enabled` は
   空文字を未設定として扱う（`ADR-egui-trace-hatch-empty-only`）。PowerShell の
   `SetEnvironmentVariable($name, $null, 'Process')` は変数を消さず空文字で作るため、
   **`-ExtraVariables @{ SNOTRA_EGUI_INPUT_TRACE = '1' }` のように明示的な非空値を渡す。**
   `Start-SnotraProcess` の予約名は `SNOTRA_CONFIG_DIR` / `SNOTRA_TRACE` の 2 つだけで、この名前は通る
   （`SnotraSmoke.psm1:341-352` 実測）。env は `Invoke-SnotraEnvironment` が `Set-Item Env:` で立ててから
   `Start-Process` を呼ぶ形なので子は継承する。復元は `Remove-Item` であって空文字を作らない（自分で実読）。
2b. **`SNOTRA_TRACE` と `SNOTRA_EGUI_INPUT_TRACE` は独立の 2 系統であり、両方立てる必要がある**
   （敵対枠 3b・採用。自分でも `trace.rs` の `env_flag`＝許可リスト と `env.rs` の `trace_hatch_enabled`＝
   空文字以外真 が別物であることを確認した）。**`egui_results:show` / `egui_hide:done` は `SNOTRA_TRACE` 側**、
   `rx_key` / `drop_key` / `take` は `SNOTRA_EGUI_INPUT_TRACE` 側から出る。片方だけでは
   「どこで止まったか」と「打鍵が届いたか」を同じ時系列へ並べられない。
   `Start-SnotraProcess` では `-Trace` スイッチ（予約名経由）と `-ExtraVariables` の**併用**になる。
3. **2 つのストリームを両方捕まえる必要がある。** `rx_key` / `take` / `drop_key` は本体の **stderr**（`eprintln!`）へ、
   `SNOTRA_SMOKE_INJECT` は `Write-Host` で **PowerShell ホスト**へ出る。突き合わせるには
   `-StandardErrorPath` に加えてホスト側出力も採る（`Start-Transcript` かリダイレクト）。
   時計はどちらも epoch ms で同じ土俵。
4. **計器は系を乱す。** `PERFORMANCE.md:2721` が「runner では stderr 1 行が 17〜56ms かかる。
   率を測る回と機序を測る回は別の回にすること」と定め、caret spec が「計器は率だけでなく**喪失の現れ方**も変える」と
   書いている。**6/6 が計器つきで 0/N になったら、それは「直った」ではなく「タイミング依存である」という所見である。**

## 関連ファイル・シンボル（すべて grep で実在確認済み）

| パス | シンボル / 行 | 役割 |
|---|---|---|
| `snotra-egui-runtime/src/input.rs` | `admit_key` (99), `held_since_focus_gain` (22), `input_trace` (77), `take` (137-166) | 抑止・計器・心拍 |
| `snotra-egui-runtime/src/runtime.rs` | `rx_key` 発出 (215-227), `Focused(true)` 分岐 (248) | tao 配送層の計器 |
| `snotra-egui-runtime/src/env.rs` | `trace_hatch_enabled` | 空文字＝未設定 |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `egui_results:show` 発出 (1084) | 停止地点 |
| `src-tauri/src/egui_shell/results_view.rs` | `request_icons_for_results` (164), `spawn_icon_load` (199) | アイコン抽出（別スレッド・viewport 範囲） |
| `scripts/lib/SnotraSmoke.psm1` | `Send-SnotraKey` (763), `Start-SnotraProcess` の `ExtraVariables` (338) | 注入と env 経路 |
| `scripts/smoke-egui.ps1` | 367 / 388-401 / 426 | results 表示後の打鍵注入 |
| `scripts/visual-check-colors.ps1` | 295-296 | 1 文字クエリのみ |

## 再利用できる既存パターン

- **使い捨てプロファイル + 実 config 複製**: `Start-SnotraProcess -ConfigDir` と予約名ガード
- **trace 待ち**: `Wait-SnotraTraceEvent` / `Wait-SnotraTraceCondition`（`Path` に stderr ログを渡す形）
- **前面化**: `Set-SnotraForegroundWindow` + `Get-SnotraForegroundWindowLabel`（失敗時の一次証拠を警告へ載せる）
- **足場の撤去条件を成果物自身の doc へ書く**: `repro-pester-flake.ps1` の `.NOTES` が先例
  （ただし「issue が閉じたら」は自己参照で発火しない・`scaffold-removal-condition-self-reference`）

## 敵対的調査（Step 3b）の採否

出力: `workspace/adversarial-999.txt`（212 行）。sonnet 1 体・静的一次証拠のみ。

| 所見 | 採否 | 理由 |
|---|---|---|
| **壊せた 1**: H1 は research.md 自身のヘッジよりさらに弱い（release は無条件に admit・focus 往復の発生源が無い） | **採用** | 機序を自分で一次証拠に当てて裁定した（`input.rs:102-105` と U-3）。**結論（`drop_key` の有無で決着）は変わらず、根拠が構造的理由へ置き換わった** |
| **壊せた 2**: 判定表の心拍二値が「フレームは回るが 1 枚が重い」中間状態を見落とす | **採用** | `results_view.rs:574-594` の drain + `load_texture` がイベントループスレッドであることを自分で実読。判定を `ts_ms` の階差へ変え、H3′ として独立の仮説に立てた |
| **壊せなかった 5**: `SNOTRA_TRACE` と `SNOTRA_EGUI_INPUT_TRACE` の二重要件 | **採用**（強化） | 制約 2b へ昇格。研究文書側では暗黙だった |
| 壊せなかった 1〜4（smoke:egui の注入順序 / 抽出の非メインスレッド性と viewport 限定 / 3 行が窓の取り違えを覆う / `ExtraVariables` の到達） | 現状維持 | 反証されなかったことの宣言として受け取る |

**機序の説明までは写していない。** 壊せた 1 の「focus 往復の発生源が無い」は全称否定であり、
敵対枠自身が確信中と申告した——research.md 側でも⚠️付きで、偽になる形を書いて残した。

## 実測（2026-08-26・`workspace/repro-999.ps1`・release `d913a775` 時点のビルド）

すべて実 config を複製した使い捨てプロファイル（`egui_results:show rows=200`）。生ログは
`%TEMP%/snotra-evidence-999*`（**リポジトリへ入れない**——`icon:extract_failed` が利用者の実パスを逐語で載せる）。

### 判定表のどの行にも当たらなかった——**沈黙が再現しない**

| 形 | 組数 | `egui_hide:done` | ON 側の `take` 階差 max |
|---|---|---|---|
| `-PostShowDelayMs 800 -DownCount 10`（#996 の diag＝通った側） | 1 組（OFF/ON） | **2/2 到達** | 490ms（**待ちの区間・重さではない**） |
| `-PostShowDelayMs 0 -DownCount 10`（#996 の測定＝沈黙した側の形） | 4 組 | **8/8 到達** | 129〜142ms |
| `-PostShowDelayMs 0 -DownCount 200`（スクロール量を #996 に合わせる） | 3 組 | **6/6 到達** | 143〜150ms |
| 同上 + `auto_update` を実 config の値へ戻す（D-2 の逸脱を戻す） | 2 組 | **4/4 到達** | 155ms |

**合計 10 組 20 回**（OFF 10 / ON 10。`%TEMP%` の run ディレクトリを数え直した）。
`rx_key` は注入数と一致し、`drop_key` は起動時の合成 2 件のみ。

**H2（フレーム不回転）も H3′（重いフレーム）も、この標本には現れていない。**
⚠️ ただし**計器なしの 10 回は H2 に対して構造的に盲目である**（`take` 行が出ない）。
言えるのは**計器つきの 10 回について**である。

### 代わりに確定したこと: **Down キーは届き、`ArrowDown` として egui へ積まれていた**

> **訂正（2026-08-26）**: ここには当初「ただし `ArrowDown` としてではない」と書き、
> 「アプリがどう解釈したかは決まらない」と結んでいた。**どちらも誤りである。**
> 同じログの `push_key` が `mapped=true` を 400/400 出しており、機序は下に書き直した。
> 経緯と教訓は `workspace/followup-issue-draft.md`。

`-DownCount 200` の計器つきの回で、注入 408（`SNOTRA_SMOKE_INJECT`）に対し `rx_key` 413 行
（＝**全部届いている**。差の 5 は起動時の合成と修飾キー）。physical の内訳は:

```
400 physical=Numpad2      ← 注入した VK_DOWN (0x28)
  3 physical=AltLeft / 2 ControlLeft / 2 KeyA / 2 Escape / 2 KeyK / 2 Unidentified(Windows(0))
```

**physical が `Numpad2` になる理由**: `Send-SnotraKey` は
`keybd_event($VirtualKey, 0, $flags, 0)` と撃つ（`SnotraSmoke.psm1:772-773`）——**`bScan` が 0 である**。
矢印キーは scancode で numpad と区別されるため、`bScan=0` では numpad 側に落ちる。

| probe | 撃ち方 | 届いた physical | egui が受けた key |
|---|---|---|---|
| 現状 | `keybd_event(0x28, 0x00, 0/2)` | `Numpad2` 400/400 | **`ArrowDown`**（`mapped=true`） |
| scancode も渡す | `keybd_event(0x28, 0x50, 0x1/0x3)` | **`ArrowDown` 40/40** | `ArrowDown` |

⚠️ 「拡張フラグだけ立てても `Numpad2` のまま（40/40）」も実測したが、**その枝はコミットした script に
残っていない**（`bScan` を渡す形へ上書きした）ため、**この 1 行だけ再現手順が無い**。判定には要らない。

**しかし physical は挙動を決めていない。** `push_key`（`input.rs:346-356`）が
`physical=Numpad2 repeat=false mapped=true` を **400/400** 出している。4 段で確かめた:

1. `active_key = key_from_tao(&event.logical_key).or_else(|| key_from_key_code(event.physical_key))`（`input.rs:336-337`）
2. `key_from_key_code` は KeyA–KeyZ しか map しない（`input.rs:435-468`）→ `Numpad2` では `None`。
   ゆえに `mapped=true` は **logical 側**から来ている
3. logical が `Character("2")`（NumLock ON の姿）なら WM_CHAR が伴い `rx_text` が 400 行出るが、**実測 2 行**
4. **NumLock を実測して OFF**（`GetKeyState(0x90)`）→ Numpad2 の logical は `ArrowDown`

**issue の (A)/(B) はどちらでもない——打鍵は届き、`ArrowDown` として実っていた。**
#996 が区別できなかったのは、issue 自身が書いたとおり**選択の移動を出す trace が無い**からで、
`push_key` の `mapped=` がその代わりになる。

⚠️ physical を読む消費者は `admit_key` の `held_since_focus_gain` だけであり、
押下と解放が同じ physical で対になる限り破れない。**注入と物理キーボードが混ざる状況では対にならない**が、
矢印を撃つ検査は現時点で 0 件である。

### #996 の全キー沈黙については

**20 回（OFF 10 / ON 10）回して 1 度も再現しなかった。** OFF 側＝ #996 と同じ計器なしの条件でも 0/10 なので、
「計器が現象を消した」ではない。`plan.md` D-1 の指示に従い、既知の逸脱（`auto_update` の無効化）を
戻した回（実 config の `auto_update = "full"`・`-DownCount 200`）も 2 組測ったが **4/4 とも到達**した
——**逸脱は原因ではない**。

複製したプロファイルの実効値（`profile.txt`）: `auto_update = "full"`（戻した回）/ `window_width = 300` /
`show_icons = true` / `font_family = "Segoe UI"` / `font_size = 13` / `max_results = <既定>`。

⚠️ **`p_lo` は計算しない。** `plan.md` D-4 の規則は「OFF 側が m 回中 k 回再現したら」を前提にしており、
**k = 0 ではその式が定義されない**。20 回 0 件から言えるのは
「**この機体・この日・この形では再現しない**」という下限の主張だけであり、
「#996 の観測が誤りだった」も「現在の main では起きない」も**これより強い**（#996 は 6/6 で観測している）。

## 未解決の疑問

- **U-1**: 6/6 の再現は計器なしの条件で得られたものである。計器を立てた回で再現率が落ちたとき、
  何回まで回して「再現しない」と言うか（率と機序を別の回で測る規律との兼ね合い）
- **U-2**: `take` の `focused=` と `GetForegroundWindow` が食い違う実行があるか（H1 の消去点が駆動されるかを決める）
- **U-3**: results 窓は `focusable(false)` + `SW_SHOWNOACTIVATE` で `Focused` を一度も受けない
  （`runtime.rs:246-249` のコメント）。main 側が results 表示に伴って `Focused` を受ける経路があるかは未確認
- **U-4**: 実ユーザーの手入力で同じことが起きる証拠は無い（#996 の測定は人手操作で完走し `icons.bin` を残した）。
  この非対称が (B) を支持するのか、単に手入力が遅いだけなのかは、計器の結果を見てから問う

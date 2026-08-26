# ADR-injected-arrow-key-physical-identity: 注入した矢印キーが `Numpad2` として届くことを欠陥として扱わない

## 文脈

`scripts/lib/SnotraSmoke.psm1` の `Send-SnotraKey` は `keybd_event($VirtualKey, 0, $flags, [UIntPtr]::Zero)` と撃つ。**`bScan` が 0 である。**

#999 の調査（実索引 313,028 件・release ビルド・`SNOTRA_EGUI_INPUT_TRACE` つき）で、注入した `VK_DOWN` (0x28) 400 件が本体側に **`physical=Numpad2`** として届くことを実測した。`physical=ArrowDown` は 0 件である。

**この観測から「矢印キーが numpad として配送される欠陥である」と一度結論し、修正 issue の草案まで書いた。その結論は偽であった。** 同じログの `push_key`（`snotra-egui-runtime/src/input.rs`）が `physical=Numpad2 mapped=true` を 400/400 出しており、`mapped` は「egui のキーとして積まれた」を意味する。導出は 4 段:

1. `active_key = key_from_tao(&event.logical_key).or_else(|| key_from_key_code(event.physical_key))`
2. `key_from_key_code` は `KeyA`〜`KeyZ` しか map しないので `Numpad2` では `None`。ゆえに `mapped=true` は **logical 側**から来ている
3. logical が `Character("2")`（NumLock ON の姿）なら WM_CHAR が伴い `rx_text` が 400 行出るはずだが、実測は 2 行
4. `GetKeyState(0x90)` で NumLock を実測して OFF——NumLock OFF の `Numpad2` の logical は `ArrowDown`

⚠️ **`bScan` と拡張ビットのどちらが `Numpad2` を招いたかは確定していない。** 現象（`bScan=0` で `Numpad2` になる／`bScan=0x50` + `KEYEVENTF_EXTENDEDKEY` で `ArrowDown` になる）までが実測である。

## 決定

**`Send-SnotraKey` を直さない。** 注入した矢印キーが `physical=Numpad2` として届くことを欠陥として扱わない。

## 検討した代替案と却下理由

- **`Send-SnotraKey` に `-Extended` を足し、`MapVirtualKey` で導いた scancode と `KEYEVENTF_EXTENDEDKEY` を渡す**: 却下。**直す理由が無い。** egui のイベントへ載る `physical_key` は `key_from_key_code` 経由であり（`input.rs` の `on_keyboard_event`）、A–Z しか map しないので **`Numpad2` でも `ArrowDown` でも等しく `None`** になる。下流に両者を区別する材料が無く、`Numpad2` として届いた 400 件はすべて `ArrowDown` として実っている。
- **`keybd_event` を `SendInput` へ移す**: 上と同じ理由で却下。加えて `Send-SnotraKey` は**打鍵注入の唯一の実装**であり（smoke / Pester / 視覚検査のすべてがここを通る）、動機の無い書き換えは全検査の入力層を一度に動かす。
- **「矢印を撃つ検査を書いた人が最初に踏む穴」として先回りで直す**: 却下。**踏まない**——その人の打鍵も `ArrowDown` として実る。なお現時点で矢印キーを注入する検査は 0 件である（`scripts/` の `*.ps1` / `*.psm1` を `0x25`〜`0x28` で走査・実測）。
- **`admit_key` の `held_since_focus_gain` が `Numpad2` と `ArrowDown` を別のキーとして数える点を塞ぐ**: 却下。**この筋書きは成立しない**——`held.insert` は `if is_synthetic` の枝にしか無く、通常の押下はそもそも集合へ入らない。
  - この却下理由は**書き直したものである**。当初は「押下と解放が同じ physical で対になるので破れない」と条件つきで書いたが、その条件は発動しない枝の話だった。**判定が正しくても、添えた機序は独立に誤りうる**（ルート `CLAUDE.md`「レビューの委譲では〜機序まで一次証拠で裁定する」の実例）。
- **`rx_key` に logical も載せる**: 却下（当面）。#999 では `push_key` の `mapped=` が同じ問いに答えたため、新しい出力を足さずに決着した。`mapped=false` が出て初めて「何として届いたか」が要る。

## 帰結

- **`physical=Numpad2` を見ても、それだけでは「Down が効いていない」と読まないこと。** `push_key` の `mapped=` を併せて見る。`rx_key` は配送を、`push_key` は適用を答える別々の層である。
- **計器の一覧を散文へ写さない。** #999 は `research.md` に 4 種の表を作り、その表を母集団として扱ったせいで `push_key` を見に行かなかった。実際の `input_trace(` は 6 種（`drop_key` / `push_key` / `push_text` / `rx_key` / `rx_text` / `take`）である。母集団はソースを走査して得ること——`grep -h -A 1 'input_trace(' …`（rustfmt が種別のリテラルを次行へ折るため `-A 1` が要る。1 行前提の式は 0 件を返す）。
- **#999 が報告した「results 表示後の全キー沈黙」は、この ADR の射程外である。** 20 回（計器なし 10 / あり 10）測って 1 度も再現せず、帰属は決まっていない。**次に遭遇したときの測り方だけをここへ残す**——再現ハーネス自体は `workspace/` に住んでいてサイクル末に消えるため:
  - `Start-SnotraProcess -Trace -ExtraVariables @{ SNOTRA_EGUI_INPUT_TRACE = '1' }` で**2 系統を同時に立てる**（`SNOTRA_TRACE` と `SNOTRA_EGUI_INPUT_TRACE` は独立で、前者が `egui_results:show`、後者が `rx_key` / `push_key` / `take` を出す）
  - **注入側の計器は呼び出し側 PowerShell の env が握る**。`Start-SnotraProcess` は env をその中でだけ立てて戻すので、打鍵を撃つ区間で `$env:SNOTRA_EGUI_INPUT_TRACE` を立て直さないと `SNOTRA_SMOKE_INJECT` が 1 行も出ない
  - 注入時刻は情報ストリームへ出るので `Send-SnotraKey ... 6>> $log` で拾う（本体の `rx_key` は子プロセスの stderr で別ストリーム。時計はどちらも epoch ms）
  - 判定は `take` の**行の有無ではなく `ts_ms` の階差**で行い、**階差が伸びただけでは重さを名乗らない**（待ちの区間でも伸びる）

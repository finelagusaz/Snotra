# #999 の結論（issue へ貼る 4 点）

一次証拠は `workspace/research.md`「実測（2026-08-26）」、ハーネスは `workspace/repro-999.ps1`。
生ログは `%TEMP%/snotra-evidence-999*` に置き、**リポジトリへは入れていない**
——`[trace]` の `icon:extract_failed` が利用者の実ファイルパスを逐語で載せるためである。

## 1. Down キーは本体へ届いていたか — **届いていた。`ArrowDown` として egui へ積まれていた**

**issue の (B) は偽である。** `SNOTRA_EGUI_INPUT_TRACE` を立てた回で、
1 回あたり **`SNOTRA_SMOKE_INJECT` 408 行に対し `rx_key` 413 行**（Down 400 = down/up 各 200、
残りは hotkey・クエリ 1 文字・Escape と起動時の合成）。**配送は 1 つも落ちていない。**
**(A) も偽である**——`take` の心拍は最後まで途切れていない。

**egui まで届いていた。** `push_key` 行（`input.rs:346-356`）が
`physical=Numpad2 repeat=false mapped=true` を **400/400** 出している。`mapped` は
`active_key.is_some()`＝egui のキーとして積まれたことを意味する。

**その `active_key` は `ArrowDown` である**（自分で 4 段確かめた）:

1. `active_key = key_from_tao(&event.logical_key).or_else(|| key_from_key_code(event.physical_key))`（`input.rs:336-337`）
2. `key_from_key_code` は **KeyA–KeyZ しか map しない**（`input.rs:435-468`）ので `Numpad2` では `None`
   ——ゆえに `mapped=true` は **logical 側**から来ている
3. logical が `Character("2")`（NumLock ON の姿）なら WM_CHAR が伴い `rx_text` が 400 行出るはずだが、
   **実測は 2 行**である
4. **NumLock を実測して OFF**（`GetKeyState(0x90)`）——NumLock OFF の Numpad2 の logical は `ArrowDown`

**`physical=Numpad2` は挙動に影響しない見かけである。** 原因は `Send-SnotraKey` の
`keybd_event($VirtualKey, 0, $flags, 0)`（`SnotraSmoke.psm1:772-773`）で `bScan` が 0 なことだが、
tao の logical 解決がこれを吸収している。physical を読む消費者は
`admit_key` の `held_since_focus_gain` だけであり、そこは押下と解放が同じ physical で対になるので破れない。

| probe | 撃ち方 | 届いた physical | egui が受けた key |
|---|---|---|---|
| 現状 | `keybd_event(0x28, 0x00, 0/2)` | `Numpad2` 400/400 | **`ArrowDown`**（`mapped=true`） |
| scancode も渡す | `keybd_event(0x28, 0x50, 0x1/0x3)` | `ArrowDown` 40/40 | `ArrowDown` |

⚠️ 中段の「拡張フラグだけ立てても `Numpad2` のまま（40/40）」も実測したが、
**その枝はコミットした script に残っていない**（`bScan` を渡す形へ上書きした）ため、
**この 1 行だけ再現手順が無い**。判定には要らない（上表の 2 行で足りる）。

**#996 が (A)/(B) を区別できなかったのは、issue 自身が書いたとおり選択の移動を出す trace が無いからである。**
`SNOTRA_EGUI_INPUT_TRACE` の `push_key`（`mapped=`）がその代わりになる——**この計器は「打鍵が egui へ実ったか」まで見える。**

## 2. 沈黙の契機 — **契機を特定する前に、沈黙が再現しなかった**

**20 回（計器なし 10 / あり 10）回して 0 件**。内訳（`%TEMP%` の run ディレクトリを数え直した）:

| 形 | 組数 | `egui_hide:done` | ON 側の `take` 階差 max |
|---|---|---|---|
| `-PostShowDelayMs 800 -DownCount 10`（#996 diag＝通った側） | 1 | 2/2 | **490ms** |
| `-PostShowDelayMs 0 -DownCount 10`（#996 測定＝沈黙した側の形） | 4 | 8/8 | 129〜142ms |
| `-PostShowDelayMs 0 -DownCount 200`（スクロール量を合わせる） | 3 | 6/6 | 143〜150ms |
| 同上 + `auto_update` を実 config の値へ戻す | 2 | 4/4 | 155ms |

`rx_key` は注入数と一致し、`drop_key` は起動時の合成 2 件のみ。

**490ms の階差は重いフレームではない**——`-PostShowDelayMs 800` の待ちの中で、
**入力も再描画要求も無いために `take` が呼ばれていないだけ**である。
**ゆえに「階差が伸びる → フレームが重い」は成り立たない**（`research.md` の判定表が持っていた
偽陽性の口であり、ここで明示的に宣言する）。**階差を重さの証拠にするには、
同じ区間に入力か再描画要求が在ることを併せて示す必要がある。**

**H2（フレーム不回転）も H3′（アイコン適用のバーストでフレームが重くなる）も、この標本には現れていない。**

⚠️ **ただし計器なしの 10 回は H2 に対して構造的に盲目である**（`take` 行が出ないため）。
上の「現れていない」が言えるのは**計器つきの 10 回についてだけ**である。

**計器のせいで沈黙が消えたのではない**——計器なしの 10 回でも `egui_hide:done` へ到達している。

⚠️ **「現在の main では起きない」とは言えない。** 20 回 0 件から言えるのは
「この機体・この日・この形では再現しない」という下限の主張だけで、#996 は 6/6 で観測している。

## 3. `smoke:egui` / `check:colors` への影響 — **どちらも影響を受けない**

- **矢印キーを注入する既存の検査は 1 つも無い**（`scripts/` の `*.ps1` / `*.psm1` を `0x25`〜`0x28` で走査して 0 件）。
  ゆえに **1 の欠陥は既存の検査の射程外**である
- `smoke:egui` は `egui_results:show` の後に打鍵する条件へ**既に入っている**（`smoke-egui.ps1:367` で観測 →
  `388-401` で BackSpace / `c` / Shift+`;` / `\` → `426-428` で Escape）が、**2 の沈黙は再現しなかった**
- `check:colors` は 1 文字クエリを 1 回注入するのみ（`visual-check-colors.ps1:295-296`）
- **false green にはならない**（issue の記述どおり）。`smoke:egui` の失敗はすべて `$failures` への追加＝ exit≠0 であり、
  観測タイムアウトが検査項目になっている

## 4. 帰属 — **製品の欠陥は見つからなかった。注入経路の欠陥も見つからなかった**

- **1 は欠陥ではない。** 打鍵は届き、`ArrowDown` として egui へ積まれていた。
  `physical` が `Numpad2` と読めるのは `bScan=0` の見かけであり、挙動には影響しない
- **2（全キー沈黙）は帰属を決められない。** 20 回とも再現しなかったので、
  製品側の欠陥とも注入経路固有とも言えない。#996 の測定は人手操作では完走して `icons.bin` を残しており、
  実ユーザーの手入力で同じことが起きる証拠は今も無い

## 残余と、次に何をするか

- **R-1 は取り下げた。** 当初「`Send-SnotraKey` の `bScan=0` が矢印キーを numpad として配送する欠陥である」と
  書いたが、**`push_key` の `mapped=true` と NumLock の実測で前提が崩れた**（§1）。
  `Send-SnotraKey` を直す理由は**この調査からは出ていない**
  - ⚠️ **残る小さな含み**: `physical` を読む唯一の消費者である `admit_key` の
    `held_since_focus_gain` は、`Numpad2` と `ArrowDown` を**別のキーとして数える**。
    押下と解放が同じ physical で対になる限り破れないが、
    **「注入は `Numpad2`・物理キーボードは `ArrowDown`」が混ざる状況では対にならない**。
    今そういう検査は無い（下の 3 のとおり矢印を撃つ検査が 0 件）ので、**issue にはしない**
- **R-2（2 の扱い）**: 再現しないものを追い続けない。**次に遭遇したときに一撃で割れる状態は既に作った**
  ——`workspace/repro-999.ps1` の形（`SNOTRA_TRACE` と `SNOTRA_EGUI_INPUT_TRACE` を併用し、
  注入時刻を `6>>` で拾う）と、`research.md` の判定表がそれである
- **R-3（判定表の口）**: 「`take` の階差が伸びる → フレームが重い」は偽陽性を持つ（§2）。
  待ちの区間でも伸びる。**次に使う人が同じ誤読をしないよう、判定表側にも書いた**

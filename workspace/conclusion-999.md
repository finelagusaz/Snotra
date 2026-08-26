# #999 の結論（issue へ貼る 4 点）

一次証拠は `workspace/research.md`「実測（2026-08-26）」、ハーネスは `workspace/repro-999.ps1`。
生ログは `%TEMP%/snotra-evidence-999*` に置き、**リポジトリへは入れていない**
——`[trace]` の `icon:extract_failed` が利用者の実ファイルパスを逐語で載せるためである。

## 1. Down キーは本体へ届いていたか — **届いていた。ただし `ArrowDown` としてではない**

`SNOTRA_EGUI_INPUT_TRACE` を立てた 3 回（各 200 打鍵）で、**注入 400 に対し `rx_key` 413 行**。
配送は 1 つも落ちていない。しかし physical の内訳は **`Numpad2` が 400 件**で、`ArrowDown` は 0 件だった。

**機序**（対照を取って確定）: `Send-SnotraKey` は `keybd_event($VirtualKey, 0, $flags, 0)` と撃つ
（`scripts/lib/SnotraSmoke.psm1:772-773`）——**`bScan` が 0 である**。矢印キーは scancode で numpad と
区別されるため、`bScan=0` では numpad 側へ落ちる。

| probe | 撃ち方 | 届いた physical |
|---|---|---|
| 現状 | `keybd_event(0x28, 0x00, 0/2)` | `Numpad2` 400/400 |
| 拡張フラグだけ | `keybd_event(0x28, 0x00, 0x1/0x3)` | **`Numpad2` 40/40**（変わらない） |
| scancode も渡す | `keybd_event(0x28, 0x50, 0x1/0x3)` | **`ArrowDown` 40/40** |

**issue の (A)/(B) はどちらでもない。** 配送は止まっておらず（(A) 偽）、届いていないのでもない（(B) 偽）。
**別人格として届いていた。** #996 がこれを区別できなかったのは、issue 自身が書いたとおり
**選択の移動を出す trace イベントが無い**からである。

⚠️ `Numpad2` をアプリがどう解釈したかは決まらない——`rx_key` は physical しか出さず logical は載らない。
`egui_input:changed` は 1 件（最初の `A`）だけなので**文字としては入っていない**が、選択が動いたかの観測点は無い。

## 2. 沈黙の契機 — **契機を特定する前に、沈黙が再現しなかった**

18 回（計器なし 9 / あり 9）回して **0 件**。内訳:

| 形 | 組数 | `egui_hide:done` |
|---|---|---|
| `-PostShowDelayMs 800 -DownCount 10`（#996 diag＝通った側） | 1 | 2/2 |
| `-PostShowDelayMs 0 -DownCount 10`（#996 測定＝沈黙した側の形） | 4 | 8/8 |
| `-PostShowDelayMs 0 -DownCount 200`（スクロール量を合わせる） | 3 | 6/6 |
| 同上 + `auto_update` を実 config の値へ戻す | 2 | 4/4 |

計器つきの回はいずれも健全: `take` の階差 max **129〜150ms**（心拍の間引き 100ms のすぐ上＝**重いフレームは無い**）、
`rx_key` は注入数と一致、`drop_key` は起動時の合成 2 件のみ。
**H2（フレーム不回転）も H3′（アイコン適用のバーストでフレームが重くなる）も、この標本には現れていない。**

**計器のせいで消えたのではない**——計器なしの 9 回でも 0 件である。

⚠️ **「現在の main では起きない」とは言えない。** 18 回 0 件から言えるのは
「この機体・この日・この形では再現しない」という下限の主張だけで、#996 は 6/6 で観測している。

## 3. `smoke:egui` / `check:colors` への影響 — **どちらも影響を受けない**

- **矢印キーを注入する既存の検査は 1 つも無い**（`scripts/` の `*.ps1` / `*.psm1` を `0x25`〜`0x28` で走査して 0 件）。
  ゆえに **1 の欠陥は既存の検査の射程外**である
- `smoke:egui` は `egui_results:show` の後に打鍵する条件へ**既に入っている**（`smoke-egui.ps1:367` で観測 →
  `388-401` で BackSpace / `c` / Shift+`;` / `\` → `426-428` で Escape）が、**2 の沈黙は再現しなかった**
- `check:colors` は 1 文字クエリを 1 回注入するのみ（`visual-check-colors.ps1:295-296`）
- **false green にはならない**（issue の記述どおり）。`smoke:egui` の失敗はすべて `$failures` への追加＝ exit≠0 であり、
  観測タイムアウトが検査項目になっている

## 4. 帰属 — **1 は注入経路固有。2 は帰属を決められない**

- **1（Down が `ArrowDown` として届かない）は注入経路固有である。** 原因は `Send-SnotraKey` の `bScan=0` であり、
  物理キーボードは scancode と拡張ビットを伴う。**製品の欠陥ではない**
- **2（全キー沈黙）は帰属を決められない。** 再現しなかったので、製品側の欠陥とも注入経路固有とも言えない。
  #996 の測定は人手操作では完走して `icons.bin` を残しており、実ユーザーの手入力で同じことが起きる証拠は今も無い

## 残余と、次に何をするか

- **R-1（1 の直し方）**: `Send-SnotraKey` に scancode と `KEYEVENTF_EXTENDEDKEY` を渡す形へ直すか、
  `SendInput` へ移す。**いま矢印キーを撃つ検査が無いので急がない**が、
  **撃つ検査を書いた人が最初に踏む**性質の穴である（黙って numpad として届く）
- **R-2（2 の扱い）**: 再現しないものを追い続けない。**次に遭遇したときに一撃で割れる状態は既に作った**
  ——`workspace/repro-999.ps1` の形（`SNOTRA_TRACE` と `SNOTRA_EGUI_INPUT_TRACE` を併用し、
  注入時刻を `6>>` で拾う）と、`research.md` の判定表がそれである
- **R-3**: `rx_key` は physical しか出さない。1 のような「別人格として届く」型を将来また割るなら logical も要る

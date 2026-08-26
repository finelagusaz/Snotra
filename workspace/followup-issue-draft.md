# フォローアップ issue の草案（R-1）——**起票はユーザーの指示を待つ**

タイトル案:

    fix(smoke): Send-SnotraKey が矢印キーを numpad として配送している（bScan=0）

本文案:

---

## 何が起きているか

`scripts/lib/SnotraSmoke.psm1` の `Send-SnotraKey` は次の形で撃つ:

```powershell
$flags = if ($Up) { 0x2 } else { 0 }
[SnotraSmokeInterop.Native]::keybd_event($VirtualKey, 0, $flags, [UIntPtr]::Zero)
```

**`bScan` が 0 である。** 矢印キーは scancode（と lParam の拡張ビット）で numpad と区別されるため、
`VK_DOWN` (0x28) を撃つと本体には **`physical=Numpad2`** として届く。#999 の調査で実測した:

| probe | 撃ち方 | 届いた physical |
|---|---|---|
| 現状 | `keybd_event(0x28, 0x00, 0/2)` | `Numpad2` **400/400** |
| 拡張フラグだけ足す | `keybd_event(0x28, 0x00, 0x1/0x3)` | **`Numpad2` 40/40**（変わらない） |
| scancode も渡す | `keybd_event(0x28, 0x50, 0x1/0x3)` | **`ArrowDown` 40/40** |

観測は `SNOTRA_EGUI_INPUT_TRACE` の `rx_key` 行（release ビルド・実索引 `rows=200`）。

## なぜ今まで表に出なかったか

**矢印キーを注入する検査が 1 つも無い**（`scripts/` の `*.ps1` / `*.psm1` を `0x25`〜`0x28` で走査して 0 件）。
ゆえに現時点で赤くなっている検査は無い。

## なぜ直す価値があるか

**沈黙する形の穴だからである。** 矢印キーを撃つ検査を書いた人は、
「打鍵は届いているのに選択が動かない」を最初に踏む——`keybd_event` は成否を返さず、
本体側にも選択の移動を出す trace が無いので、**注入が効いていないことと、アプリが反応しないことが同じ見た目になる**。
#999 はまさにこの見分けが付かない状態から始まった。

## 直し方の候補

- `Send-SnotraKey` に `-Extended` を足し、`MapVirtualKey(vk, MAPVK_VK_TO_VSC)` で scancode を導いて渡す
- `keybd_event` を `SendInput` へ移す（scancode と拡張ビットを構造体で渡せる）

**打鍵の実装は 1 か所である**（`Send-SnotraKey`）ため、直す面も 1 か所で済む。

## 撤去条件

修正がマージされたら閉じる。

---

## 併せて（issue にはしない・記録のみ）

`rx_key` は physical しか出さず logical を載せない。#999 の「別人格として届く」型を将来また割るなら
logical も要る——ただし**今それを足す理由は無い**（矢印を撃つ検査が無い）。上の修正が入り、
矢印を撃つ検査が現れたときに再考する。

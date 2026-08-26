# フォローアップ issue の草案（R-1）——**取り下げた。起票しない**

2026-08-26。当初ここには
「`Send-SnotraKey` が矢印キーを numpad として配送している（`bScan=0`）」という issue の草案を書いていた。

**前提が崩れたので取り下げる。**

## 何が誤りだったか

草案は「`VK_DOWN` を撃つと本体には `physical=Numpad2` として届く」を根拠に、
**アプリが Down として扱えていない**と含意していた。physical の観測は正しかったが、**含意が誤りだった。**

同じログに `push_key`（`input.rs:346-356`）が
`physical=Numpad2 repeat=false mapped=true` を **400/400** 出しており、`mapped` は
**egui のキーとして積まれたこと**を意味する。`key_from_key_code` は KeyA–KeyZ しか map しない
（`input.rs:435-468`）ので、その `Some` は logical 側＝`key_from_tao` から来ている。
`rx_text` が 400 押下に対し 2 行しか無く、NumLock の実測が OFF であることから、
**logical は `ArrowDown` である**。

**打鍵は正しく届いていた。** `bScan=0` は physical の見かけを変えるだけで、tao の logical 解決が吸収している。

## なぜ「直す価値がある」も消えるか

草案は「矢印キーを撃つ検査を書いた人が最初に踏む」と書いたが、**踏まない**——その人の打鍵も
`ArrowDown` として届く。**残るのは physical の表示だけで、それを読む消費者は
`admit_key` の `held_since_focus_gain` 1 か所である**（押下と解放が同じ physical で対になるので破れない）。

## 教訓（`RETROSPECTIVE.md` へ回す候補）

**計器の 1 行だけを見て機序を決めた。** `rx_key` は physical しか出さないので
「別人格として届いた」と読めたが、**同じ計器がもう 1 段深い `push_key` を出しており、
そちらが `mapped=` で「egui へ実ったか」を答えていた**。
`research.md` の計器表は 4 種を挙げて `push_key` を落としており、
**表を書いた自分が、表に無い行を探しに行かなかった。**

一次証拠は `workspace/conclusion-999.md` §1。

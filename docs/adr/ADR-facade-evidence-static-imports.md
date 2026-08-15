# ADR-facade-evidence-static-imports: facade の evidence 用静的 import を残す（#1094）

## 文脈

`scripts/governance-check.mjs`（facade）の再輸出を実際の消費者まで絞ったとき（#1094）、facade が `checks/` を静的 import する箇所が 2 つ残った——evidence の算出に使う `clippyDisallowedCount`（`G-clippy-disallowed.mjs`）と `adrFiles`（`G-adr-file-names.mjs`）である。

その検査モジュールは、隣のテストごと消えても `ERR_MODULE_NOT_FOUND` で落ちる。すなわち **#1088 の manifest 差分が唯一の検知器になる射程から外れる**。「せっかく絞ったのだから全部揃えたい」という圧力が構造的に掛かる面である。

**この 2 つは、射程から外れる経路の全部ではない。** `governance/instrument.mjs` が計器の算出のため `G-skill-table` を、`checks/` の外に在るテスト（`governance/lib.test.mjs` / `governance-check.test.mjs`）が数本を、それぞれ独立に静的 import している。**数を書かない**——母集団の導出も射程の記述も `scripts/governance-manifest.test.mjs`「フォールトインジェクション — 検査 ID が manifest の集合から消えたときに diffManifest／undeclared が発火するかの実測（#1088）」が正本であり、**本 ADR はそれを指すだけにする**（走査コマンドの写しをここへ置かない）。

本 ADR が扱うのは、このうち **facade の evidence 経由の分だけ**である。

## 決定

**3 本とも残す。** 前 2 本は #1093 から在る意図的な設計であり、facade の当該 import 直上のコメントが「登録行と違い、ファイルが消えれば import が失敗して鳴る」という意図の正本を持つ。3 本目は `instrument.mjs` の関心（計器の算出）であって facade の公開面の問題ではない。

## 却下した案: evidence の導出を `ctx.record` 経由へ移す

`G-clippy-disallowed.mjs` / `G-adr-file-names.mjs` の `run()` を `ctx.record("clippy", …)` の形へ変え、facade は `ctx.clippy` を読む。既存の 4 項目（見出し参照・散文の識別子・近傍の見出し参照・ADR の短縮引用）と同じ形になり、facade からの静的 import が落ちる。

**却下する。理由は 2 つある。**

### 1. 交換する不変条件が同格ではない

静的 import は「**ファイルが消えれば必ず鳴る**」を無条件で保証する。`ctx.record` は「**検査側が record を呼ぶ限り**値が届く」という条件付きの保証に落ちる。前者は機構が持ち、後者は規約が持つ。`docs/development-principles.md`「構造的設計原則と強制の階梯」の向きに対して逆行する。

### 2. その条件が破れたとき、両方の検査層が沈黙する（実測）

`ctx.record` の呼び忘れは**誰にも捕まらない**。稼働中のツリーへは触れず使い捨て worktree で、`G-heading-refs.mjs` の `run()` から `ctx.record` の呼び出しだけを外し（findings は正しく返したまま）測った。

```
governance:check — 全検査 passed（… / 見出し参照 undefined 件を md 47 件 + .rs 101 件から照合 / …）
exit=0
```

```
Test Files  31 passed (31)
     Tests  721 passed (721)
vitest exit=0
```

**evidence が `undefined 件` と印字しながら exit 0 で通り、`npm test` も全緑である。** 検出は exit code、出力は証拠（#471）という契約の下で、この経路は exit code を一切動かさない。

**この沈黙経路は却下案が新設するものではなく、既に在る。** evidence の一部は今日すでに `ctx.record` 経由であり、上の測定はその現存の性質を測ったものである。却下案が行うのは**その経路を広げること**であり、失う静的 import の対価としては割に合わない。

（この位置づけは実測で 1 段ずれた。計画レビューの独立導出は「却下案が新しい沈黙経路を作る」と報告したが、所見は正しく、機序は「既にある経路を広げる」が正しい。）

## 却下した案: 残余を隠して「全検査が manifest で守られる」と書く

**却下する。** 偽の全称になる。射程は少なくとも 2 方向で限定される——(1) `checks/` の外から静的 import されている検査、(2) `.mjs` 単独消失は隣のテストが捕まえるため manifest が唯一になるのはペア消失のときだけ。どちらも実測済みで、射程の正本は `scripts/governance-manifest.test.mjs` のフォールトインジェクション節に置いた。

**この項は #1094 のサイクル自身が踏んだ形でもある。** 調査段階の走査コマンドが `grep -v '\.test\.mjs:'` でテストファイルを一律に母集団から外し、**兄弟でないテスト**（`governance/lib.test.mjs` / `governance-check.test.mjs`）による静的 import を丸ごと落とした。その結果「残余は 3 本」という数え上げが計画・ADR 草案・PR 本文まで伝播し、実装後のレビューが全 19 本のペア消失を実測して訂正した。**「テストの import は隣のものだけ」は前提であって観測ではない。**

## 残余

- 上記の `ctx.record` 呼び忘れの沈黙経路は、`ctx.record` を使う evidence 項目について**今日も開いている**。この ADR は塞いでいない——塞ぐなら「evidence 文字列が `undefined` を含まない」という 1 行のカナリアが最小の手当てである。#1094 の射程外として受容する。

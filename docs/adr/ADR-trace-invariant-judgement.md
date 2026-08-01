# ADR-trace-invariant-judgement: results 窓の trace から不変条件を判定する（#757）

trace の presence（イベントが出たか）ではなく「起きてはならないことが起きていないか」を判定する仕組みを `scripts/lib/SnotraTraceInvariants.psm1` へ置いた。**採用した形はコードとテストが持つ**ので、ここには**却下した代替案と、却下の理由**だけを記す。

## 却下 1: 区間ごとに独立して判定する（issue #757 本文の素案）

各目視項目の区間で trace を切り、その中だけで H1/H4/H5 を評価する案。

- **境界を跨ぐ違反を落とす。** 項目 N で `egui_hide:done` が出て、違反の `egui_results:show` が項目 N+1 の区間に落ちると、どちらの区間内にも収まらず消える。#671 PR A′ の事故は「hide の後に results が残る」形であり、**人が次の項目へ進む間に現れるのが自然**なので、これは例外ではなく主経路である
- 採った形（trace 全体を `seq` 昇順に 1 パスで舐め、違反イベントの `seq` が属する区間へ**帰属**させる）は、区間マーカーを評価の境界ではなく帰属の道具として使う。回帰点は `SnotraTraceInvariants.Tests.ps1` の「区間の境界を跨ぐ H1 違反を落とさず、hide のあった区間へ帰属させる」

## 却下 2: 「捨てた行」を「全行 − parse 成功」で数える

`Read-SnotraTraceEvents` が返さなかった行をすべて「捨てた行」と見なす案。

- **正常な実行が毎回 degrade して検出器が無意味になる**（実測）。2026-07-30 の `manual-smoke` が残した実ログは全 25 行のうち `[trace]` 行が 24（すべて整形式）で、残る 1 行は `[index-load] cache_hit=true total=785ms ...` という**非 trace の診断行**だった。素朴な差分では捨てた行が常に 1 以上になり、`PASS` が毎回 `SKIP` へ落ちる
- 採った形は「**`[trace]` で始まるのに parse できなかった行**」だけを数える。この規則は degrade の意味を決める判定そのものなので `Read-SnotraTraceSnapshot`（`scripts/lib/SnotraSmoke.psm1`）が単独で持ち、呼び出し側に写しを置かない

## 却下 3: 捨てた行があれば `smoke-egui.ps1` を赤にする

`PASS` → `SKIP` の degrade に加え、捨てた行が 1 行でもあれば `$failures` へ足す案（fail-closed）。

- **今日は無害に済んでいる状態を CI の赤へ変える。** 移行前の判定は生行の部分一致だったため、途中で切れた行があっても event 名だけは拾えていた。`e2e.yml` で走る smoke に、不変条件と無関係な理由で落ちる経路を新設することになり、#757 が求めていない
- 採った形は degrade だけを適用し、捨てた行数は証拠として出す。**赤にするのは不変条件の違反と判定器の停止だけ**である

## 却下 4: `egui_hide:done` のたびに H1 の窓を開き直す

hidden 窓の開始を、`egui_hide:done` を見るたびに更新する案。

- **2 つの hide に挟まれた違反が評価から消える。** `hide_egui_main`（`src-tauri/src/egui_shell/window_coordinator.rs`）は呼び出し点が 2 つあり、片方（`EGUI_HIDE_REQUESTED` listener）に可視性ガードが無いため `egui_hide:done` は遷移を問わず出る。連続して出た hide の間に現れた違反が、窓の打ち直しで区間の外へ落ちる
- 採った形は「未 hidden → hidden」の遷移でだけ窓を開き、連続する hide は**開いている窓を延長する**

## 却下 5: 目視項目の一覧を PR 本文と `$items` で 1 つへ寄せる（#757 コメントの「二重の正本」）

`scripts/manual-smoke.ps1` の `$items` と PR 本文の目視表を、どちらかを正本として同期させる案。

- **両者は写しではなく別の母集団である。** `docs/adr/ADR-folder-location-display-surface.md`「却下 6」が確定させたとおり、`$items` はどの変更でも壊れうる**横断不変条件**の常設項目で、PR 本文の表はその PR 限りの受け入れ確認である。寄せると、機能単位の確認が全 PR に恒久コストとして乗る
- 直したのは**主張のほう**である——「項目の SSOT は PR 本文の目視表であり `$items` はその写しである」という記述（`docs/build-commands.md` とスクリプト冒頭）が構造的に保てないので、別母集団であると書き改めた

## 却下 6: H1 の判定を `smoke-egui.ps1` と `manual-smoke.ps1` で別々に持つ

移行コストを避け、`smoke-egui.ps1` の orphan 検出（生行の正規表現）をそのまま残す案。

- **同じ不変条件の導出が 2 か所になる。** `AGENTS.md`「検証の作法」が禁じる形で、放置すれば片方だけが直る
- ただし**移行は挙動同値ではない**（当初そう書いて誤っていた）。旧判定は JSON の妥当性を要求しないので、途中で切れた行からも event 名を拾えた。この差は却下 3 の degrade で受け止め、`$failures` は増やさない

## 射程外として残したもの（却下ではなく未着手）

- **#760**（main が作業領域の下端を割ると results が丸ごとタスクバー下へ入る）は H1〜H5 のどれでも捕まらない。**位置が trace に載っていない**ため、判定するには trace のスキーマ変更が要る
- `C:/tmp/snotra836-tools/` のカテゴリ D 治具（打鍵注入と窓矩形キャプチャ）のリポジトリ取り込みは別軸の問い

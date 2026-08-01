# plan — #757 results 窓の trace 不変条件（H1/H4/H5）を manual-smoke へ入れる

## 目的

`scripts/manual-smoke.ps1` が trace に対してしているのは **presence の表示**（イベントの件数と最後の 1 行を並べる）であって、#757 が求める「**起きてはならないことが起きていないか**」ではない。#671 PR A′ はまさにその形の回帰——`egui_results:hide` は出たのに窓は残った——を presence 検査が緑のまま通した。

本件は残る本体 3 つを入れる: **区間マーカー**・**H1/H4/H5 の自動判定**・**目視結果との並置**。

## 受け入れ条件

1. `Test-SnotraTraceInvariants` が H1 / H4 / H5 を判定し、**3 つとも故意の違反列で FAIL することを Pester が実測する**（フォールトインジェクション・`.claude/rules/safety-nets.md`）
2. **判定不能は PASS へ化けない**——「該当イベントが 1 件も無い」「`rows` が読めない」「main の可視状態が未観測」「区間が閉じていない」「trace が無い」「**parse できず捨てた行がある**」のすべてが `SKIP` として現れる。**沈黙経路を列挙して全部塞ぐ**
3. **hide 側の非対称が織り込まれている**——`egui_results:hide` の連続を H5 が FAIL としない（`hide_egui_main` は遷移していなくても出す・要求レベル）
4. `manual-smoke.ps1` の記録に、項目ごとの**目視判定と trace 判定が並ぶ**。**不一致は明示される**（目視 PASS × trace FAIL は特に目立たせる）
5. trace 判定の FAIL は **exit code 1** に届く（検出は exit code・#471）
6. `docs/build-commands.md` の「二重の正本」の記述が現実と整合する
7. **H1 の導出が 1 か所になる**——`smoke-egui.ps1` の orphan 検出（`smoke-egui.ps1:394-415`）が新モジュールを呼び、移行前後で判定が一致することを実測している（D6）

## 設計上の決定（実装者が迷わないための前提）

### D1. 全体を 1 パスで評価し、違反を区間へ「帰属」させる

issue 本文は「H1/H4/H5 を**区間ごとに**判定する」と書くが、**区間ごとに独立評価すると境界を跨ぐ違反を落とす**——項目 2 で hide し、項目 3 で `egui_results:show` が出た場合、H1 の違反はどちらの区間内にも収まらない。

ゆえに**状態機械は trace 全体を `seq` 昇順に 1 パスで舐め、違反イベントの `seq` が属する区間へ結果を帰属させる**。区間マーカーは**評価の境界ではなく帰属の道具**である。

### D2. 判定の単位

| 不変条件 | 評価の単位 | PASS の意味 | SKIP になる条件 |
|---|---|---|---|
| H1 | **hidden 窓 1 つ**（`egui_hide:done` 〜 次の `egui_show:done`）。区間は `egui_hide:done` の側へ帰属 | 窓が閉じるまで `egui_results:show` が 1 件も現れなかった | 区間に `egui_hide:done` が無い / trace 末尾まで `egui_show:done` が来ず**窓が閉じていない**（ただし下の非対称を見よ） |

**連続する `egui_hide:done` は新しい窓を開かない——開いている窓を延長する。** `hide_egui_main`（`window_coordinator.rs:296`）は呼び出し点が 2 つあり（hotkey 経路 `main.rs:388` / `EGUI_HIDE_REQUESTED` listener `mod.rs:401`。**listener 側に可視性ガードが無い**）、`egui_hide:done`（339 行）は遷移を問わず無条件に出る。窓の開始を hide のたびに打ち直す実装だと、**2 つの hide に挟まれた違反が評価から消えうる**。窓を開くのは「未 hidden → hidden」の遷移だけとする。

**H1 の「閉じていない窓」は違反と無違反で扱いが違う。** 違反（`egui_results:show` の出現）は**それ自体で確定**ゆえ窓が閉じていなくても `FAIL`。無違反は「まだ後続が来うる」ので `PASS` ではなく `SKIP`。この非対称が無いと、`smoke-egui.ps1` のように**最後まで hidden で終わる**実行で検出器が丸ごと黙る（下の D6）。
| H4 | `egui_results:show` 1 件 | `rows` が読めて 1 以上だった | 区間に `egui_results:show` が無い / `rows` が読めない（スキーマドリフト） |
| H5 | `egui_results:show` 1 件 | 直前の show 以降に hide を挟んでいた（または初回の show） | 区間に `egui_results:show` が無い |

**H5 の hide 判定は `egui_results:hide` のどちらの発火源（`from = "hide_main"` / `"drive"`）も等しく separator とする。** `hide_main` 側は要求レベルで無条件に出るが、それが余分に出るのは「既に隠れている」ときだけであり、直前に show があれば必ず遷移するので H5 の健全性は保たれる。

### D3. 未観測を PASS にしないための初期状態

main の可視状態は trace 開始時点で**未観測**として始める。最初の `egui_show:done` / `egui_hide:done` を見るまで H1 は評価しない（`SKIP`）。「起動直後は hidden だろう」という推測を初期値に置かない。

### D4. モジュールを分ける理由

`manual-smoke.ps1` は `[Console]::IsInputRedirected` で非対話環境を拒むため、**エージェントも CI もスクリプト全体を実行できない**。判定を純関数として `scripts/lib/` へ出せば、`npm run test:powershell`（Pester）が人の手を借りずに検証でき、フォールトインジェクションもそこで閉じる。

### D7. parse できなかった行があるときは PASS を SKIP へ落とす

`Read-SnotraTraceEvents`（`SnotraSmoke.psm1:342-349`）は `[trace] ` で始まらない行と `ConvertFrom-Json` に失敗した行を**黙って捨てる**。捨てられた行に決定的な違反が載っていたら、判定は何事も無かったように `PASS` を返す——**「検証されていない」が「問題が無かった」へ化ける**まさにその形である。

**捨てた行が生じる機序は測っていない**（下の実測では 0 件）。しかし **`Read-SnotraTraceEvents` が行を捨てる事実は `SnotraSmoke.psm1:342-349` で確定している**ので、degrade はその一点だけで正当化される——機序の有無に依存しない。

参考: `serde_json` は `preserve_order` 無効（`Cargo.lock:4482-4488` に `indexmap` 依存が無い）ゆえキーは辞書順で、`"event"` は `data` オブジェクトの**後**に来る。途中で切れた行があれば event 名だけが載る形になりうる。

ゆえに:

- **捨てた行数を判定器へ渡す**（`-DroppedLineCount`）。0 でなければ、その実行の `PASS` はすべて `SKIP`（理由「parse できなかった行がある」）へ落とす
- **`FAIL` は落とさない**。見つかった違反は捨てた行に関係なく確定である
- 捨てた行数は記録にも出す（どれだけ見えていないかを人が読めるように）

**数え方を誤ると検出器が毎回無意味になる（実測）。** 2026-07-30 の `manual-smoke` 実行が残した実ログ（`%TEMP%\snotra-manual-smoke-trace.log`）を測ると **全 25 行 / `[trace]` 行 24 / 整形式 24 / 途中で切れた行 0**、そして**非 trace の診断行が 1 行**あった（`[index-load] cache_hit=true total=785ms ...`）。

- ゆえに `DroppedLineCount` は「**`[trace]` で始まるのに `ConvertFrom-Json` に失敗した行**」だけを数える。stderr に出る非 trace の診断行は数えない——素朴に「全行 − parse 成功」で数えると**正常な実行が必ず SKIP へ落ちる**
- 数えるのは呼び出し側（`Read-SnotraTraceEvents` は成功分しか返さないため）。同じ数え方を `manual-smoke.ps1` と `smoke-egui.ps1` の両方で使う

### D6. H1 は既に `smoke-egui.ps1` に別導出がある——移行する

`/dry-check` の grep で判明した。`scripts/smoke-egui.ps1:394-415` の orphan 検出は「**最後の** `egui_hide:done` より後ろの行に `egui_results:show` があれば FAIL」であり、**H1 を最終 hidden 窓だけに絞った版**である（行の正規表現・JSON を parse しない）。

- **同じ概念の導出が 2 か所になる。** `AGENTS.md`「検証の作法」が禁じる形であり、放置すると片方だけ直る
- 概念は同一である（表層形が別概念を担っているわけではない）——本件の H1 はその**厳密な一般化**であって、狭い版が捕まえるものは広い版も捕まえる
- ゆえに **`smoke-egui.ps1` の判定を新モジュールへ移す**。`Wait-SnotraTraceEvent` を既に使っている（＝ `SnotraSmoke.psm1` を import 済み）ので配線は 1 行増えるだけ
- **オーケストレーションは移さない**——1500ms の静定待ち・`$resultsChecked` ゲート・`$failures.Count -eq 0` ゲート・失敗文言は smoke-egui 側に残す。移すのは判定だけ
- **検出力は 1 点だけ変わる（同値ではない）**: smoke-egui の実行は最後まで hidden で終わるため窓が閉じない。D2 の非対称により orphan があれば `FAIL`、無ければ `SKIP`（＝ failure を足さない）——ここまでは現行と同じ。**違うのは parse できない行の扱いである**。現行は生行の部分一致ゆえ途中で切れた行でも event 名を拾うが、`Read-SnotraTraceEvents` は `ConvertFrom-Json` に失敗した行を捨てる。ゆえに **D7 の degrade（`PASS` → `SKIP`）だけを smoke-egui にも適用し、捨てた行数は既存の `--- 失敗時の証拠 ---` ブロックへ出す**。**`$failures` へは足さない**——今日は無害に済んでいる状態（旧判定は生行を見るので切れた行でも event 名を拾う）を CI の赤へ変えるのは、issue が求めていない新しい失敗経路の追加である

### D5. 射程外（drift 防止のため明記する）

- **#760**（main が作業領域の下端を割ると results が丸ごとタスクバー下へ入る）は H1〜H5 のどれでも捕まらない。**位置は trace に載っておらず、判定するには trace のスキーマ変更が要る。** 本件では触らない
- `C:/tmp/snotra836-tools/` のカテゴリ D 治具のリポジトリ取り込みは別軸。混ぜない
- **`SPEC.md` は更新しない**——製品の挙動を 1 つも変えず、検証ハーネスだけを足すため

## 変更ファイル一覧と対象シンボル

| ファイル | 種別 | 対象シンボル |
|---|---|---|
| `scripts/lib/SnotraTraceInvariants.psm1` | 新規 | `Get-SnotraTraceMarker` / `Test-SnotraTraceInvariants` / `Format-SnotraTraceVerdictTable`（内部ヘルパー `Get-SnotraTraceProperty`） |
| `scripts/lib/SnotraTraceInvariants.Tests.ps1` | 新規 | Pester（`run-pester.ps1` が `scripts/lib` を discover するため自動で拾われる） |
| `scripts/manual-smoke.ps1` | 改修 | 冒頭の `Import-Module` 2 本 / 実施ループの区間マーカー / `Show-Trace` / 記録の書き出し / exit code |
| `scripts/smoke-egui.ps1` | 改修 | orphan 検出（394-415 行）の**判定だけ**を新モジュールへ移す（D6） |
| `docs/build-commands.md` | 改修 | カテゴリ D 節（51-63 行付近） |

## インターフェース

```powershell
# 最後に観測した seq（1 件も無ければ 0）。区間マーカーはこれを操作の「前」に打つ。
Get-SnotraTraceMarker -Events <psobject[]> -> [long]

# 純関数。trace 行の parse は呼び出し側（SnotraSmoke.psm1 の Read-SnotraTraceEvents）が済ませる。
Test-SnotraTraceInvariants -Events <psobject[]> -Sections <hashtable[]> [-DroppedLineCount <int>] -> [hashtable]
#   Sections: @( @{ Id = 1; Title = '...'; StartSeq = 0 }, ... )  … StartSeq 昇順
#   DroppedLineCount: parse できず捨てた [trace] 行の数（既定 0）。0 でなければ PASS を SKIP へ落とす（D7）
#   戻り値:
#     Sections    : @( @{ Id; Title; H1; H4; H5 } )        各値は 'PASS' / 'FAIL' / 'SKIP'
#     Overall     : @{ H1; H4; H5 }                        1 つでも FAIL があれば FAIL、PASS が 1 つも無ければ SKIP
#     Violations  : @( @{ Invariant; Seq; SectionId; Message } )
#     Unjudgeable : @( @{ Invariant; Seq; SectionId; Reason } )   判定不能の理由（PASS へ化けさせないための証跡）
```

- `Id = 0` は「起動〜項目 1 のマーカーより前」の擬似区間（この間の事象を捨てないため）
- StrictMode 下で欠落プロパティへ触ると `PropertyNotFoundException` になる（実測）。**プロパティの読みは必ず `Get-SnotraTraceProperty`（`$obj.PSObject.Properties[$name]` の indexer）を通す**——`.PSObject.Properties.Name -contains` は空オブジェクトで例外になる（実測）

## 不変条件と異常系

- **`Test-SnotraTraceInvariants` は例外を投げない**。壊れた入力（`seq` 欠落・`event` 欠落・`Sections` が空）はすべて `SKIP` + `Unjudgeable` の理由行へ落とす。判定器が落ちて記録が書けないほうが害が大きい
- **`Events` は呼び出し側の順序に依存せず `seq` 昇順へ整列してから評価する**
- **`Read-SnotraTraceEvents` は壊れた行を黙って捨てる**（`SnotraSmoke.psm1:342-349`）。`manual-smoke.ps1` / `smoke-egui.ps1` の両方で **trace ファイルの `[trace]` 行数と parse 成功件数の差**を数え、`-DroppedLineCount` として判定器へ渡す（D7）。**記録へ出すだけでは足りない**——判定の結果そのものを degrade させないと、捨てた行に載っていた違反が `PASS` に化ける
- **`Sections` の個々のエントリが壊れていても落ちない**（`StartSeq` / `Id` 欠落）。帰属先が決まらない事象は擬似区間 `Id = 0` へ寄せる
- 記録の trace 判定欄は、**判定できなかった理由まで書く**（「SKIP」だけでは「合格」と読まれる）

## 実装順序

### Phase 1 — 判定器（TDD）

- [ ] `cargo build -p snotra` を先に走らせる。**`run-pester.ps1` は `Invoke-Pester` の前に実行ファイルの実在を要求して throw する**（`Resolve-SnotraCargoExecutable` → `Test-Path`）ため、ビルドが無いと Red も測れない
- [ ] `scripts/lib/SnotraTraceInvariants.psm1` を**空のスタブ**として置く（3 関数を定義だけして `Export-ModuleMember`。中身は `throw '未実装'` でよい）。**モジュールが無いとテストの discovery が module-load error で落ち、それは Red ではない**——Red は `Invoke-Pester` が出す**アサーションの失敗**を指す
- [ ] `scripts/lib/SnotraTraceInvariants.Tests.ps1` を先に書く。合成 trace（PowerShell のオブジェクト配列）で下記を固定する
  - [ ] 正常列（show → hide → show）で H1/H4/H5 が `PASS`
  - [ ] **H1 違反**: `egui_hide:done` の後に `egui_results:show` → H1 が `FAIL`
  - [ ] **H4 違反**: `rows = 0` の `egui_results:show` → H4 が `FAIL`
  - [ ] **H5 違反**: hide を挟まない連続 `egui_results:show` → H5 が `FAIL`
  - [ ] **hide の非対称**: `egui_results:hide` が連続しても FAIL にならない
  - [ ] **境界跨ぎ**（D1 の回帰点）: hide が項目 2、違反 show が項目 3 の区間 → H1 が `FAIL` で、**違反は項目 2（hide のあった区間）へ帰属**する
  - [ ] **連続 `egui_hide:done`**（D2）: hide → 違反 show → hide → `egui_show:done` の列で H1 が `FAIL`。**2 つの hide に挟まれた違反が消えないこと**が回帰点
  - [ ] **`-DroppedLineCount` が 0 でなければ PASS が SKIP へ落ちる**（D7）。同じ列で **FAIL は FAIL のまま**であること
  - [ ] **判定不能が PASS にならない**: 空 `Events` / 該当イベント無しの区間 / `rows` 欠落 / main 可視状態が未観測 / 窓が閉じていない / 捨てた行がある — すべて `SKIP` かつ `Unjudgeable` に理由が載る
  - [ ] 壊れた入力（`event` 欠落・`seq` 欠落・`Sections` 空・`Sections` の要素に `StartSeq` / `Id` 欠落）で例外を投げない
- [ ] `pwsh -File scripts/run-pester.ps1` を実行し **Red を確認する**——**アサーションが落ちていること**を出力で確かめる（container のロードエラーは Red ではない。落ち方を見ずに次へ進まない）
- [ ] スタブを実装で置き換えて Green にする

### Phase 2 — `manual-smoke.ps1` への配線

- [ ] 冒頭で `SnotraSmoke.psm1` と `SnotraTraceInvariants.psm1` を `Import-Module -Force`
- [ ] 実施ループで、**項目の読み上げより前**に区間マーカーを打つ（`$sections += @{ Id; Title; StartSeq = Get-SnotraTraceMarker ... }`）
- [ ] `Show-Trace` を「presence の表示 + その時点の trace 判定」へ拡張する（合否の主体が目視であることは文言で保つ）
- [ ] 記録の表へ `trace 判定` 列（`H1/H4/H5` の要約）を足し、**目視と trace の不一致を専用の節で名指しする**
- [ ] `[trace]` 行数と parse 成功件数を数え、差を `-DroppedLineCount` として判定器へ渡す（D7）。記録のヘッダにも出す
- [ ] exit code: 目視 FAIL または trace FAIL のいずれかで `1`
- [ ] 冒頭のコメント（25-28 行「trace は診断であって合否ではない」）を、**H1/H4/H5 は合否であり presence とは別物である**と読める形へ改める

### Phase 2b — `smoke-egui.ps1` の H1 を新モジュールへ移す（D6・重複導出の解消）

**新 API の導入と呼び出し点の移行を 1 タスクに束ねる**（`AGENTS.md`「条件別チェック」）。旧判定を残したまま新 API を足すと導出が 2 か所になる。

- [ ] 移行**前**に、現行の orphan 検出が実際に鳴ることをフォールトインジェクションで 1 度測る（合成 trace ファイルを与えて判定部だけを走らせる。**稼働中のガードは弱めない**——`smoke-egui.ps1` 本体を無害化しない）
- [ ] 394-415 行の判定を `Test-SnotraTraceInvariants` の H1 へ置き換える。1500ms の静定待ち・`$resultsChecked` ゲート・`$failures.Count -eq 0` ゲート・失敗文言は**残す**
- [ ] 移行**後**に同じ合成 trace で同じく鳴ることを測り、**移行前後で判定が一致する**ことを記録する
- [ ] `Get-Content` の行を `Read-SnotraTraceEvents` へ替える（JSON parse 済みオブジェクトが要る）。捨てた行数を数えて `-DroppedLineCount` へ渡し、**0 でなければ `--- 失敗時の証拠 ---` ブロックへ出す**（`$failures` へは足さない・D6 / D7）

### Phase 3 — ドキュメントと検証

- [ ] `docs/build-commands.md` カテゴリ D に、trace 判定 3 つが自動で走ること・**判定不能は SKIP であって合格ではないこと**を足す
- [ ] 同 63 行「項目の SSOT は PR 本文の目視表であり `$items` はその**写し**である」を訂正する。`docs/adr/ADR-folder-location-display-surface.md`「却下 6」が確定させたとおり、**両者は写しではなく別の母集団**である（`$items` = どの変更でも壊れうる横断不変条件の常設 13 項目 / PR 本文の目視表 = その PR 限りの受け入れ確認）。同じ趣旨が `manual-smoke.ps1` の 46-47 行にもあるので同時に直す
- [ ] `npm run governance:check`
- [ ] `npm run test:powershell`（Pester・**全件**）
- [ ] `npm test`（Vitest — scripts/ の JS 検査に影響が無いことの確認）
- [ ] `npm run smoke:egui`（カテゴリ C。**`smoke-egui.ps1` を変更したため必須**。実行中の snotra を kill するので、走らせる前に手元の snotra を閉じる）

## テスト方針と検証コマンド

- **判定ロジックの検証は Pester に閉じる**（D4）。合成 trace は「正しい列」と「違反列」を対で持ち、**故意に壊して FAIL することを実測する**（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）
- **稼働中のガードは弱めない**——変異は合成データ側に当てるので、`manual-smoke.ps1` 本体を無害化する操作は行わない
- 検証コマンド（`docs/build-commands.md` が SSOT）:

```powershell
npm run test:powershell     # カテゴリ E 相当（Pester）
npm run governance:check    # カテゴリ F（docs 変更ゆえ必須）
npm test                    # カテゴリ E（Vitest）
```

- **カテゴリ D 自体（`npm run smoke:manual` の通し実施）はエージェントが実行できない**（対話入力）。PR 本文のチェックリストへ送り、人間の端末で 1 度通して「trace 判定欄が実際に埋まること」を確認する

## SPEC.md・関連文書の更新要否

| 文書 | 要否 | 理由 |
|---|---|---|
| `SPEC.md` | **不要** | 製品の挙動を変えない（検証ハーネスのみ） |
| `docs/build-commands.md` | **要** | コマンドと手順の SSOT。カテゴリ D の記述を更新（Phase 3） |
| `.claude/skills/implement/SKILL.md` | **不要** | 「`manual-smoke.ps1` の目視判定はエージェントが完了できない」は**変わらない**（対話入力の要求は残る） |
| `AGENTS.md` / ルート `CLAUDE.md` | **不要** | トリガー表・フック表のどちらにも新しい振り分けを作らない |
| `src-tauri/CLAUDE.md` | **不要** | 「trace の presence 検査は状態の検査ではない」の記述は本件の前提であり、変える必要がない |

## 未確定（実装前に潰す）

- [x] StrictMode 下で `rows` 欠落の trace 行を読むと落ちないか — **落ちる**。`$b.data.rows` は `PropertyNotFoundException`、`$b.data.PSObject.Properties.Name -contains 'rows'` も空オブジェクトで例外（2026-08-01 実測）。**`$obj.PSObject.Properties[$name]` の indexer なら安全**（`data` 自体の欠落も同じ形で `$null` を返す）。ゆえに `Get-SnotraTraceProperty` を通す設計に確定
- [x] trace の書き出しが行単位で届くか（区間マーカーの成立条件） — **届く**。`trace.rs:49` は `eprintln!` 直書きで、Rust の `std::io::Stderr` は無バッファ。`seq` は単一 `AtomicU64`（`trace.rs:43`）ゆえ全順序（`src-tauri/CLAUDE.md`「モジュール構成」`trace.rs` 項）
- [x] H4 が本当に契約違反か（`rows = 0` の show が正常に起きないか） — **起きない**。`layout::present_results`（`layout.rs:207-214`）が `desired_height > 0.0` を連言に持ち、`results_window_height(0, _, _) = 0`。`Visible` は `count > 0` を含意する
- [x] H5 が本当に不変条件か — **不変条件である**。`ResultsWindow::show`（`results_window.rs:91-104`）は `visible.swap(true)` が true を返したら早期 return し、呼び出し側は戻り値が `true` のときだけ trace する（`window_coordinator.rs:567`）
- [x] H1 に正常系の擬陽性が無いか — **無い**。`hide_egui_main` は `egui_results:hide`（327 行）→ `egui_hide:done`（339 行）の順で出すため、正常な teardown が H1 の窓へ入らない
- [x] `run-pester.ps1` が新規テストを拾うか — **拾う**。`$testPath = Join-Path $PSScriptRoot 'lib'` を `Invoke-Pester -Path` へ渡す（ディレクトリ丸ごと discover）。ただし**実行前に snotra 実行ファイルの実在を要求する**ため、ローカルでは `cargo build -p snotra` が先に要る

## plan-review 結果

- リスク: **高**（ガバナンス文書 `docs/build-commands.md` の変更 + 複数モジュールにまたがるインターフェース新設）
- レビュー方式: 計画準拠レビュー 1 体（`general-purpose` / `sonnet`・成果物 `workspace/plan-review-trace-invariants.md`）
- エージェント数: 1
- 事前に実行した check スキル: `/dry-check`（H1 の別導出を `smoke-egui.ps1:394-415` に発見 → D6 / Phase 2b へ）

### 要対処（3 件・すべて再照合して成立・計画へ反映済み）

- **連続 `egui_hide:done` で H1 の窓が打ち直され、2 つの hide に挟まれた違反が消えうる** — 根拠を再照合: `hide_egui_main` の呼び出し点は `main.rs:388`（hotkey・`plan_hotkey` の可視ガードあり）と `mod.rs:401`（`EGUI_HIDE_REQUESTED` listener・**可視ガード無し**）の 2 つで、`egui_hide:done`（`window_coordinator.rs:339`）は遷移を問わず出る → **D2 に「連続 hide は窓を延長する」を明記**し Pester 項目を追加
- **D6 の「挙動同値」が偽** — 旧判定（`smoke-egui.ps1:401-414`）は生行の部分一致で JSON の妥当性を要求しないが、`Read-SnotraTraceEvents` は `ConvertFrom-Json` の成功を要求する。`Cargo.lock:4482-4488` の `serde_json` に `indexmap` 依存が無い＝ `preserve_order` 無効＝キーは辞書順ゆえ `"event"` は `data` の後に来る（途中で切れた行でも event 名だけは載りうる） → **D6 の主張を「1 点だけ変わる」へ訂正**し、捨てた行があれば fail-closed にする
- **捨てた行が集計値としてしか現れず、判定を degrade させない** — `SnotraSmoke.psm1:342-349` が握り潰す。決定的な違反行が捨てられれば `PASS` に化ける → **D7 を新設**（`-DroppedLineCount` が 0 でなければ PASS を SKIP へ落とす。FAIL は落とさない）

### 軽微

- `Sections` の個々のエントリが壊れている場合（`StartSeq` / `Id` 欠落）の Pester 項目が無い → 「不変条件と異常系」と Phase 1 のテスト項目へ追加（帰属表示にしか影響しないが安い）

### 独立レビュー後の再照合で自分が直したもの（3 件）

- **Phase 1 の「Red を確認する」が成立しない書き方だった** — `run-pester.ps1` は `Invoke-Pester` の前に実行ファイルの実在を要求して throw し、モジュール不在ならテストは discovery の module-load error で落ちる（＝アサーションの Red ではない）→ **スタブを先に置き、Red の定義を明記**
- **D6/D7 が CI に新しい失敗経路を作っていた** — 「捨てた行が 1 行でもあれば `$failures` へ足す」は、今日は無害な状態（旧判定は生行を見る）を `e2e.yml` の赤へ変える。issue が求めていない → **degrade だけを適用し、`$failures` へは足さない**へ訂正
- **`DroppedLineCount` の数え方を実測で確定した** — 実ログ（`%TEMP%\snotra-manual-smoke-trace.log`・2026-07-30）を測ると全 25 行中 `[trace]` 24 行はすべて整形式で、**非 trace の診断行が 1 行**（`[index-load] ...`）。素朴に「全行 − parse 成功」で数えると**正常な実行が毎回 SKIP へ落ちて検出器が無意味になる** → **`[trace]` で始まるのに parse できなかった行だけを数える**

### 未検証

- **捨てた行が実際に生じる機序**（`eprintln!` の write 分割で行が途中で切れるか）は測っていない。上の実測では 0 件。D7 は機序ではなく「`Read-SnotraTraceEvents` が行を捨てる」という確定した事実だけに依拠する形へ書き直した

### 判断

- 実装着手: **可**（人間の承認後）

## 人間レビュー

- [x] 承認済み — 2026-08-02 / 問い: "**Phase 2b（`smoke-egui.ps1` の H1 移行）を含めてよいか。** DRY の観点では必要ですが、CI で走る自動回帰の最低線に手を入れるので、切り離したければ切れます" / "`workspace/plan.md` へ注釈を書き込んでいただくか、明示的にご承認ください。承認を確認するまで実装へは進みません。" / 回答: "2b も含めて承認、実装して"

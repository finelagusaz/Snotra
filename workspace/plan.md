# 計画 — #1461 measure-memory.ps1 の合算を snotra と設定サイドカーに絞り、除外した子孫を出力する

調査は `workspace/research.md`。要求はユーザーの決定（候補 1 ＋ 除外の表示）に従う。

## 判定

バグ（意図の文書と挙動の食い違い）。`.DESCRIPTION` が前提にしていた「合算されるのは snotra とサイドカーだけ」に挙動を合わせ、記述も実際の挙動へ直す。`SPEC.md` は計測スクリプトを扱わないので変更しない。

## 目的と受け入れ条件

1. 合算（PrivWS / PrivComm の合計と表）に入るのは、ルート（`-ProcessName` に名前が一致するプロセス）と、ルートの**直接の子**でイメージ名が `snotra-settings.exe` のものだけである
2. ルートから辿れたがそれ以外の子孫（Snotra から起動したアプリ、サイドカーが `open::that` で開いたプロセス、その子孫）は合算に入らず、件数と「名前 (PID)」が別の行に出る。0 件のときも「0 件」と出る
3. `.SYNOPSIS` / `.DESCRIPTION` / `.PARAMETER ProcessName` / 件数行 / exit 2 のメッセージ / `docs/build-commands.md` の説明が、上の挙動と一致する
4. 選別の判定は純関数として Pester で測られ、uint32 と int が混ざった ppid（#532 SU2 の罠）でも 0 件に黙って落ちない
5. 実機で、Snotra から起動したアプリが生きている状態で `npm run measure:memory` を打ち、そのアプリが表に無く除外の行に出ることを確かめる（issue の受け入れ 1）

## 変更するファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/lib/SnotraMemoryScope.psm1`（新設） | 関数 `Select-SnotraMemoryScope`。入力: `-Processes`（`ProcessId` / `ParentProcessId` / `Name` を持つオブジェクト列。`Win32_Process` をそのまま渡せる）、`-RootIds`（uint32[]）、`-SidecarName`（既定 `snotra-settings.exe`）。出力: `[pscustomobject]@{ Included = [uint32[]]; Excluded = @([pscustomobject]@{ Name; ProcessId }) }`。Included = ルート ∪ {ルートの直接の子で Name が SidecarName に `-eq`（大小無視）で一致}。Excluded = ルートからの BFS で辿れた子孫 − Included（PID 昇順）。PID と ppid は uint32 に揃える。`.DESCRIPTION` に「なぜ絞るか（#1461）」「#1444 と同じ 2 つ。外からは `SettingsProcessState` の `Child` を区別できないので、直接の子で名前が一致するものを選ぶ」「サイドカー名を固定する理由（`window.rs` `launch_settings_process` が固定名で spawn する）」を書く |
| `scripts/lib/SnotraMemoryScope.Tests.ps1`（新設） | 下の「テスト方針」のケース |
| `scripts/measure-memory.ps1` | `Import-Module (Join-Path $PSScriptRoot 'lib/SnotraMemoryScope.psm1') -Force`。インライン BFS（`$byPpid` / `$tree` / `$queue`）を `Select-SnotraMemoryScope` の呼び出しへ置き換え、perf を `Included` の PID で引く。件数行を「ルート {n} 件 / 設定サイドカー {k} 件 / perf 取得 {p} 件」へ（合算対象 = ルート + サイドカー。合算対象の件数だけではルートを必ず含むので、サイドカーの有無が読めない）。表と合計の後に「合算から除外した子孫 {e} 件（Snotra から起動したアプリ等）」と、`e > 0` なら各「Name (PID)」を 1 行ずつ（黄色）。exit 2 のメッセージを「perf カウンタが 0 件です。perf 未提供か、PID の引き違いの疑いがあり、『0 MiB だった』と読んではならない。ルートとサイドカーの件数と併せて手法を疑うこと」へ（Included は必ずルートを含むので、0 件の原因は perf 側に限られる。独立レビュー要対処 3）。`.SYNOPSIS`「プロセスツリー全体を合算する」→「snotra と設定サイドカーを合算する」。`.DESCRIPTION` の BFS の項を、偽の前提を消して「ルートと、その直接の子の設定サイドカーだけを合算する。Snotra から起動したアプリも子孫に入るので、除外して件数と名前を別に出す（#1461）。理由の正本は `lib/SnotraMemoryScope.psm1`」へ。`.PARAMETER ProcessName` に「ルートの名前だけを変える（サイドカー名は固定）」を足す。`.DESCRIPTION` の WorkingSet64 の項の「ツリー合算で二重計上する」は「合算で二重計上する」へ（2 プロセスの合算でも共有ページは二重に数えられるので、理由は残る）。`.DESCRIPTION` の「プロセス件数を必ず出力する」の項は、「BFS が沈黙して 0 件を返す罠を出力から検知できる」を消し、「ルート 0 件（exit 1）と perf 0 件（exit 2）を件数で見せる。ppid の型の罠（#532 SU2）は実行時の出力では『子が居ない』と区別できないので、`SnotraMemoryScope.Tests.ps1` が固定する」へ書き直す（独立レビュー要対処 2） |
| `docs/build-commands.md` | `measure:memory` の行の「ツリー合算」→「snotra + 設定サイドカーを合算」 |

触らない（確認済み）: `scripts/measure-memory-stages.ps1`（`&` で呼ぶだけで、合算の範囲を書かない。修正は自動で波及する）、`PERFORMANCE.md`（「プロセスツリー」は当時の形の歴史記述・`:147` は名指しだけ）、`package.json`（入口は変わらない）、`src-tauri/`・`snotra-settings/`（製品は変えない）。

## 実装の順序

1. Pester のテストを先に書き、モジュールが無いので赤になることを確かめる
2. `SnotraMemoryScope.psm1` を書き、緑にする
3. `measure-memory.ps1` を置き換え、doc と出力の語を直す
4. `docs/build-commands.md` を直す
5. 検証

## 不変条件と異常系

- 「測れなかった」と「0 だった」を区別する（既存の契約）: ルート 0 件で exit 1、perf 0 件で exit 2 は維持する。Included は必ずルートを含むので、選別が空になるのはルートが無いときだけ。`measure-memory-stages.ps1:81` が頼る終了コードの意味（1: プロセス不在 / 2: perf 0 件）は変えない
- ppid と PID の型を uint32 に揃える（#532 SU2）。テストで int 混在を置いて固定する
- Excluded の表示は件数を必ず出す（0 件でも）。除外の行が黙って消えることを防ぐためで、「選別が壊れて 0 件」と「除外するものが無かった」の区別は出力ではできない（型の罠と同じく Pester が受け持つ）
- ルートが複数あってもよい（既存の挙動を保つ）。ルート同士の重複は PID の集合で吸収される
- 既知の限界（doc に書く）: ユーザーが `snotra-settings.exe` 自体を Snotra から起動した場合は合算に入る。ppid の PID 再利用による偽の親子は扱わない

## テスト方針と検証コマンド

`SnotraMemoryScope.Tests.ps1` のケース（入力は偽の `Win32_Process` 相当のオブジェクト）:

- a. ルート + サイドカー子 + 起動アプリ子 + サイドカーの孫（ブラウザ）+ 起動アプリの孫 → Included = {ルート, サイドカー}、Excluded = 3 件（名前と PID）
- b. 起動アプリの子として `snotra-settings.exe` という名前の孫 → Excluded に入る（直接の子だけを拾う）
- c. サイドカー名が大文字（`SNOTRA-SETTINGS.EXE`）→ Included に入る
- d. `ParentProcessId` が int・`ProcessId` が uint32 の混在 → a と同じ結果（SU2）
- e. 子孫なし → Included = {ルート}、Excluded は空
- f. ルート 2 件 → 両方とそれぞれのサイドカーが Included
- g. 無関係なプロセス（ルートの子孫でない）は Included にも Excluded にも出ない
- h. ルート B がルート A の子（snotra から snotra を起動）→ B は Included にあり、Excluded に出ない
- i. ppid が循環する入力（PID 再利用で A→B→A）→ BFS が止まり、各 PID が高々 1 回だけ出る
- j. ルートの直接の子に類似名 `snotra-settings-helper.exe` → Excluded に入る（名前は完全一致）

テスト側は `@($r.Included)` / `@($r.Excluded)` で受ける（1 件のとき配列が解けるため）。

assert は Included の PID 集合と、Excluded の「Name / ProcessId」の組まで見る（件数だけを見ない。`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。

変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が、モジュールの複製に当てる。注入する変異と、赤くなるべきケース:

- 「直接の子」の条件を外し、子孫全体からサイドカー名を拾う → b
- uint32 への揃えを外す → d
- 名前比較を `-ceq` にする → c
- 名前比較を部分一致（`-like "$SidecarName*"` 等）にする → j
- （訪問済み判定を外す変異は i を赤ではなく無限ループにするので、注入しない。i は終了性の回帰テストとして置く）
- 「対象を見ているか」の向き: Included にルートだけを返す（サイドカーの選別を素通しにする）→ a / f
- Excluded を空で返す → a（除外の表示が黙って 0 件になる退行）

この検査が見ないもの（`docs/development-principles.md`「検証の手段と、見ていないもの」）: 実プロセスの親子関係が仮定どおりか（サイドカーが直接の子か・起動アプリの親が Snotra か）と、perf を PID で引いて合算する I/O 側。どちらも下の実機確認だけが見る。

検証コマンド（`docs/build-commands.md`「変更後の検証チェックリスト」から当てはまるもの）:

```
npm run test:powershell
npm test
npm run governance:check
```

実機確認（受け入れ 5）。主エージェントが hotkey 注入で自分で回す（`SnotraSmoke.psm1` の関数を使う。#1444 と同じ手法）。記録先は `workspace/verify-1461.txt`。

0. `Get-Process -Name snotra, snotra-settings` が 0 件であることを確かめる。常駐の Snotra が生きていたら、止めずにユーザーへ止めてよいかを尋ねる（ルートが 2 件になり、常駐側の子孫が混ざるため）
1. `cargo build -p snotra` の後に `cargo build -p snotra-settings` を打ち、`(Get-Item target/debug/snotra-settings.exe).Length -gt 0` を確かめる（worktree の既存は tauri-build の 0 バイトの placeholder）
2. 子として起動する probe を用意する: scratchpad の空ディレクトリへ `C:\Windows\System32\cmd.exe` を `probe1461.exe` という名で写す（#1444 と同じく cmd.exe の写し。Windows 11 の notepad はパッケージアプリで、親が Snotra にならない、または既存のプロセスへ合流しうるので使わない）
3. `New-SnotraVerificationProfile` で分離プロファイルを作る。`-PathEntries` に probe のディレクトリを索引させる行を渡す（書式は `scripts/smoke-egui.ps1` の seed に倣う）。ホットキーは分離プロファイルの既定（Alt+F12）
4. `Start-SnotraProcess` で `target/debug/snotra.exe` を起動し、索引の構築完了と表示を `scripts/smoke-egui.ps1` と同じ待ち方（trace の待機・`Wait-SnotraWindow`）で待つ。この時点の snotra の子孫（debug の conhost 等）を基準集合として記録する。**probe を先に起動する**（サイドカーの存命中はホットキーが無視される・`main.rs` `setup_hotkey_listener`）: `Send-SnotraKeyChord` で表示 → `Send-SnotraUnicodeText 'probe1461'` → `Send-SnotraKey` Enter。続けて表示 → `/o` → Enter で設定サイドカーを起動する
5. `Get-CimInstance Win32_Process` で、サイドカーと probe の `ParentProcessId` が snotra の PID であることを確かめ、probe の子（conhost 等）も記録する（前提が崩れていたら、ここで止めて記録する）
6. **変更前（Red）**: main 版の `measure-memory.ps1`（`git show origin/main:scripts/measure-memory.ps1` を scratchpad へ書き出したもの）を打ち、probe が表と合計に入ることを記録する
7. **変更後（Green）**: `npm run measure:memory` を打ち、表が `snotra` と `snotra-settings` だけで、`probe1461.exe (PID)` が除外の行に出ることを記録する
8. probe（とその子）を止めて打ち直し、除外の行が 4 の基準集合と一致する（probe 由来の行が消える）ことを記録する。最後に snotra とサイドカーを止め、プロファイルを消す
9. 波及先: `measure-memory-stages.ps1` は `& measure-memory.ps1` で同じファイルを呼ぶだけなので、7 の結果がそのまま当てはまる。走らせない（release 本体と、実行中の snotra の kill が要る）。PR 本文で「未実行・理由」と書く

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（計測スクリプトは仕様の対象外）
- `docs/build-commands.md`: 上表のとおり 1 行
- `PERFORMANCE.md`: 不要（研究記録の「採否」を参照）
- `docs/build-commands.md` の判定モジュールの索引（`SnotraWindowColors.psm1` / `SnotraInputMetrics.psm1` を名指す行）へ新設の psm1 は足さない。あの 2 行は判定を持つ検査ツールの節にあり、`measure:memory` の行は 1 行の索引である。選別の理由の正本はモジュールの doc とし、`measure-memory.ps1` の doc から指す

## 作業項目

### Phase 1 — 選別の純関数

- [x] `scripts/lib/SnotraMemoryScope.Tests.ps1` に a〜j を書き、モジュール不在で赤になることを確かめる
- [x] `scripts/lib/SnotraMemoryScope.psm1` に `Select-SnotraMemoryScope` と doc を書き、`npm run test:powershell` を緑にする

### Phase 2 — スクリプトと文書

- [x] `scripts/measure-memory.ps1` の BFS を置き換え、件数行・除外行・exit 2 のメッセージ・`.SYNOPSIS` / `.DESCRIPTION` / `.PARAMETER ProcessName` を直す
- [x] `docs/build-commands.md` の `measure:memory` の行を直す
- [x] 「ツリー合算」「子孫」「BFS」の語を `git grep` で数え、`measure-memory` を説明している現役の記述に古い語が残っていないことを確かめる

### Phase 3 — 検証

- [ ] `npm run test:powershell`・`npm test`・`npm run governance:check` を実行する
- [x] 実機確認 0〜8 を行い、Red / Green の出力を `workspace/verify-1461.txt` へ記録する。結果: probe とサイドカーの親は snotra（5）。変更前は probe と conhost 2 件まで合算して PrivWS 73.9 MiB（6）、変更後は snotra + サイドカーの 69.5 MiB で probe1461.exe と conhost 2 件が除外の行に出た（7）。probe を止めると除外は基準集合の conhost 1 件に戻った（8）。debug の snotra は自分の conhost を子に持ち、変更前はそれも合算していた

## 未確定（実装前に潰す）

- [x] `-ProcessName` を変えたときのサイドカー名 — 固定の `snotra-settings.exe`。`window.rs` `launch_settings_process` が固定名で spawn するため（敵対的調査の争点 1 と未解決の疑問 1）
- [x] 除外した子孫に PrivWS を付けるか — 付けない。名前と PID だけ。合算に入っていない数字を並べると、読み手が足し算に混ぜうるため
- [x] 判定をモジュールへ切り出すか、スクリプト内に残すか — 切り出す。欠陥の本体は選別であり、実機に起動アプリを置かないと再現しない。純関数にすれば Pester で回帰を固定でき、SU2 の型の罠もテストで固定できる（`SnotraWindowColors.psm1` / `SnotraInputMetrics.psm1` と同じ分離）
- [x] `measure-memory-stages.ps1` への波及を実機で走らせるか — 走らせない。同じファイルを `&` で呼ぶだけで、選別は 7 の実機確認が見る。走らせるには release 本体と実行中の snotra の kill が要り、得るものに見合わない（実機確認 9）
- [x] 実機確認で Snotra から起動するアプリ — notepad ではなく cmd.exe の写し `probe1461.exe`。#1444 で親が Snotra になると実測された形で、Windows 11 の notepad はパッケージアプリのため親子関係が保証されない（実機確認 2・5 で ppid を確かめる）

## 人間レビュー

- [x] 承認済み — 2026-11-18 / 問い: "この計画で実装に進めてよろしいですか？" / 回答: "承認する"

## plan-review 結果

- リスク: 高（`docs/build-commands.md` はガバナンス文書の `*.md`、`scripts/**` はセーフティネット）
- レビュー方式: 計画準拠レビュー 1 体（Step 2。「候補 1 ＋ 除外を表示」は issue に無い人間の裁定なので Step 2b ではない）。観点は「散文と挙動の整合」「新設する検査と実機確認の検出力・実行可能性」の 2 つ。全文は `workspace/plan-review-1461.md`
- エージェント数: 1

### 要対処

- 実機確認の順序: サイドカーの存命中はホットキーが無視される — 計画の修正（probe を先に起動）— `src-tauri/src/main.rs` `setup_hotkey_listener` の `SettingsProcessState` の判定で再照合した
- 「出力で型の罠を検知できる」の doc と不変条件が偽になる — 計画の修正（件数行にサイドカー件数を出し、doc は「Pester が固定する」へ書き直す）— `measure-memory.ps1:13-15` を読み、Included が必ずルートを含むことから再照合した
- exit 2 の新しい文言「選別の不成立」が偽 — 計画の修正（perf 側の原因だけを挙げる）— 同上

### 軽微

- 誤字（数わる）— 修正済み
- ⚠️ debug 本体や probe が conhost を子に持ちうる — 実機確認 4・5・8 を基準集合との比較へ修正
- 待ち方とキー送出の関数が未指定 — 実機確認 4 に名指し
- 変異の漏れ（部分一致・訪問済み判定）— ケース j と変異を追加、訪問済みの変異は注入しない理由を記載
- `@()` で受ける — テスト方針に記載
- build-commands.md の判定モジュール索引へ足すか — 足さないと決め、理由を記載

### 未検証

- サイドカーと probe の親が snotra であること（実機確認 5 で測る）
- `cargo build -p snotra-settings` の所要（未測定・実装時に測る）

### 判断

- 実装着手: 可（人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 3 件。すべて計画へ反映（実機確認の順序・件数行と doc の書き直し・exit 2 の文言）
- 未検証: サイドカーと probe の親子関係（実機確認 5 で測る）
- 散文の全件列挙: 語 3 / 総件数 30 行 / ★未掲載 0（計画へ足した 0） / ☆行未掲載 0（計画へ足した 0）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 2 / 主の見落とし 3

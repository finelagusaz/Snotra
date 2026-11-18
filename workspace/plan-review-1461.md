# #1461 計画の独立レビュー

対象: `workspace/plan.md`（根拠は `workspace/research.md`）。観点は「散文と挙動の整合」と「検査・実機確認の検出力と実行可能性」の 2 つ。人間の裁定（候補 1 ＋ 除外の表示）は前提として受け取った。

## 要対処

### 1. 実機確認の手順 4 は、書かれた順では実行できない（ホットキーはサイドカーの存命中は無視される）

計画の手順 4 は「hotkey で表示 → `/o` + Enter でサイドカー起動 → **hotkey で表示** → `probe1461` + Enter」である。

- `src-tauri/src/main.rs:538-545` の `HOTKEY_PRESSED` リスナーは、`SettingsProcessState` が `Some`（サイドカー存命中）なら `return` する。コメントも「設定画面の起動中はホットキーを無視する」と書く。
- `PERFORMANCE.md:673-674`（#1444）も同じことを書く（「サイドカーの存命中はホットキーが無視される」）。
- よって、サイドカーを起動した後の 2 回目の hotkey は効かない。probe を起動できず、Red / Green の前提が作れない。

直し方は 2 通りある。どちらも手順に明記する。
- 順序を入れ替える。probe を先に起動してから `/o` でサイドカーを起動し、サイドカーが生きたまま測る。
- もう 1 つは、手順 4 に「サイドカーを閉じてから hotkey を押す」を足す。ただし、閉じると Red / Green の計測時にサイドカーが居ない。前者がよい。

手順 8 の「probe を止めて打ち直し、除外 0 件」は、サイドカーが生きたまま行うなら問題ない。

### 2. 「除外が働いていない」と「除外するものが無かった」を出力で区別できない（不変条件の主張が偽になる）

計画の不変条件は「Excluded の表示は件数を必ず出す（0 件でも）。『除外が働いていない』と『除外するものが無かった』を出力で区別する」と書く。しかし、その出力では区別できない。

- #532 SU2 の罠（型不一致で BFS が子を 1 件も辿れない）が起きると、Included はルートだけ、Excluded は 0 件になる。サイドカーも黙って落ちる。
- この出力は「サイドカーが居ない・除外するものが無い」ときと同じである。件数行「ルート n / 合算対象 m / perf p」の m は、ルートを必ず含むので、サイドカーの有無を示さない。
- `measure-memory.ps1:13-15` の DESCRIPTION は「BFS が沈黙して 0 件を返す罠を**出力から検知できる**」と書く。計画は、この箇条書きを直す対象として挙げていない（挙げているのは「BFS の項」と exit 2 のメッセージだけ）。旧コードでもルートが tree に入るので検知は弱かったが、新設計は Included の主体が「選別の結果」なので、偽が強まる。
- 提案は 2 つある。件数行に「うちサイドカー {k} 件」を足して、サイドカーが 0 か 1 かを出力へ出す。もう 1 つは、この箇条書きを「型の罠は Pester（`SnotraMemoryScope.Tests.ps1`）が測る。実行時の出力では検知できない」と書き直す。

### 3. exit 2 のメッセージの新文言も偽になる

計画は「BFS 不成立」を「合算対象の選別の不成立」へ、「ツリー件数」を「合算対象の件数」へ直す。しかし、Included は必ずルートを含むので、rows が 0 になるのは perf カウンタが提供されない場合、または perf にルートの PID が無い場合だけである。「選別の不成立」はこの状況の原因として偽になる。計画自身が不変条件で「選別が空になるのはルートが無いときだけ」と書いている（それは exit 1 で先に落ちる）。「perf カウンタが対象の PID を 1 件も返さなかった。perf 未提供か PID の取り違えを疑う」へ寄せる。

## 軽微

- 計画の本文に誤字がある。`.DESCRIPTION` の WorkingSet64 の項の直し「共有ページは二重に数わる」は「数えられる」の誤り。
- ⚠️ 実機確認の手順 8 の「除外 0 件」は、debug ビルドでは成り立たない可能性がある。debug はコンソールサブシステム（`SnotraSmoke.psm1:366` のコメント）なので、`conhost.exe` が snotra の子として現れうる。また、probe（cmd.exe の写し）も `conhost.exe` を子に持つ。Excluded は「ルートから辿れた子孫 − Included」なので、これらは除外の行に出る。手順 7 の「probe が除外の行に出る」は成り立つが、手順 8 の「0 件」は未測定である。手順 5 で `Win32_Process` の全子孫を先に列挙して記録し、手順 8 の期待値を「probe 分を除いた件数」へ直す。または release を使う。
- 手順 3「`-PathEntries` に probe のディレクトリを索引させる行を渡す（書式は `smoke-egui.ps1` の seed に倣う）」は、書式の指示が足りる（`smoke-egui.ps1:118-131` の `[[paths.scan]]` と `path` / `extensions = [".exe"]` / `include_folders = false`、パスの `\` を `/` に置換）。ただし、次の 2 点が書かれていない。(a) 索引の構築完了を待つ手段（trace イベントか sleep か）。(b) キー送出の関数（`Send-SnotraKey` / `Send-SnotraUnicodeText`）と、ウィンドウ待ち（`Wait-SnotraWindow`）の使い方。`/o` が単独で `OpenSettings` になることは `search_state.rs:158` で確認できた（完全一致のみ・trim 後）。「追加の判断なしで実行できる」には、この 2 点を 1 行ずつ足すこと。
- 変異注入の一覧に、次の 2 つが無い。(i) BFS の訪問済み判定を外す変異は、ケース i で「赤」ではなく**無限ループで止まる**（Pester がタイムアウトするまで赤にならない）。i の期待を「ハングしないこと」とし、ケースに時間上限を付けるか、変異の期待を「ハング」と書く。(ii) サイドカー名の判定を部分一致（`-like '*snotra-settings*'`）にする変異は、どのケースも赤にしない。`snotra-settings-helper.exe` のような類似名を 1 件置く。
- `Select-SnotraMemoryScope` の戻り値の `Included = [uint32[]]` は、1 件のとき PowerShell が配列を解く。テスト側は `@($r.Included)` で受けること。実装時に気をつける程度の注意。
- `docs/build-commands.md:126,156` は、既存の psm1（`SnotraWindowColors.psm1` / `SnotraInputMetrics.psm1`）を 1 行で名指ししている。新設の `SnotraMemoryScope.psm1` を同じ行に足す要否は、計画に書かれていない。`measure:memory` の行（`:274`）に psm1 を名指す必要は無い（呼び出し元の doc に正本がある）ので、足さない判断でよいが、判断を計画に 1 行書くこと。
- `PERFORMANCE.md:147` の「`scripts/measure-memory.ps1`」は名指しだけで、計画のとおり変更不要（`git grep` で `measure-memory` を数えた: `PERFORMANCE.md:147` と `docs/build-commands.md:274-275` と stages のみ）。過去の計測（egui 期）は旧スコープの合算だが、当時は子孫がサイドカーだけの条件で測ったと読める。「記録の数字が変わる」ことは無い。

## 未検証

- 計画が書く新しい主張は、次の範囲で一次資料と照合した。
  - 「サイドカー名は `window.rs` が固定」: 真。`src-tauri/src/commands/window.rs` の `launch_settings_process` が `exe_dir.join("snotra-settings.exe")` で spawn する（`exists()` 確認後に `Command::new`）。
  - 「サイドカーはルートの直接の子」: コード上は真（`Command::new(...).spawn()`）。実プロセスの親子は未実測（計画も手順 5 で確かめる設計）。
  - 「#1444 と同じ 2 つ」: `working_set.rs` の `//!` と `trim_idle_working_set` は、自プロセスと `SettingsProcessState` の `Child` を対象にする。「同じ 2 つ」は、名前で選ぶ側が上位集合である点を計画が doc に書く（「外からは `Child` を区別できない」）。この書き方なら偽にならない。
- ⚠️ `cmd.exe` の写し（`probe1461.exe`）を `ShellExecuteW` で起動したとき、親が snotra になることは、`PERFORMANCE.md:662` が #1444 で実測済みとして書く。ただし、今回の分離プロファイルでも同じかは、手順 5 の実測まで未検証。
- ⚠️ `cargo build -p snotra-settings` が、この worktree で通るか。現在の `target/debug/snotra-settings.exe` は 0 バイトの placeholder（確認: 0 バイト）。ビルドの所要は未測定。
- Pester の `Describe` が `scripts/lib/*.Tests.ps1` を自動で拾うことは `run-pester.ps1:15` の `$testPath` で確認した。`governance:check` が新設の psm1 / Tests を要求する検査は、`git grep` した範囲では見つからなかった（`.rs` の `//!` 照合だけ）。
- 新設ファイルの母集団に当たる検査（`SnotraSmoke.Tests.ps1` の PositionalBinding の検査が `scripts/*.ps1` だけか `lib` も見るか）は読んでいない。psm1 は関数に `[CmdletBinding()]` のみで、`param` を持つスクリプトではないので当たらないと思うが、未確認。

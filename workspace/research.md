# 調査 — #1461 measure-memory.ps1 が Snotra から起動したアプリまで子孫として合算する

## issue の要約

`scripts/measure-memory.ps1`（`npm run measure:memory`）は、ルート `snotra` から子孫を BFS で辿り、全員の PrivWS / PrivComm を合算する。`.DESCRIPTION` は「現構成でツリーに乗りうる子は設定サイドカー snotra-settings だけである」と書くが、偽である。Snotra から起動したアプリ（`ShellExecuteW` / オープナーの `Command`）と、サイドカーが `open::that` で開いたプロセスも子孫に入り、合算される。

ユーザーの決定（2026-11-18・Step 1 の質問）: 候補 1 に「除外を表示」を加える。合算するのはルート `snotra` と、その子の `snotra-settings` だけにする。辿れたが除外した子孫は、件数と名前を別に出し、混入しなかったことを出力から分かるようにする。

## 関連するファイル・シンボル

- `scripts/measure-memory.ps1` — 修正対象。`.SYNOPSIS`「プロセスツリー全体を合算する」と `.DESCRIPTION` の BFS の項が偽の前提を書く。BFS は `$byPpid`（uint32 に揃えた ppid → 子 pid の索引）と `$tree` で行い、`Win32_PerfFormattedData_PerfProc_Process` を `$tree` で絞る。出力の件数行は「ルート / ツリー / perf 取得」。表には既に `Name` 列がある（issue の候補 2「名前も出す」は実装済み）。
- `src-tauri/src/commands/window.rs` `launch_settings_process` — サイドカーを `Command::new(exe_dir.join("snotra-settings.exe")).spawn()` で起動する。サイドカーは Snotra の**直接の子**で、イメージ名は `snotra-settings.exe`。
- `src-tauri/src/commands/launch.rs` `launch_item_core` — `ShellExecuteW` を Snotra のプロセス内で呼ぶ。起動したアプリの親は通常 Snotra になる（#1444 で実機確認・`PERFORMANCE.md`）。
- `snotra-settings/src/app.rs:493,496`・`snotra-settings/src/tabs/backup.rs:115` — `open::that`。サイドカーの子（Snotra の孫）を作りうる。
- `src-tauri/src/working_set.rs` — #1444 の先例。trim の対象を「自プロセスと設定サイドカー」に絞り、「子孫は辿らない」と `//!` に理由を書いている。今回の合算対象と揃える。
- `docs/build-commands.md:274` — `npm run measure:memory` の説明が「ツリー合算」と書く。
- `PERFORMANCE.md:147` — 計測ハーネスとして `scripts/measure-memory.ps1` を名指しするだけ。挙動の記述は無い（変更不要の見込み）。
- `scripts/measure-memory-stages.ps1` — 同じ欠陥のパターン（ppid で辿る合算）を持たない。`ParentProcessId` / `WorkingSetPrivate` を使うのは `measure-memory.ps1` だけ（`grep -rln` で確認）。

## 再利用できる既存のパターン

- 判定を `scripts/lib/*.psm1` の純関数に置き、I/O をスクリプト側に残す分離（`SnotraWindowColors.psm1` / `SnotraInputMetrics.psm1`・#953）。Pester は `scripts/run-pester.ps1` が `scripts/lib` の `*.Tests.ps1` を拾う。
- `npm` 入口の `[CmdletBinding(PositionalBinding = $false)]` は `SnotraSmoke.Tests.ps1` が `package.json` から母集団を導いて検査する。`measure-memory.ps1` は既に満たす。

## 技術的な制約

- CIM の `ParentProcessId` は uint32 で返る。PID 集合の型を揃えないと BFS が黙って 0 件になる（#532 SU2）。選別関数へ移しても、この型の揃えを保つ。
- `Win32_PerfFormattedData_PerfProc_Process.Name` は `.exe` なし・同名多重で `#N` 付きのインスタンス名である。名前で絞るなら、`Win32_Process.Name`（`snotra-settings.exe`）で選び、perf は PID で引く。
- サイドカーの判定は「ルートの直接の子で、イメージ名が `snotra-settings.exe`」で足りる。外からは `SettingsProcessState` の `Child` を区別できないので、ユーザーが `snotra-settings.exe` 自体を Snotra から起動した場合は合算に入る（実害は無視できる）。
- ppid は親の死後に PID が再利用されると偽の親子を作りうる。既存の BFS も同じで、名前で絞ると影響はさらに狭まる。今回は扱わない。
- `scripts/**` はセーフティネットの `paths` に入る。マージはユーザーとの合意後（ルート `CLAUDE.md` 最重要ルール 2）。PR 本文冒頭に「マージは合意してから」。

## 未解決の疑問（→ 計画で決着）

- `-ProcessName` を既定以外にしたとき、サイドカー名をどうするか → **固定の `snotra-settings.exe`**。実名は `window.rs` が固定で決めるので、ルート名から導くと実態とずれる。`-ProcessName` はルート名だけを変える引数として doc に書く。
- 除外した子孫を、PrivWS 付きで出すか → **名前（`Win32_Process.Name`）と PID だけ**。合算に入っていないことを数字の並びで誤読させない。
- 実機確認の手順 → 計画の「検証」に置く。worktree の `target/debug/snotra-settings.exe` は tauri-build が置いた 0 バイトの placeholder なので、`cargo build -p snotra-settings` で実体を作ってから測る（`scripts/prepare-sidecar.ps1` と同じビルド）。

## 敵対的調査（3b）の所見と採否

出力: `workspace/adversarial-1461.txt`（sonnet 1 体）。計測時、実機に snotra / snotra-settings は動いておらず、争点 1 はコードからの導出のみ。

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| サイドカーは直接の子・イメージ名 `snotra-settings.exe` | 壊せなかった（インストール版は ⚠️ 未実測） | 採る。`window.rs` は `snotra-settings.exe` を探して spawn するので、起動に成功したなら名前はそれに決まる（自分で `window.rs:65-85` を読んで裁定） |
| `measure-memory-stages.ps1` は `measure-memory.ps1` を `&` で 4 回呼び、欠陥を継承する | 壊せた（「同じパターンは他に無い」の言い方が不正確） | 採る。`measure-memory-stages.ps1:11,68,85` で確認。修正は 1 ファイルで波及先も直るが、波及先として計画の検証に入れる。stages の doc は `measure-memory.ps1` を名指すだけで、合算の範囲を書かないので変更不要 |
| perf の `Name` は `.exe` なし・`#N` 付き。選別は `Win32_Process.Name`、perf は PID で引く | 壊せなかった（`cmd#7`/`cmd#8` を実測） | 採る |
| ⚠️ 名前比較の大文字小文字 | 確信なし | 採る。PowerShell の `-eq` は既定で大小を区別しないので `-eq` を使い、Pester で大文字の名前のケースを置く |
| 「#1444 の trim 対象と一致」は厳密には偽（trim は `SettingsProcessState` の `Child` だけ、名前で選ぶ方は上位集合） | 壊せた | 採る。doc は「一致」ではなく「#1444 と同じ 2 つに絞る。外からは `Child` を区別できないので、名前で選ぶ」と書く |
| ⚠️ `-ProcessName snotra*` だとサイドカーもルートに入る | 確信なし | 採らない（対処しない）。既定の使い方の外で、ルートに入っても合算額は二重にならない（`$tree` は PID の集合）。doc に「ルートは名前の完全一致で選ぶ想定」とだけ書く |
| 直す語は build-commands.md の 1 行だけではない（SYNOPSIS・件数行「ツリー」・exit 2 の「BFS 不成立」「ツリー件数」） | 壊せた | 採る。計画の変更対象に入れる |
| PERFORMANCE.md は変更不要 | 壊せなかった | 採る。`:74-75,87,101,116` の「プロセスツリー」は当時の形を述べる歴史の記述で、`:147` は名指しだけ |
| uint32 の型揃え・PID 再利用・psm1+Pester の型・PositionalBinding | 壊せなかった | 採る |

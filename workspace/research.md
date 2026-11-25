# 調査: issue #1543

## issue の要約

#1539 の測定で、trace 1 行と同じ形を書く小さな Rust の stderr への書き込みが、起動の形によって約 10 倍違った
（1 回で書く形で、`Start-Process -RedirectStandardError` が p50 0.12 ms、cmd の `2>file` が p50 0.014 ms）。
差の理由を一次の観測（ハンドルの種類・フラグ）で特定して `PERFORMANCE.md` に記録し、起動の形を替えて計測値の
汚染を減らせるなら `Start-SnotraProcess` を直すかどうかを決める。直すなら smoke の前提（`docs/build-commands.md`）を壊さない。

## 関連するファイル・モジュール・関数

- `scripts/lib/SnotraSmoke.psm1` の `Start-SnotraProcess`: `Start-Process @startParameters` を呼ぶ。`-StandardErrorPath` →
  `RedirectStandardError`、`-StandardOutputPath` → `RedirectStandardOutput`。`-NoNewWindow` が無ければ `WindowStyle = 'Hidden'`（#872 の理由のコメントつき）。
  env は `Invoke-SnotraEnvironment` で親に一時的に立てて継承させる。戻り値は `Start-Process -PassThru` の `System.Diagnostics.Process`
- `scripts/lib/SnotraSmoke.Tests.ps1` の `Describe 'Start-SnotraProcess'`: `Start-Process` を Mock して、渡すパラメータ（`RedirectStandardError`・
  `WindowStyle` と `NoNewWindow` の排他）を assert する単体テストと、実バイナリの統合テスト（`$env:SNOTRA_PESTER_EXE`）がある
- 呼び出し元（`Start-SnotraProcess`）: `scripts/smoke-egui.ps1`（2 か所）・`scripts/smoke-startup.ps1`・`scripts/bench-startup.ps1`・
  `scripts/visual-check-colors.ps1`・`scripts/visual-input-metrics.ps1`・`SnotraSmoke.Tests.ps1`
- `Start-Process -RedirectStandardError` を直接使うもの（`Start-SnotraProcess` を経ない）: `scripts/manual-smoke.ps1`・`scripts/measure-memory-stages.ps1`
- `src-tauri/src/trace.rs` の `render_line` / `write_line`: #1539 以降、trace 1 行を `write_all` 1 回で `std::io::stderr()` へ書く
- `PERFORMANCE.md`「採用: trace 1 行を 1 回の書き込みにする（#1539）」（測定条件に「stderr は `Start-SnotraProcess -StandardErrorPath`」）と、
  「計測と受け入れ基準」の trace の所要の項（「1 行あたり p50 約 0.11 ms」「`hotkey:listener_enter` は約 8〜10 ms の峰が残る（機序は未確認）」）
- `docs/build-commands.md` の smoke の節（`smoke-startup.ps1` / `smoke-egui.ps1` の前提）

## 一次資料から分かったこと（PowerShell の実装）

PowerShell の `Process.cs`（`src/Microsoft.PowerShell.Commands.Management/commands/management/Process.cs`）を `gh api` で取得して読んだ。
タグ `v7.5.0` と `v7.7.0` で、該当の関数は同一である（ローカルの pwsh は 7.7.0・.NET 11.0.0）。

- `-RedirectStandardError` があると、`Start-Process` は `UseShellExecute = false` の経路で、自前の `StartWithCreateProcess` から
  Win32 `CreateProcess` を直接呼ぶ。`SetStartupInfo` が `STARTUPINFO.hStdError` に `GetSafeFileHandleForRedirection(path, FileMode.Create)` の
  ハンドルを入れ、`STARTF_USESTDHANDLES` を立てる。**子へ渡るのはファイルのハンドルそのもので、パイプではない**。親が受けて書き写す形ではない（ソースの読み・実測は未）
- `GetSafeFileHandleForRedirection` は `File.OpenHandle(path, mode, FileAccess.ReadWrite, FileShare.ReadWrite | FileShare.Inheritable, FileOptions.WriteThrough)` で開く。
  **`FileOptions.WriteThrough`（Win32 の `FILE_FLAG_WRITE_THROUGH`）が付いている**。cmd の `2>file` がこのフラグを付けるとは考えにくい（未確認）
- `CreateNoWindow` が偽（`-NoNewWindow` なし）なら `CREATE_NEW_CONSOLE` と `STARTF_USESHOWWINDOW` を立てる（`-WindowStyle Hidden` はここで効く）

仮説: 単価の差は `FILE_FLAG_WRITE_THROUGH` による（各 `WriteFile` がキャッシュに留まらず、ファイル伸長のメタデータも含めて書き抜けるまで待つ）。

## 再利用できる既存のパターン

- #1539 の測定と同じ形の小さな Rust（trace 1 行と同じ形を `write_all` 1 回で書く・間隔 20 ms・100 行）。コミットされていないので、作り直す
- `Invoke-SnotraEnvironment`（env の一時設定と復元）
- .NET 11 の `ProcessStartInfo` には `StandardErrorHandle` / `StandardOutputHandle`（`SafeFileHandle`）がある（ローカルの pwsh 7.7.0 で `GetProperties()` を列挙して確かめた）。
  自前で開いたハンドルを、P/Invoke なしで子へ渡せる

## 技術的な制約

- CI（`smoke.yml`・`ci.yml` の Pester）は `windows-latest` の既定の `pwsh` で走る。runner の pwsh の .NET が `StandardErrorHandle` を持つか（.NET 11 以上か）は未確認。
  持たないなら、その API に頼る修正は CI で動かない（P/Invoke の `CreateProcess` へ戻ることになる）
- `Start-SnotraProcess` の戻り値は snotra 本体の `Process` でなければならない（呼び出し側が `Stop-Process`・`HasExited`・PID で使う）。
  `cmd /c "exe 2>file"` で包む形は、PID が cmd のものになるので、この契約を壊す
- debug ビルドはコンソールサブシステムで、stdio をリダイレクトするとコンソールウィンドウが出る。`WindowStyle = 'Hidden'` で隠している（#872）。
  起動の形を替えるなら、前面を奪わない性質を保つ
- smoke は、子の実行中に stderr のファイルを `Get-Content` で読む（`smoke-egui.ps1` の seed 健全性の検査など）。`FileShare.ReadWrite` を保ち、
  書いた行が読み手から見えることを保つ（write-through を外してもキャッシュ上で見える見込みだが、未確認）
- `smoke-egui.ps1` のコメント: `-RedirectStandardError` は起動時に空のファイルを作る（`FileMode.Create`）。この前提に依る検査がある
- `SnotraSmoke.Tests.ps1` の単体テストは `Start-Process` の Mock に依っている。起動の形を替えるなら、テストも替わる

## 一次観測（2026-11-25・HAL9000・pwsh 7.7.0 / .NET 11.0.0・scratchpad の一時プローブ。リポジトリへは入れない）

子は Rust の小さなコンソールアプリ（release）。自分の stderr を `GetFileType` と `NtQueryInformationFile(FileModeInformation)` で読み、
trace 1 行と同じ形（約 150 バイト）を `write_all` 1 回で 100 回書いて、1 回ごとの所要を測る（間隔 20 ms）。A → B → C → D を 2 巡回した。

| 起動の形 | ハンドル | mode | p50（µs・1 巡目 / 2 巡目） | p90 |
|---|---|---|---:|---:|
| A: `Start-Process -RedirectStandardError -RedirectStandardOutput -WindowStyle Hidden`（`Start-SnotraProcess` と同じ） | DISK | 0x22（WRITE_THROUGH + SYNCHRONOUS_IO_NONALERT） | 279 / 207 | 492 / 327 |
| B: `cmd /c "probe 2>file"` | DISK | 0x20 | 24 / 21 | 34 / 34 |
| C: `ProcessStartInfo.StandardErrorHandle` に `File.OpenHandle(..., FileOptions.None)` | DISK | 0x20 | 24 / 18 | 30 / 31 |
| D: C と同じで `FileOptions.WriteThrough` | DISK | 0x22 | 211 / 158 | 316 / 230 |

- **子へ渡るのはディスクのファイルで、パイプではない**（4 形とも `FILE_TYPE_DISK`）。親が受けて書き写す形ではない
- **差の原因は `FILE_WRITE_THROUGH` である**。起動の経路と子を固定し、フラグだけを替えた C と D で、単価が約 9 倍往復した
- 倍率は環境で揺れる（この測定で約 9〜12 倍、敵対的調査の子（pwsh の Stream.Write+Flush）で 4〜5 倍、issue の測定で約 10 倍）。機序は共通である
- 敵対的調査は、`-NoNewWindow` でも mode が同じ（0x22）であることと、自前ハンドル（None・ReadWrite 共有）で子の実行中に `Get-Content` が書いた行を読めること、`OpenHandle(Create)` 直後に空ファイルがあることを観測した（1 例ずつ）
- release 本体（windows サブシステム）の子では観測していない。フラグはファイルオブジェクトの性質なので子の種類に依らない見込みで、#1539 の表の後の `egui_frame` p50 114〜117 µs は A の規模と合う

## CI の環境（runner-images の readme・2026-11-16 のイメージ）

- `windows-latest` = Windows Server 2025（VS2026）。PowerShell 7.6.10。pwsh 7.6 は .NET 10 でビルドされている（`global.json` の SDK 10.0.201・タグ v7.6.0）
- `ProcessStartInfo.StandardErrorHandle` は dotnet/runtime の `release/11.0` にあり、`release/10.0` に無い。**CI の既定 pwsh では使えない**（runner 上で直接は確かめていない）
- pwsh 7.6.0 の `Process.cs` も `FileOptions.WriteThrough` で開く。**CI の smoke も write-through の単価を負っている**

## 敵対的調査（3b）の採否

- 採る: `Start-Process -RedirectStandardError` を直接呼ぶ箇所の数え漏れ。`scripts/bench-startup.ps1` の `-UseVerificationProfile` でない分岐（`Start-Process` を直接呼ぶ）も該当する（上の「関連するファイル」の 2 件に加えて 3 件）
- 採る: cmd で包む形が壊す契約の具体（`Wait-SnotraWindow` がウィンドウのオーナー PID を `$Process.Id` と比べる・`Stop-Process -Id` が本体を残す・`Get-SnotraWorkingSetMB -ProcessId` が cmd を測る）
- 採る（⚠️ のまま）: write-through を外したとき、子の強制終了後に最後の行が失われないことは推論で未測（OS のキャッシュはプロセス終了で失われない見込み。OS クラッシュは別）。smoke の 100 ms ポーリングを write-through なしで回す検証はしていない
- 壊せなかった: 争点 1（経路）・争点 2（原因）・争点 4（cmd で包むと契約が壊れる）・争点 5（空ファイル・実行中の読み）

## 未解決の疑問

- （解消）ハンドルの種類とモード・フラグだけで差が再現するか・runner の pwsh と .NET の版は、上の観測で解消した
- `Start-SnotraProcess` を直すか（下の計画の未確定欄で裁定する）
- `hotkey:listener_enter` の 8〜10 ms の峰が、write-through に関係するか（アイドル後の最初の書き込み）。本 issue の範囲外として扱うかどうか

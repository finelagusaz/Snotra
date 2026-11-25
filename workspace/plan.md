# 計画: issue #1543（Start-Process のリダイレクトで stderr への書き込みが cmd の約 10 倍かかる理由）

## 目的と受け入れ条件

目的: 単価の差の理由を一次の観測で特定して `PERFORMANCE.md` に記録し、`Start-SnotraProcess` を直すかどうかを決める。

受け入れ条件:

1. `PERFORMANCE.md` に、差の原因（`Start-Process` が stderr のファイルを `FileOptions.WriteThrough` で開く → 子のハンドルが `FILE_WRITE_THROUGH`）と、
   それを確かめた一次の観測（ハンドルの種類・mode・フラグだけを替えた A/B の表）が、規約どおりの出所（日付・機体・版・標本数）つきで載っている
2. 同じ節に、`Start-SnotraProcess` を直さない決定と、その理由、再開の条件が載っている
3. `PERFORMANCE.md` の、`Start-SnotraProcess` のリダイレクトで測った trace の所要を述べる既存の箇所から、新しい節へ辿れる
4. `npm run governance:check` が通る

## 変更するファイル

- `PERFORMANCE.md`
  - 新しい節「不採用: `Start-SnotraProcess` の起動の形を替えて stderr の write-through を外す（#1543）」を、「採用: trace 1 行を 1 回の書き込みにする（#1539）」の直後に置く
  - 「採用: trace 1 行を 1 回の書き込みにする（#1539）」の測定条件の stderr の行に、この単価には write-through が乗ることと新しい節への参照を 1 句足す
  - 「計測と受け入れ基準」の trace の所要の項（「1 行あたり p50 約 0.11 ms」）に、この値が write-through の単価を含むことと新しい節への参照を 1 句足す

## 実装の順序

1. 新しい節を書く（下の「節の中身」）
2. 既存の 2 か所へ参照の 1 句を足す
3. `npm run governance:check` を走らせる

### 節の中身

- 原因: PowerShell の `Process.cs`（`GetSafeFileHandleForRedirection`）が `File.OpenHandle(..., FileShare.ReadWrite | FileShare.Inheritable, FileOptions.WriteThrough)` で開き、
  `STARTUPINFO.hStdError` に入れて `CreateProcess` で子へ渡す（v7.5.0 / v7.6.0 / v7.7.0 で同じ）。子へ渡るのはディスクのファイルで、パイプではない。cmd の `2>file` はこのフラグを付けない
- 一次の観測の表（`workspace/research.md`「一次観測」の A〜D の 4 行。日付・機体 HAL9000・pwsh 7.7.0 / .NET 11.0.0・子は Rust のコンソールアプリ release・100 行 × 2 巡・間隔 20 ms・プローブはコミットしていない）
- 倍率は子と環境で揺れる（約 4〜12 倍）が、フラグだけを替えた C / D で往復したので機序は共通であること
- CI: `windows-latest` は PowerShell 7.6.10（.NET 10）。同じ `WriteThrough` で開くので、CI の smoke も同じ単価を負う。.NET 11 の `ProcessStartInfo.StandardErrorHandle` は .NET 10 に無い
- 直さない理由: trace を含むフレームの計測は A/B の差分で読む作法になっており、汚染は両側に等しく乗る。直すなら CI で効く形は P/Invoke の `CreateProcess` で、共有の smoke 配管に C# が入り、Pester の `Start-Process` の Mock も作り直しになる。.NET 11 の API だけで直すと、ローカルと CI で計測の条件が分かれる。過去の trace を含む値とも比べにくくなる
- 却下した起動の形: `cmd /c` で包む形（戻り値の `Process` が cmd になり、`Wait-SnotraWindow` のオーナー PID の照合・`Stop-Process`・`Get-SnotraWorkingSetMB -ProcessId` が壊れる）
- 再開の条件: trace の書き込みの絶対値で合否を決める計測を置くとき、または CI の pwsh が `StandardErrorHandle` を持つ .NET へ上がったとき（それでも条件が揃うかを先に確かめる）
- 範囲: `Start-SnotraProcess` を経ずに `Start-Process -RedirectStandardError` を直接呼ぶ `scripts/manual-smoke.ps1`・`scripts/measure-memory-stages.ps1`・`scripts/bench-startup.ps1`（`-UseVerificationProfile` でない分岐）も同じ単価を負う
- 範囲外: `hotkey:listener_enter` の約 8〜10 ms の峰との関係は測っていない（write-through の単価は 0.2 ms 規模で、峰を説明しない）

## 不変条件と異常系

- コードと smoke の挙動は変えない（`scripts/` に触らない）
- 既存の記述は、参照の 1 句を足すだけで、値や結論を書き換えない（`PERFORMANCE.md`「この文書へ記録するときの規約」の遡及しない規約）
- 新しい節の数値は、`workspace/research.md` の表からだけ写す（記憶や敵対的調査の要旨から書かない）

## テスト方針と検証コマンド

- 文書だけの変更なので、テストは足さない
- `npm run governance:check`（ガバナンス文書の検査。G-references で `/` を含むパスの実在を見る）
- 書いた節の数値を `workspace/research.md` の表と突き合わせる

## SPEC.md・関連文書の更新の要否

- `SPEC.md`: 不要（製品の挙動に関わらない）
- `docs/build-commands.md`: 不要（smoke の前提を変えない）

## 作業項目

### Phase 1 — 記録

- [ ] `PERFORMANCE.md` に新しい節を書く（「節の中身」のすべての項目を含む）
- [ ] 「採用: trace 1 行を 1 回の書き込みにする（#1539）」の測定条件に参照の 1 句を足す
- [ ] 「計測と受け入れ基準」の trace の所要の項に参照の 1 句を足す
- [ ] 新しい節の数値を `workspace/research.md` の表と突き合わせる
- [ ] `npm run governance:check` を通す

## 未確定（実装前に潰す）

- [x] 子へ渡るハンドルの種類とフラグ — scratchpad のプローブで `GetFileType` と `NtQueryInformationFile(FileModeInformation)` を読んだ。4 形とも DISK、`Start-Process` と WriteThrough 付きの自前ハンドルだけが `FILE_WRITE_THROUGH`（`workspace/research.md`「一次観測」）
- [x] CI で .NET 11 の API が使えるか — runner-images の readme で `windows-latest` は PowerShell 7.6.10、pwsh 7.6 の SDK は 10.0.201、`StandardErrorHandle` は dotnet/runtime の `release/11.0` にあり `release/10.0` に無い。使えない
- [x] （裁定）`Start-SnotraProcess` を直すか — `AskUserQuestion` で「記録だけ / .NET 11 なら自前ハンドル / P/Invoke で全環境」を問い、回答は「記録だけ（推奨）」。直さず、理由と再開の条件を記録する

## 承認

- [x] 承認済み — 2026-11-25 / 問い: "workspace/plan.md の計画を承認しますか？" / 回答: "承認して実装へ"

## セルフレビュー

- リスク: 通常（`PERFORMANCE.md` だけの変更。永続化・並行性・状態遷移・セーフティネットに触らない）
- plan-review: 該当なし（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 要対処: 1 件 — 敵対的調査が挙げた、`Start-Process -RedirectStandardError` を直接呼ぶ 3 件目（`scripts/bench-startup.ps1` の分岐）を節の範囲の項へ入れた
- 未検証: release 本体（windows サブシステム）の子でのハンドルの mode は観測していない（フラグはファイルオブジェクトの性質なので同じ見込み。#1539 の表の後の値と規模が合う）。節にもそう書く
- check スキル: なし（条件別チェックのうち該当はガバナンス文書の変更の `governance:check` だけ。検証コマンドに入れた）
- 散文の全件列挙: 該当なし（通常リスク）

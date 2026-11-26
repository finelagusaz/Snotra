# 調査: issue #1541

## issue の要約

#1539（trace 1 行を 1 回の `write_all` にした変更）の後も、`hotkey:listener_enter` の行だけは「行の組み立て + `write_all` 1 回」に約 8〜10 ms かかる回が残る（p50 が巡によって 0.4 ms / 8.3 ms に分かれた）。
この所要がどこ（stderr のロック・`WriteFile`・スレッドの起床）にかかっているかを計装で区切って測り、`PERFORMANCE.md` に記録する。直せるなら直し、直さないなら trace の読み方を `PERFORMANCE.md`「計測と受け入れ基準」へ書く。

## 関連する事実（一次資料）

- `src-tauri/src/trace.rs` の `trace` → `render_line` → `write_line(&mut std::io::stderr(), ..)`。`write_all` は `Stderr` の `ReentrantMutex` を取って `WriteFile` を呼ぶ（std の `Stderr` はバッファを持たない）。
- `snotra-egui-runtime/src/stderr_line.rs` の `emit` / `emit_to` も同じ形（`std::io::stderr()` へ `write_all` 1 回）。プロセスの stderr ハンドルは 1 つで、2 つの crate の書き込みは同じ std のロックを取り合う。
- `hotkey:listener_enter` は `src-tauri/src/main.rs` の `setup_hotkey_listener` が登録した listener の先頭で書く。listener は `src-tauri/src/platform/mod.rs` のメッセージループで `WM_HOTKEY` を受けて `app_handle.emit(HOTKEY_PRESSED, ())` したときに走る（Win32 のプラットフォームスレッド。tauri 2 の `listen` のハンドラが emit したスレッドで同期に呼ばれる、という前提は要確認）。
- **同じ規模の峰がもう 1 つある**: #1542 の測定（`PERFORMANCE.md`「採用: egui の計装の 1 行を 1 回の書き込みにする（#1542）」）で、`SNOTRA_EGUI_INPUT push_key` の **down の約 3〜4 割**が 8〜10 ms かかった。up と他の `kind` では 1 巡に 0〜1 件。`push_key` は `snotra-egui-runtime/src/windows_ime.rs` の `trace_keyboard_event` が、ウィンドウの subclass（egui のイベントループのスレッド）で書く。**`listener_enter` とは別のスレッドで同じ規模の峰が出ている。**
- 両方の峰に共通するのは「書き込みの空白（アイドル）の後の最初の 1 行」であること:
  - `listener_enter`: 非表示のアイドル（約 570 ms）の後に最初に書く行（#1539 の節）。
  - `push_key` down: `Send-SnotraKey`（`scripts/lib/SnotraSmoke.psm1`）は打鍵の間に 30〜80 ms 待つ。down の行は待ちの直後で、up の行は down の直後に来る。
- 計測時の stderr は `Start-SnotraProcess -StandardErrorPath` のリダイレクトで、PowerShell が `FileOptions.WriteThrough` で開いたディスクのファイルのハンドルである（`PERFORMANCE.md`「不採用: `Start-SnotraProcess` の起動の形を替えて stderr の write-through を外す（#1543）」）。write-through の単価は 1 回あたり約 0.2 ms（同節の A/D）。同節は「8〜10 ms の峰との関係は測っていない」と書いている。
- 開発機 HAL9000 のディスク: NVMe（SOLIDIGM SSDPFKKW020X7）と USB の SSD。計測ファイルを置いたドライブは記録に無い。
- #1526 の測定（同じリダイレクト・#1539 より前）でも、`listener_enter` の書き込みは約 2 ms と約 10 ms の二峰だった。#1004 では連続する trace の `ts_ms` 差が、処理が無くても 10〜18 ms 空いた（`PERFORMANCE.md`「計測と受け入れ基準」）。

## 仮説（測って区別する）

- **H1 記憶装置の省電力からの復帰**: write-through の `WriteFile` は装置への書き込みの完了を待つ。空白の後は装置（NVMe の APST など）が低電力状態に入っており、復帰に数 ms〜10 ms かかる。予言: 所要は空白の長さに依存し、write-through を外す（cmd の `2>file`）と峰が消える。スレッドにはよらない。
- **H2 スレッドの起床・CPU の省電力**: 待ちから戻るスレッドの再スケジュール（コアの C-state・タイマ分解能）が区間に入る。予言: `WriteFile` の区間でスレッドの CPU サイクル（`QueryThreadCycleTime`）は小さく、壁時計だけが長い。write-through の有無によらず、空白に依存しうる。
- **H3 ロックの取り合い**: 別のスレッドが同時に stderr へ書いていて、`ReentrantMutex` で待つ。予言: ロック取得の区間が長い。直前に別スレッドの行がある。
- **H4 入力の配送に結びついた何か**（キー入力直後の優先度ブースト・他プロセスの処理）: 予言: 空白を同じにしても、入力のない書き込みでは峰が出ない。

H1 と H2 は、本体の外の小さなプローブ（空白を振って write-through あり/なしで 1 行を書く）で区別できる。H3 は本体の計装（ロック取得と `WriteFile` を分けて計時）で測る。H4 は、本体で `egui_frame`（別スレッド・入力なし）の行を同じだけ空けた直後に書いたときの所要で測る（issue の「調べること」の 3 つ目）。

## 再利用できる既存のパターン

- #1539 / #1542 / #1543 の測定の形: 一時の計装（コミットしない）で「組み立て + 書き込み」を計時して次の行へ載せる。版ごとに別の worktree と `CARGO_TARGET_DIR`。前 → 後 → 前 → 後の交互。
- #1543 のプローブ: 小さな Rust のコンソールアプリ（コミットしない）が、自分の stderr を `GetFileType` / `NtQueryInformationFile(FileModeInformation)` で読み、150 バイトの行を `write_all` で書いて 1 回ごとに計時する。起動の形 A（`Start-Process` のリダイレクト）と B（`cmd /c 2>file`）。
- 計測ハーネス: `scripts/lib/SnotraSmoke.psm1` の `Start-SnotraProcess`・`Send-SnotraKey`・`New-SnotraVerificationProfile`。

## 技術的な制約

- 一時の足場（計装・プローブ・計測スクリプト）はコミットしない。撤去までを新設の時点で決める（`docs/development-principles.md`「観測するとき」）。
- 製品の挙動（`SNOTRA_TRACE` 無効時）は変えない。trace の出口（`trace.rs` の `write_line`、`stderr_line.rs` の `emit_to`）の「1 行 1 回の書き込み」とテストは保つ。
- 直す候補が「リダイレクトから write-through を外す」なら、それは #1543 が却下した変更で、再開の条件（trace の絶対値で合否を決める計測を置く・CI の .NET が上がる）はまだ満たされていない。
- `PERFORMANCE.md` は書式の規約を同文書「この文書へ記録するときの規約」に持つ。数の写しを増やさない（「計測と受け入れ基準」からは表の節を指す）。
- 計測は実機（HAL9000）でしかできない。省電力の設定（電源プラン・NVMe の idle timeout）は機体で変わるので、記録に条件として残す。

## 敵対的調査（3b）の反映

全文は `workspace/adversarial-1541.txt`。採否:

- **採る** listener のスレッド: tauri 2.12.5 の `src/event/listener.rs` の `emit_filter` は handlers を `try_lock` して callback を同期に呼ぶ。`listener_enter` はプラットフォームスレッドで書かれる（未解決の疑問 1 は解消）。`try_lock` が失敗した回だけ `flush_pending` を呼ぶ別スレッドへ回る（⚠️ 起きたかは測っていない。計装で書いたスレッドの ID を載せて確かめる）。
- **採る** 「up は down の直後」は裏付けが無い。`Send-SnotraKey` 自身は待たず、既定の `Send-KeyTap` の HoldMs・`Send-SnotraKeyChord` の InterKeyDelayMs は 50 ms で、down と up の間にも待ちがありうる。#1542 の「30〜80 ms」に一致する既定はリポジトリに無く、呼び出しスクリプトも残っていない。**「空白の後の最初の 1 行」という共通点は、up が速いことを説明できない。** 空白（スレッドを問わない直前の行との間隔）も測られていない。上の「関連する事実」の共通点の 2 項目は、仮説として扱い直す。
- **採る** H1 の弱化: 機体の `powercfg`（バランス・AC。現在値）は Primary NVMe Idle Timeout 200 ms・許容遷移遅延 15 ms。`listener_enter` の空白（約 570 ms）は H1 と整合するが、`push_key` down の空白（数十 ms）は 200 ms 未満で、H1 は 2 つの峰を同じ機序で説明できない。#1543 のプローブ（20 ms おき）でも峰は出ていない（p90 ≤ 0.5 ms）。
- **採る** 峰の上限: 最大値が 2 行・2 スレッドで 9.5〜9.8 ms に揃い、#1539 前の p50 は 10,013 / 10,014 µs。装置の復帰より、上限の決まった待ち（タイムアウト付きの待ち・周期）に合う可能性がある。分布を全体で記録する。
- **採る** H5 を足す: ミニフィルタ（ウイルス対策の検査）・NTFS のファイル拡張など、ファイルシステムの側の待ち。H2 と予言が同じ（壁時計だけ長い）なので、ファイルの置き先（C: の NVMe / D: の USB / `NUL`）を替えて区別する。
- **採る** 観察者効果の疑い: `Wait-SnotraTraceCondition`（PollMs 既定 100 ms）が stderr のファイルを繰り返し開く。計測の回では、操作中に stderr のファイルを読まない。`Send-SnotraKey` は呼び出し側の env に `SNOTRA_EGUI_INPUT_TRACE` があると `Write-Host` を挟むので、計測の回では呼び出し側の env に立てない。
- **採る** 計装の帰属: 「次の行へ載せる」形は、載せ先が共有の変数だと別スレッドの行へ帰属がずれる。今回の計装は、`write_all` の所要をスレッド局所の変数に置き、同じスレッドの次の行へ載せる。行には書いたスレッドの ID も載せる。
- **採る（記録だけ）** 修正の別案: `listener_enter` の trace を、#1526 の `t0` と同じ考え方で処理の後ろへ動かす案。#1526 で `t0` を trace の後ろへ移したので、`egui_show:done` の `ms` にはもう入っていない。残る影響は、trace 有効時に実際の表示が遅れることだけで、計測値は汚さない。修正の要否は裁定へ回す。
- **採る** #1543 の却下の前提（単価 0.2 ms）は峰を含まない。峰が write-through に由来すると分かったら、#1543 の節にその事実を書き足す（再開の条件を変えるかは裁定）。
- **採らない（機序の説明）** B3（release の本体のハンドルの mode が未観測）は事実だが、今回の M1 で本体を (a)(b)(c) で受けるので、プローブと本体の差は問題にならない。
- 壊せなかった項目: trace の経路・`Stderr` のロックと無バッファ・ディスク構成・再利用パターン・技術的な制約。

## 未解決の疑問

- tauri 2 の `listen` ハンドラは emit したスレッドで同期に呼ばれるか（`listener_enter` を書くスレッドの特定）。
- #1539 の計測で stderr のファイルを置いたドライブ（NVMe か USB の SSD か）。今回の計測では両方で測るか、片方に固定して記録する。
- H1 が正しいとき、「直す」選択肢は #1543 の却下と同じ判断になるか、別の形（例: trace をメモリに貯めてから書く）があるか。後者は製品の診断出力の形を変える。

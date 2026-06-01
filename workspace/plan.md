# plan — issue #355: 非表示時に WebView2 プロセスツリーへ EmptyWorkingSet を適用

## ゴール / 受け入れ条件

- 全 hide 経路（hotkey トグル + フロントエンド起因：フォーカス喪失/Escape/クリック起動/スラッシュ）で、Snotra プロセスツリー全体（自プロセス + WebView2 子孫）の working set をトリミングする
- 非表示アイドルの Private WS が baseline（~110MB）から数MB へ落ちる（手動計測で確認）
- 再表示が体感即時を維持（~44ms、UI・検索結果・アイコンが正常描画）
- 失敗時（Toolhelp/OpenProcess 失敗）は黙ってスキップ＝機能影響ゼロ（best-effort）

## 設計判断

1. **適用範囲 = 全 hide 経路**（hotkey + frontend）。理由: ランチャーが最も多く隠れるのはフォーカス喪失（frontend 経路）であり、hotkey 限定では大半の hide で回収できない。`EmptyWorkingSet` はスレッド非依存ゆえ IPC スレッドの `notify_main_hidden` からも安全に呼べる（suspend_webview のような with_webview 非同期制約がない）
2. **新規モジュール `src-tauri/src/working_set.rs`** に集約。理由: 呼び出しが 2 モジュール（main.rs / commands/system.rs）にまたがるため共有が必要（DRY）。BFS の純ロジックを純関数に切り出してユニットテスト可能にする。main.rs 肥大化も回避
3. **show 経路に対応操作は追加しない**。trim の「逆」は OS の透過 re-fault で自動。明示 untrim API は存在しない（対称性の非対称はこれが正しい）
4. **best-effort・panic しない**（release は `panic="abort"`）。全 Win32 失敗を握りつぶす

## 変更ファイル一覧

### 1. `src-tauri/src/working_set.rs`（新規）
- `pub(crate) fn collect_descendant_pids(pairs: &[(u32 /*pid*/, u32 /*ppid*/)], root: u32) -> Vec<u32>`
  - 純関数。root を含む子孫 PID を BFS で収集。循環参照（ppid が自己/既訪問）に対してガード（visited セット）。**ユニットテスト対象**
- `#[cfg(windows)] pub(crate) fn trim_idle_working_set(root_pid: u32)`
  - `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)` → `Process32FirstW`/`Process32NextW` で `(th32ProcessID, th32ParentProcessID)` を収集 → `collect_descendant_pids` → 各 PID に対し `OpenProcess(PROCESS_QUERY_INFORMATION|PROCESS_SET_QUOTA, false, pid)` → `EmptyWorkingSet` → close。全失敗はスキップ
  - 自プロセスは root_pid として収集に含め、他の PID と統一的に OpenProcess する（`GetCurrentProcess()` 疑似ハンドル分岐は YAGNI、統一処理で DRY）
- **ハンドルは RAII ガードで閉じる（要対処・plan-review 指摘）**: 明示 `CloseHandle` は `?`/`continue`/early-return で漏れやすい。`icon.rs:255` の `BitmapCleanup` に倣い、`struct HandleGuard(HANDLE)` + `impl Drop`（`if !is_invalid() { CloseHandle }`）を `working_set.rs` 内に定義し、`CreateToolhelp32Snapshot` の HANDLE と各 `OpenProcess` の HANDLE を生成直後に wrap する。全分岐で確実に解放
- `#[cfg(not(windows))] pub(crate) fn trim_idle_working_set(_root_pid: u32) {}`（no-op）
- `#[cfg(test)] mod tests`: `collect_descendant_pids` の純テスト（OS 非依存で全 CI 実行可）

#### windows 0.62.2 API 実装注意（plan-review でソース確認済み）
- API は全て 0.62.2 で利用可能（`EmptyWorkingSet`→`Win32_System_ProcessStatus` / Toolhelp 群→`Win32_System_Diagnostics_ToolHelp` / OpenProcess 等→`Win32_System_Threading` 既存 / `CloseHandle`→`Win32_Foundation` 既存）
- `PROCESSENTRY32W` は `Default`（zeroed）。**`Process32FirstW` 前に必ず `entry.dwSize = size_of::<PROCESSENTRY32W>() as u32` を設定**（未設定だと失敗）
- 戻り値型: `CreateToolhelp32Snapshot`→`Result<HANDLE>`、`Process32FirstW/NextW`/`OpenProcess`(`Result<HANDLE>`)/`EmptyWorkingSet`/`CloseHandle`→`Result<_>`。`GetCurrentProcess`→`HANDLE`（Result でない）。全て `if let Ok(..)` / `let Ok(..) else` で扱い、**`unwrap`/`expect`/添字アクセスを使わない**（best-effort・panic させない）
- `PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA` は `PROCESS_ACCESS_RIGHTS` の `BitOr` 実装で型的に通る

### 2. `src-tauri/src/main.rs`
- `mod working_set;` を追加
- hotkey-hide 経路（L579-581 付近、`suspend_webview(&w)` の直後）に `working_set::trim_idle_working_set(std::process::id());` を追加

### 3. `src-tauri/src/commands/system.rs`
- `notify_main_hidden`（L41-44）の `emit("window-hidden")` の後に `crate::working_set::trim_idle_working_set(std::process::id());` を追加

### 4. `src-tauri/Cargo.toml`
- `windows` features に `Win32_System_ProcessStatus`（EmptyWorkingSet）と `Win32_System_Diagnostics_ToolHelp`（CreateToolhelp32Snapshot / PROCESSENTRY32W / Process32FirstW/NextW）を追加

### 5. `src-tauri/CLAUDE.md`（ドキュメント同期）
- **モジュール構成セクション**に `working_set.rs` を追加（新規 .rs ファイル、AGENTS.md 規定）。文言例: 「`working_set.rs`: 非表示アイドル時に Win32 `EmptyWorkingSet` でプロセスツリー全体の working set を回収（Windows のみ、非 Windows は no-op）。`collect_descendant_pids()` は Toolhelp の (pid,ppid) 上を BFS する純関数（ユニットテスト対象）。best-effort・失敗は握りつぶす」
- 「WebView2 TrySuspend / Resume パターン」節に EmptyWorkingSet 併用を追記:
  - hide 時、`suspend_webview` の後（hotkey 経路）/ `notify_main_hidden`（全 frontend 経路）で trim を呼ぶ
  - **TrySuspend と EmptyWorkingSet は別レイヤー**: TrySuspend=論理目標（MemoryUsageTargetLevel.Low、圧迫待ち）、EmptyWorkingSet=物理 working set の即時トリミング。補完的で競合しない
  - `EmptyWorkingSet` はスレッド非依存（with_webview 非同期制約がない）ため frontend hide（tokio スレッド）からも適用可
  - show 側に逆操作は不要（OS が透過 re-fault）。trim は hide 前後どちらで走っても無害（再 fault するだけ・OS page 標準動作）
  - 削減対象は物理 RAM（working set）のみ、commit は不変

## 実装順序

1. `working_set.rs` 作成（純関数 + Win32 + no-op + テスト）→ `cargo test -p snotra`（純関数テストが落ちることを確認 = Red → Green）
2. `Cargo.toml` に features 追加
3. `main.rs` に `mod` 宣言 + hotkey-hide 呼び出し
4. `system.rs` に notify_main_hidden 呼び出し
5. `cargo check` / `cargo clippy`
6. ドキュメント同期（CLAUDE.md）
7. リリースビルドで手動計測（hide → Private WS 数MB、show → ~44ms・UI 正常）

## 不変条件

- **trim は機能挙動を変えない**: ウィンドウの hide/show・状態機械・IPC 契約は不変。物理メモリ working set のみ縮小。失敗しても「メモリが減らない」だけ
- **best-effort で panic しない**: Toolhelp/OpenProcess/EmptyWorkingSet の全失敗を握りつぶす（release `panic="abort"` のため panic させない）
- **ハンドルリーク防止（生成/破棄ペア）**: `CreateToolhelp32Snapshot` の HANDLE と各 `OpenProcess` の HANDLE は必ず `CloseHandle` する。早期 return パスでも漏らさない（RAII ガード or 明示 close を全分岐に）
- **BFS の停止性**: `collect_descendant_pids` は visited セットで循環（ppid 参照の輪・自己参照）を防ぎ必ず停止する。PID 再利用で実在しない ppid を指していても、存在する pid 集合内でのみ辿るので無限ループしない
- **異常順序/失敗時**: trim が visible なウィンドウに対して race で走っても無害（再 fault するだけ）。hide 前に走っても最終的に hide されれば trim 効果は次のアイドルで効く

## テスト方針

- **ユニットテスト**（`working_set.rs` `#[cfg(test)]`、OS 非依存）:
  - `collect_descendant_pids`: (a) 単純な親→子→孫の連鎖、(b) 複数子・孫（browser→{renderer,gpu,utility} の実形状を模す）、(c) 循環参照でも停止、(d) root のみ（子なし）、(e) 無関係プロセス混在で巻き込まない
- **Win32 部分はユニットテストしない**（AGENTS.md）。手動計測で代替
- **検証コマンド**（`docs/build-commands.md` 準拠）:
  - A: `cargo check -p snotra-core -p snotra -p snotra-settings` / `cargo clippy -p snotra-core -p snotra -p snotra-settings -- -D warnings` / `cargo test -p snotra`（working_set テスト）
  - C（ウィンドウ表示順に触れる）: `npm run smoke:startup` / `npm run e2e:tauri`
- **手動計測**（リリースビルド、調査時の PowerShell スクリプト再利用）:
  - hide（hotkey）→ 非表示 Private WS が数MB に落ちる
  - hide（フォーカス喪失：別ウィンドウをクリック）→ 同様に落ちる ← frontend 経路の確認
  - show → ~44ms 維持・検索結果/アイコン正常描画
  - メモリ圧迫下の再表示レイテンシ最悪値

## SPEC.md 更新要否

**不要**（research.md 参照）。SPEC は suspend_webview/メモリ機構を記載しておらず、本変更はユーザー可視挙動・状態機械・IPC 契約を変えない内部最適化。文書化は src-tauri/CLAUDE.md に閉じる。issue 本文の「SPEC 同期（必須）」は撤回（実装後に issue へコメントで訂正）。

## セルフレビュー

### 5a. check スキル結果
- **/plan-review**（Explore × 3 並列）実施済み。総評: completeness 高、着手可。
  - 経路網羅性: (A)hotkey + (B)notify_main_hidden で全 hide 経路（focus-lost / Escape / click / Enter / Shift+Enter / slash）を網羅。frontend は `hideMainWindow → notifyMainHidden` に統一済みで漏れなし
  - windows 0.62.2 API: 使用予定 API 全て正当とソース確認。実装注意（dwSize 初期化・Result 型・no unwrap）を計画に反映済み
  - **要対処 1 件（反映済み）**: ハンドルリーク → RAII `HandleGuard`（icon.rs `BitmapCleanup` 踏襲）に修正
- **/symmetric-check**: 対称ペア show/hide の検証は plan-review Agent 1 が実施。結論: hide→trim の show 側カウンターパートは**不要**（EmptyWorkingSet に逆操作 API はなく、show 時に OS が透過 re-fault する）。suspend_webview↔resume_webview の既存対称は不変。→ 対称適用漏れなし
- **/cache-check**: 非該当（キャッシュ・incremental ロジックに触れない）

### 5b. チェックリスト
1. **対称コードパス**: ✓ show/hide 検証済み（上記）。trim は hide 側のみ、show 側は OS 自動 re-fault で対称
2. **影響範囲の網羅性**: ✓ hide 経路を grep 列挙（main.rs hotkey / system.rs notify_main_hidden / ui の hideMainWindow 集約）。2 チョークポイントで全網羅
3. **境界条件**: ✓ BFS の循環/自己参照/PID 再利用/root のみ/無関係混在をテストケース化。Win32 失敗（snapshot/OpenProcess）は best-effort スキップ
4. **リソース管理**: ✓ HANDLE 生成/破棄ペアを RAII `HandleGuard`(Drop→CloseHandle) で保証（要対処を反映）
5. **既存パターンとの整合**: ✓ best-effort・`#[cfg(windows)]`/no-op ペア（suspend_webview 踏襲）、RAII ガード（BitmapCleanup 踏襲）。新規パターンなし
6. **YAGNI 違反**: なし。2 call-site は frontend hide（最多経路）を覆うため必要。GetCurrentProcess 疑似ハンドル分岐は YAGNI として排除し統一 OpenProcess に
7. **シンプル化の挑戦**: 新規状態（AtomicBool/Mutex/子プロセス）を**導入しない**——trim はステートレスな関数呼び出し。失敗時は「メモリが減らないだけ」で副作用ゼロ。これ以上単純化すると hide 経路網羅を落とす
8. **破壊不変条件の明示**: 本変更に「壊れたら即アウト」の不変条件は**ない**（best-effort・機能挙動不変）。検知手段: collect_descendant_pids ユニットテスト + 手動メモリ計測（hide 後 Private WS 数MB / show ~44ms / UI 正常）+ smoke:startup / e2e:tauri で起動・表示の回帰検知

### 判定
- completeness: **高**
- 着手可否: **可**（要対処はすべて plan.md に反映済み）

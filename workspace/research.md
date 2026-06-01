# research — issue #355: 非表示時に WebView2 プロセスツリーへ EmptyWorkingSet を適用

## issue の要約

ランチャーは 99% 非表示常駐。実機計測（installed release / 32GB・SSD）で、非表示アイドルの Private WS ≈ 110MB が、既存の `TrySuspend`/`MemoryUsageTargetLevel.Low` では**全く回収されない**（圧迫なし実機では OS が working set をトリミングしないため。120 秒放置でも不変）ことが判明。hide 経路で Win32 `EmptyWorkingSet` をプロセスツリー全体に能動適用し、アイドル常駐を **110MB → 数MB（約 107MB 回収）** する。再表示レイテンシは実測で劣化しない（無圧迫 41ms / 圧迫下 44ms、UI 正常性も目視確認済み）。

## 関連コード

- `src-tauri/src/main.rs`
  - `suspend_webview()` / `resume_webview()`（L162-207）: 既存の WebView2 メモリ機構。trim ヘルパーはこれと対をなす位置づけ
  - hotkey-hide 経路（hotkey-pressed リスナー内 L564-581）: `w.hide()` → `emit("window-hidden")` → `suspend_webview(&w)`。**ここが hotkey トグル hide のチョークポイント**
  - `std::process::id()` で自 PID を取得
- `src-tauri/src/commands/system.rs`
  - `notify_main_hidden()`（L41-44）: `main_visible=false` + `emit("window-hidden")`。**フロントエンド起因 hide 全経路（フォーカス喪失・Escape・クリック起動・スラッシュ）の IPC チョークポイント**。現状ここでは suspend していない（with_webview 非同期制約のため）
- `ui/src/MainApp.tsx` / `ui/src/lib/commands.ts`: フロントエンド hide は `api.notifyMainHidden()` + `win.hide()` を呼ぶ（L53-54 フォーカス喪失、L293-294 クリック起動、commands.ts L19-20 スラッシュ等）。hotkey-hide の `window-hidden` リスナー（MainApp L90）は `setMainVisible(false)` のみで notifyMainHidden は呼ばない → hotkey と frontend は別経路
- `src-tauri/Cargo.toml`: `windows = "0.62.2"` features。現状 `Win32_System_Threading`（OpenProcess/GetCurrentProcess/PROCESS_*）・`Win32_Foundation`（CloseHandle）は有効だが、**`Win32_System_ProcessStatus`（EmptyWorkingSet）と `Win32_System_Diagnostics_ToolHelp`（CreateToolhelp32Snapshot/PROCESSENTRY32W）は未宣言**
- `src-tauri/CLAUDE.md`: 「WebView2 TrySuspend / Resume パターン」節がメモリ機構の文書ホーム

## 既存パターン

- **WebView2 TrySuspend/Resume**（main.rs）: hide 時 suspend、show 時 resume。best-effort・silently-ignore の作法。trim ヘルパーも同じ best-effort 作法に揃える
- **`#[cfg(windows)]` / `#[cfg(not(windows))]` ペア**: suspend_webview 等が踏襲。trim ヘルパーも Windows 実装 + 非 Windows no-op で揃える
- **プロセスツリー走査**: 調査時の PowerShell スクリプトが Toolhelp 相当の (pid,ppid) BFS を実証済み（renderer/gpu/utility は browser プロセスの孫なので全ツリー走査が必須）

## 技術的制約

- **Win32 API はユニットテスト前提にしない**（AGENTS.md）。`EmptyWorkingSet`/Toolhelp 呼び出し自体はテスト不可。BFS の純ロジック（(pid,ppid) リスト → 子孫 PID 収集）を純関数に切り出してユニットテストする
- **`EmptyWorkingSet` はスレッド非依存**（純粋な Win32 プロセス API）。`suspend_webview` の「with_webview が IPC ハンドラスレッドだと非同期ディスパッチ」制約（src-tauri/CLAUDE.md）を持たないため、tokio スレッドで動く `notify_main_hidden` からも安全に呼べる → **全 hide 経路に適用可能**
- **削減対象は物理 RAM（working set）であって commit ではない**（commit ~195MB 不変）
- **EmptyWorkingSet に「逆操作」は不要**: trim されたページは show 時に OS が透過的に re-fault する。明示的な untrim API は存在せず、show 経路に対応操作を追加する必要はない（対称性の非対称はこれが理由）
- **release は `panic = "abort"`**: ヘルパーは panic しない設計（全 Win32 呼び出しの失敗を握りつぶす best-effort）。catch_unwind 不要
- **best-effort**: Toolhelp スナップショット失敗・OpenProcess 失敗（子プロセスが race で終了）は黙ってスキップ。失敗してもメモリが減らないだけで機能影響なし

## SPEC.md 更新要否（重要・issue の記述を訂正）

issue 本文では「SPEC.md 同期（必須）」としたが、**調査の結果これは誤り**:

- SPEC §8（ウィンドウ動作）は表示/非表示の**ユーザー可視挙動と状態機械**を記述するが、`suspend_webview`/TrySuspend/MemoryUsageTargetLevel/メモリ機構は**一切記載していない**（grep 確認済み）
- EmptyWorkingSet は同層の内部メモリ最適化で、**ウィンドウの表示/非表示挙動・状態機械・IPC 契約を一切変えない**（窓は従来どおり hide/show する）
- 既存姉妹機構（suspend_webview）が SPEC 非記載である以上、整合上も EmptyWorkingSet を SPEC に書く必要はない

→ **SPEC.md 更新は不要**。文書化は `src-tauri/CLAUDE.md` の「WebView2 TrySuspend / Resume パターン」節に EmptyWorkingSet の併用を追記する形で行う（コンパニオン機構として）。

## 未解決の疑問

- なし（要求・実装方針とも確定）。設計判断「適用範囲＝全 hide 経路」は計画で確定（下記）

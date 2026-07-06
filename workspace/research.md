# 調査: issue #396 — instant コマンドの E2E カバレッジ追加（exec/url 両経路）

## issue の要約

#394 でインスタントコマンドに `url`（ShellExecuteW 経路）と `exec`（Command::new 経路）の2実行方式を追加した。現状 `e2e/tauri.slash.e2e.ts` はスラッシュコマンド（`/o` 等）のみを検証しており、`@<name> <query>` → Enter という instant コマンドの実キー入力〜実行パスは E2E 未カバーで、retrospective での実機 smoke 時も目視確認に頼っていた。

やること: tauri-driver E2E に `@<name> <query>` 入力 → Enter 実行のシナリオを追加し、url 種別・exec 種別の両方を「起動の観測可能な副作用」で間接検証する。

優先度は低（引数生成は unit テスト、ディスパッチは DTO テストで担保済み、起動対象自体は手動 smoke で担保済み）。今回のスコープは「実キー入力 → IPC → dispatch」という配線の E2E カバレッジそのもの。

## 関連コード

### バックエンド（dispatch の実体）

- `src-tauri/src/commands/instant.rs`
  - `get_instant_commands(prefix_input, app)`: 前方一致でコマンド候補を返す IPC
  - `execute_instant_command(name, query, app)`: `action` を `match` し `InstantAction::Url` → `launch_item_core`（ShellExecuteW）、`InstantAction::Exec` → `launch_exec_core`（Command::new + CreateProcessW 直叩き）にディスパッチ。クリップボード読み取りは engine ロック解放後に行う
- `src-tauri/src/commands/launch.rs`
  - `launch_item_core(path)`: COM STA スレッドで `ShellExecuteW("open", path)`。戻り値 `raw_code > 32` で `LaunchResult::ok`
  - `launch_exec_core(exe, args, query, clipboard)`: `expand_env` → `expand_exec_args` で引数展開 → `Command::new(exe).args(..).creation_flags(CREATE_NO_WINDOW)` で `Stdio::null()` 付き spawn（コンソール窓非表示）
- `snotra-core/src/config.rs`
  - `InstantAction`（`#[serde(untagged)]`）: `Url { url }` / `Exec { exe, args }` / `Legacy { command }`。TOML では `url = "..."` か `exe = "..."` + `args = "..."` のフィールド有無で variant が決まる
  - `SearchConfig.instant_command_prefix`（既定 `"@"`, `default_instant_command_prefix()`）
- `snotra-core/src/instant.rs`
  - `expand_exec_args`: `split_args` → 各トークン env 展開（`%VAR%`）→ 修飾子パイプ変数置換（encode なし）
  - `expand_instant_command`: url は http(s) 開始時のみ URL エンコード

### フロントエンド（`@<name> <query>` → Enter の配線）

- `ui/src/stores/search.ts`
  - `interpKind()`: `query` が `instantCommandPrefix()`（既定 `@`）で始まれば `"instant"`
  - `handleInstantQueryInput`: `@` 以降スペースまでの部分をコマンド名としてデバウンス付き IPC 取得（`scheduleInstantCommandFetch`, 30ms）
  - `executeInstantCommandSelected`: 選択中候補の `name` + スペース以降の残りを `query` として `api.executeInstantCommand(name, query)` を呼ぶ。成功時 `query` をクリア（`interpKind` は plain へ純粋導出）
  - `activateSelected` → `tryModalActivate`: `interpKind()==="instant"` なら `executeInstantCommandSelected()` にディスパッチ
- `ui/src/components/SearchWindow.tsx`（215-233行目）: Enter キーで `activateSelected()` を呼び、`launched===true` なら `hideMainWindow()`（main 非表示）。**tool/folder/instant/通常起動のすべてに共通のディスパッチであり、instant 固有の分岐はない**——「main が非表示になる」という信号は起動系全般の成功シグナルであって instant 固有ではない
- `ui/src/stores/instantCommand.ts`: `getInstantCommands` の結果を `SearchResult[]` に変換し `.result-row` として表示（既存テストの `.result-row` セレクタと同一）

## 既存パターン（E2E harness）

`e2e/tauri.slash.e2e.ts` は Playwright（`workers: 1`、`fullyParallel: false` — 直列実行、同時に1アプリインスタンスのみ）+ `tauri-driver` + `selenium-webdriver` + `edgedriver` 構成。

- `harness` フィクスチャ: テストごとに `setupFixtureDir()`（`.txt` フィクスチャ3件を tmp dir に作成）→ `prepareE2EConfig(fixtureDir)`（`config.toml` を `buildE2EConfigToml(fixtureDir)` の内容で上書き、既存設定はバックアップ）→ `spawnTauriDriver()` → `createWebDriverSession()`（app バイナリを起動しセッション確立）
- `disposeHarness`: セッション終了 → tauri-driver kill → **config.toml を元に戻す** → **fixtureDir を再帰削除**（テスト間の隔離。今回作る instant コマンドの副作用ファイルも fixtureDir 配下に置けば自動クリーンアップされる）
- 既存の「起動成功の間接検証」パターン: `/o` テストは `getMainAlwaysOnTop(driver)` が `false` になることを、「Enter で検索結果を起動すると main が非表示になる」テストは `waitForHiddenLabel(driver, "main", ...)` を確認する。**このリポジトリでは検索結果 Enter 起動で実際に notepad 等の外部 GUI プロセスが開くことを許容してきた実績がある**（該当テストのコメント「起動成功 → hideMain() で main が非表示になる（side effect: txt がエディタで開く）」）。副作用プロセスの明示的な kill は行われていない
- `buildE2EConfigToml(fixtureDir)`: TOML 文字列を組み立てるヘルパー。Windows パスの `\` は二重化してから **ダブルクォート文字列**に埋め込んでいる（`path = "${escapedDir}"`）。**TOML のリテラル文字列（シングルクォート `'...'`）を使えばエスケープ不要**（バックスラッシュを含む Windows パスを埋め込む際、二重化の手間を回避できる。`docs/build-commands.md` にも「TOML に `"` を含む値はシングルクォートを使う」という同種の注意がある）

## 実行経路の設計（url / exec を判別可能な形で検証する）

`instant_commands` を config に2件追加し、それぞれ実行後に**判別可能な副作用**をポーリングする。

### exec 種別（`cmdmark`）— 強い検証（marker file の内容一致）

- `exe = "cmd.exe"`, `args = '/c echo {query}> "<fixtureDir>\instant-exec-marker.txt"'`（`>` の直後に半角スペースが必須。後述）
- `split_args`（`snotra-core/src/instant.rs`）は**引用符文字自体をトークンに含めない**実装（クオート内外の切替マーカーとしてのみ機能し、`current` へ push されない）。そのため `>` と開き引用符の間にスペースを置かないと、`{query}>` と引用符内パスが**1トークンに融合**し（`--open="My File"` → `--open=My File` と同じ挙動）、fixtureDir にスペースを含む環境でリダイレクトが壊れる。`>` の直後にスペースを1つ入れることで `{query}>` とクオート済みパスを別トークンに分離する（既存 config の `'/c type "{path}"'` パターンと同型）
- `{query}` 部分だけが実行時クエリに置換され、パス側トークンはそのままリテラルとして展開される
- Rust の `Command::args` は要素にスペース/タブが無ければ Windows 側のクオート処理を行わず素通しし、スペースがあれば自動でクオートを付与する（`build_launch_args_quoted_fixed_args_with_path` テストと同型の挙動）。よってマーカーパスにスペースの有無いずれでも、生成されるコマンドラインは cmd.exe に正しく渡り、リダイレクトが機能する
- `CREATE_NO_WINDOW`（`launch_exec_core`）によりコンソール窓は表示されない。**GUI を一切開かず、ファイル書き込みという決定的な副作用のみを残す**ため、CI で最も安全かつ強い検証ができる
- 検証: `@cmdmark <query>` → Enter 後、main 非表示 **かつ** マーカーファイルの内容に `<query>` が含まれることをポーリング確認する（テンプレート展開・実プロセス起動の両方を実証）

### url 種別（`urlmark`）— 既存パターンに揃えた検証（main 非表示のみ）

- `url = '<fixtureDir>\<既存フィクスチャファイル名>'`（`E2E_FIXTURE_FILENAMES[0]` の絶対パス、TOML リテラル文字列でエスケープ不要）
- `InstantAction::Url` は `launch_item_core`（生 ShellExecuteW、`resolve_opener` を経由しない）を呼ぶ。既存の「Enter で検索結果を起動する」テストは openers 経由（`launch_with_tool_core`）であり、**instant の url 経路（COM STA スレッド上の生 ShellExecuteW 呼び出し）は今回が初のカバレッジ**
- ShellExecuteW の url 引数に副作用を仕込む手段がない（`lpParameters` は常に `None`）ため、exec のような marker file 検証はできない。**既存の `/o`・Enter 起動テストと同じ「main 非表示」の間接検証**に留める（issue 本文の「直接観測が難しい場合は状態変化で」に該当する典型ケース）
- `.txt` を開くことで既定の関連付け（Windows 既定では notepad.exe）が起動しうるが、これは既存テストで既に許容されている副作用パターンと同じ

## 技術的制約

- Win32 API を新規に呼ぶ実装コードは書かない（テストのみの変更）ため、`SendInput`/`SetForegroundWindow` 等の同期性確認は不要
- `split_args`/`expand_exec_args`/`launch_item_core` はいずれも `snotra-core`/`src-tauri` の既存実装で、変更を加えない（E2E で「配線」を検証するのみ）
- E2E の `config.toml` は妥当な TOML でなければ `Config::default()` にフォールバックし全フィクスチャが失われる（`docs/build-commands.md` の既知の注意点）。TOML リテラル文字列（`'...'`）を使うことでバックスラッシュのエスケープミスを構造的に回避する
- `instant_commands` は `Config` のトップレベルフィールドであり `[search]` セクションとは独立。`instant_command_prefix` は既定 `"@"` のままで変更不要
- Playwright は `workers: 1` / `fullyParallel: false` のため、既存テストと新規テストが同時に別プロセスを起動する競合は発生しない
- 新規追加する2つの instant コマンド名（`urlmark` / `cmdmark`）は先頭文字（`u`/`c`）から分岐させ、`filter_instant_commands` の前方一致フィルタがタイピング中のどの時点でも両者を同時にヒットさせないようにする（`plan-review` で当初案 `iurl`/`iexec` の共通接頭辞 `i` が一時的な曖昧候補を生むリスクを指摘され修正）

## 未解決の疑問

なし。要求は一意に解釈可能（実装方法はコード調査で解決済み）。

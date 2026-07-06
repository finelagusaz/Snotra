# 実装計画: issue #396 — instant コマンドの E2E カバレッジ追加（exec/url 両経路）

## 変更ファイル一覧

### `e2e/tauri.slash.e2e.ts`（唯一の変更ファイル。テストのみ、プロダクションコード変更なし）

1. **フィクスチャ定数の追加**
   - `E2E_INSTANT_URL_COMMAND_NAME = "urlmark"`
   - `E2E_INSTANT_EXEC_COMMAND_NAME = "cmdmark"`
   - 先頭文字を意図的に分岐させる（`u` vs `c`）。**plan-review で発覚した問題への対処**: 当初案の `iurl`/`iexec` は共通接頭辞 `i` を持つため、Selenium の `sendKeys` が1文字ずつ WebView に入力イベントを発火させる過程で、`@i` の時点で両コマンドが前方一致し一時的に2件の曖昧な候補が `.result-row` に表示されうる（`scheduleInstantCommandFetch` の 30ms デバウンスがタイピング速度次第で毎打鍵ごとの fetch を防ぎきれない場合がある）。先頭文字から分岐させることで、`filter_instant_commands` の前方一致がどのタイミングでも両者を同時にヒットさせなくなる（根本対処）
   - `E2E_INSTANT_EXEC_MARKER_FILENAME = "instant-exec-marker.txt"`
   - `E2E_INSTANT_EXEC_QUERY = "e2eexecmarker"`（cmd.exe のリダイレクト・エコーに安全な英数字のみ。スペース・`&|^%<>()"` を含まない）

2. **`buildE2EConfigToml(fixtureDir)` に `[[instant_commands]]` を2件追加**
   - `urlmark`（Url 種別）: `url = '<fixtureDir と E2E_FIXTURE_FILENAMES[0] を path.join した絶対パス>'`（TOML リテラル文字列、エスケープ不要）
   - `cmdmark`（Exec 種別）: `exe = "cmd.exe"`, `args = '/c echo {query}> "<fixtureDir と E2E_INSTANT_EXEC_MARKER_FILENAME を path.join した絶対パス>"'`（TOML リテラル文字列。**`>` と開き引用符の間に半角スペースを1つ入れる**——plan-review で発覚: `snotra-core/src/instant.rs` の `split_args` は引用符文字自体をトークンに含めない実装のため、スペース無しで `{query}>"<path>"` と書くと引用符が消えて `{query}>` とパスが1トークンに融合し、fixtureDir にスペースを含む環境（ユーザー名にスペースがある等）でリダイレクトが壊れる。既存 config の `'/c type "{path}"'`（openers の "Type" ツール）と同型——`>` の直後に空白を置いてトークンを分離することで、`{query}>` と `"<path>"` が別トークンになり、パスにスペースがあっても Rust の `Command::args` が該当トークンだけを自動でクオートする）
   - 関数シグネチャ（`fixtureDir: string`）は変更不要。関数内で `path.join(fixtureDir, ...)` を使い、既存の `escapedDir`（ダブルクォート文字列用）とは別に、リテラル文字列にそのまま埋め込む生パスを組み立てる

3. **ヘルパー関数の追加**
   - `async function typeInstantCommandOnce(driver, prefix, name, query)`: `.search-input` を全選択+削除してから `${prefix}${name} ${query}`（query が空なら末尾スペースなし）を入力する。既存の `typeQueryOnce` と同様の1回入力パターンを踏襲（打ち直しによる `searchGeneration` bump 事故を避ける、#369 の教訓を継承）
   - marker file の読み取りは Node の `readFile`/`access`（既存 import 済み）をそのまま使う。新規ヘルパー関数は不要（`driver.wait` 内で直接 `readFile().catch(() => "")` して `includes()` 判定すれば足りる）

4. **テストケースの追加（2件）**
   - `"@<name> url インスタントコマンドが実行され main が非表示になる"`
     - `switchToLabel(driver, "main")` → `typeInstantCommandOnce(driver, "@", "urlmark", "")`
     - `.result-row` が **ちょうど1件**（`length === 1`）表示されるまで `driver.wait`（候補フェッチの30msデバウンス+IPC完了待ち。`>0` ではなく `===1` にすることで、名前分岐による根本対処に加えた defense-in-depth とする）
     - Enter 送信の直前に `.search-input` を再取得（既存テスト「Shift+Enter でツール選択リストが表示され...」等と同じ慣習。`.result-row` 出現待ちの間に再レンダリングが起きても stale element を踏まない）
     - `Key.ENTER` を送る
     - `waitForHiddenLabel(driver, "main", 8_000)` で成功を確認
   - `"@<name> exec インスタントコマンドが実行され main が非表示になり marker ファイルに query が書き込まれる"`
     - `switchToLabel(driver, "main")` → `typeInstantCommandOnce(driver, "@", "cmdmark", E2E_INSTANT_EXEC_QUERY)`
     - `.result-row` が **ちょうど1件**表示されるまで `driver.wait`
     - Enter 送信の直前に `.search-input` を再取得
     - `Key.ENTER` を送る
     - `waitForHiddenLabel(driver, "main", 8_000)` で成功を確認
     - **加えて** `driver.wait` でマーカーファイル（`path.join(harness.fixtureDir, E2E_INSTANT_EXEC_MARKER_FILENAME)`）の内容取得を試み、`E2E_INSTANT_EXEC_QUERY` を含むまでポーリング（タイムアウト 8_000ms、`readFile` 失敗（未作成）は空文字列にフォールバックしてリトライを続行）

## 実装順序

1. 定数追加（`E2E_INSTANT_*`）
2. `buildE2EConfigToml` へ `[[instant_commands]]` 追加（TOML リテラル文字列化）
3. `typeInstantCommandOnce` ヘルパー追加
4. url 系テストケース追加 → 単体で `npm run e2e:tauri` 実行し green を確認
5. exec 系テストケース追加（marker file 検証含む）→ 単体実行で green を確認
6. フルスイート実行（既存テストに副作用（fixtureDir 内の余分なファイル・config 変更）が波及していないか確認）

## 不変条件

- **fixtureDir のライフサイクルは既存 harness に従う**: `disposeHarness` が毎テスト後に `fixtureDir` を再帰削除するため、`cmdmark` が書き込む marker file は当該テスト内でのみ存在し、他テストの `.result-row` 件数アサーション（`E2E_FIXTURE_FILENAMES.length` 等）に影響しない。marker file 名（`instant-exec-marker.txt`）は `E2E_SEARCH_QUERY`（`"snotra-e2e"`）を含まないため、仮に生成タイミングが早まっても検索結果件数系テストの対象にならない
- **`buildE2EConfigToml` は依然として妥当な TOML を生成する**（`docs/build-commands.md` の既知の注意点）: `[[instant_commands]]` の2エントリはリテラル文字列（`'...'`）でパスを埋め込むため、バックスラッシュのエスケープ漏れによる parse 失敗リスクを構造的に回避する。追加後、`toml::from_str` 相当のパースが通ることは E2E 実行そのもの（アプリ起動時に `Config::load` が走る）で実証される
- **`urlmark` / `cmdmark` の名前は前方一致で相互に衝突しない（先頭文字から分岐）**: `filter_instant_commands` は `name.starts_with(input)`（大文字小文字区別なし）で候補を絞るため、先頭文字が異なる（`u` vs `c`）2名は**タイピング中のどの部分文字列に対しても**同時にマッチしない（plan-review で当初案 `iurl`/`iexec` の共通接頭辞 `i` が一時的な曖昧候補を生むリスクを指摘され、名前を変更して根本対処）。既存の instant コマンド定義は無い（今回新規追加のみ）ため、既存コマンドとの衝突も無い
- **Enter 前に `.result-row` の出現を待つ**: `executeInstantCommandSelected` は `getInstantCommandItems()`（IPC 応答済みの候補一覧）から `selected()` index で解決するため、IPC 応答前に Enter すると候補未着で `cmd` が `undefined` になり `launched=false`（`hideMainWindow` 未呼出）でテストがタイムアウトする。両テストとも `.result-row` 出現待ちを Enter の前に置く
- **exec テストは「main 非表示」と「marker file 内容」の両方を独立に確認する**: 前者は IPC/dispatch 配線の成功、後者は実引数展開（`{query}` 置換）と実プロセス起動（`CREATE_NO_WINDOW` の `Command::spawn`）の両方が正しく機能したことを示す。どちらか一方が壊れても検出できるよう、片方だけで確認を打ち切らない
- **url テストは main 非表示のみで検証する（意図的な非対称）**: `InstantAction::Url` は `lpParameters` を持たないため、`{query}` を url に含めても副作用として観測可能な形にできない（クエリを含めると url 自体が変質し ShellExecuteW のターゲットとして無効になる）。そのため url テストでは query を空にし、既存の `/o`・Enter 起動テストと同じ「main 非表示」シグナルに揃える。これは検証の手抜きではなく、issue 本文が明示する「直接観測が難しい場合は状態変化で」に対応する設計判断であり、plan.md に理由を明示する
- **`.txt` を開く既定の関連付け（notepad 等）は既存テストで既に許容されているリスクと同一**: 新規に GUI プロセスを起動する副作用パターンを追加するわけではない（「Enter で検索結果を起動する」テストが既に同種の副作用を許容している）。プロセスの明示的な kill は既存パターンと同様に行わない（スコープ外、YAGNI）

## テスト方針

- 追加するテストは E2E（`e2e/tauri.slash.e2e.ts`）の2件のみ。ユニットテスト・DTO テストは issue 本文の通り既存で担保済みのため追加しない
- 検証コマンド: `npm run e2e:tauri:setup`（初回のみ）→ `npx tauri build --no-bundle` → `npm run e2e:tauri`
- 既存9件のテストが green のまま維持されることを確認する（今回の変更は `buildE2EConfigToml` の追記のみで、既存 `[hotkey]`/`[appearance]`/`[paths]` 等のセクションは変更しない）
- 本 PR には `docs/build-commands.md` の運用に従い **`e2e` ラベル**を付与する（`E2E & Smoke` workflow を CI で走らせるため）

## SPEC.md 更新要否

不要。挙動変更を伴わない（テストカバレッジ追加のみ）。`docs/`・`CLAUDE.md` 系ドキュメントの更新も不要（`e2e/tauri.slash.e2e.ts` の内部実装追加であり、モジュール構成・横断パターンに変更はない）。

## セルフレビュー

`/plan-review` を3並列 Explore エージェント（config/バックエンド前提・フロントエンド配線・スコープ/CI/doc同期）で実施。局所的な単一ファイル変更（リネーム・config キー変更・データ移行を伴わない）のため Step 2b（独立導出+差分）は省略。

### 要対処（反映済み）

1. **`split_args` の引用符処理**: `snotra-core/src/instant.rs` の `split_args` は引用符文字自体をトークンに追加しない実装であり、当初案 `args = '/c echo {query}>"<path>"'`（`>` と開き引用符の間にスペース無し）では引用符が消え `{query}>` とパスが1トークンに融合する。fixtureDir にスペースを含む環境（ユーザー名にスペース等）でリダイレクトが壊れるリスクがあったため、`>` の直後に半角スペースを1つ挿入し（`{query}> "<path>"`）、既存の `'/c type "{path}"'` パターンと同型のトークン分離にした
2. **instant コマンド名の共通接頭辞による曖昧候補リスク**: 当初案 `iurl`/`iexec` は共通接頭辞 `i` を持ち、Selenium の逐次キー入力中に一時的に両者が前方一致した曖昧な候補リストになりうるため、`urlmark`/`cmdmark`（先頭文字 `u`/`c` で分岐）に変更。加えて `.result-row` の待機条件を `length > 0` から `length === 1` に強化し defense-in-depth とした

### 軽微な懸念（反映済み）

- Enter 送信直前に `.search-input` を再取得するステップを明記（既存テストの慣習との整合）

### 問題なし（3エージェント共通の確認事項）

- TOML untagged enum の解決順序・`toml` crate のリテラル文字列パース・`Config::load()` が `validate()` を呼ばないこと・`instant_commands`/`[search].instant_command_prefix` の独立性・`Command::args` のスペース有無によるクオート挙動（既存 `build_launch_args_quoted_fixed_args_with_path` テストと同型）
- `executeInstantCommandSelected`/`SearchWindow` の Enter ハンドラの理解（`raw`/`nameEnd`/`instantQuery` 計算）は計画通りで正しい
- issue 要求とのスコープ整合（exec の marker file 検証は過剰ではなく、issue が明示する「実キー入力→IPC→実プロセス起動」という未担保ギャップを埋めるのに必要）
- CI 環境制約（`windows-latest`、`cmd.exe`/既定関連付けは既存テストが既に許容済みの副作用パターンと同等かそれ以下のリスク）
- `docs/build-commands.md` の `e2e` ラベル運用との整合、SPEC.md 更新不要の判断、RETROSPECTIVE.md の既存教訓との整合

### 総評

計画の completeness: 高（3エージェントの指摘を反映済み）
実装着手可否: 可

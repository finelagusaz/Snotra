# CLAUDE.md

このリポジトリで Claude Code が作業するときの運用ガイド。`*.md` で意図を管理し、コードで実装事実を管理する。

## プロジェクト概要とアーキテクチャ

Snotra は Windows 専用のキーボードランチャー。バックエンドは Rust（Tauri v2）、フロントエンドは SolidJS + TypeScript で構築。システムトレイ/グローバルホットキー/IME などの Windows 固有機能は `windows` クレートで直接実装。グローバルホットキー（既定: `Alt+Q`）で検索ウィンドウを表示し、検索と起動を行う。

### 3層分担（責務分離）

- 第1層（意図管理）: `SPEC.md` と `CLAUDE.md`
- 第2層（実装事実）: `snotra-core/src/*.rs`, `src-tauri/src/*.rs`, `ui/src/**`
- 第3層（整合運用）: 挙動変更を伴う変更では、意図（`SPEC.md`）と実装を同時に整合させる

### ディレクトリ構成

```
Snotra/
  Cargo.toml              # workspace (snotra-core, src-tauri)
  snotra-core/            # 純ロジック lib crate → snotra-core/CLAUDE.md
  src-tauri/              # Tauri v2 バイナリ crate → src-tauri/CLAUDE.md
  ui/                     # SolidJS フロントエンド → ui/CLAUDE.md
  package.json, vite.config.ts, tsconfig.json
```

Cargo ワークスペース構成で、純ロジックライブラリ（`snotra-core`）と Tauri バイナリ（`src-tauri`）を分離。GUI は SolidJS + CSS 変数ベースのテーマシステムで、Tauri IPC 経由で Rust バックエンドと通信。

### 横断的な実装パターン

- 検索ウィンドウは起動時に作成し `visible: false`、ホットキーで表示/非表示を切替
- フォルダ展開は「開始時スナップショットを保持し、`Escape` で一括復帰」モデル
- 履歴/インデックス/アイコン保存は `.tmp` を使った原子的書き込み
- アイコンは検索時にオンデマンドで抽出し base64 PNG としてフロントエンドに送信、キャッシュは終了時に永続化
- テーマは CSS カスタムプロパティで動的に切替
- 検索結果ウィンドウの同期は `results-sync` イベント1本で扱い、`results-updated` / `results-count-changed` を新規実装で使わない
- `launch_item` は `LaunchResult(status/code/message)` を返す契約で扱い、失敗通知の自動クリアは単一タイマーを再利用して競合を防ぐ
- 起動時にスレッドを並列 spawn する場合、そのスレッドが発火するイベントに依存する機能（ホットキー・トレイ等）はスレッド init フェーズで有効化せず、main 側でリスナー/ウィンドウ準備が整った後にコマンド（`RegisterInitialHotkey` / `SetTrayVisible`）で有効化する（「有効化 ≥ リスナー登録」不変条件）

### 参照先

- 意図（仕様）: `SPEC.md`
- 設定値・デフォルト: `snotra-core/src/config.rs`
- ドキュメント参照: context7 MCP が設定済み。Tauri v2 / SolidJS / Rust クレートの最新 API を調べる際は context7 を使う

## コミュニケーション原則

- タスクが真に曖昧でない限り、分析・計画より実行にバイアスをかける
- ユーザーが具体的な計画や修正指示を既に提示している場合、プランモードへの遷移・事前の全体探索を禁止する。読むファイルは直接関係する最小限（1〜2ファイル）に絞り、最初の Edit/Write から着手する
- コミット・PR 作成を指示された場合、確認やプランモードなしに即実行する
- コミットを作成するときは**必ず feature ブランチを作成してから**行う。`main` への直接コミット・プッシュは禁止。ブランチ名は `feat/<機能名>` / `fix/<バグ名>` とする
- ユーザーが計画書・設計書を提示して実装を依頼した場合、内容を忠実に実装する。計画書の要素を省略・統合・削除するのは明示的に指示された場合のみ行う
- 不明点がある場合は、1つの焦点を絞った質問をしてから実装に移る
- ユーザーが分析・調査・助言を求めた場合は、調査結果のみを報告する。明示的に指示されない限り、実装計画やコード変更に踏み込まない

## ビルド・実行コマンド

**Windows 不要**（macOS/Linux でも実行可能）:

```bash
npm test                          # フロントユニットテスト（Vitest）
npm run build                    # フロントエンドビルド（typecheck → vite build、プロジェクトルートから実行）
```

**Windows 必須**（`windows` クレートや Win32 API・実行バイナリに依存）:

```bash
cargo test -p snotra-core        # ユニットテスト
cargo test --release -p snotra-core bench_ -- --ignored --nocapture  # 検索パフォーマンス計測
cargo check -p snotra            # Rustバックエンド型チェック
cargo clippy -p snotra-core -p snotra  # lint チェック
npm run verify                   # Rust + フロントエンド一括検証（cargo check + npm run build）
npm run smoke:startup             # 起動時ウィンドウ生成スモーク（trace検証）
npm run e2e:tauri:setup           # Tauri Driver E2E 用セットアップ
npm run e2e:tauri                 # Playwright + Tauri Driver E2E
npm run tauri dev                # 開発実行（ホットリロード付き）
npm run tauri build              # リリースビルド
```

### E2E/スモーク運用メモ

- `scripts/smoke-startup.ps1` は `SNOTRA_TRACE=1` で起動し、`main:ensure_window:ok`（`results/about/settings`）の存在と `*:error` 不在を検証する
- `e2e/tauri.slash.e2e.ts` は Playwright runner 上で `tauri-driver + selenium-webdriver + edgedriver` を使い、起動入力・`/a`・`/o` の動作を検証する
- E2E セットアップは `npx tauri build --no-bundle` を使う（`cargo build --release` は `localhost` 向きバイナリになり `ERR_CONNECTION_REFUSED` で失敗する）
- スラッシュコマンドの実行順（`hide -> /a|/o|/s`）は `ui/src/lib/commands.test.ts` で固定し、順序変更時は必ず更新する
- Tauri Driver E2E の可視判定は `document.visibilityState` を真実源にしない。`plugin:window|is_visible` を優先して判定する

## 開発原則

### KISS

- `main.rs` に業務ロジックを増やさない
- 責務を跨ぐ実装をしない
- 新規コードは既存のファイル構成・命名規則・スタイルパターンに合わせる。独自パターンを導入する前に既存パターンの利用を検討する

### DRY

- 責務の集約先は各サブディレクトリの `CLAUDE.md` に記載
- 同一ロジックの繰り返しは2回まで許容し、3回目で抽出を検討する（無理な抽象化よりも多少の重複を許容）

### YAGNI

- 使う予定だけの抽象化（不要な trait/generics/レイヤー）を導入しない
- 現在の要求範囲を超える機能追加を行わない
- 拡張性より、現要件での単純さと可読性を優先する

## 開発ワークフロー

実装・修正に着手する際は、原則として以下のステップに沿って進行すること。

**サイクル開始前**: `RETROSPECTIVE.md` を読み、前回サイクルで生まれたバグのパターン・経路を確認する。同じ構造的ミスを繰り返していないか意識しながら以下のステップを進める。

0. 「バグ」か「仕様変更」かを判定する
   - バグ: `SPEC.md` の意図に合わせてコード修正
   - 仕様変更: `SPEC.md` → コード → `CLAUDE.md` の順に更新
1. 要件（受け入れ条件）をテスト可能な形で確定する
2. 影響範囲（読む/触る/触らない）を列挙する
   - **対称コードパス確認**: 変更対象に対称ペア（`clicked`/`double-clicked`、`show`/`hide`、`enter`/`exit` 等）が存在する場合、それぞれへの適用要否を明示的に判断して記録する
   - **git マージ状態の確認は `git diff` で行う**: `git log` のコミットメッセージはマージ済みかどうかの確認に使わない。実際のコード差分（`git diff main...HEAD` 等）で確認する
   - **関数使用箇所検索**: 新規定義・変更した関数が使える既存コードパスを grep で列挙し、適用の要否を判断する（関数を作ることとその関数を使うことは別の判断）
   - **同一パターン全コードパス検索**: バグを発見したとき、そのバグパターン（根本原因）をコードベース全体で検索し、同一パターンの別箇所を列挙する
   - **変更なしの根拠明示**: 「変更なし」と判断するとき、影響するケースを列挙して根拠を裏付ける
   - **キー/識別子形式の変更は全コードパスで同時に揃える**: 識別子・キー形式を変更する場合は「新規記録」「既存データ移行」「外部参照API」の3者が揃っているか確認する
3. 事前調査（レビュー未然防止）を実施する
   - 比較関数 + データ構造は「先頭要素が最良/最悪どちらか」を一文で明示
   - 最適化は「意味を変えない不変条件」を箇条書きで定義
   - 挙動変更なし前提は代表入力/出力をベースライン化して差分検証
   - 境界条件を列挙し最低1件ずつ検証ケースを用意
   - **リソース管理は生成/破棄ペアで計画する**: `listen()`・`ResizeObserver`・`URL.createObjectURL` など「戻り値がライフサイクルを持つもの」を生成する際は、破棄（`unlisten`・`disconnect`・`revokeObjectURL`）の「場所・構造・理由」まで同時に記述する
   - **サンプルコードに構造の理由を付記する**: リソース管理・非同期処理の配置が問われる箇所に「なぜこの構造か」をコメントする
4. 失敗するテスト（または最小再現）を追加する
5. それが落ちることを確認する（Red）
6. 最小実装で通す（Green）
7. テストが通るまで 6 を反復する
8. 変更後の検証を実行する（スキップ不可）
   - Rust ファイルを触った場合: `cargo check -p snotra-core -p snotra`（必須）、追加で `cargo test / clippy` も検討
   - TS ファイルを触った場合: `npm run typecheck`（PostToolUse フックが自動実行）+ `npm run build`（必須・プロジェクトルートから実行）
   - ウィンドウ生成/表示順・ホットキー・スラッシュコマンドを触った場合: `npm test` + `npm run smoke:startup` + `npm run e2e:tauri` を必須で実行
9. 報告は「追加/更新テスト名 + 検証した不変条件」を必ず含める

- Win32 依存モジュール（`src-tauri/src/` 内の `hotkey.rs`, `ime.rs`, `platform.rs`）はユニットテスト前提にしない
- モジュール固有の不変条件・TDD ルールは各サブディレクトリの `CLAUDE.md` を参照

### RETROSPECTIVE.md の運用

- **更新タイミング**: サイクル終了後（実装・レビュー・追加修正まで完了したとき）
- **更新方法**: 上書き（追記しない）。前回サイクルの内容を新サイクルの振り返りで置き換える
- **フォーマット**: 「よかったこと・伸びしろ・ネクストアクション」の3セクション構成
  - **ネクストアクション**: チェックリスト形式。ドキュメント改善（教訓の抽出）は直接 `CLAUDE.md` / `ui/CLAUDE.md` に反映し、実装・大規模ドキュメント更新が必要なものは GitHub Issues に起票する
- **更新手順**: 新しいパターン・教訓を先に `CLAUDE.md` / `ui/CLAUDE.md` / スキルに抽出してから、`RETROSPECTIVE.md` を上書きする。抽出前に上書きすると教訓が失われる

### 利用できるスキル

| スキル | 使うとき（ステップ対応） | 呼び出し例 |
|--------|------------------------|-----------|
| `/symmetric-check` | ステップ 2: コードパス変更・バグ発見時に対称ペアの適用漏れを確認する | `/symmetric-check result-clicked: added emitSelectionUpdate` |
| `/dry-check` | ステップ 2: 関数を新規定義・変更したとき、手書き重複が残っていないか確認する | `/dry-check show_main_and_emit: show() + set_focus() + emit(window-shown)` |

## デバッグ・バグ修正の原則

- バグ修正時は、コードを書く前に根本原因を一文で明示する
- 根本原因の説明には「壊れた不変条件（何が常に成り立つべきだったか）」を必ず1つ含める
- 最初の修正案が失敗した場合、同じ深さで別の推測を試みるのではなく、より深い調査に切り替える
- バグ修正時は、修正対象のパターンをコードベース全体で検索し、同一パターンが他の箇所にも存在しないか確認してから完了とする
- `snotra-core`（純ロジック層）に UI 表示文字列を持たない。エラー状態の意味は `is_error: true` フラグで伝え、エラーメッセージのような表示文字列は UI 層（`ResultRow.tsx` 等）が決める責務を持つ
- Win32 / Tauri 固有の注意事項は `src-tauri/CLAUDE.md`、データ永続化の注意は `snotra-core/CLAUDE.md` を参照
- `tauri.conf.json` や platform 固有ファイルに設定を追加する際は、その設定が Windows でサポートされているか事前に確認する（例: `backgroundThrottlingPolicy` は Windows 非対応でビルドエラーになる）
- 修正案が API 境界をまたぐとき、「呼び出し側パッチ」と「API 側で責務を完結させる修正」の両案を比較し、後者を優先する
- 競合しやすい一時状態（通知・ローディング・遅延処理）を導入する場合は、タイマー/購読のライフサイクルを単一管理し、再実行時に必ず前回ハンドルを破棄する

## パフォーマンス最適化プレイブック

→ `PERFORMANCE.md` を参照

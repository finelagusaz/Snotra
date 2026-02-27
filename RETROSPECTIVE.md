# Retrospective — Phase 1-2 リファクタリングサイクル

対象フェーズ: 再設計提案（reconstruct.md）→ 実行計画（plan.md）→ Phase 1 並列実行 → Phase 2 並列実行 → CI 整備

---

## 1. 実施した内容

### 設計・計画

- コードベースをゼロから再設計するなら、という仮想提案を `workspace/reconstruct.md` に出力
- reconstruct.md を実コードと照合し、事実に基づく修正を加えた実行計画を `workspace/plan.md` に策定
- TODO チェックリスト（~90項目）を Phase 1/2/3 に分類

### Phase 1: ファイル分割と共通化（4ステップ並列実行）

| Step | 内容 | 効果 |
|------|------|------|
| 1.1 | `commands.rs`（829行）→ 7ファイルのディレクトリモジュール | 最大ファイル解消 |
| 1.2 | `platform.rs`（606行）→ 4ファイル、`hotkey.rs` 吸収 | Win32 モジュールの見通し改善 |
| 1.3 | `BinFile` struct 導入、原子的書き込みの一元化 | 4モジュールの重複パターン解消 |
| 1.4 | `ConfigError` enum + `Config::validate()` 追加 | 設定値の型安全な検証 |

### Phase 2: 責務分離と facade 導入（4ステップ）

| Step | 内容 | 効果 |
|------|------|------|
| 2.1 | `stores/folder.ts` 抽出 | search.ts からフォルダ状態を分離 |
| 2.2 | `resultsWindowController.ts` 抽出、generation 変数リネーム | App.tsx を 410行→245行に削減 |
| 2.3 | `Engine` facade 導入、`AppState` を単一 `Mutex<Engine>` に統合 | 3重ロック解消 |
| 2.4 | `error.rs` 新設、Result-based binfmt API | エラー型の集約 |

### CI 整備

- `rust-check` ジョブを追加（Windows 上で cargo check + test + clippy）

---

## 2. 発見したバグとパターン

### バグ A: WPARAM の import 先誤り（Step 1.2 で混入、Windows 実機で発覚）

**根本原因**: `platform.rs` をディレクトリモジュールに分割する際、`WPARAM` を `Win32::UI::WindowsAndMessaging` に配置した。正しくは `Win32::Foundation`。

**壊れた不変条件**: Win32 の型は正しいモジュールパスから import されなければならない。

**発見経路**: macOS のクロスコンパイル（`cargo check --target x86_64-pc-windows-gnu`）でも同じエラーが出ていたが、「macOS だから出る既知のエラー」と誤認して見過ごした。Windows 実機で `npm run tauri dev` を実行して初めて発覚。

**教訓**: 後述「クロスコンパイルエラーを既知と断定しない」参照。

### バグ B: BinError::Display のテスト失敗（CI で発覚）

**根本原因**: `[u8; 4]` を `{:?}` でフォーマットすると `[84, 69, 83, 84]` になるが、テストは `"TEST"` という文字列を期待していた。macOS ではテスト実行不能（Windows 依存コード）のため、CI で初めて検出された。

**教訓**: CI があったからこそ、push 直後に検出できた。CI 追加の判断は正しかった。

### バグ C: clippy 警告 9件（CI で発覚）

**根本原因**: pre-existing の `collapsible_if`、`needless_return`、`unnecessary_cast` が `-D warnings` でエラーになった。

**教訓**: CI に clippy を入れるとき、既存コードベースの警告を先にゼロにしておくべきだった。

---

## 3. 構造的ミスのパターン（今後への教訓）

### 「クロスコンパイルエラーを既知と断定しない」

macOS で `cargo check --target x86_64-pc-windows-gnu` を実行すると、Windows 固有の API（`std::os::windows` 等）でエラーが出る。これは本当に「macOS だから」のエラーだが、今回の WPARAM エラーは **変更によって新たに発生したエラー** だった。両者を目視で区別するのは困難。

**対策**: CI に Windows での `cargo check` を追加した。ローカルでの判別に頼らず、CI を信頼源にする。

### 「ファイル分割時は import パスを機械的に検証する」

大きなファイルを分割すると、use 文が新ファイルにコピーされ、元のモジュールパスと異なるパスに配置されることがある。分割後は全 use 文がコンパイルを通ることを確認する必要がある。

**対策**: 分割後に必ず `cargo check` を通すこと自体は実施していたが、クロスコンパイルのエラーを誤認したため無効化されていた。CI が本質的な対策。

### 「reconstruct.md の提案を鵜呑みにしない」

ゼロからの再設計提案には事実誤認が複数含まれていた:
- SearchWindow.tsx が「6000行超」→ 実際は 223行
- Mutex デッドロックリスク → 同時保持パターンなし
- requestId の一元化提案 → 2つは別責務（ウィンドウ操作 vs 検索状態）
- フォルダ展開のスタック化 → SPEC.md は一括復帰が仕様で YAGNI

**教訓**: 理想設計と現行コードを照合する plan.md のステップが重要だった。照合なしに reconstruct.md を実行していたら、不要な変更や破壊的変更が入っていた。

---

## 4. うまくいったこと

### ワークツリー並列実行

Phase 1 の 4 ステップ、Phase 2 の 3 ステップ（2.1/2.3/2.4）を git worktree で並列実行し、cherry-pick で main に統合した。独立した変更を並列に進めることで、逐次実行と比べて大幅に時間を短縮できた。

### 3層の設計プロセス（reconstruct → plan → execute）

「理想→計画→実行」の3段階で進めたことで、実装前に事実誤認を修正できた。特に plan.md での照合ステップが、不要な変更の防止に直結した。

### Phase 3 の「やらない判断」

3ステップとも現時点で実施不要と判断し、実施判断基準を明記して GitHub issue (#75, #76, #77) に移管した。YAGNI の原則に従い、計画にあっても不要なものは実行しなかった。

### CI 追加の即断

WPARAM バグの発見直後に「なぜ早く気づけなかったか」を分析し、その場で CI に `rust-check` を追加した。問題発見→原因分析→仕組み化のサイクルが速かった。

---

## 5. 残存リスク（次サイクルへ引き継ぎ）

| 問題 | 場所 | 判断 |
|------|------|------|
| App.tsx の win.on* 系 cleanup 未登録 | App.tsx | HMR のみ影響・保留 |
| IME タイミング競合 | platform/mod.rs | HWND 直接渡しで緩和済み・理論上残存 |
| searchGeneration の二重管理 | resultsWindowController.ts / search.ts | リネーム済み・ui/CLAUDE.md に注意点記録 |
| `workspace/reconstruct.md` の削除判断 | workspace/ | 参照価値が薄れたら削除 |

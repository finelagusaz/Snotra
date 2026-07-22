# Retrospective — #532 SU1（snotra-egui-runtime を wgpu → softbuffer へ置換）

## よかったこと

### spec 段階の多レンズ + codex + advisor 敵対レビューが、実装前に設計の穴と偽 blocker を潰した
brainstorm → spec の直後に 3 レンズ（描画正当性・境界/API・不変条件）+ codex + advisor で反証を試み、実装 1 行前に spec を硬化した。codex の blocker「`softbuffer::Surface` は !Send」は softbuffer 自身の `__assert_send`（Surface は Send・!Sync のみ）で一次証拠反証。size 同一フレーム同期・free の present 非依存・失敗リトライ・surface のスレッド親和性生成を spec に織り込んだ。**設計の反証は一次ソースで裏取りする**（memory [[codex-exec-adversarial-review]]）。

### 「壊れた出力から推論しない」が 3 度機能した
rust-analyzer が編集途中の中間状態を捉え、T2/T4/T5 で「型が見つからない/フィールド無し」等の首尾一貫した compile 診断を出したが、いずれも `cargo test`/`cargo check` の再実行で緑を確定（規則2）。診断と実測が矛盾するたび、診断を根拠にせず cargo を一次証拠にした。既存機構が near-miss を覆った。

### 計測が仮説を繰り返し反証し、winit 再アーキを回避した
T8 で「IME 遅延が秒単位」と聞き、私も advisor も「起床経路（InvalidateRect→RedrawRequested）バグ」を仮説にしたが、計装実行が反証（preedit→redraw 2ms・候補位置も正常）。黙って乗り換えず advisor に突き合わせ、advisor は perf の H-A/H-B 判別へ再定位。ユーザーの「英語でも遅い・ウィンドウ移動も鈍い」の一言で **debug ビルド起因**と確定し、release で 8× 改善・許容水準と実測。winit 再アーキ（runtime 全体を tauri-runtime-wry から外す）は計測が不要と証明した。

### IME 修正を純関数へ抽出して TDD + 実機で二段検証した
IMM32 の判定ロジックを `classify_ime_message`（純関数）へ抽出し TDD で固定（STARTCOMPOSITION と未確定 COMPOSITION を Suppress・確定 GCS_RESULTSTR は Tao 経路維持）。繊細な Win32 は実機で「二重表示解消・候補位置・確定/Esc/ASCII 非二重」を検証した。回帰防止の不変条件を `snotra-egui-runtime/CLAUDE.md` へ機構化。

---

## 伸びしろ

### perf の訴えでは debug/release を最初に切り分けるべきだった
「秒単位のもたつき」を IME 起床経路の問題として深掘り（仮説・計装）してから、正体が **debug ビルドの CPU ラスタ遅さ**（release 8×）だと判明した。未最適化の `fill_mesh` 密ループを `cargo run`（debug）で測り実問題と誤認した。性能調査は release 再測を最初の一手にすべきだった。教訓は `docs/development-principles.md` デバッグ節へ配置。

### 「無改変の経路」を「検証済み」と暗黙に扱った
spec は IME を「renderer 直交ゆえ動くはず」と前提したが、runtime の IMM32 経路は人間が一度も動かしておらず（#582 は削除済み winit spike を検証）、T8 で二重表示が露見した。差分が触っていなくても、新アーキで未実行の経路は未検証——受け入れで能動的に動かす計画を立てるべきだった。教訓は `docs/development-principles.md` デバッグ節へ配置。

### アーキ判断を未 root-cause の症状で下しかけた
「秒＝深い問題」の前提で winit vs 自作の Phase 2 アーキ分岐を advisor に諮ろうとし、advisor に「自分の Iron Law（root-cause 先行）違反」と正された。症状を計測で切り分けてから scope を決める。`recommend-native-over-handrolled` memory は「ツールロジックの再発明を避ける」話で、意図的に選んだ統合アーキ（tao/wry プラグイン）の放棄を促すものではない——memory に流されず適用範囲を見極める。

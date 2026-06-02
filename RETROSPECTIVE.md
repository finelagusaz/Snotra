# Retrospective — #355 メモリ削減: 非表示時 EmptyWorkingSet でアイドル working set 回収（#335 検証 → deep-research → 実機計測 → 実装 → マージ）

## よかったこと

### 実証ファーストで方向を決め、誤った投資を早期に排除した
見積もり・俗説を机上で採否せず、すべて実機計測で検証/反証してから方針を固めた。#335 アイコン圧縮（実測 ~0.06MB で却下）、deep-research（「妙案ほぼ無し」を 25 主張の敵対的検証で確認）、TrySuspend（無圧迫の実機では working set を回収しないと計測で実証）、tokio スレッド削減（VirtualQueryEx で「スタック計 2.17MB・無効」と判定）。本命の EmptyWorkingSet も PowerShell で手動実証（110→数MB）してから実装に入った。「効くと思う」より「測った」で進めたことで、桁違いに小さい/効かないレバーへの実装着手を防げた。

### 自分の仮説を自分で覆した（前提固執の回避）
属性分析の途中で「Rust 30MB / 49 スレッド → スレッド削減が有望レバー」と一度見立てたが、`VirtualQueryEx` で committed private 42.8MB を「スタック 2.17MB / ヒープ 40.7MB」に分解し、**スレッド削減はメモリ上ほぼ無意味**と自分で結論を反転させた。仮説に都合のよいデータだけ見ず、分解して定量化した。

### plan-review が実装手戻りをゼロにした
実装前の `/plan-review`（Explore×3）が windows 0.62 の API シグネチャ（`PROCESSENTRY32W.dwSize` 初期化・全 API が `Result`）と HANDLE リーク（明示 close → RAII `HandleGuard`）を先取りで潰した。結果、実装は一発で `cargo check`/`clippy -D warnings`/`test` 6/6 を通過し、plan と実装の乖離はゼロ。「windows クレートは事前に型・シグネチャ確認」という既存ルールが計画段階で機能した好例。

### 「機構が効く」と「コードが自動発火する」を分けて検証した
先行調査の手動 `EmptyWorkingSet`（機構の実証）と、実バイナリをビルドしての hide イベント駆動（統合パスの実証）を**別物として両方**確認。新ビルドで Ctrl+K（hotkey）112→9.4MB、Escape（frontend）98.5→22.8MB、再表示 UI 正常、`smoke:startup` 5/5。「調査で効いた」を「実装で効く」と早合点しなかった。

---

## 伸びしろ

### issue 本文の「ドキュメント更新が必要」主張を後で訂正した
#355 起票時に「SPEC.md 同期（必須）」と書いたが、実装調査で「類似既存機構（`suspend_webview`/TrySuspend）が SPEC に一切記載されていない」と判明し、不要へ訂正コメントを出した。**起票時にコードで一次確認していれば**避けられた手戻り（コメント是正）。ドキュメント更新の要否を主張する前に、対象ドキュメントに類似機構が実際に記載されているかを確認する——「『変更なし』の根拠を列挙する」の対称（「『変更が必要』も根拠を確認する」）。

### 非同期 IPC × メインスレッド同期の「副作用のタイミング依存」を設計段階で予測しきれなかった
plan-review は frontend 経路の `notifyMainHidden`（fire-and-forget）と `win.hide()` の順序を検証していたが、その**メモリ trim 効率への影響**（trim が hide 完了前に走り frontend は 22.8MB と hotkey 9.4MB より浅い）までは予測できず、実機計測で初めて判明 → #361 へ切り出し。非同期 IPC とメインスレッド同期処理が並行する経路では、副作用（trim 等）の「効果の深さ」がタイミング依存になりうる、という観点が次回の設計レビューに有用。

### レビュー severity の実証フィルタ（AI レビュー全般のパターン）
in-repo Explore レビューが「Critical 3 件」を出したが、実証精査で 2 件が plausible-but-wrong（「trim 並行実行が UB」は捏造＝2 経路は hide ごと排他かつ `EmptyWorkingSet` は非破壊、「window-hidden 二重 emit」は本 PR 由来でない既存挙動）。鵜呑みにせず棄却・降格し、棄却分も根拠付きで PR に透明記録した。外部 Codex 限定でなく **AI レビュー全般**の構造として [[feedback_codex_review_unreliable]] を一般化（採否前に「PR 由来か」「実コード/公式仕様と整合か」を一次確認）。CodeRabbit CLI 未導入も依頼時に発覚——レビューツールの可用性は事前把握の余地。

---

## ネクストアクション

- [ ] **#361**: frontend hide 経路の `EmptyWorkingSet` タイミング最適化（`notify_main_hidden` が tokio で `win.hide()` と並行 → trim を hide 完了後へ。22.8→9.4MB 寄りに）。優先度低・polish
- [ ] 次回リリースビルド時、**メモリ圧迫下での frontend 経路 trim 後の再表示レイテンシ最悪値**を実機計測（SSD で体感不能の確認。#361 の検証観点と兼ねる）

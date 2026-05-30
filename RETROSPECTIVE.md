# Retrospective — メモリ削減監査→実装（PR #331）と E2E 全滅の根因修正

## よかったこと

### 監査ファースト — コードに触る前に削減候補と地雷を構造把握
メモリ削減を、層別（core 常駐 / 起動スパイク / icon+IPC / frontend / deps）の多エージェント監査から始め、27 候補を抽出→懐疑的検証まで回した。「無条件 safe で恒常 RSS を大きく削れる候補はゼロ。効果のあるものは全て付帯リスク付き risky」という冷徹な結論と、踏んではならない地雷（並列 Vec の struct 化、起動 clone の `Arc` 共有、`opt-level` core s 化）を実装前に確定。素朴で危険な「メモリは減るが遅くなる」案を弾けた。

### 同一マシン A/B でのみ退行を判定した
`Box<str>` 化の性能評価を、別環境の `PERFORMANCE.md` 記録値と比べるのではなく、同一マシン・単一スレッドで `main` vs ブランチを A/B 計測して「検索・構築とも退行なし」を実証。E2E でも同じ A/B（`main` でも 11 件同様に失敗）で「自分の変更の回帰ではない」を即断定でき、調査を正しい方向（環境/ハーネス）へ向けられた。

### 仮説を実証で潰し続けた系統的デバッグ
E2E の「検索結果が出ない」を rAF→indexing→UI ゲート→stale cache→TOML parse 失敗 と 5 仮説で追い、毎回「憶測でなくデータ」で潰した。決め手は最下層の直接観測——backend の `search` IPC を executeScript で直接叩き（UI ゲートを飛び越え backend が 0 件と判明）、アプリに `startup-diag` でロード済み config を吐かせ、最後は `cat -A` で不可視の未エスケープ引用符を炙り出した。

### 黙殺された根を見逃さず起票した
真因は E2E ハーネスの不正 TOML 生成だが、それを「見えなく」していた `Config::load()` の `unwrap_or_default()`（parse 失敗を黙殺し default にフォールバック）という製品側の根を切り分け、別 issue #338 として起票。対症（ハーネス修正）と根治候補（黙殺の解消）を分けて追跡できた。

---

## 伸びしろ

### E2E が長期間「環境問題」の体裁で壊れていた（二重の隠蔽）
検索系 E2E は (a) driver/WebView2 Runtime のバージョンドリフトで全セッションが落ち、(b) その先で不正 config TOML が `unwrap_or_default()` に黙殺されて default にフォールバック、という二段の隠蔽で「環境のせい」に見えていた。`unwrap_or_default()` の黙殺は以前から潜在しており、エラーを 1 行ログするだけで発見は数分で済んだはず。外部入力の parse 失敗を黙殺しない原則を `development-principles.md` に反映済み（#338 で実装追跡）。

### 最下層観測へもっと早く行けた
推論が 5 仮説を二転三転した。「上位層の症状から推測せず最下層を直接観測する（backend IPC 直叩き・ロード済み状態ダンプ）」を最初の一手にしていれば、`backendCount=0` を即座に得て config/index 側へ直行できた。教訓を `development-principles.md` のデバッグ節に反映済み。

### 監査の検証エージェント 4 件が構造化出力を返さず脱落した
最初のメモリ監査ワークフローで verify エージェント 4 件が StructuredOutput 未呼び出しで脱落し、起動スパイク層が丸ごと未検証になった。再検証ワークフローで補完したが、脱落の検出は journal 突き合わせで手動だった。schema 必須化でも稀に取りこぼすことを前提に、脱落 ID の自動抽出と再投入を一手でできるようにしておくと取り回しが良い。

---

## ネクストアクション

- [ ] PR #331（メモリ削減 ①Box<str> 化 / ②IndexCacheRef）のレビュー・マージ
- [ ] `fix/e2e-config-toml-and-driver` を push して PR 化（`Fixes #332`、E2E 14/14 green）
- [ ] `chore/retrospective-mem-e2e`（本 retrospective + docs 反映）を push して PR 化
- [ ] #338（`Config::load()` の parse 失敗黙殺）の改修方針を判断（ログ可視化 + 上書き回避 + `.bak` 退避）
- [ ] メモリ削減後続 #333〜#337 を ROI 順（IconCache LRU → mimalloc 実測 → …）で着手判断

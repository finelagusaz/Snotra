# Retrospective — インスタントコマンドに exec(exe+args) 種別を追加 (#394)

## よかったこと

### 多観点サブエージェントレビューが「実装前」に致命的データ損失欠陥を回収した
設計合意後、user / 実装者 / QA の3レンズで並列レビューした。QA が `toml 1.1.2` で実証——初期の serde `flatten`+`untagged` 表現は旧 `command =` config を deserialize できず `Config` 全体の parse 失敗 → `config.toml.bak` 退避 → **全設定リセット**（`apply_migrations` は deserialize 後に走るため移行で救えない）になることを発見した。3レンズは**別クラス**を捕捉: user＝移行発見 UX の欠落、実装者＝`flatten`+named-legacy の serde 非互換と表現の曖昧性、QA＝データ損失の実証。前サイクル #392 の「レビュアーの独立性は枠組みの独立が効く」を継承しつつ、対象を**「コード」から「設計・計画」へ前倒し**した点が新しい。記録（前サイクルの教訓）が次サイクルで実働した。

### 最リスクの表現を「de-risk first」のゲートに据えた
legacy deserialize 往復（T2）を実装の**最初のタスク**かつ release gate に置き、渋った場合の退避B（フラット `Option` 群 + validation）を明示した。最も壊れやすい表現選択を、その上にディスパッチ・UI・移行を積む前に検証する構造。「新形式の往復が通る」だけでは false-green になるという QA の指摘を、ゲート条件に「旧形式の deserialize」を加えることで塞いだ。

### ゼロ回帰移行を「賢くしない」判断で守った
旧 `command` 文字列を exe/args へ自動分割せず、`url` へ**逐語移行**した。`Url` 経路は現状の `ShellExecuteW(command)` とバイト等価のため、今日動く config（URL・引数なし exe・スペース入りパス）を1つも壊さない。唯一壊れていた引数つきコマンドは「今日も壊れている」ので回帰ではなく、新 UI での再作成に委ねた。実機 smoke（実ロードパス `load_from_dir_reporting`）で移行+全設定保持+新形式での再保存を裏取りした。

### 前サイクルの教訓がそのまま機能した
クレート単位のタスク分解で「compile-fail を改名検出器に使う」（#388 由来）、識別子改名をシンボル粒度で扱う、を今サイクルでも実働させた。`InstantCommand` の型変更が下流クレートを意図的に compile-fail させ、`.command` 参照の漏れを機械的に列挙できた。

---

## 伸びしろ

### docs タスクが、impl タスクが意図的に省いた機能を記述した
docs 同期（Task 7）が SPEC §19.8 に「exe ファイルブラウズダイアログ」を記述したが、実装（Task 6）は plan で「任意（流用可）」とした通りテキスト欄のみだった。最終全ブランチレビューが **SPEC↔実装の乖離**（存在しない機能を SPEC が記述）を検出。根因は、docs を実装と**別タスク・サイクル末**に書くと、計画の楽観的な全体像をそのまま記述しやすいこと。→ AGENTS.md に「SPEC・docs は as-built を記述する」を反映。picker 自体は issue #395 で follow-up。

### 計画のサンプルコードが unsafe panic を内包していた
plan の `expand_env` が brief の擬似コードをそのまま再現し、`ExpandEnvironmentStringsW` のバッファを clamp せず `buf[..written-1]` でスライスしていた。env 値が2回呼び出し間で伸びると境界外 → panic、release は `panic="abort"` のため **app abort** に化ける。最終/タスクレビューが検出し1行修正（`.min(buf.len())`）。unsafe ブロックは plan のサンプルコードでも実装同等の境界精査が要る。→ src-tauri/CLAUDE.md に「Win32 2回呼び出しパターンは clamp」を反映。

### 計画テスト・転用テストの不変条件が滑った
spec のテスト T5（混在 variant の往復）が plan の具体タスクに落ちず、既存 round-trip テストの `[1].name=="memo"` アサーションが転用で消えた。後者は前サイクルの「改名・転用で不変条件を孤立させない」の再発で、**plan が著者したテスト書き換えにも同じ規律が適用される**ことを確認した（手書き編集に限らない）。両者は最終 fix wave で復活。

# Retrospective — #404 follow-up（§7.2 タブ補完 #408 / ホットキー Rust/TS 乖離解消 #409）

## よかったこと

### issue の二択前提をコードで裏取りし、二択の外の第三解を発見した
#409 は「ホットキー検証の Rust/TS 乖離を (a) Rust に揃える / (b) doc に揃える（要判断）」の二択で来たが、実コード（`hotkey_input.rs` の egui キャプチャは IME/かなキーを入力不能・`parse_vk`→`RegisterHotKey` 失敗で捕捉）と git 履歴（`57267b4`）で前提を裏取りすると、TS `hotkeyValidation.ts` は旧フロント UI 時代の**死蔵コード**（呼び出し元ゼロ）と判明。二択の外の (c)「孤児削除」が最小かつ整合的だった（既存 validate の哲学「競合キーは拒否・解釈不能キーは登録失敗に委譲」とも整合・`268 deletions` の純減）。**前提の裏取りは「与えられた選択肢の検証」だけでなく「選択肢空間の再導出」に効く**——#395 と違い着手前に proactively 裏取りできた。

### 削除の安全性を「静的 / 探索 / 意味解析」の独立三経路で裏付けた
不可逆寄りの「ファイル削除」を、① `git grep`（import 文を全 tracked から exhaustive 捕捉）② 計画を見せない独立 Explore（barrel/path alias/動的 import の不在まで再確認・影響集合が私の分析と 1:1 一致）③ `tsc --noEmit`（live import が残れば module-not-found で落ちる＝**compile を「死蔵検出器」に**）で裏付けた。枠組みの異なる三経路が同じ結論に収束＝完全性の能動的証拠。改名時の「compile-fail＝改名検出器」の削除版として AGENTS.md に一般化した。

### 検証手段の「質」で多エージェント fan-out を右サイジングした
doc-only の #408、死蔵削除の #409 とも、`/plan-review` の多エージェント fan-out・`code-reviewer` を機械的に回さず、**その問いに最も強い検証手段**（#408: 真実源 `TabId` の直読 / #409: exhaustive な git grep + compiler の死蔵検出）で代替し、判断と根拠を会話に明記した。ceremony の重さでなく検証の強さで担保する判断。

---

## 伸びしろ

### 連続 issue 着手で start-issue のブランチ作成順を一度飛ばした
#408 マージ後 main に復帰した状態で #409 の調査・計画を **main 上の untracked ファイル**として書き、ブランチ作成（start-issue Step 2）が Step 6 まで遅れた。untracked が新ブランチへ持ち越され実害は無かったが、ワークフロー順（ブランチ作成 → 調査）を逸脱。**連続着手では「マージ → main 復帰 → 次 issue は Step 2 のブランチ作成を最初に」を徹底する**。

### proportionality 省略の連続が「レビュー既定省略」化するリスク
fan-out 省略は #408/#409 とも妥当だったが、判断が続くと「レビューは省くもの」が既定化しかねない。**省略の条件は「その問いに対しより強い検証手段が実在すること」**（doc-only の真実源直読・exhaustive grep + compiler の死蔵証明）であり、安全性が自明でない変更・runtime ロジックを追加する変更では fan-out を戻す——境界を意識的に引く。

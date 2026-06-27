# Retrospective — 設定UIの見出しに Semibold ウェイトを適用 (#399)

## よかったこと

### 多層レビューが「別クラスの欠陥」を順に捕捉した
`/plan-review`（独立2サブエージェント・要対処ゼロ）→ `code-reviewer`（**High** を1件実証検出）→ **視覚スモーク**（整列バグ検出）が、それぞれ別レイヤーの問題を捕まえた。特に code-reviewer は epaint 0.34 の `set_fonts` eager-parse 内部実装まで辿り、「`YuGothB.ttc` は在るが face 2 が無い環境で範囲外 face index → panic → release `abort` で設定プロセスが起動不能」を release-blocker として前倒し回収した。plan-review の「計画の妥当性」、code-review の「実装の堅牢性」、視覚スモークの「描画の正しさ」が**重ならない盲点クラス**を分担した好例。

### `apply_type_ramp` の SSOT レバーを素直に拡張した
ctx-level の一括設定（#398 で確立）に「`Heading.family` の切り替え」を1行足すだけで、`ui.heading()` を呼ぶ唯一の2箇所（`section_heading` + `modal_header`）が同時に Semibold 化した。`ui.heading()` / `TextStyle::Heading` を grep で事前列挙し、巻き込みゼロを着手前に確定。新規パターンを作らず既存レバーに乗せた。

### issue が明記した不確実性を「実証」で潰した
issue が「face index の調査が必要」と認めた不確実性を、推測でなく `YuGothB.ttc` の `name` テーブル列挙（実ファイルの header 解析）で **face 2 = Yu Gothic UI Semibold** と確定してから計画に落とした。同じ実証姿勢を `face_index_valid`（ttc `numFonts` の事前検証）にも適用した。

---

## 伸びしろ

### レンダリング系欠陥は AI レビュー層を素通りし、視覚スモークでのみ顕在化した
「Latin（Segoe UI Semibold・tweak なし）と CJK（Yu Gothic UI Semibold・tweak 0.3）を1つの `FontFamily` に混ぜると、混在見出し『PATH 実行ファイル』で異なる vertical metrics によりベースラインがずれる」——この欠陥は型チェック・clippy・ユニットテスト・plan-review・code-reviewer のいずれも検出できず、**視覚スモークの目視で初めて顕在化**して手戻り1ラウンドになった。計画時に「複数フォントを1ファミリに混在させるなら、Latin+CJK 同一行の整列をエッジケースに挙げる」を持てていれば最初から単一フォントを選べた。→ `snotra-settings/CLAUDE.md` に「フォント登録の注意点」を反映。メモリ [[feedback_codex_review_unreliable]] にも「AI レビューの境界＝レンダリング欠陥は runtime 視覚スモーク必須」を記録。

### graceful-degrade の「不在」を浅く定義していた
当初の degrade は `std::fs::read` の Err（ファイル不在）しか想定せず、「ファイルは在るが face index が無い／パース不能」を見落とした。egui の `set_fonts` が全フォントを eager parse して panic（release `abort`）する挙動と結びつく重大欠陥で、code-reviewer が回収した。「不在時フォールバック」を計画する時点で、不在の種類（**ファイル不在 / 存在するが不正 / パース不能**）を分解して各検知点を列挙していれば自分で気づけた。→ `snotra-settings/CLAUDE.md` の「フォント登録の注意点」に「不在の種類を分解する」として反映。

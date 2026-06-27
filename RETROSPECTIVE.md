# Retrospective — メモリ削減後続（IconCache 件数上限・派生値化 #387）＋ 件数 config キーの命名整理 #388（方法論を #390 でスキル化して同セッションで実践）

## よかったこと

### 多層・異視点レビューが段階的に「異なるクラスの盲点」を回収した
#388 で plan-review(3 Explore) → Codex 計画 → 独立導出 → code-reviewer → Codex アドバーサリアル、と層を重ねた。それぞれが**別クラス**を捕捉: Codex の異視点が `engine.rs` accessor 呼び出し元の漏れ（in-repo plan-review が見落とした完全性ギャップ）、独立導出が残り完全性の能動的証拠、code-reviewer がテスト転用で消えた「新キー優先」ガードの穴。**レビュアーの独立性は「実行の独立」より「枠組みの独立」が盲点に効く**ことを実証。冗長な同一視点より異視点を足す方が漏れを拾う。

### 「検証するな、導出せよ」で検証層を一層畳んだ（#387）
`icon_cache_cap` を「独立 config キー + validation + floor で *cap ≥ ワーキングセット* を後追い保証」する構成から、**表示ワーキングセットからの派生値**へ置換。`CacheConfig`・`ConfigError` variant・i18n・floor が丸ごと不要になり正味 −74 行、不変条件は検証ではなく構造で成立。`A ≥ f(B)` を validation+floor で守っている時点で「A := f(B) と定義し直せ」のサイン——**validation を書いたからこそ従属関係がコードに露出し、導出すべきと見抜けた**。

### compile-fail を「改名検出器」に使った（#388）
3 config キーの改名で、Phase 1（core）完了後に `cargo build -p <下流 crate>` で mid-verify。accessor 改名は呼び出し元を必ず compile-fail させるため、漏れが機械的に列挙された。コンパイラ駆動リファクタで人手の grep 漏れを補完。

### 気づきをその場でスキル化し、同セッションで実践した
#388 の plan-review で自分の盲点を継承した経験 → 共通項「string でなく symbol 粒度」「枠組みの独立」を抽出 → `/plan-review` Step 2b（独立導出+差分）として **#390 でスキル化** → 直後の #388 実装で実践し完全性ゲートに使った。「記録への信頼で動く」が 1 セッション内で循環。

---

## 伸びしろ

### plan-review サブエージェントに「自分の分解」を渡し、盲点を継承した
#388 の plan-review で `engine.rs` の accessor 呼び出し元を見落とした根因は、サブエージェントに「3キーが被覆されているか検証して」と**こちらの string 粒度の枠組み**を渡したこと。同じ分解を渡せば同じ盲点を共有する。横断的変更（リネーム・移行・スイープ）では「成果物監査」だけでなく「枠組み非依存の独立再導出+差分」を併用する。→ `/plan-review` Step 2b に構造化済み（#390）。

### テストの転用で、元テストが守っていた不変条件を孤立させた
`migrate_legacy_does_not_overwrite_explicit_search_values`（「新キーは legacy で上書きされない」を担保）を改名で intermediate-vs-oldest legacy 用へ転用した結果、トップ層の「新キー優先」ガードが消えた——`get_or_insert` を `= Some(v)` に誤改変しても全テストが通る穴を code-reviewer が指摘。テストの改名・転用は「どの命題を証明していたか」も追跡し、失われるガードは別テストで補う。→ AGENTS.md に反映済み。

### 改名を「キー文字列」粒度で推論し、漏れと誤検出を両方生んだ
config キー文字列で grep すると、間接命名（`label_max_history` が result_limit を指す・accessor 経由・派生 signal）を取りこぼし、同名別概念（汎用 top-k パラメータ・bench 値 `max_results=50`）を誤検出した（名前↔概念は多対多）。改名は**シンボル単位で全呼び出し元を列挙し、概念で分類**する。→ AGENTS.md に反映済み。

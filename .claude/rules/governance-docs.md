---
paths:
  - "AGENTS.md"
  - "CLAUDE.md"
  - "docs/adr/**"
  - "scripts/*.mjs"
  - "scripts/*.ps1"
  - "scripts/lib/**"
---

# ガバナンス文書の参照と命名のルール

`AGENTS.md` / ルート `CLAUDE.md` / `docs/adr/` / `governance:check` は互いに参照し合う。**他を指すときは正準形 `` `<対象>`「<見出し>」 ``（対象は `<path>.md` か `/skill-name`）で書く。** この形だけが `governance:check` の G-heading-refs で照合され、見出しの改名・消滅は参照元を名指しして CI が落とす。

- **序数で他を指してはならない。** 見出しの序数だけでなく、**ファイル名の連番・検査 ID・その他の引用される識別子すべて**が対象である——番号は構造を凍らせ、ずれても誰も気づかない。**連番はさらに並行作業で衝突する**: 値が確定するのはマージの瞬間なので、2 本の PR が同じ値を見る（ADR の連番で 3 回・#812）。
- **名前はテーマ・目的が決まった時点で、何を指すか分かる形で付ける。** 「いま空いている最大値 + 1」という操作を持たない名前には、衝突する余地が無い。ADR は `docs/adr/ADR-<slug>.md`（引用は `ADR-<slug>`）、`governance:check` の検査は `G-<name>` とする（#812。全 ADR 移行済み・連番への回帰は G-adr-file-names が落とす）。
- **G-heading-refs が見るのは正準形だけである。** 散文形（「ルート `CLAUDE.md` のフック節」）・節を移動したときの意味の変化は検知されない（助詞が挟まった近傍形は G-near-heading-refs が拾う）。ゆえに**移動・圧縮・分割の完全性は依然として機構の外**にあり、`/plan-review`「Step 2b」相当の**独立再導出**（旧内容を SSOT に、全命題の着地を作者と別枠組みで確認）で裏取りする。
- **ADR 本文内の参照は照合されない**——凍結された歴史であり腐るに任せる（`ADR-adr-frozen-history`）。ADR を消すときは生きた層の引用を散文化してから（G-adr-citations が赤で強制する）。
- **既に消滅した節の名前を正準形で書かない。** 正準形は「今ある場所への指し」であって過去の名前の記録ではない（G-heading-refs 導入時に `PERFORMANCE.md` で実例 1 件）。歴史を書くならバッククォートを外して散文にする。
- **これらの編集に PostToolUse hook 検査は走らない**（`selectChecks` がガバナンス文書に検査を割り当てず空集合を返す。`CLAUDE.md`「フック」）。G-heading-refs を含む決定的な検査は PR CI の governance-check job が事後に走る（#587）。

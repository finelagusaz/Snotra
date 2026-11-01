# 調査 — issue #1328 永続化の規則の正本を 1 つに決める

## issue の要約

`.claude/skills/persistence-check/SKILL.md:18` は「SSOT は `snotra-core/CLAUDE.md`「データ永続化の注意」」と宣言するが、中身はスキルの側が厚い。#1327 で CLAUDE.md 側を「編集の瞬間に守る指示」だけに縮めたため、宣言と厚さが逆向きになった。

**決定（2026-11-01・ユーザー合意）: 案 (c) 役割で持ち主を分ける。**
- 規則（編集時に守る命令）の正本 = `snotra-core/CLAUDE.md`「データ永続化の注意」（6 行、現状維持）
- 検証の仕方と根拠（変更の分類・テストのアンチパターン・境界条件の表・インシデント番号）の正本 = スキル
- スキルから規則の言い直しを除き、CLAUDE.md の該当行（または一次の doc）を指す

却下: (a) スキル正本（外部の参照元が「節に規則がある」前提で腐る・編集時の自動配送を失う）／(b) CLAUDE.md 全面正本（根拠の行き場が #1327 の「理由・経緯は書かない」に反する）／(d) 重複を宣言で許す（`AGENTS.md`「文書に事実の写しを増やす変更」に反する）。

## 届く時機（配送）

- `snotra-core/CLAUDE.md`: snotra-core のファイルを触ると harness が注入（ルート `CLAUDE.md` / `AGENTS.md`「ドキュメント参照」）
- スキル: 起動時のみ。起動の契機は `AGENTS.md:52`（条件別チェック表）・`.claude/rules/snotra-core.md:23`・`.claude/skills/implement/SKILL.md:94`

## 重複の対応表（CLAUDE.md 行 ↔ スキル行）

| CLAUDE.md（規則） | スキル側の同内容 | 種類 |
|---|---|---|
| :55 版バンプ（意味変更も） | :21-22 典型パターン 1/2、:44-47 Step 2、:52 「セマンティクス変更の見落とし」 | :52 は規則の言い直し＋#338。:44-47 は検査項目 |
| :56 `deserialize_failed → save()` 禁止 | :24 パターン 4、:79-80 | :80「→ フォールバック…昇格」は規則の逐語に近い言い直し |
| :57 失敗の種類で分ける＋兄弟分岐 | :25 パターン 5、:81-86 | :82-84 の 3 区分は `config/io.rs` `load_from_dir_reporting` の doc（区分の正本）の写し（三重目） |
| :58 TOML フィールド移動 | :97 | スキルの方が欠落（`Config::default()` の `field: None`・部分テストの注意が無い） |
| :59 serde 表現変更 | :73 | 規則の言い直し＋#394 |
| :60 凍結バイト列で証明・spike | :58-63, :70, :72 | 検査項目とアンチパターン。**spike はスキルに無い** |

スキル固有（正本として残す）: Step 1 の分類（:29-39）・不動点のアンチパターン（:66-67, :71）・`serde(default)` の fixture（:74）・Step 6 の境界条件表・出力の規律。

## 見つかった写しの腐り（今回直す対象）

- **:94 の見出し参照が壊れている**: スキルは「history.rs のキー正規化チェックリスト」、実見出しは `snotra-core/CLAUDE.md:62`「history.rs のキー正規化に関するチェックリスト」
- **:92-93 の項目要約が古い**: 「（struct / version / fallback / load / save / CachedMasks / new_with_cached_masks）」は 7 項目。実リスト（`snotra-core/CLAUDE.md:76-84`）は 9 項目で、8（直前の版の受け手を数え直す）・9（golden bytes）が欠落
- スキル内の「CLAUDE.md」が無修飾（:92, :94）——どの CLAUDE.md か曖昧

## 外から「データ永続化の注意」を規則の出典として名指す参照元（変えない前提の裏付け）

- `snotra-core/src/config/tests.rs:311`、`snotra-core/src/indexer/cache/tests.rs:267,396,457`
- `PERFORMANCE.md:816`（spike の規則を引用）
- `docs/adr/ADR-index-v7-cache-miss.md:21`（凍結）
→ (c) は CLAUDE.md の 6 行を変えないので、これらは腐らない。

## 再利用できる既存パターン

- `.claude/rules/snotra-core.md:8`「事実の正本は … 本 rule は『どこを読むか』だけ（要約を置かない）」——指す側は要約を置かない
- `/implement` の SKILL.md:94「SSOT はその表。二重管理を避けるためここに再掲しない」
- `/retrospective` SKILL.md:64「本文の SSOT は skill 側」——教訓の振り分け先としてのスキル

## 技術的制約

- スキル変更はセーフティネット（ルート `CLAUDE.md` 最重要ルール 2）→ 案 (c) は合意済み
- `docs/check-skill-skeleton-design.md`: 必須は母集団（`/persistence-check` = 対象 struct ✅）と費用対称性。母集団・停止の構造を崩さない
- `governance:check`（`npm run governance:check`）が照合するのは、フェンスの外にある正準形（バッククォートで閉じた対象＋「…」）だけ。:94 はどちらにも当たらないので素通りする（疑問 1）
- `.claude/` 編集は分類器に flag されうる（メモリ）

## 未解決の疑問（すべて裁定済み）

1. ~~`governance:check` はなぜ :94 を捕まえないか~~ → 理由は 2 つ（敵対的調査 B1、`scripts/governance/lib.mjs:209` `HEADING_REF` と `:124` `linesOutsideFences` を自分で読んで確認）: (a) 正準形はバッククォートで閉じた対象＋「…」だけ、(b) Step 5 は ``` フェンスの内側で、フェンス内は走査されない。**直すなら参照をフェンスの外へ出し正準形で書く**。フェンス内で直しても検証は偽の緑になる
2. 検査項目（`[ ]` の問い）は検証の形としてスキルに残す。除くのは規則の本文・帰結（「→ フォールバック…昇格」、3 区分の列挙、「Legacy で受理し移行」、TOML 移動の手順、:52 の言い直し）だけで、そこは正本を指す
3. 典型パターン 1〜5 は「規則の根拠になった失敗の型」としてスキルに残す（(c) で根拠はスキルが持つ）

## 敵対的調査（3b）の所見と採否

出典: `workspace/adversarial-1328.txt`（sonnet 1 体）。

**壊せた**
- B1 :94 を governance:check が捕まえない理由は 2 つ（正準形でない＋フェンス内） — **採る**。lib.mjs を自分で読んで確認した。計画は参照をフェンスの外へ出す形にし、変異で赤になることを確かめる
- B2 「TOML 移動規則はスキルに無い」は偽。:97 にある（`take()`・`field: None`・部分テストの注意が欠けている） — **採る**。対応表のとおり、:97 は「欠落した写し」として扱う。上の対応表は元から :97 を挙げていたが、「技術的制約」の言い回しを直した
- B3 スキル固有の列挙漏れ: :96（Config を deserialize する新経路で apply_migrations を要否判断）と :45-47（汎用 struct の旧版フォールバック）は CLAUDE.md に無い — **採る**。どちらも検査項目としてスキルに残す（消さない）。W3 は自分で確かめた: `docs/architecture.md`「データ永続化」節（:102-）に apply_migrations の記述は無い
- B4 3 区分は写しではなく再編（doc は 5 項目、スキルは parse 失敗と InvalidData を束ねている）。意味のずれは無い — **採る**。それでも区分の正本は doc なので、スキルは列挙をやめて doc を指す（再編版は将来ずれうる）
- B5 参照元の漏れ: `PERFORMANCE.md:993`、`snotra-core/src/config/io.rs:59`（節名なし） — **採る**。どちらも CLAUDE.md の 6 行が変わらなければ腐らないので、作業は増えない

**壊せなかった**: K1 :94 の壊れは実在／K2 7 項目 vs 9 項目／K3 行番号と引用の一致／K4 spike はスキルに無い／K5 :73 と CLAUDE.md :59 は矛盾しない／K6 governance:check は今日 green

**⚠️**: W1 フェンス内を走査しないのは仕様（lib.mjs のコメントで宣言済み）。今回はフェンスの外へ出すので争点にしない／W2 → 疑問 2・3 で裁定／W3 → B3 で確認済み／W4 行番号は作業ツリー基準（実装時に見出しとシンボル名で引き直す）

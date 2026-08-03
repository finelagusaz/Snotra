# research: issue #884 実装順序 5「寄せる」— SPEC 語彙降格（本サイクルの射程）

## issue の要約

#884 の中心原理: SPEC のドリフトは語彙の層で決まる（層 1 SPEC 固有語彙・層 2 ユーザー観測名・層 3 パス・層 4 シンボル名・層 5 式値）。あるべき粒度は「1〜2 で書き、ナビは 3、4 を追放、5 は導出ペアのみ」。本サイクルの射程はユーザー裁定（2026-08-03）で**実装順序 5（語彙 4 の系統的降格）のみ**——装置 2〜4・6（mermaid 語彙化・極性反転・述語拡張・ペア検査)は降格後の残余人口を実測してから別途裁定。

## 現時点の再計測（2026-08-03・#885/#888 適用後の SPEC）

計測スクリプト: scratchpad `enumerate-spec-spans.mjs`（バッククォート内スパン全数列挙 + 機械分類。fence 追跡・mermaid 語彙 49 語・config キー 87 個・production .rs 照合）。

- **総スパン 712**（unique ではなく出現数）
- 機械分類: L4:snake 154 / L3:path 119 / L2:config-key 62 / L4:pascal-camel 28 / L1:spec-vocab 27 / L4:rust-path 20 / L2:event 12 / L5:value 7 / other 246 ほか
- L4 バケットには誤含みが多い（`main` `results` は窓名=層 1、`trim` `lower` 等はテンプレート構文=層 2、`Ctrl` 等はホットキー語彙=層 2）。**選り分け後の真の層 4 候補は約 60 件**（初稿の「58 件」と符合）
- 発見: `compute_window_height`（@184）と `get_bootstrap_payload`（@399）は**負の契約形（旧 `X` は撤去/消滅）で既に正しく書かれている**——N2 の射程であり本サイクルでは動かさない

## 関連ファイル・シンボル（実在確認済み）

- `SPEC.md`（唯一の変更対象・1150 行余）
- 照合先: production `.rs`（snotra-core / snotra-egui-runtime / src-tauri / snotra-settings）、`src-tauri/src/events.rs`（イベント名正本）、`snotra-core/src/config.rs`（config キー・`icon_cache_cap`）
- 検出器: `scripts/governance-check.mjs` の G-stale-identifiers（SPEC は `STALE_EXTRA_DOCS` に含まれ、camelCase・SCREAMING_SNAKE のみ射程。snake_case 単語は射程外＝今回の降格対象の大半は機械で守られていない層）

## 再利用できる既存パターン

- #885 の降格形: 「式ごと落としてパスへ参照」（語彙 5→3）・「文言の正本は `strings.rs` の `launch_timeout`」（アンカー参照）
- #888 の降格形: 観測文＋所在参照（機序はコード隣接コメントが正本）
- 負の契約形: 「旧 `X` は撤去済み（#NNN）」——N2 が将来極性反転で検査する正準形。今回は不変

## 技術的制約

- 3 層分担: 挙動変更ゼロ・コード無変更（SPEC の記述形式のみ）。ゆえにベースラインは「変更前後で SPEC が束縛する観測可能な契約が同一」であること
- G-heading-refs: 見出し参照形（`CLAUDE.md`「…」）は既に機械照合されており、これへ寄せる降格は検査済みの辺に乗る
- 並行セッション: `fix/visual-color-same-frame` ブランチが origin に存在。§11 色関連の文言に触れる編集（本計画に 1 件も無いことを確認済み——E 表は §11 を触らない）と衝突しない

## 未解決の疑問（→ plan の「未確定」へ）

- なし（置換文言はすべて現文言を読んで確定済み。判断が割れうる keep 群は plan の K 表に理由付きで列挙し、承認で確定する）

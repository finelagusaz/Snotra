# plan-review: #1106（観点 1: 状態遷移 / 観点 2: 値の運び方）

## 要対処

- **`DriveResultsInputs`/`FrameVisibleRows` の doc 拡張が「制約の理由」を名指す指示を欠く** —
  `window_coordinator.rs:772-775`（`DriveResultsInputs` の doc）は現在 `width` / `row_height` の
  2 種の「別種の制約」しか語らず、`fn max_results` の doc（`:745-746`）は「読み点の制約を持たない」
  と明記している。計画フェーズ3チェックリスト（plan.md:133-134）は「制約が生まれたことへ改める」
  としか書いておらず、**新しく生まれる制約が width とも row_height とも異なる第三の理由である**
  ことを doc へ書く指示が無い。実際に読み取れる理由は「起動側ゲート（`activate_or_execute` /
  `shift_activate`）と**同一の 1 回読み**でなければならない——ズレれば #1106 の症状そのものが
  1 フレームでも再発する」であり、width の理由（main への同時適用）にも row_height の理由
  （`VisualSnapshot` 由来・#673 決定 4）にも一致しない。`read_config` の doc
  （`mod.rs:411`「同じ値の一貫性が要る読みは1回にまとめること」）はこの一般原則を裏書きするが、
  **どの一貫性が要るか（誰と一致させるためか）は個別に書かないと伝わらない**。
  このリポジトリでは読み点の理由を書き漏らすこと自体が再発した欠陥クラスである
  （`present_results` の doc が `#752 F2` の非対称を、`FrameIndexing` の doc が `#1077` の単一読みを
  それぞれ名指しで書いている先例と対照）。**提案**: フェーズ3のチェックリストへ、
  「`FrameVisibleRows` は `AtomicBool` の live-read ではなく config 由来ゆえ `indexing` より
  レースの窓は狭いが、活動ゲートと表示ゲートの単一読み一致という同じ理由で単一化する」ことと、
  `DriveResultsInputs.max_results` の制約が width/row_height のどちらとも異なる第三の理由である
  ことを明記する 1 文を追加する。

- **D3 が引く `ADR-activation-gate-placement` 却下 1 の理由は、gate④には部分的にしか転移しない** —
  却下 1（`docs/adr/ADR-activation-gate-placement.md:20-26`）は 2 つの独立理由を持つ:
  (a) 「ガードの意味は選んだ行の性質であり、`start_launch` に届く時点で行の生データが無い」
  （意味論の層のミスマッチ）、(b) 「却下しても数は減らない——`shift_activate` の `tools >= 2` 枝は
  `start_launch` を通らない」（構造）。gate④の述語 `results_area_collapsed(max_results: u32)`
  （plan.md:28-30）は `view_kind` も `instant_rows` も引数に取らない——**理由 (a) が前提とする
  「行選択の性質を再導出する層のミスマッチ」はそもそも成立しない**（gate④は行を一切見ない）。
  D3（plan.md:65-71）は「決定をそのまま踏襲する」とだけ書き、この非対称に触れていない。
  **結論自体は変わらない**——理由 (b)（`shift_activate` の `tools >= 2` 枝が `start_launch` を
  通らないため、`start_launch` に置いても呼び出し点は 2 か所のままで得るものが無い）は gate④にも
  そのまま当たり、`activate_or_execute` / `shift_activate` の冒頭配置を単独で正当化できる。
  **ただし「そのまま踏襲する」という書き方は、理由 (a) が転移しないことを覆い隠す**——将来 gate⑤
  のような「行を見ない」述語を検討する読者が、却下 1 を「意味論的に必ず却下される」と誤読し、
  実際には理由 (b) だけで決まる判断だと気づかない恐れがある。**提案**: D3 に 1 文足し、
  「却下 1 の理由 (a) は gate④の述語が行選択を引数に取らないため転移しない。それでも理由 (b)
  （呼び出し点の数が減らない）だけで結論は変わらない」と明記する。

## 軽微

- **`SPEC.md`「4.5 最大列挙数」の対象 bullet（`SPEC.md:189`）は連言②と連言④を同じ文で語っており、
  計画の追記先がどちらに係るか曖昧になりうる** — 現行文「0 件のときは窓を出さない（§8.6 の連言
  『結果が空でない』）。最大表示件数が 0（`config.toml` の手編集でのみ到達する）のときも出さない」
  は 1 文目が連言②、2 文目（「も」で連結）が連言④を指す。計画（plan.md:107-108）は
  「起動にも使えないことを1行足す」とだけ書いており、2 文目（連言④）にだけ係ることを明示していない。
  意味的な実害は小さい（0 件は元々 `on_enter` の `!results().is_empty()` で起動できない・
  `launcher_controller.rs:1414`）が、追記が 1 文目にも係ると読めると、この issue が対象にしていない
  連言②まで #1106 の変更範囲であるかのように読める。**提案**: 追記文を 2 文目の末尾に付け、
  「（連言④のとき）」等で係り先を明示する。

- **`SPEC.md`「8.6 状態遷移図」への参照追加（`SPEC.md:560`）が、2 つの独立ゲートを「も従う」の
  1 文で連結すると OR ではなく AND に読める** — 現行文は `Shift+Enter [tools >= 2]` の遷移が
  「§4.7 の表示ゲートにも従う」と書く（gate③=indexing）。計画（plan.md:109-112）はここへ
  「§4.5 の連言④にも従う」を足す方針だが、実装（`launcher_controller.rs:557-563` /
  `:586-592`）では gate③と gate④は**独立した 2 本の早期 return**（どちらか一方が真なら止まる。
  状態モデル節「× indexing: 直交、両方真なら両方のガードが効く。順序は問わない」— plan.md:216）
  であり、両方が真であることを要求する AND ではない。1 文へ「〜にも従う」を連ねると、将来の
  読者が「両方成立して初めてゲートが効く」と誤読しうる。**提案**: 「独立に、どちらか一方でも
  真なら」等、OR であることを明示する語を足す。

- **`shift_activate` 内の gate④ 挿入位置（`plain_results_hidden` / `folder_load_pending` との
  相対位置）が計画に明記されていない** — D3 は「`shift_activate` の `enter_tool` 枝の手前」とだけ
  書く（plan.md:65, 146）。`shift_activate`（`launcher_controller.rs:576-624`）は
  instant/Tool 短絡（:577-581）→ gate③（:586-592）→ `folder_load_pending`（:593-599）→ 行取得
  → `tools >= 2` 分岐、の順であり、gate④をこの末尾（`enter_tool` 直前）に置いても、
  gate③/`folder_load_pending` が先に別理由で return するケースと排他的に重ならないため機能的な
  欠落は無いことを実装コードのトレースで確認した。ただし `activate_or_execute` 側は
  「`plain_results_hidden` の手前」と明示されている（plan.md:68）のに対し `shift_activate` 側は
  そうではなく、実装者が同じ規則（gate③より前）で書くのか、末尾（`enter_tool` 直前）で書くのか
  文面だけでは一意に決まらない。実害は無いが、レビュー時に「④は carve-out が無いので常に手前」
  という規範（plan.md:68-69）と実装位置の対応を確認しにくい。**提案**: `shift_activate` 側も
  具体的な挿入点（`folder_load_pending` の直後・`tools.len() >= 2` 判定の直前）を明記する。

## 未検証

- **観点 2（値の運び方）の実際のレース窓の広さ**: 計画は `/race-check` をフェーズ5（実装後）へ
  意図的に遅延している（#784 に倣う・plan.md:162-163）。ドキュメント上の推論（config 由来ゆえ
  `AtomicBool` より窓が狭い）は research.md の技術的制約1で述べられているが、実測はまだ無い。
  計画通りフェーズ5で検証される前提であり、計画自体の欠陥ではない。
- **`FrameVisibleRows` / `read_visible_rows` の実装コード自体**: 本レビュー時点でコードは未実装
  （plan.md のみ）。型の構築・re-export・呼び出し点の配線が計画通りに実装されるかは、
  フェーズ3の実装完了後でなければ確認できない。

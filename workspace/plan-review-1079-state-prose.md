# plan-review: #1079 状態遷移 + 散文の数え上げ（観点2本のみ）

対象: `workspace/plan.md`（issue #1079「folder 往復で is_unsettled が偽になる」の実装計画）

## 要対処

- [観点2] `docs/adr/ADR-row-replacement-choke-point.md` の :21 と :34 を編集する計画項目（Phase 4 最終 2 タスク）は、このリポジトリ自身が確立した「ADR は凍結された歴史であり編集しない」規約に反する。 / 根拠: `docs/adr/ADR-adr-frozen-history.md`「ADR 本文は決定日時点の世界の記述として凍結し…改名・移動に追随させない」。`.claude/rules/governance-docs.md`「ADR 本文内の参照は照合されない——凍結された歴史であり腐るに任せる」。同じ状況（後発の変更で ADR の記述が古くなる）に対する既存の先例が 3 件あり、いずれも「旧 ADR は編集しない」を明言している: `docs/adr/ADR-show-path-derives-drawn-height.md:15`「旧 ADR は編集しない（ADR-adr-frozen-history: ADR は凍結された歴史）。反転はここに記録する。」、`docs/adr/ADR-doc-promise-over-area-ratchet.md:27`「旧 ADR は凍結ゆえ編集しない」、`docs/adr/ADR-retire-area-budget.md:39` 同文言。plan.md はこの先例を踏まえず「式の逐語写しを同期する」と書いており（plan.md:101, plan.md:60）、`AGENTS.md`「文書に事実の写しを増やす変更」の「正本を 1 か所に定め他は参照へ」という一般則を、正しくは「参照・写しではなく凍結対象」である ADR にまで機械的に適用してしまっている。 / この指摘を偽にする手順: `docs/adr/` 配下で「#1079 以降にマージされた PR が既存 ADR の本文（決定・却下理由の記述）を書き換えた」実例を `git log -p -- docs/adr/ADR-row-replacement-choke-point.md` 等で 1 件示せれば、この規約は「新規 ADR の追記」に限られず「既存 ADR の是正編集」も許容していることになり、指摘は崩れる（今回の grep では該当 3 ADR はいずれも「新しい ADR を書いて反転を記録する」パターンのみで、既存 ADR 本文を書き換えた例は確認できなかった）。

## 軽微

- [観点2] `docs/architecture.md:228` の対象文には「Escape は行を同期で差し替えることによってこれより早く（in-flight の）窓を閉じる」という主張が含まれる（同行後半）。修正後は Escape の `put_rows` は相変わらず同期だが、直後に `restored_rows_stale` が真になりうるため「Escape で窓が閉じる」という言い切りは folder 経路に限り成り立たなくなる。plan.md の Phase 4 タスク（:101「folder の往復を跨いで持ち越される経路を足す」）はこの行の対象として挙がっているが、「窓を閉じる」という既存の言い切り自体をどう弱めるかまでは書いていない——実装時に見落とすと、追記した一文と既存の「これより早く閉じる」が同一段落内で矛盾したまま残りうる。
- [観点2] `SearchState::is_unsettled` の doc（:561-562、`search_state.rs`）にある「この述語が自分の意味に反して偽を返す既知の状態が 1 つある——folder を往復した直後である」という文は、本 issue の修正が入れば偽になる（既知の反例が無くなる）。plan.md の変更ファイル表は当該行を含む :524-567 の範囲を「同期」対象に含めており（plan.md:56）機械的には拾えているが、Phase 4 のタスク一覧（plan.md:96-102）にはこの一文の削除・書き換えが明示個別項目として無い。範囲に含まれてはいるので見落としの実害は低いが、「第 3 disjunct の意味を書く」という追記作業に気を取られてこの既存の「既知の反例は 1 つ」という主張の除去を落とす経路がある。

## 未検証

- Phase 3 の変異検査（`put_rows` の clear を消す／`on_escape` の代入を消す／`is_unsettled` の第 3 disjunct を落とす）は計画段階であり実装前のため、実際に指定どおりのテストが落ちるかは確認していない（plan.md 自身も「未検証」として明記済み・plan.md:148）。
- `search_state.rs` 全 1622 行のうち 1101 行以降（テスト本体の後半、境界条件・折り返しテスト群）は今回読んでいない。境界条件 5 本（plan.md:127-133）に対応する既存の類似テストパターン（`late_worker_rows_are_dropped_after_navigate_folder` 等・既読の 855-949 行）は確認したが、後半にある可能性のある別の `enter_folder` 呼び出しパターン（1200 行以降の folder token 関連テスト群）との相互作用までは読んでいない。ただし production 呼び出し点は `launcher_controller.rs` の 2 箇所のみとグレップで確認済みであり、テスト側の記述はコンパイル時に signature 変更で機械的に検出されるため、観点1 の結論には影響しないと判断した。

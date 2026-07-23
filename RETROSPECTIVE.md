# Retrospective — #532 Phase 2 SU4（アイコン + 視覚 pass + §11 テーマ消費）

## よかったこと

### 着手前に 2 プローブで設計フォークを実測確定した
worker 要否と font_family honor 可否という 2 つの設計の分岐を、実装前に使い捨てプローブで測って閉じた（#634 が SU3.5 前に G-SYNC を測ったのと同じ刻み・ユーザーの「まず測る」流儀）。**両方とも仮説を実測が覆した**: (1) Probe 1 は 8 件アイコンバッチ warm 合計 p50=8ms/max=11.7ms を出し「update() 内同期で足りる」を否定 → worker を数字で正当化。(2) Probe 2 は Segoe+YuGothic でベースラインずれが**出ない**ことを示し「honor は #579 を再発させる」という advisor+自分の仮説を覆した（ずれはフォントの組で決まり、MS システムフォント同士は揃う）。土俵を疑ってから設計に入ったことで、間違ったアーキテクチャを作らずに済んだ。

### task-scoped review が実バグを捕捉し、whole-branch はほぼ空だった
subagent-driven の各タスク review が Task 3（name 折り返し・CJK 幅過小評価）と Task 5（per-repaint thread pileup）という**実挙動バグ**を捕まえ、修正 → 再レビュー Approved で閉じた。最終 whole-branch review（opus）の指摘は stale doc 1 行のみ——M2/M3 では whole-branch だけが read-without-write 横断非対称を拾っていたが、今回はそのクラスが task 段階で予防できていた。多段レビューが機能した。

### controller が毎タスク cargo build/test で接地した
この環境の LSP diagnostics が編集途中の stale を出し、実装者の「DONE・green」報告と矛盾する事象がほぼ全タスクで起きた。報告も diagnostics も鵜呑みにせず、毎回 `cargo build/test` の実行結果で接地したことで、幻の欠陥を追わず・実際の破損も見逃さずに済んだ。

---

## 伸びしろ

### 視覚スモークの起動対象を誤り、ユーザーに検出された
視覚スモークのコマンドを `cargo run --release`（`-p` 欠落）で渡し、ワークスペースの別 bin `snotra-egui-mvp`（Phase 1 スパイク・SU 実装を含まない）が起動した。ユーザーの「MVP が対象で良いのか」で発覚。**構造的原因は、`docs/build-commands.md` のカテゴリ D（UI 視覚スモークのトリガー）が `npm run tauri dev`（WebView2 経路）しか書いておらず、egui 経路の起動コマンドを欠いていたこと**。製品 egui 経路 `SNOTRA_EGUI_MAIN=1; cargo run -p snotra` をカテゴリ D と単独起動リストに明記して塞いだ（コマンドの SSOT を接地させる方が、注意書きより効く）。

### dispatch を narrate しonly で tool 呼び出しを発行し忘れた
Task 5 で「実装者を起動します」と述べたが Agent 呼び出しを実際に発行せず、ユーザーの「実装者は活動中か」で発覚。行動を述べたら**実際に発行したか確認する**——[[confabulating-tool-results]] の隣接失敗（行動を語ることと行うことの混同）。メモリ `lsp-diagnostics-stale-mid-edit` に addendum として記録。

### メモリ本文更新時に索引行を同期し忘れた
SU4 完了を issue-532 メモリ本文に書いたが `MEMORY.md` の索引行を更新せず、サイクル末 health-check の 7b（索引↔本文一致）が検出。メモリ書き込み規律「本文更新時に一行ポインタも更新」の取りこぼし。retrospective の Step 6 で同期済み。**本文と索引は同じ編集で揃える**。

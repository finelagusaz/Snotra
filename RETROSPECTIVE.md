# Retrospective — 履歴保存 I/O の Engine ロック外実行（#526）

## よかったこと

### ロック外 I/O 化で失われる順序保証を TDD で先に固定した
`Engine` mutex の外へ保存を移すだけでは、遅延した古い snapshot が新しい `history.bin` を上書きできる。`PreparedHistorySave` の準備時点ではファイルが作られないことに加え、保存順序を逆転させても新しい内容が残るテストを置き、性能修正で永続化の単一書き手契約を壊さない構造にできた。

### 同一パターンの3経路を共通 API 境界で揃えた
起動履歴、フォルダ展開履歴、終了時 flush の全経路を `prepare_*` → guard 解放 → `save()` の同じ形に揃えた。剪定容量の live-read とオンディスク形式を変えず、ファイル I/O だけを `Engine` のクリティカルセクションから分離できた。

---

## 伸びしろ

### rustfmt の対象指定を module tree 非再帰だと思い込んだ
`main.rs` を rustfmt に渡すと子 module まで整形され、無関係な差分が一時的に広がった。開始時の clean 状態を根拠に Rust 差分を戻して意図した patch を再適用したが、検証コマンドの作用範囲を実行前に確認すべきだった。全体の format check は既存未整形で失敗するため、今回の合否根拠には `git diff --check`、clippy、変更 crate のテストを用いた。

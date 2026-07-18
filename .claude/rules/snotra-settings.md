---
paths:
  - "snotra-settings/**/*.rs"
---

# snotra-settings ルール

詳細は `snotra-settings/CLAUDE.md` を参照。

- **`saved` は「検証を通った config 全体」に差し替わる時だけ更新する**: `draft` は自由に変更可。`saved` が進む契機は save 成功と完全な config のロード（初期化・import 等）で、いずれも部分編集ではなく全体差し替えである。編集中の `draft` を部分的に `saved` へ写す経路を作ると部分保存バグになる。核心は、バリデーション/IO 失敗時に `saved` を進めないこと
- **egui API 型注意**: `Key::ALL` は `&[Key]`（`for &key in`）、`color_edit_button_srgba` は永続変数の `&mut Color32` を渡す（一時変数だと変更が消える）
- **フレームごとに重い処理を呼ばない**: `list_system_fonts()` 等は `new()` で一度だけ取得しキャッシュ
- **PickerState の `active = false` リセット忘れ**: キャンセル・成功の両パスで必ず。忘れるとボタン永久無効化
- **Opener ターゲット変更は remove → add**: ルールの `target` を直接書き換えると同じルール内の他ツールも巻き込まれる

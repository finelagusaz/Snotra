# TODO

未解決の既知問題。解決したら削除する。

## M6: App.tsx の win.on* 系 cleanup 未登録

**場所**: `ui/src/App.tsx`
**影響**: HMR（ホットリロード）のみ。本番ビルドでは `App.tsx` がアンマウントされないため実害なし
**原因**: `win.onFocusChanged` / `win.onResized` / `win.onMoved` の戻り値（UnlistenFn）を捨てており、`unlistenFns` に収集されていない
**該当箇所**:
- `win.onFocusChanged(...)` (main ウィンドウ)
- `win.onResized(...)` (main / settings)
- `win.onMoved(...)` (main / settings)
**修正方針**: 各 `win.on*` 呼び出しの戻り値を await し `unlistenFns` に push する（既存の `listen()` と同じパターン）
**対応タイミング**: `App.tsx` を大きく変更する際に同時対応するのが合理的

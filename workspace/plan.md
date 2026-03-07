# Plan: Issue #155 — 起動直後のホットキー表示で検索フレーズの先頭が飛ぶ

## 前提

これは「バグ」— SPEC.md の「ホットキーで表示 → 即入力可能」という意図に対して、先頭文字が飛ぶ/ビープ音が鳴る挙動はバグ。

## 進捗

- **フェーズ 1**: ✅ 完了（MenuMaskKey 技法で `send_alt_key_up()` を改善）
- **フェーズ 2**: ⏸ 保留（フェーズ 1 の手動テスト結果を待って判断）
- **フェーズ 3**: ⏸ 保留（フェーズ 1 の手動テスト結果を待って判断）

## 改善方針

3つの独立した改善を段階的に適用する。それぞれが部分的に効果を持ち、組み合わせで問題を最大限緩和する。

---

## フェーズ 1: Alt キーリリースの合成（Rust 側）

### 目的
Alt+Q でホットキー発火後、ウィンドウ表示前に Alt の key-up イベントを OS に合成し、WebView2 に Alt 押下状態が残留しないようにする。

### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/main.rs` | `wait_alt_release_or_timeout()` 後、`show_main_and_emit()` 前に `send_alt_key_up()` を呼ぶ |
| `src-tauri/Cargo.toml` | `Win32_UI_Input_KeyboardAndMouse` feature に `SendInput` 関連を追加（既存 feature で足りるか要確認） |

### 実装詳細

`wait_alt_release_or_timeout()` の後に以下を追加:

```rust
fn send_alt_key_up() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU,
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_MENU,
                dwFlags: KEYEVENTF_KEYUP,
                ..Default::default()
            },
        },
    };
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}
```

呼び出し箇所（`main.rs` hotkey リスナー内、Alt 待機パス）:

```rust
} else if is_alt_pressed() {
    std::thread::spawn(move || {
        wait_alt_release_or_timeout();
        if hotkey_generation_for_wait.load(Ordering::SeqCst) != current_gen {
            return;
        }
        send_alt_key_up();  // ← 追加
        show_main_and_emit(&handle_for_show, ime_control);
    });
} else {
    send_alt_key_up();  // ← Alt 非押下時も念のため合成
    show_main_and_emit(&handle_for_hotkey, ime_control);
}
```

### 不変条件
- `send_alt_key_up()` は `show_main_and_emit()` の直前に必ず呼ぶ（Alt 待機パス / 非待機パスの両方）
- `is_alt_pressed()` がタイムアウトした場合（Alt がまだ物理的に押されている）でも `send_alt_key_up()` を呼ぶ。理由: ホットキー発火後の Alt 残留はいかなる場合もクリアすべき。ユーザーが Alt を押し続けている場合、物理キーの再 key-up で再度状態がリセットされるため問題ない
- `send_alt_key_up()` は Left/Right Alt ではなく汎用 `VK_MENU` のみ送信。`GetAsyncKeyState` で L/R を個別チェックしているが、key-up 合成は汎用で十分

### リスク
- 他アプリのキーフックが Alt key-up を受け取る → ホットキー発火直後なので、通常のユーザー操作と区別不可。実害なし
- UIPI 制約 → デスクトップアプリ間では問題なし

---

## フェーズ 2: input フォーカスの高速化（フロントエンド側）

### 目的
`window-shown` から input フォーカスまでの遅延を削減する。

### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `ui/src/components/SearchWindow.tsx` | `focusInputSoon()` の 2フレーム遅延を 1フレームに削減 |

### 実装詳細

現在:
```typescript
requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    inputRef?.focus();
  });
});
```

変更後:
```typescript
requestAnimationFrame(() => {
  inputRef?.focus();
});
```

2フレーム目が必要だった理由は「native show/focus timing との race」だが、120ms/280ms のリトライが既にあるため、初回を 1フレームに短縮しても安全。

### 不変条件
- リトライ（120ms, 280ms）は維持する
- trace ログは 1フレーム分に簡略化

---

## フェーズ 3: Alt ガードの文字救済（フロントエンド側）

### 目的
`SearchWindow.tsx` の既存 Alt ガード（L146-151）で、Alt 残留時の入力を「消す」のではなく「文字として処理する」に変更する。

### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `ui/src/components/SearchWindow.tsx` | `handleKeyDown` の Alt ガード改善 |

### 実装詳細

```typescript
if (e.altKey && !e.ctrlKey && e.key.length === 1) {
    e.preventDefault();
    // Alt 残留でも文字を入力として処理する
    if (inputRef) {
        const start = inputRef.selectionStart ?? inputRef.value.length;
        const end = inputRef.selectionEnd ?? start;
        const before = inputRef.value.slice(0, start);
        const after = inputRef.value.slice(end);
        inputRef.value = before + e.key + after;
        inputRef.selectionStart = inputRef.selectionEnd = start + e.key.length;
        inputRef.dispatchEvent(new InputEvent("input", { bubbles: true }));
    }
    return;
}
```

### 不変条件
- `e.preventDefault()` は維持（ビープ音防止）
- `onInput` ハンドラへの委譲は `dispatchEvent` 経由。SolidJS のリアクティブチェーンは正常に機能する
- ツール選択中 / インデックス構築中の `handleInput` 内ガードは既存のまま機能

### 注意
- フェーズ 1 の Alt key-up 合成が十分に機能する場合、フェーズ 3 は不要。**フェーズ 1 の効果を手動テストで確認してから実装を判断する**

---

## テスト方針

### ユニットテスト
- Rust 側: `send_alt_key_up()` は Win32 API 直接呼び出しのためユニットテスト不適。`cargo check` で型チェック
- フロントエンド側: `SearchWindow.tsx` のコンポーネントテストは現時点で存在しない。新規追加は YAGNI

### ビルド検証（必須）
- `cargo check -p snotra-core -p snotra`
- `npm run build`
- `npm test`

### 手動検証（必須）
1. アプリ起動直後に Alt+Q → すぐ「abc」入力 → 「abc」が欠落なく入力されること
2. ビープ音が鳴らないこと
3. Alt+Q でトグル（hide → show）後に同じ検証
4. 他アプリ（Explorer, VS Code）が Alt+Q 後に異常動作しないこと
5. Ctrl+Alt 等の複合修飾キー付きホットキーでの動作確認（設定変更して検証）

## SPEC.md 更新要否

不要。既存の「ホットキーで表示」仕様の範囲内の改善。

---

## セルフレビュー

### 1. 対称コードパス
- `show` / `hide` ペア: `hide` パスでは Alt 合成不要（非表示にするだけ）→ 変更なし ✓
- `is_alt_pressed() == true` / `false` の分岐: 両パスで `send_alt_key_up()` を呼ぶ ✓
- `focusInputSoon()` / `clearFocusRetryTimers()` ペア: 変更なし ✓

### 2. 影響範囲の網羅性
- `is_alt_pressed()`: `main.rs` の hotkey リスナー内のみ ✓
- `focusInputSoon()`: `SearchWindow.tsx` 内の 2箇所（`focusInputWithRetries`, `setInputRef`）✓
- `handleKeyDown` Alt ガード: `SearchWindow.tsx` のみ ✓
- `send_alt_key_up()` は新規関数。呼び出し箇所は hotkey リスナー内の 2箇所のみ ✓

### 3. 境界条件
- Alt タイムアウト時: `send_alt_key_up()` を呼ぶ（Alt が物理的に押されていても、ホットキー後の残留クリアが目的）✓
- `inputRef` 未定義時: optional chaining で安全 ✓
- ホットキーが Alt 以外の修飾キー（Ctrl+Q 等）の場合: `is_alt_pressed()` が `false` → Alt 非待機パスに入るが `send_alt_key_up()` は呼ばれる。Alt が押されていなければ `SendInput` の key-up は無害 ✓

### 4. リソース管理
- 新規リソース追加なし ✓

### 5. 既存パターンとの整合
- `SendInput` は Win32 API の標準的な入力シミュレーション。既存の `windows` クレート feature で対応可能か要確認 ✓

### 6. YAGNI 違反チェック
- フェーズ 3 の文字救済は、フェーズ 1 で解消される可能性が高い → 段階的アプローチで YAGNI 準拠 ✓

### 7. シンプル化の挑戦
- フェーズ 1: `send_alt_key_up()` は ~10行。シンプル ✓
- フェーズ 2: 単純な行削減 ✓
- フェーズ 3: 複雑度が高い。フェーズ 1 の効果を確認してから判断 ✓

### 8. 破壊不変条件
- **`SendInput` で Alt key-up を合成**: 他アプリに key-up が配信される。物理リリース後の合成なので論理的に無害。検知: 手動テストで他アプリの挙動確認
- **input フォーカス 1フレーム化**: 初回 focus が確立しない可能性。検知: リトライ（120ms/280ms）がフォールバック

# 調査 — issue #1211（アンインストール時にスタートアップのレジストリ値を消す）

## 1. issue の要約

`HKCU\...\Run` の値名 `Snotra`（#1210 で設定アプリから登録・解除できるようにしたもの）が、
アンインストール後に残らないようにしたい。issue の案は `tauri.conf.json` の
`bundle.windows.nsis.installerHooks` へ `.nsh` を渡し、`NSIS_HOOK_PREUNINSTALL` で
`DeleteRegValue` を呼ぶこと。

## 2. 結論（先に書く）: issue の前提は偽である

**Tauri 2.11.4 の NSIS テンプレートは、この値の削除を既に持っている。** ゆえに `.nsh` を足す
必要は無く、足すと**更新のたびにスタートアップ登録が消える退行**になる（→ §5）。

一次証拠は**レンダリング済みの成果物**である（テンプレートの推測ではない）。
`npx tauri build --debug --bundles nsis` を実行し、生成された
`target/debug/nsis/x64/installer.nsi` を読んだ（2026-09-02 実測）:

```
 35: !define PRODUCTNAME "Snotra"
 39: !define INSTALLMODE "currentUser"
 46: !define MAINBINARYNAME "snotra"
...
812:   ; Removes the Autostart entry for ${PRODUCTNAME} from the HKCU Run key if it exists.
813:   ; This ensures the program does not launch automatically after uninstallation if it exists.
814:   ; If it doesn't exist, it does nothing.
815:   ; We do this when not updating (to preserve the registry value on updates)
816:   ${If} $UpdateMode <> 1
817:     DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
```

- `${PRODUCTNAME}` は `"Snotra"` に展開される。`snotra_core::autostart::RUN_VALUE_NAME`（`"Snotra"`）と**逐語で一致する**
- `INSTALLMODE` は `currentUser` ゆえアンインストーラは昇格せず、`HKCU` は利用者自身のハイブである
- 削除は `Section Uninstall` の末尾側（`UNINSTKEY` 削除の直後）に置かれ、`$UpdateMode <> 1` で守られている

## 3. 母集団（誰の機体に残るのか）

**結論は 1 本の論拠だけで閉じる**——余分な根拠を積むと、そのどれかが腐ったときに結論ごと腐る。

- **荷重を持つ唯一の論拠**: スタートアップ登録ができるのは #1210（PR #1215, `63236153`）以降であり、**この機能はまだリリースされていない**（`git merge-base --is-ancestor 63236153 v0.19.2` が偽・v0.19.2 の HEAD は `11828aab`）。登録し得た利用者が居ない以上、**残り得る値も存在しない**
- ゆえに「登録済みの値を持つ利用者」は、#1215 を含む最初のリリース以降にしか現れない。そのリリースをビルドする CLI は `package-lock.json` で 2.11.4 に固定されている（`npm ci` を使う `.github/workflows/release.yml:42`）

**傍証（結論の必要条件ではない）**: v0.18.3 / v0.19.0 / v0.19.1 / v0.19.2 の各タグでも `@tauri-apps/cli` は 2.11.4 だった（`git show <tag>:package-lock.json` で実測）。**v0.18.3 より前のタグは走査していない**——機能が存在しないタグは母集団の外なので、上の論拠には要らない。

## 4. 更新経路では消えない（＝守られている）

`tauri-plugin-updater` 2.10.1 は NSIS インストーラを**必ず `/UPDATE` 付きで**起動する
（`updater.rs:812` `.chain(once(OsStr::new("/UPDATE")))`）。
テンプレート側は `un.onInit` で `/UPDATE` を `$UpdateMode` へ読み、上記 §2 の削除を飛ばす。
`installMode` は `passive`（`tauri.conf.json`）なので `/P` も付く。

**scope**: 「更新で消えない」と言えるのは**この updater 経路だけ**である。利用者が新しい
インストーラ exe を手で実行して既存インストールに重ねた場合、再インストールページで
「先にアンインストールする」枝（`reinst_uninstall`）へ進むと、`$UpdateMode = 0` のまま
旧アンインストーラが呼ばれ、**そこでは値が消える**（`installer.nsi:340-352` 実測）。
これは上流の挙動であり本リポジトリの変更対象ではないが、**「更新では消えない」を全称で書かない**。

## 5. issue の案（`.nsh`）を採らない理由

**第一の理由は「不要だから」である**（§2）。削除は既に在り、`.nsh` は同じ削除を二重に書く写しになる。

**第二の理由は、issue が書いたとおりに素朴に実装すると退行することである。**
`NSIS_HOOK_PREUNINSTALL` は `Section Uninstall` の**先頭**で `!insertmacro` される
（`installer.nsi` の `Section Uninstall` 直下）。上流の削除を守る `$UpdateMode <> 1` は
その先にあるので、フックの中で**無条件に** `DeleteRegValue` すれば
**updater 経由の更新でも消える**——#1210 の登録が更新のたびに失われる。

**ただしこの退行は回避可能である**（敵対的枠の所見 (a)2 を採用・断定を弱めた）。
`$UpdateMode` は `un.onInit`（`Section Uninstall` より前に走る）で `/UPDATE` から設定済みなので、
フック内に `${If} $UpdateMode <> 1` を自分で書けば同じガードを再現できる。
**ゆえに「`.nsh` は必ず退行する」とは言えない**——言えるのは「issue の案文どおりに書けば退行する」
までであり、`.nsh` を採らない判断は §2 の「不要」と §6 の「写しが増える」で独立に立つ。

## 6. 残る本当の論点: 写しの結合

削除が効くのは `RUN_VALUE_NAME == tauri.conf.json の productName` である間だけである。

| 側 | 場所 | 値 |
|---|---|---|
| Rust | `snotra-core/src/autostart.rs:41` `RUN_VALUE_NAME` | `"Snotra"` |
| インストーラ | `src-tauri/tauri.conf.json` `productName` → `${PRODUCTNAME}` | `"Snotra"` |

- どちらかがずれても**コンパイルは通り、テストも緑のまま**、アンインストール時の削除だけが静かに空振りする。issue の「確認すること」が問うていた写しの問題は、`.nsh` を書かなくても**この形で残る**
- **隣接する第 2 の結合（射程外だが記録する）**: `MAIN_EXE_FILE_NAME`（`"snotra.exe"`）と
  `${MAINBINARYNAME}`（`"snotra"`・`src-tauri` の Cargo package 名から来る）。ずれると
  autostart が存在しない exe のパスを書く。**#1210 由来であって #1211 の削除経路とは別の命題**

## 7. 関連ファイル・シンボル（実在を確認済み）

| パス | 役割 |
|---|---|
| `snotra-core/src/autostart.rs` | `RUN_VALUE_NAME` / `MAIN_EXE_FILE_NAME` / `enable` / `disable` / `is_enabled` |
| `src-tauri/tauri.conf.json` | `productName`・`bundle.windows` 未設定（`nsis` 節そのものが無い） |
| `SPEC.md` §7.7 L457 | 「アンインストールしても値が残る（#1211 で扱う）」——**偽の残余** |
| `scripts/governance/checks/` | 検査の置き場。各モジュールは `id` と `run` を export し、ファイル名 == id |
| `scripts/governance/registry.mjs` | 検査を**ディレクトリ走査から導出**する（`checks/` の**外**にある。忘れうる登録行は存在しない） |
| `scripts/governance/checks/G-clippy-disallowed.mjs` | 再利用できる先例: 非 md（TOML）を読み、2 ファイルの整合を見る検査 |
| `.github/workflows/release.yml:42,64` | `npm ci` → `npx tauri build --bundles nsis` |

## 8. 再利用できる既存パターン

- **検査の追加は `checks/` にファイルを置くだけ**（登録行は無い・`registry.mjs` の `//!`）。`snapshot.read(rel)` で任意ファイルを読める
- `G-clippy-disallowed` が TOML を正規表現で近似パースする作法（`stripTomlComment` / `tomlLine`）を持つ。今回は**片側が JSON** なので `JSON.parse` が使える（`snapshot.read` は文字列を返す）
- 検査を足すと `governance:manifest` の `checks` 列が動く → PR 本文へ `+G-<id>` の逐語宣言が要る（`governance-manifest.mjs`）

## 9. 技術的制約

- `governance-check.mjs` の契約は「依存ゼロ・決定的（ネットワーク・時刻・環境変数に非依存）」。検査は Node 標準のみで書く
- 検査ファイルには `<id>.test.mjs` が対になっている。新設時もテストを対で置く
- セーフティネットの新設は `.claude/rules/safety-nets.md` の対象——**フォールトインジェクションで一度は実測する**（片側をずらして赤になることを確かめる）
- ポータブル版（ZIP）にアンインストーラは無い（issue 記載どおり）。この経路は塞げない

## 10. 未解決の疑問

- なし（§2〜§5 はすべて一次証拠で決着済み）。判断が要るのは「検査を置くか、両側のコメントで済ませるか」——`plan.md` の未確定欄で潰す

## 11. 敵対的調査（3b）の所見と採否

全文と「偽にする手順」は `workspace/adversarial-1211.txt`。

**壊せなかった項目**（＝反証されず残った主張）: §2 の値名の逐語一致と WOW64 非該当・
`currentUser` ゆえ非昇格・§3 の「0 人」の骨格・§4 の updater が必ず `/UPDATE` を付け Msi 分岐へ
落ちないこと・issue 本文要約の正確性・`SPEC.md` §7.7 の残存記述・`MAIN_EXE_FILE_NAME` と
`${MAINBINARYNAME}` の一致・`tauri.conf.json` の実値・`node_modules` と CI の CLI 版一致（ともに 2.11.4）。

**壊せた項目と採否**:

| # | 所見 | 採否 | 理由 |
|---|---|---|---|
| (a)1 | §7 の `registry.mjs` の参照がパスを明示せず、`checks/registry.mjs` と誤読させる | **採用** | 実体は `scripts/governance/registry.mjs`（`checks/` の外）。§7 の表を 2 行に割った |
| (a)2 | §5 の却下理由が強すぎる。`.nsh` 内で `$UpdateMode` を自前ガードすれば更新退行は避けられる | **採用** | 機序を一次証拠で自分で裁定した——`un.onInit` は `Section Uninstall` より前に走り `$UpdateMode` を設定済み（`installer.nsi` 実測）。ゆえに所見は正しい。§5 を「案文どおりに書けば退行する」へ弱め、却下の荷重を §2・§6 へ移した |
| ⚠️1 | release ビルドの `installer.nsi` と debug 版の差を測っていない | **採用し、自分で潰した** | テンプレート側の当該ブロックには handlebars の条件節も `!if` も**無く**、置換は `${PRODUCTNAME}` のみ（CLI バイナリ内のテンプレート実測）。この define の入力は `product_name` であり、ビルドプロファイルは入力に入らない。**ゆえに debug/release で分岐する経路が存在しない**——release ビルドは不要 |
| ⚠️2 | 「0 人」の根拠として CLI 版一致を積んでいるが、結論は「機能が未リリース」の 1 点で閉じる | **採用** | 余分な根拠は腐る側になる。§3 を「荷重を持つ唯一の論拠」と「傍証」へ分けた |
| ⚠️3 | v0.18.3 より前のタグを走査していない | **採用（射程の宣言として）** | 論拠自体は健全（機能が存在しないタグは母集団の外）。§3 に走査していない事実を明記した |
| ⚠️4 | (a)1 と同一 | — | (a)1 で処理済み |

**採らなかった機序の説明**: なし（各所見の機序は上表のとおり自分で一次証拠に当てて裁定した）。

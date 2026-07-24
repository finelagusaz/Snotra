# SU7: 配布・検証方針（#580 消化計画）設計

日付: 2026-07-24 / 対象: #532 Phase 2 SU7 のうち配布検証（flip 基準 4 の「#580 核心」）/ 先行: SU6.5（PR #655・ゲート G1〜G4 全合格・retrospective は PR #656）
ロードマップ: `2026-07-21-phase2-softbuffer-migration-roadmap.md` の SU7 行
スコープ確定: brainstorm で「署名鍵の CI 方針」を出発点に検討し、**鍵管理は現状維持・残る設計対象は検証手順と環境**と確定。e2e 後継方針と flip 実装は本 spec の対象外（別設計）。

## 背景と言語化

brainstorm の裏取りで、#580 の前提が 3 点更新された。

- **署名鍵は既に CI に在る**。`release.yml` が `secrets.TAURI_SIGNING_PRIVATE_KEY`（+ password）で署名付き NSIS と `latest.json` を生成し、`.sig` 存在検証・起動スモークを経て draft release へ上げる経路が現役（v0.18.3・2026-07-20 まで実績）。#580 の「鍵がローカルに無い」は正しいが、それは是正すべき欠落ではなく維持すべき状態である。
- **旧版の updater は publish 済みの Latest しか見ない**。endpoint は `releases/latest/download/latest.json` 固定（`src-tauri/tauri.conf.json`）で、draft / prerelease はこの URL に現れない。publish 前に「旧版 → 新版」の実更新を試すには endpoint を差し替えた旧版を別途ビルドする仕掛けが要る。
- **実行環境は Windows 11 Home**。Windows Sandbox / Hyper-V は使えず、素の環境検証にはサードパーティ VM の導入が要る。
- 補足: #580 コメント（2026-07-21）の「portable ZIP が生成されない」gap は `bundle.targets` 視点の指摘。`release.yml` は `Compress-Archive` で ZIP（snotra.exe + snotra-settings.exe）を手動生成しており、項目 3 の生成機構は既存。残るは成果物の動作確認のみ。

## 決定事項

### 決定 1: 鍵管理は現状維持。検証用の新機構は作らない

GitHub Actions secret + draft ゲート（人手検証後に手動 publish）を維持し、鍵をローカルへ持ち出さない。endpoint override 機構・検証専用チャネル・prerelease 経由の事前実更新検証は**作らない**——一回限りの仕掛けのコストに対し、既存機構＋下記の梯子で全項目が埋まるため。

### 決定 2: 項目 1（旧版 → egui 版の実更新）は publish 後に実機で検証する

draft の NSIS を install 系検証で先に潰してから publish し、実機の旧版から実更新する。v0.18.3 への実更新（SU6.5 スモーク）で同じ列の実績がある。publish 後に壊れていた場合の回復は次 patch ＋ WebView2 フォールバックフラグ（決定 4）。

### 決定 3: install 系検証（項目 2・3・6）は実機のみで回す。VM は見送る

config / history をバックアップした上で、上書き install → uninstall → 新規 install の列で 3 項目を代替する。実機には残留 AppData がありうるため「完全に素の環境」ではない——この純度低下は受容する（→「リスクと受容」）。素の環境整備が将来必要になれば別 issue とする。

### 決定 4: flip と WebView2 撤去は 1 リリース分離する

v0.19.0 で flip（既定 egui。WebView2 経路はフォールバック用フラグで温存）、実運用 soak を経て次版で撤去。分離の対価（二経路並行維持の 1 リリース延長）より、次の 2 つを取る:

- **ロールバック手段**: 既定 egui で重大不具合が出ても、フラグで WebView2 へ退避できる。
- **egui 側 updater の実弾検証**: 項目 1 は「旧版 **から** egui 版へ」しか検証しない。「egui 版 **から** 次版へ」——egui 経路の check → toast → Installing 表示（両ボタン disabled・#580 コメント 2026-07-23 の追記）→ `on_before_exit` 保存 → installer passive 起動 → 再起動——は、v0.19.0 → 次版の更新で初めて実弾になる。撤去リリースがこの検証機会をちょうど供給する。

## リリース梯子（実施手順）

### 一段目: v0.19.0（flip・WebView2 温存）

1. `create-release.yml`（workflow_dispatch）で draft build。CI が署名・`latest.json` 生成・`.sig` 検証・起動スモークまで自動実行する。
2. **draft 検証（実機・publish 前）**: config / history をバックアップ → draft の NSIS で上書き install → 動作スモーク → uninstall → 新規 install → 動作スモーク → portable ZIP の展開・起動確認。ここで項目 2・3・6 が埋まる。
3. v0.18.3（公開済み NSIS）を入れ直してから publish（Latest 化）→ 旧版の updater から**実更新** → egui 既定で起動・動作確認。ここで項目 1 が埋まる。

### 二段目: 次版（WebView2 撤去）

4. v0.19.0 の egui 経路 updater で実更新: check → toast → [今すぐ更新] → Installing 表示（両ボタン disabled を目視）→ `on_before_exit` 保存（history flush・icon cache・設定サイドカー終了）→ installer passive → 再起動。ここで項目 4 が実弾になり、完走で #580 を close する。

### #580 チェックリストとの対応

| #580 項目 | 埋まる場所 |
|---|---|
| 1. 旧署名版 → softbuffer 版の実更新 | 一段目 手順 3 |
| 2. 署名付き NSIS 生成 | 一段目 手順 1（CI）+ 手順 2（実 install で接地） |
| 3. portable ZIP | 一段目 手順 2（既存 `Compress-Archive` 成果物の動作確認） |
| 4. `on_before_exit` 保存・再起動 | コード検証済（#580 コメント 2026-07-21）→ 二段目 手順 4 で実弾 |
| 5. 3 更新モード維持 | 検証済（同コメント + SU6.5 スモーク）。一段目で full モードが再接地 |
| 6. 新規 / 上書き / uninstall | 一段目 手順 2 |
| 追記: Installing 目視列（#647） | 二段目 手順 4 |

## スコープ外

- **e2e 後継方針**（WebView2 撤去による `e2e/` 基盤喪失・#567 との順序整理・egui 自動回帰 smoke の定式化）——SU7 受け入れ条件だが独立のトピックとして別途設計する。
- **flip 実装そのもの**（既定反転の方式・フォールバックフラグの形・撤去の範囲）——別 spec。
- **VM / 素の環境の整備**——見送り。必要が生じたら別 issue。

## リスクと受容

- **実機は素の環境ではない**: uninstall 直後でも AppData 等の残留がありうるため、新規 install 検証の純度は VM に劣る。受容する（uninstall → install の列で代替。決定 3）。
- **publish 後検証の窓**: 実更新が壊れていた場合、次 patch を出すまで Latest が壊れた状態で晒される。draft 検証で install 系を先に潰すため、残余は updater ハンドオフのみ——この経路は v0.18.3 実更新で近縁の実績があり、回復手段（次 patch + フォールバックフラグ）も持つ。受容する（決定 2）。
- **soak 期間の二経路並行維持**: v0.19.0 と撤去リリースの間、WebView2 経路の保守が 1 リリース分残る。フラグ境界はウィンドウ生成時に限定済み（ロードマップ「リスク」節）で薄い。受容する（決定 4）。

import { createSignal } from "solid-js";
import type { LaunchResult } from "../lib/invoke";
import { createOwnedTimer } from "../lib/ownedTimer";
import { t } from "../lib/i18n";

/** 起動失敗・ホットキー失敗などの一時通知（search-overlay に表示）。
 *  自動クリアタイマーは本モジュールが一元管理する（唯一のタイマーハンドル）。 */
const [launchNotice, setLaunchNotice] = createSignal<string | null>(null);
/** delayMs は呼び出し元ごとに可変（2400/3000/5000）なため、arm() の msOverride で毎回指定する。
 *  createOwnedTimer(2400) の 2400 は override 省略時の既定値。 */
const launchNoticeTimer = createOwnedTimer(2400);

export function clearLaunchNotice() {
  launchNoticeTimer.cancel();
  if (launchNotice() !== null) {
    setLaunchNotice(null);
  }
}

export function setLaunchNoticeWithAutoClear(message: string, delayMs?: number) {
  clearLaunchNotice();
  setLaunchNotice(message);
  launchNoticeTimer.arm(() => {
    setLaunchNotice(null);
  }, delayMs);
}

export function setHotkeyFailureNotice(message: string) {
  setLaunchNoticeWithAutoClear(message, 5000);
}

/** LaunchResult の失敗/タイムアウトに応じた通知を表示する */
export function notifyLaunchFailure(result: LaunchResult) {
  const detail = result.message ? ` (${result.message})` : "";
  if (result.status === "timeout") {
    setLaunchNoticeWithAutoClear(t("notice.launch.timeout", { detail }));
  } else {
    setLaunchNoticeWithAutoClear(t("notice.launch.failed", { detail }));
  }
}

export { launchNotice };

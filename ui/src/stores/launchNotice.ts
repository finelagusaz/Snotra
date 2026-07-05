import { createSignal } from "solid-js";
import type { LaunchResult } from "../lib/invoke";
import { t } from "../lib/i18n";

/** 起動失敗・ホットキー失敗などの一時通知（search-overlay に表示）。
 *  自動クリアタイマーは本モジュールが一元管理する（唯一のタイマーハンドル）。 */
const [launchNotice, setLaunchNotice] = createSignal<string | null>(null);
let launchNoticeTimer: ReturnType<typeof setTimeout> | undefined;

export function clearLaunchNotice() {
  if (launchNoticeTimer !== undefined) {
    clearTimeout(launchNoticeTimer);
    launchNoticeTimer = undefined;
  }
  if (launchNotice() !== null) {
    setLaunchNotice(null);
  }
}

export function setLaunchNoticeWithAutoClear(message: string, delayMs = 2400) {
  clearLaunchNotice();
  setLaunchNotice(message);
  launchNoticeTimer = setTimeout(() => {
    launchNoticeTimer = undefined;
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

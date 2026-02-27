import type { Window } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import * as api from "./invoke";
import { trace } from "./trace";
import type { ResultsSyncPayload } from "./searchEvents";

const RESULTS_GAP = 4;
const RESULT_ROW_HEIGHT = 30;
const RESULTS_PADDING = 8;

export interface ResultsWindowController {
  getResultsWindow(): Promise<WebviewWindow | null>;
  handleResultsSync(payload: ResultsSyncPayload): Promise<void>;
  handleMainMoved(logicalPos: { x: number; y: number }): Promise<void>;
  handleMainResized(logicalHeight: number): void;
  getCachedScaleFactor(): number;
}

/**
 * Encapsulates all results-window positioning, sizing, show/hide logic.
 * Created once per main window lifetime in App.tsx.
 */
export function createResultsWindowController(
  mainWindow: Window,
): ResultsWindowController {
  let resultsWindowPromise: Promise<WebviewWindow | null> | undefined;
  let lastResultsSize: { width: number; height: number } | undefined;
  let lastResultsPosition: { x: number; y: number } | undefined;
  let windowOpsGeneration = 0;
  let cachedScaleFactor = 1;
  let cachedMainLogicalHeight = 52;
  let pendingResultsPosition: { x: number; y: number } | undefined;
  let positionApplyInFlight = false;

  const getResultsWindow = async (): Promise<WebviewWindow | null> => {
    if (!resultsWindowPromise) {
      trace("app:results_window:ensure:start");
      try {
        const created = await api.ensureWindow("results");
        trace("app:results_window:ensure:ok", { created });
        resultsWindowPromise = WebviewWindow.getByLabel("results");
        trace("app:results_window:get_by_label", {
          exists: resultsWindowPromise !== null,
        });
      } catch (e) {
        trace("app:results_window:ensure:error", { error: String(e) });
        throw e;
      }
    }
    return resultsWindowPromise;
  };

  const queueResultsPosition = (
    rw: WebviewWindow,
    nextPosition: { x: number; y: number },
  ) => {
    pendingResultsPosition = nextPosition;
    if (positionApplyInFlight) {
      return;
    }
    positionApplyInFlight = true;
    void (async () => {
      while (pendingResultsPosition) {
        const target = pendingResultsPosition;
        pendingResultsPosition = undefined;
        await rw.setPosition(new LogicalPosition(target.x, target.y));
        lastResultsPosition = target;
      }
      positionApplyInFlight = false;
    })().catch((e) => {
      console.error("Failed to sync results window position:", e);
      positionApplyInFlight = false;
    });
  };

  const handleResultsSync = async (payload: ResultsSyncPayload): Promise<void> => {
    const { generation, results, shouldShow, reason } = payload;
    const count = results.length;
    trace("app:event:results_sync", {
      generation,
      count,
      shouldShow,
      reason,
      windowOpsGeneration,
    });
    if (generation < windowOpsGeneration) return;
    windowOpsGeneration = generation;
    const isStale = () => generation !== windowOpsGeneration;

    if (!shouldShow) {
      const rw = await WebviewWindow.getByLabel("results");
      if (!rw || isStale()) return;
      const visible = await rw.isVisible();
      if (isStale()) return;
      if (visible) {
        trace("app:results_window:hide", { reason, generation });
        await rw.hide();
      }
      return;
    }
    const rw = await getResultsWindow();
    if (!rw || isStale()) return;

    // Use current main window width (may have been updated via settings)
    const [currentSize, currentSf, mainPos, mainVisible] = await Promise.all([
      mainWindow.innerSize(),
      mainWindow.scaleFactor(),
      mainWindow.outerPosition(),
      mainWindow.isVisible(),
    ]);
    if (isStale()) return;

    const mainSize = currentSize;
    const sf = currentSf;
    const currentWidth = currentSize.toLogical(currentSf).width;
    cachedScaleFactor = currentSf;
    cachedMainLogicalHeight = currentSize.toLogical(currentSf).height;

    // Resize results window based on count
    const resultsHeight = Math.min(count * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2, 400);
    if (
      !lastResultsSize ||
      lastResultsSize.width !== currentWidth ||
      lastResultsSize.height !== resultsHeight
    ) {
      await rw.setSize(new LogicalSize(currentWidth, resultsHeight));
      if (isStale()) return;
      lastResultsSize = { width: currentWidth, height: resultsHeight };
    }

    // Position results below main
    const logicalMainPos = mainPos.toLogical(sf);
    const logicalH = mainSize.toLogical(sf).height;
    const nextPosition = {
      x: logicalMainPos.x,
      y: logicalMainPos.y + logicalH + RESULTS_GAP,
    };
    if (
      !lastResultsPosition ||
      lastResultsPosition.x !== nextPosition.x ||
      lastResultsPosition.y !== nextPosition.y
    ) {
      await rw.setPosition(new LogicalPosition(nextPosition.x, nextPosition.y));
      if (isStale()) return;
      lastResultsPosition = nextPosition;
    }

    if (!mainVisible) {
      const visible = await rw.isVisible();
      if (isStale()) return;
      if (visible) {
        await rw.hide();
      }
      return;
    }

    const visible = await rw.isVisible();
    if (isStale()) return;
    if (!visible) {
      trace("app:results_window:show", { generation, count, reason });
      await rw.show();
      if (isStale()) {
        await rw.hide();
        return;
      }
      void api.setWindowNoActivate();
    }
  };

  const handleMainMoved = async (logicalPos: { x: number; y: number }): Promise<void> => {
    const rw = await getResultsWindow();
    if (!rw) return;
    const nextPosition = {
      x: logicalPos.x,
      y: logicalPos.y + cachedMainLogicalHeight + RESULTS_GAP,
    };
    if (
      lastResultsPosition &&
      lastResultsPosition.x === nextPosition.x &&
      lastResultsPosition.y === nextPosition.y
    ) {
      return;
    }
    queueResultsPosition(rw, nextPosition);
  };

  const handleMainResized = (logicalHeight: number): void => {
    cachedMainLogicalHeight = logicalHeight;
  };

  const getCachedScaleFactor = (): number => cachedScaleFactor;

  // Initialize geometry cache asynchronously
  void (async () => {
    try {
      const [sf, size] = await Promise.all([
        mainWindow.scaleFactor(),
        mainWindow.innerSize(),
      ]);
      cachedScaleFactor = sf;
      cachedMainLogicalHeight = size.toLogical(sf).height;
    } catch (e) {
      console.warn("Failed to initialize main window geometry cache:", e);
    }
  })();

  return {
    getResultsWindow,
    handleResultsSync,
    handleMainMoved,
    handleMainResized,
    getCachedScaleFactor,
  };
}

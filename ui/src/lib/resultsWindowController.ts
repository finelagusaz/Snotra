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
  getCachedScaleFactor(): number;
  updateMainVisible(visible: boolean): void;
  updateMainPosition(logicalPos: { x: number; y: number }): void;
  updateMainSize(logicalWidth: number, logicalHeight: number): void;
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
  let cachedMainLogicalWidth = 600;
  let cachedMainLogicalPosition: { x: number; y: number } | undefined;
  let cachedMainVisible = false;
  let geometryInitialized = false;
  let pendingResultsPosition: { x: number; y: number } | undefined;
  let positionApplyInFlight = false;
  let cachedResultsVisible = false;

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
      if (cachedResultsVisible) {
        trace("app:results_window:hide", { reason, generation });
        await rw.hide();
        cachedResultsVisible = false;
      }
      return;
    }
    const rw = await getResultsWindow();
    if (!rw || isStale()) return;

    // Use cached geometry (updated by onMoved/onResized/onFocusChanged listeners)
    const currentWidth = cachedMainLogicalWidth;

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

    // Position results below main (use cached position)
    if (cachedMainLogicalPosition) {
      const nextPosition = {
        x: cachedMainLogicalPosition.x,
        y: cachedMainLogicalPosition.y + cachedMainLogicalHeight + RESULTS_GAP,
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
    }

    if (!cachedMainVisible) {
      if (cachedResultsVisible) {
        await rw.hide();
        cachedResultsVisible = false;
      }
      return;
    }

    if (!cachedResultsVisible) {
      trace("app:results_window:show", { generation, count, reason });
      await rw.show();
      if (isStale()) {
        await rw.hide();
        cachedResultsVisible = false;
        return;
      }
      cachedResultsVisible = true;
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

  const getCachedScaleFactor = (): number => cachedScaleFactor;

  const updateMainVisible = (visible: boolean): void => {
    cachedMainVisible = visible;
  };

  const updateMainPosition = (logicalPos: { x: number; y: number }): void => {
    cachedMainLogicalPosition = logicalPos;
  };

  const updateMainSize = (logicalWidth: number, logicalHeight: number): void => {
    cachedMainLogicalWidth = logicalWidth;
    cachedMainLogicalHeight = logicalHeight;
  };

  // Initialize geometry cache asynchronously
  void (async () => {
    try {
      const [sf, size, pos, visible] = await Promise.all([
        mainWindow.scaleFactor(),
        mainWindow.innerSize(),
        mainWindow.outerPosition(),
        mainWindow.isVisible(),
      ]);
      cachedScaleFactor = sf;
      const logical = size.toLogical(sf);
      cachedMainLogicalHeight = logical.height;
      cachedMainLogicalWidth = logical.width;
      const logicalPos = pos.toLogical(sf);
      cachedMainLogicalPosition = { x: logicalPos.x, y: logicalPos.y };
      cachedMainVisible = visible;
      geometryInitialized = true;
    } catch (e) {
      console.warn("Failed to initialize main window geometry cache:", e);
    }
  })();

  return {
    getResultsWindow,
    handleResultsSync,
    handleMainMoved,
    getCachedScaleFactor,
    updateMainVisible,
    updateMainPosition,
    updateMainSize,
  };
}

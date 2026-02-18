import { type Component, createSignal, onMount, Show } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import SearchWindow from "./components/SearchWindow";
import SettingsWindow from "./components/SettingsWindow";
import { resetForShow } from "./stores/search";
import { applyTheme } from "./lib/theme";
import * as api from "./lib/invoke";

const App: Component = () => {
  const [isSettings, setIsSettings] = createSignal(false);

  onMount(async () => {
    const win = getCurrentWindow();
    const label = win.label;
    setIsSettings(label === "settings");

    // Load config and apply theme
    const config = await api.getConfig();
    applyTheme(config.visual);

    if (label === "main") {
      // Restore search window position
      const placement = await api.getSearchPlacement();
      if (placement) {
        await win.setPosition({
          type: "Logical",
          x: placement.x,
          y: placement.y,
        });
      }

      // Reset search on window-shown
      listen("window-shown", () => {
        resetForShow();
      });

      // Auto-hide on focus lost
      if (config.general.auto_hide_on_focus_lost) {
        win.onFocusChanged(({ payload: focused }) => {
          if (!focused) {
            win.hide();
          }
        });
      }

      // Apply window width from config
      const currentSize = await win.innerSize();
      if (config.appearance.window_width > 0) {
        await win.setSize({
          type: "Logical",
          width: config.appearance.window_width,
          height: currentSize.toLogical(await win.scaleFactor()).height,
        });
      }

      // Save position on move (debounced)
      let moveTimer: ReturnType<typeof setTimeout> | undefined;
      win.onMoved(({ payload: pos }) => {
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          api.saveSearchPlacement(pos.x, pos.y);
        }, 500);
      });
    }

    if (label === "settings") {
      // Restore settings window position and size
      const [placement, size] = await api.getSettingsPlacement();
      if (size) {
        await win.setSize({
          type: "Logical",
          width: size.width,
          height: size.height,
        });
      }
      if (placement) {
        await win.setPosition({
          type: "Logical",
          x: placement.x,
          y: placement.y,
        });
      }

      // Save position on move (debounced)
      let moveTimer: ReturnType<typeof setTimeout> | undefined;
      win.onMoved(({ payload: pos }) => {
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          api.saveSettingsPlacement(pos.x, pos.y);
        }, 500);
      });

      // Save size on resize (debounced)
      let resizeTimer: ReturnType<typeof setTimeout> | undefined;
      win.onResized(({ payload: sz }) => {
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(() => {
          api.saveSettingsSize(sz.width, sz.height);
        }, 500);
      });
    }
  });

  return (
    <Show when={!isSettings()} fallback={<SettingsWindow />}>
      <SearchWindow />
    </Show>
  );
};

export default App;

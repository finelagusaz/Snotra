import type { Component, JSX } from "solid-js";
import { Show } from "solid-js";

interface SettingRowProps {
  label: string;
  description?: string;
  block?: boolean;
  children: JSX.Element;
}

const SettingRow: Component<SettingRowProps> = (props) => {
  return (
    <div
      class="setting-row"
      classList={{ "setting-row--block": props.block }}
    >
      <div class="setting-info">
        <span class="setting-label">{props.label}</span>
        <Show when={props.description}>
          <span class="setting-description">{props.description}</span>
        </Show>
      </div>
      <div class="setting-control">{props.children}</div>
    </div>
  );
};

export default SettingRow;

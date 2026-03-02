import type { Component } from "solid-js";

interface ToggleSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  id?: string;
  "aria-label"?: string;
}

const ToggleSwitch: Component<ToggleSwitchProps> = (props) => {
  return (
    <label class="toggle-switch">
      <input
        id={props.id}
        type="checkbox"
        aria-label={props["aria-label"]}
        checked={props.checked}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
      />
      <span class="toggle-track" />
    </label>
  );
};

export default ToggleSwitch;

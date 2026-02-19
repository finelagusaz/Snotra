import type { Component } from "solid-js";

interface ToggleSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
}

const ToggleSwitch: Component<ToggleSwitchProps> = (props) => {
  return (
    <label class="toggle-switch">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
      />
      <span class="toggle-track" />
    </label>
  );
};

export default ToggleSwitch;

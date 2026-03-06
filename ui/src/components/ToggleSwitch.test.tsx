// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import ToggleSwitch from "./ToggleSwitch";

describe("ToggleSwitch", () => {
  it("checked=false のとき input が未チェック", () => {
    const { getByRole } = render(() => (
      <ToggleSwitch checked={false} onChange={() => {}} />
    ));
    const input = getByRole("checkbox") as HTMLInputElement;
    expect(input.checked).toBe(false);
  });

  it("checked=true のとき input がチェック済み", () => {
    const { getByRole } = render(() => (
      <ToggleSwitch checked={true} onChange={() => {}} />
    ));
    const input = getByRole("checkbox") as HTMLInputElement;
    expect(input.checked).toBe(true);
  });

  it("クリックで onChange が呼ばれる", async () => {
    const onChange = vi.fn();
    const { getByRole } = render(() => (
      <ToggleSwitch checked={false} onChange={onChange} />
    ));
    const input = getByRole("checkbox");
    fireEvent.click(input);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("id と aria-label が input に反映される", () => {
    const { getByRole } = render(() => (
      <ToggleSwitch
        checked={false}
        onChange={() => {}}
        id="test-toggle"
        aria-label="テスト切替"
      />
    ));
    const input = getByRole("checkbox") as HTMLInputElement;
    expect(input.id).toBe("test-toggle");
    expect(input.getAttribute("aria-label")).toBe("テスト切替");
  });

  it("toggle-track span が存在する", () => {
    const { container } = render(() => (
      <ToggleSwitch checked={false} onChange={() => {}} />
    ));
    expect(container.querySelector(".toggle-track")).not.toBeNull();
  });
});

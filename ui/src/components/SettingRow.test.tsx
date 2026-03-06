// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { render } from "@solidjs/testing-library";
import SettingRow from "./SettingRow";

describe("SettingRow", () => {
  it("label をテキストとして表示する", () => {
    const { getByText } = render(() => (
      <SettingRow label="テスト設定"><input /></SettingRow>
    ));
    expect(getByText("テスト設定")).toBeTruthy();
  });

  it("controlId がない場合 label は span で描画される", () => {
    const { getByText } = render(() => (
      <SettingRow label="スパン確認"><input /></SettingRow>
    ));
    const el = getByText("スパン確認");
    expect(el.tagName).toBe("SPAN");
  });

  it("controlId がある場合 label は label 要素で描画され for 属性が設定される", () => {
    const { getByText } = render(() => (
      <SettingRow label="ラベル確認" controlId="ctrl-1"><input /></SettingRow>
    ));
    const el = getByText("ラベル確認");
    expect(el.tagName).toBe("LABEL");
    expect(el.getAttribute("for")).toBe("ctrl-1");
  });

  it("description がある場合に表示される", () => {
    const { getByText } = render(() => (
      <SettingRow label="設定" description="補足説明"><input /></SettingRow>
    ));
    expect(getByText("補足説明")).toBeTruthy();
  });

  it("description がない場合は補足テキストが存在しない", () => {
    const { container } = render(() => (
      <SettingRow label="設定"><input /></SettingRow>
    ));
    expect(container.querySelector(".setting-description")).toBeNull();
  });

  it("block=true で setting-row--block クラスが付与される", () => {
    const { container } = render(() => (
      <SettingRow label="ブロック" block={true}><input /></SettingRow>
    ));
    const row = container.querySelector(".setting-row");
    expect(row?.classList.contains("setting-row--block")).toBe(true);
  });

  it("children がコントロール領域に描画される", () => {
    const { container } = render(() => (
      <SettingRow label="子要素">
        <button data-testid="child-btn">押す</button>
      </SettingRow>
    ));
    const control = container.querySelector(".setting-control");
    expect(control?.querySelector("button")).not.toBeNull();
  });
});

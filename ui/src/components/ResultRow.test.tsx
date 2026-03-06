// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import ResultRow from "./ResultRow";
import type { SearchResult } from "../lib/types";

vi.mock("../lib/truncatePath", () => ({
  truncatePath: (path: string) => path,
}));

function makeResult(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    name: "app.exe",
    path: "C:\\Program Files\\app.exe",
    isFolder: false,
    isError: false,
    ...overrides,
  };
}

describe("ResultRow", () => {
  it("パスをテキストとして表示する", () => {
    const { container } = render(() => (
      <ResultRow
        result={makeResult()}
        isSelected={false}
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={() => {}}
      />
    ));
    expect(container.textContent).toContain("C:\\Program Files\\app.exe");
  });

  it("フォルダの場合はパス末尾に \\ を付与する", () => {
    const { container } = render(() => (
      <ResultRow
        result={makeResult({ path: "C:\\Users", isFolder: true })}
        isSelected={false}
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={() => {}}
      />
    ));
    expect(container.textContent).toContain("C:\\Users\\");
  });

  it("isSelected=true で selected クラスと aria-selected=true が設定される", () => {
    const { container } = render(() => (
      <ResultRow
        result={makeResult()}
        isSelected={true}
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={() => {}}
      />
    ));
    const row = container.querySelector(".result-row")!;
    expect(row.classList.contains("selected")).toBe(true);
    expect(row.getAttribute("aria-selected")).toBe("true");
  });

  it("isError=true で error クラスが設定される", () => {
    const { container } = render(() => (
      <ResultRow
        result={makeResult({ isError: true })}
        isSelected={false}
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={() => {}}
      />
    ));
    const row = container.querySelector(".result-row")!;
    expect(row.classList.contains("error")).toBe(true);
  });

  it("onClick がクリックで呼ばれる", () => {
    const onClick = vi.fn();
    const { container } = render(() => (
      <ResultRow
        result={makeResult()}
        isSelected={false}
        font="12px sans-serif"
        onClick={onClick}
        onDoubleClick={() => {}}
      />
    ));
    fireEvent.click(container.querySelector(".result-row")!);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("onDoubleClick がダブルクリックで呼ばれる", () => {
    const onDoubleClick = vi.fn();
    const { container } = render(() => (
      <ResultRow
        result={makeResult()}
        isSelected={false}
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={onDoubleClick}
      />
    ));
    fireEvent.dblClick(container.querySelector(".result-row")!);
    expect(onDoubleClick).toHaveBeenCalledTimes(1);
  });

  it("icon が指定された場合 img 要素が描画される", () => {
    const { container } = render(() => (
      <ResultRow
        result={makeResult()}
        isSelected={false}
        icon="data:image/png;base64,abc"
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={() => {}}
      />
    ));
    const img = container.querySelector("img");
    expect(img).not.toBeNull();
    expect(img!.getAttribute("src")).toBe("data:image/png;base64,abc");
  });

  it("icon が未指定の場合フォールバック絵文字が表示される", () => {
    const { container } = render(() => (
      <ResultRow
        result={makeResult()}
        isSelected={false}
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={() => {}}
      />
    ));
    const fallback = container.querySelector(".icon-fallback");
    expect(fallback).not.toBeNull();
  });

  it("showIcons=false でアイコン領域が非表示", () => {
    const { container } = render(() => (
      <ResultRow
        result={makeResult()}
        isSelected={false}
        showIcons={false}
        font="12px sans-serif"
        onClick={() => {}}
        onDoubleClick={() => {}}
      />
    ));
    expect(container.querySelector(".result-icon")).toBeNull();
  });
});

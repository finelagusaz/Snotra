import { describe, expect, it } from "vitest";
import {
  buildGroupedOpeners,
  cloneGroupedOpenerEntry,
  isSameGroupedOpener,
  mergeGroupedOpenerEntries,
  serializeGroupedOpeners,
} from "./openerGroups";
import type { GroupedOpenerEntry } from "./openerGroups";
import type { OpenerRule } from "./types";

function makeExtEntry(
  extensions: string[],
  name: string,
  exe: string,
  args = "",
): GroupedOpenerEntry {
  return {
    targetKind: "ext",
    extensions,
    tool: { name, exe, args },
  };
}

function makeFolderEntry(name: string, exe: string, args = ""): GroupedOpenerEntry {
  return {
    targetKind: "folder",
    extensions: [],
    tool: { name, exe, args },
  };
}

describe("opener grouping", () => {
  it("groups ext rules by exe path and args", () => {
    const openers: OpenerRule[] = [
      {
        target: "ext:.png",
        tools: [{ name: "Viewer", exe: "C:/Tools/viewer.exe", args: "" }],
      },
      {
        target: "ext:.jpg",
        tools: [{ name: "Viewer", exe: "c:\\tools\\VIEWER.exe", args: "" }],
      },
      {
        target: "ext:.jpg",
        tools: [{ name: "Viewer Alt", exe: "C:\\Tools\\viewer.exe", args: "/alt" }],
      },
    ];

    const grouped = buildGroupedOpeners(openers);

    expect(grouped).toHaveLength(2);
    expect(grouped[0].extensions).toEqual([".jpg", ".png"]);
    expect(grouped[1].extensions).toEqual([".jpg"]);
  });

  it("keeps folder and ext entries separate even with the same tool", () => {
    const openers: OpenerRule[] = [
      {
        target: "folder",
        tools: [{ name: "Explorer", exe: "explorer.exe", args: "" }],
      },
      {
        target: "ext:.zip",
        tools: [{ name: "Explorer", exe: "explorer.exe", args: "" }],
      },
    ];

    const grouped = buildGroupedOpeners(openers);

    expect(grouped).toHaveLength(2);
    expect(grouped[0].targetKind).toBe("folder");
    expect(grouped[1].targetKind).toBe("ext");
  });

  it("serializes grouped entries into ext rules with matching tool lists", () => {
    const rules = serializeGroupedOpeners([
      makeExtEntry([".jpg"], "Viewer B", "b.exe"),
      makeExtEntry([".jpg", ".png"], "Viewer A", "a.exe"),
      makeFolderEntry("Explorer", "explorer.exe"),
    ]);

    expect(rules).toEqual<OpenerRule[]>([
      {
        target: "ext:.jpg",
        tools: [
          { name: "Viewer B", exe: "b.exe", args: "" },
          { name: "Viewer A", exe: "a.exe", args: "" },
        ],
      },
      {
        target: "ext:.png",
        tools: [{ name: "Viewer A", exe: "a.exe", args: "" }],
      },
      {
        target: "folder",
        tools: [{ name: "Explorer", exe: "explorer.exe", args: "" }],
      },
    ]);
  });

  it("round-trips grouped entries through raw rules", () => {
    const entries = [
      makeExtEntry([".jpg"], "Viewer B", "b.exe"),
      makeExtEntry([".jpg", ".png"], "Viewer A", "a.exe"),
      makeFolderEntry("Explorer", "explorer.exe"),
    ];

    const roundTrip = buildGroupedOpeners(serializeGroupedOpeners(entries));

    expect(roundTrip).toEqual(entries);
  });

  it("merges ext entries with the same tool identity", () => {
    const left = makeExtEntry([".png"], "Viewer", "a.exe");
    const right = makeExtEntry([".jpg"], "Viewer Updated", "A.exe");

    expect(isSameGroupedOpener(left, right)).toBe(true);
    expect(mergeGroupedOpenerEntries(left, right)).toEqual(
      makeExtEntry([".jpg", ".png"], "Viewer Updated", "A.exe"),
    );
  });

  it("keeps the existing folder name when merging the same tool", () => {
    const left = makeFolderEntry("First Name", "explorer.exe");
    const right = makeFolderEntry("Second Name", "explorer.exe");

    expect(isSameGroupedOpener(left, right)).toBe(true);
    expect(mergeGroupedOpenerEntries(left, right)).toEqual(
      makeFolderEntry("First Name", "explorer.exe"),
    );
  });

  it("clones grouped entries without sharing references", () => {
    const original = makeExtEntry([".png"], "Viewer", "a.exe");
    const clone = cloneGroupedOpenerEntry(original);

    clone.extensions.push(".jpg");
    clone.tool.name = "Changed";

    expect(original.extensions).toEqual([".png"]);
    expect(original.tool.name).toBe("Viewer");
  });
});

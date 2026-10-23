import { describe, expect, it } from "vitest";
import { buildTree, nodeState, pieceCells, selectedSize, toggle, type FileNode } from "./fileTree";

const FILES = [
  { idx: 0, path: "Season 1/e01.mkv", size: 100 },
  { idx: 1, path: "Season 1/e02.mkv", size: 200 },
  { idx: 3, path: "Season 1/extras/making.mkv", size: 50 },
  { idx: 4, path: "readme.txt", size: 5 },
];

function find(nodes: FileNode[], path: string): FileNode {
  for (const n of nodes) {
    if (n.path === path) return n;
    const hit = n.children.length ? findOrNull(n.children, path) : null;
    if (hit) return hit;
  }
  throw new Error(`no ${path}`);
}

function findOrNull(nodes: FileNode[], path: string): FileNode | null {
  try {
    return find(nodes, path);
  } catch {
    return null;
  }
}

describe("file tree", () => {
  it("builds_nested_folders", () => {
    const tree = buildTree(FILES);
    // Folders first, then files, each sorted by name.
    expect(tree.map((n) => n.name)).toEqual(["Season 1", "readme.txt"]);
    const season = find(tree, "Season 1");
    expect(season.idx).toBeUndefined();
    expect(season.size).toBe(350);
    expect(season.children.map((n) => n.name)).toEqual(["extras", "e01.mkv", "e02.mkv"]);
    expect(find(tree, "Season 1/extras/making.mkv").idx).toBe(3);
  });

  it("folder_toggle_selects_children", () => {
    const tree = buildTree(FILES);
    const season = find(tree, "Season 1");
    const off = toggle(new Set([0, 1, 3, 4]), season, false);
    expect([...off].sort()).toEqual([4]);
    const on = toggle(off, season, true);
    expect([...on].sort()).toEqual([0, 1, 3, 4]);
    // A file toggles itself only.
    expect([...toggle(on, find(tree, "readme.txt"), false)].sort()).toEqual([0, 1, 3]);
  });

  it("mixed_state", () => {
    const tree = buildTree(FILES);
    const season = find(tree, "Season 1");
    expect(nodeState(new Set([0, 1, 3]), season)).toBe("on");
    expect(nodeState(new Set([4]), season)).toBe("off");
    expect(nodeState(new Set([0]), season)).toBe("mixed");
    expect(nodeState(new Set([4]), find(tree, "readme.txt"))).toBe("on");
  });

  it("selected_size_sums", () => {
    expect(selectedSize(FILES, new Set([0, 4]))).toBe(105);
    expect(selectedSize(FILES, new Set())).toBe(0);
  });

  it("single_file_torrent", () => {
    const tree = buildTree([{ idx: 0, path: "movie.mkv", size: 7 }]);
    expect(tree).toHaveLength(1);
    expect(tree[0].idx).toBe(0);
  });
});

describe("piece map", () => {
  it("groups_pieces_into_cells", () => {
    // 10 pieces: 1111 0000 11 -> "f0c0"
    const cells = pieceCells("f0c0", 10, 5);
    expect(cells).toEqual([1, 1, 0, 0, 1]);
  });

  it("partial_cells_are_fractions", () => {
    // 4 pieces, 1010, into 2 cells -> half and half.
    expect(pieceCells("a0", 4, 2)).toEqual([0.5, 0.5]);
  });

  it("fewer_pieces_than_cells", () => {
    expect(pieceCells("80", 2, 10)).toEqual([1, 0]);
  });

  it("bad_input_is_empty", () => {
    expect(pieceCells("", 0, 10)).toEqual([]);
    expect(pieceCells("zz", 8, 4)).toEqual([0, 0, 0, 0]);
  });
});

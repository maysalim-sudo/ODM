/** A torrent's file list as folders and files, and its piece map in cells. */

export interface FileNode {
  name: string;
  /** "/"-joined path from the torrent's root. */
  path: string;
  /** A file's own size, or the total of a folder's files. */
  size: number;
  /** The file's index; absent on folders. */
  idx?: number;
  children: FileNode[];
}

interface FileLike {
  idx: number;
  path: string;
  size: number;
}

function sortNodes(nodes: FileNode[]): void {
  nodes.sort((a, b) => {
    const fa = a.idx === undefined ? 0 : 1;
    const fb = b.idx === undefined ? 0 : 1;
    return fa - fb || a.name.localeCompare(b.name, undefined, { numeric: true });
  });
  for (const n of nodes) sortNodes(n.children);
}

/** Folders first, then files, each sorted by name. */
export function buildTree(files: FileLike[]): FileNode[] {
  const root: FileNode[] = [];
  for (const f of files) {
    const parts = f.path.split("/").filter((p) => p.length > 0);
    let level = root;
    let path = "";
    parts.forEach((name, i) => {
      path = path ? `${path}/${name}` : name;
      const last = i === parts.length - 1;
      let node = level.find((n) => n.name === name && (n.idx === undefined) === !last);
      if (!node) {
        node = { name, path, size: 0, children: [], ...(last ? { idx: f.idx } : {}) };
        level.push(node);
      }
      node.size += f.size;
      level = node.children;
    });
  }
  sortNodes(root);
  return root;
}

function fileIndices(node: FileNode): number[] {
  return node.idx !== undefined ? [node.idx] : node.children.flatMap(fileIndices);
}

/** The selection with `node` (a file, or every file in a folder) turned on or off. */
export function toggle(sel: Set<number>, node: FileNode, on: boolean): Set<number> {
  const next = new Set(sel);
  for (const i of fileIndices(node)) {
    if (on) next.add(i);
    else next.delete(i);
  }
  return next;
}

export function nodeState(sel: Set<number>, node: FileNode): "on" | "off" | "mixed" {
  const ids = fileIndices(node);
  const n = ids.filter((i) => sel.has(i)).length;
  return n === 0 ? "off" : n === ids.length ? "on" : "mixed";
}

export function selectedSize(files: { idx: number; size: number }[], sel: Set<number>): number {
  return files.reduce((sum, f) => (sel.has(f.idx) ? sum + f.size : sum), 0);
}

/**
 * The piece map in at most `cells` cells, each the share (0–1) of its pieces on disk.
 * `hex` is the bitfield, most significant bit first.
 */
export function pieceCells(hex: string, count: number, cells: number): number[] {
  if (count <= 0 || cells <= 0) return [];
  const has = (i: number): boolean => {
    const byte = parseInt(hex.slice((i >> 3) * 2, (i >> 3) * 2 + 2), 16);
    return Number.isFinite(byte) && ((byte >> (7 - (i & 7))) & 1) === 1;
  };
  const n = Math.min(cells, count);
  const out: number[] = [];
  for (let c = 0; c < n; c++) {
    const from = Math.floor((c * count) / n);
    const to = Math.floor(((c + 1) * count) / n);
    let got = 0;
    for (let i = from; i < to; i++) if (has(i)) got++;
    out.push(to > from ? got / (to - from) : 0);
  }
  return out;
}

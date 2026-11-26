// treemap.ts — squarified treemap 布局（WinDirStat / Bruls-Huizing-van Wijk 范式）
// 只做纯布局计算：输入带权重的项，输出矩形坐标。面积与权重严格成正比。

export interface TreemapInput {
  key: string;
  weight: number;
}

export interface TreemapRect {
  key: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * squarified treemap：按权重降序铺满给定矩形，尽量让每块接近正方形（可读性最好）。
 * 复杂度 O(n log n)，n ≤ 数百时毫秒级。
 */
export function squarify(items: TreemapInput[], width: number, height: number): TreemapRect[] {
  const total = items.reduce((a, b) => a + Math.max(b.weight, 0), 0);
  if (total <= 0 || width <= 0 || height <= 0) return [];

  // 归一化到面积
  const scale = (width * height) / total;
  const sorted = items
    .filter((i) => i.weight > 0)
    .map((i) => ({ key: i.key, area: i.weight * scale }))
    .sort((a, b) => b.area - a.area);

  const out: TreemapRect[] = [];
  let x = 0, y = 0, w = width, h = height;

  const worst = (row: { area: number }[], side: number): number => {
    if (row.length === 0) return Infinity;
    const sum = row.reduce((a, b) => a + b.area, 0);
    if (sum <= 0) return Infinity;
    const max = Math.max(...row.map((r) => r.area));
    const min = Math.min(...row.map((r) => r.area));
    const s2 = side * side;
    const sum2 = sum * sum;
    return Math.max((s2 * max) / sum2, sum2 / (s2 * min));
  };

  let i = 0;
  while (i < sorted.length) {
    const side = Math.min(w, h);
    const row: { key: string; area: number }[] = [];
    let j = i;
    // 贪心塞到「再加一个会更差」为止
    while (j < sorted.length) {
      const next = [...row, sorted[j]];
      if (row.length > 0 && worst(next, side) > worst(row, side)) break;
      row.push(sorted[j]);
      j++;
    }
    // 摆放这一行
    const rowArea = row.reduce((a, b) => a + b.area, 0);
    const horizontal = w >= h;
    const rowThickness = rowArea / side;
    let offset = 0;
    for (const r of row) {
      const len = r.area / rowThickness;
      if (horizontal) {
        out.push({ key: r.key, x, y: y + offset, w: rowThickness, h: len });
      } else {
        out.push({ key: r.key, x: x + offset, y, w: len, h: rowThickness });
      }
      offset += len;
    }
    if (horizontal) {
      x += rowThickness;
      w -= rowThickness;
    } else {
      y += rowThickness;
      h -= rowThickness;
    }
    i = j;
  }
  return out;
}

/** 扩展名 → 颜色（20 类 + 兜底）。色相分散，深浅统一，保证小方块也能分辨。 */
const EXT_COLORS: [string, string[]][] = [
  ["#4f9cf9", ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "tiff", "heic"]],
  ["#f97362", ["pdf", "doc", "docx", "odt", "rtf", "pages", "epub"]],
  ["#56c596", ["xls", "xlsx", "csv", "ods", "numbers"]],
  ["#f5a524", ["ppt", "pptx", "key", "odp"]],
  ["#a78bfa", ["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts"]],
  ["#ec4899", ["mp3", "wav", "flac", "aac", "ogg", "m4a", "opus", "wma"]],
  ["#22d3ee", ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst"]],
  ["#94a3b8", ["exe", "msi", "bat", "cmd", "sh", "ps1"]],
  ["#34d399", ["rs", "py", "js", "ts", "tsx", "jsx", "c", "cpp", "h", "hpp", "java", "go", "cs", "rb", "php", "lua", "vue"]],
  ["#fbbf24", ["json", "yaml", "yml", "toml", "xml", "ini", "cfg", "conf"]],
  ["#c084fc", ["ttf", "otf", "woff", "woff2"]],
  ["#fb7185", ["psd", "ai", "sketch", "fig", "xd", "blend"]],
  ["#2dd4bf", ["dwg", "dxf", "step", "stl", "obj"]],
  ["#facc15", ["txt", "md", "log"]],
  ["#60a5fa", ["html", "htm", "css", "scss", "less"]],
  ["#f472b6", ["ipynb", "r", "m", "mat", "sql"]],
];

const FALLBACK_COLORS = ["#64748b", "#7c8cf8", "#8b5cf6", "#0ea5e9", "#14b8a6"];

export function extColor(ext: string): string {
  const e = ext.toLowerCase().replace(/^\./, "");
  for (const [color, list] of EXT_COLORS) {
    if (list.includes(e)) return color;
  }
  let h = 0;
  for (let i = 0; i < e.length; i++) h = (h * 31 + e.charCodeAt(i)) >>> 0;
  return FALLBACK_COLORS[h % FALLBACK_COLORS.length];
}

import { invoke } from "@tauri-apps/api/core";
import { save, type SaveDialogOptions, type DialogFilter } from "@tauri-apps/plugin-dialog";

export type Tool = "pen" | "highlighter" | "eraser";
export type Rect = { x: number; y: number; w: number; h: number };
export type Op =
  | { type: "rotate"; deg: number }
  | { type: "flipH" }
  | ({ type: "crop" } & Rect)
  | { type: "stroke"; tool: Tool; color: string; size: number; points: [number, number][] };

/** Photo layer + marks layer (eraser only affects marks). */
export type Layers = { base: HTMLCanvasElement; marks: HTMLCanvasElement };

export const SAVE_EXTS = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "dib", "ico", "cur", "pdf"];
export const OPEN_EXTS = [
  "png", "apng", "jpg", "jpeg", "jpe", "jfif", "webp", "gif", "bmp", "dib", "rle", "ico", "cur",
  "svg", "avif", "heic", "heif", "hif", "jxl", "tif", "tiff", "jxr", "wdp", "hdp", "dds", "tga",
  "raw", "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "pef",
  "raf", "srw", "erf", "kdc", "dcr", "mef", "mrw", "mos", "3fr", "fff", "iiq", "x3f", "rwl",
];

/** Daftar pilihan format untuk Save As di Windows Explorer. */
export const SAVE_FORMAT_FILTERS: DialogFilter[] = [
  { name: "PNG Image (*.png)", extensions: ["png"] },
  { name: "JPEG Image (*.jpg;*.jpeg)", extensions: ["jpg", "jpeg"] },
  { name: "WebP Image (*.webp)", extensions: ["webp"] },
  { name: "GIF Image (*.gif)", extensions: ["gif"] },
  { name: "Bitmap Image (*.bmp;*.dib)", extensions: ["bmp", "dib"] },
  { name: "Icon (*.ico)", extensions: ["ico"] },
  { name: "Cursor (*.cur)", extensions: ["cur"] },
  { name: "PDF Document (*.pdf)", extensions: ["pdf"] },
];

const WEB_MIMES: Record<string, string> = {
  png: "image/png",
  apng: "image/apng",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  jpe: "image/jpeg",
  jfif: "image/jpeg",
  webp: "image/webp",
  gif: "image/gif",
  bmp: "image/bmp",
  dib: "image/bmp",
  ico: "image/x-icon",
  cur: "image/x-icon",
  svg: "image/svg+xml",
  avif: "image/avif",
};

/**
 * Menyusun opsi Save As Dialog agar format asli file (misal .gif) menjadi
 * pilihan default pertama di Windows Explorer, diikuti format lainnya.
 */
export function getSaveDialogOptions(currentPath?: string): SaveDialogOptions {
  const rawExt = currentPath ? extOf(currentPath) : "png";
  // Map ekstensi alias ke ekstensi simpan yang didukung
  const normExt =
    rawExt === "jpe" || rawExt === "jfif"
      ? "jpg"
      : rawExt === "apng"
        ? "png"
        : rawExt === "rle"
          ? "bmp"
          : SAVE_EXTS.includes(rawExt)
            ? rawExt
            : "png";

  // Cari filter yang cocok dengan ekstensi file saat ini
  const matchedFilter = SAVE_FORMAT_FILTERS.find((f) => f.extensions.includes(normExt));
  const otherFilters = SAVE_FORMAT_FILTERS.filter((f) => f !== matchedFilter);

  // Pastikan ekstensi yang cocok berada di indeks 0 pada filter pertama
  const primaryFilter: DialogFilter = matchedFilter
    ? {
        name: matchedFilter.name,
        extensions: [normExt, ...matchedFilter.extensions.filter((e) => e !== normExt)],
      }
    : SAVE_FORMAT_FILTERS[0];

  let defaultPath = currentPath;
  if (defaultPath) {
    // Jika ekstensi asli tidak didukung untuk disimpan (misal .heic / .raw / .svg), ganti ke normExt (.png)
    if (!SAVE_EXTS.includes(rawExt)) {
      defaultPath = replaceExt(defaultPath, normExt);
    }
  } else {
    defaultPath = `untitled.${normExt}`;
  }

  return {
    defaultPath,
    filters: [primaryFilter, ...otherFilters],
  };
}

/** Membuka dialog Save As dengan format default sesuai file asli. */
export async function promptSaveAs(currentPath?: string): Promise<string | null> {
  return await save(getSaveDialogOptions(currentPath));
}

function replaceExt(path: string, newExt: string): string {
  const lastSlash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const lastDot = path.lastIndexOf(".");
  if (lastDot > lastSlash) {
    return `${path.slice(0, lastDot)}.${newExt}`;
  }
  return `${path}.${newExt}`;
}

function canvas(w: number, h: number) {
  const c = document.createElement("canvas");
  c.width = Math.max(1, Math.round(w));
  c.height = Math.max(1, Math.round(h));
  return c;
}

function rotate(src: HTMLCanvasElement, deg: number) {
  const norm = ((deg % 360) + 360) % 360;
  if (norm === 0) {
    const c = canvas(src.width, src.height);
    c.getContext("2d")!.drawImage(src, 0, 0);
    return c;
  }
  if (norm === 90) {
    const c = canvas(src.height, src.width);
    const ctx = c.getContext("2d")!;
    ctx.translate(src.height, 0);
    ctx.rotate(Math.PI / 2);
    ctx.drawImage(src, 0, 0);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    return c;
  }
  if (norm === 180) {
    const c = canvas(src.width, src.height);
    const ctx = c.getContext("2d")!;
    ctx.translate(src.width, src.height);
    ctx.rotate(Math.PI);
    ctx.drawImage(src, 0, 0);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    return c;
  }
  if (norm === 270) {
    const c = canvas(src.height, src.width);
    const ctx = c.getContext("2d")!;
    ctx.translate(0, src.width);
    ctx.rotate(-Math.PI / 2);
    ctx.drawImage(src, 0, 0);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    return c;
  }

  const r = (deg * Math.PI) / 180;
  const cos = Math.abs(Math.cos(r)),
    sin = Math.abs(Math.sin(r));
  const c = canvas(src.width * cos + src.height * sin, src.width * sin + src.height * cos);
  const ctx = c.getContext("2d")!;
  ctx.translate(c.width / 2, c.height / 2);
  ctx.rotate(r);
  ctx.drawImage(src, -src.width / 2, -src.height / 2);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  return c;
}

function flipH(src: HTMLCanvasElement) {
  const c = canvas(src.width, src.height);
  const ctx = c.getContext("2d")!;
  ctx.translate(src.width, 0);
  ctx.scale(-1, 1);
  ctx.drawImage(src, 0, 0);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  return c;
}

function crop(src: HTMLCanvasElement, r: Rect) {
  const x = r.w < 0 ? r.x + r.w : r.x;
  const y = r.h < 0 ? r.y + r.h : r.y;
  const w = Math.max(1, Math.abs(r.w));
  const h = Math.max(1, Math.abs(r.h));
  const c = canvas(w, h);
  c.getContext("2d")!.drawImage(src, -x, -y);
  return c;
}

export function drawStroke(ctx: CanvasRenderingContext2D, op: Extract<Op, { type: "stroke" }>) {
  if (op.points.length === 0) return;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalCompositeOperation = op.tool === "eraser" ? "destination-out" : "source-over";
  if (op.tool === "highlighter") {
    ctx.globalAlpha = 0.4;
    ctx.fillStyle = op.color;
    const w = op.size * 0.25,
      h = op.size;
    ctx.beginPath();
    for (let i = 0; i < op.points.length; i++) {
      const [x1, y1] = op.points[i];
      const [x0, y0] = op.points[i - 1] ?? [x1, y1];
      const steps = Math.max(1, Math.ceil(Math.hypot(x1 - x0, y1 - y0) / Math.max(0.5, w * 0.35)));
      for (let s = 0; s <= steps; s++) {
        const t = s / steps;
        ctx.rect(x0 + (x1 - x0) * t - w / 2, y0 + (y1 - y0) * t - h / 2, w, h);
      }
    }
    ctx.fill();
    ctx.restore();
    return;
  }
  ctx.globalAlpha = 1;
  ctx.strokeStyle = op.color;
  ctx.lineWidth = op.size;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  op.points.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  if (op.points.length === 1) ctx.lineTo(op.points[0][0] + 0.01, op.points[0][1]);
  ctx.stroke();
  ctx.restore();
}

export function applyOp(l: Layers, op: Op): Layers {
  if (op.type === "rotate") return { base: rotate(l.base, op.deg), marks: rotate(l.marks, op.deg) };
  if (op.type === "flipH") return { base: flipH(l.base), marks: flipH(l.marks) };
  if (op.type === "crop") return { base: crop(l.base, op), marks: crop(l.marks, op) };
  drawStroke(l.marks.getContext("2d")!, op);
  return l;
}

/** Replays all ops on top of the original image. */
export function render(img: ImageBitmap, ops: Op[]): Layers {
  let l = { base: canvas(img.width, img.height), marks: canvas(img.width, img.height) };
  l.base.getContext("2d")!.drawImage(img, 0, 0);
  for (const op of ops) l = applyOp(l, op);
  return l;
}

export function flatten(l: Layers, background?: string) {
  const c = canvas(l.base.width, l.base.height);
  const ctx = c.getContext("2d")!;
  if (background) {
    ctx.fillStyle = background;
    ctx.fillRect(0, 0, c.width, c.height);
  }
  ctx.drawImage(l.base, 0, 0);
  ctx.drawImage(l.marks, 0, 0);
  return c;
}

export function toBlob(c: HTMLCanvasElement, mime = "image/png") {
  return new Promise<Blob>((res, rej) =>
    c.toBlob((b) => (b ? res(b) : rej(new Error("encode failed"))), mime, 0.92)
  );
}

export function encodeBmp(c: HTMLCanvasElement): Uint8Array {
  const w = c.width,
    h = c.height;
  const src = c.getContext("2d")!.getImageData(0, 0, w, h).data;
  const rowLen = w * 4;
  const fileSize = 54 + rowLen * h;
  const buf = new Uint8Array(fileSize);
  const dv = new DataView(buf.buffer);

  buf[0] = 0x42;
  buf[1] = 0x4d; // "BM"
  dv.setUint32(2, fileSize, true);
  dv.setUint32(10, 54, true);

  dv.setUint32(14, 40, true);
  dv.setInt32(18, w, true);
  dv.setInt32(22, h, true);
  dv.setUint16(26, 1, true);
  dv.setUint16(28, 32, true);
  dv.setUint32(30, 0, true);
  dv.setUint32(34, rowLen * h, true);

  let dst = 54;
  for (let y = h - 1; y >= 0; y--) {
    let srcIdx = y * rowLen;
    for (let x = 0; x < w; x++) {
      buf[dst++] = src[srcIdx + 2];
      buf[dst++] = src[srcIdx + 1];
      buf[dst++] = src[srcIdx];
      buf[dst++] = src[srcIdx + 3];
      srcIdx += 4;
    }
  }
  return buf;
}

export async function encodeIco(srcCanvas: HTMLCanvasElement, isCursor = false): Promise<Uint8Array> {
  // Format ICO/CUR maksimal mendukung dimensi 256x256
  let target = srcCanvas;
  if (srcCanvas.width > 256 || srcCanvas.height > 256) {
    const scale = Math.min(256 / srcCanvas.width, 256 / srcCanvas.height);
    target = canvas(srcCanvas.width * scale, srcCanvas.height * scale);
    target.getContext("2d")!.drawImage(srcCanvas, 0, 0, target.width, target.height);
  }

  const pngBlob = await toBlob(target, "image/png");
  const pngBytes = new Uint8Array(await pngBlob.arrayBuffer());
  const buf = new Uint8Array(22 + pngBytes.length);
  const dv = new DataView(buf.buffer);

  dv.setUint16(2, isCursor ? 2 : 1, true); // 1 = ICO, 2 = CUR
  dv.setUint16(4, 1, true); // 1 image
  buf[6] = target.width >= 256 ? 0 : target.width;
  buf[7] = target.height >= 256 ? 0 : target.height;
  dv.setUint16(10, isCursor ? 0 : 1, true); // color planes (ICO) atau hotspot X (CUR)
  dv.setUint16(12, isCursor ? 0 : 32, true); // bits per pixel (ICO) atau hotspot Y (CUR)
  dv.setUint32(14, pngBytes.length, true);
  dv.setUint32(18, 22, true);

  buf.set(pngBytes, 22);
  return buf;
}

function quantizeAndLzwFrame(w: number, h: number, rgba: Uint8ClampedArray) {
  let hasAlpha = false;
  let transparentIndex = -1;
  for (let i = 3; i < rgba.length; i += 4) {
    if (rgba[i] < 128) {
      hasAlpha = true;
      transparentIndex = 0;
      break;
    }
  }

  const counts = new Uint32Array(32768);
  for (let i = 0; i < rgba.length; i += 4) {
    if (rgba[i + 3] >= 128) {
      const c15 = ((rgba[i] >> 3) << 10) | ((rgba[i + 1] >> 3) << 5) | (rgba[i + 2] >> 3);
      counts[c15]++;
    }
  }

  const sorted: number[] = [];
  for (let i = 0; i < 32768; i++) {
    if (counts[i] > 0) sorted.push(i);
  }
  sorted.sort((a, b) => counts[b] - counts[a]);

  const maxPal = hasAlpha ? 255 : 256;
  const palette = new Uint8Array(256 * 3);
  let palCount = hasAlpha ? 1 : 0;
  const palLimit = Math.min(sorted.length, maxPal);
  for (let i = 0; i < palLimit; i++) {
    const c15 = sorted[i];
    palette[palCount * 3] = ((c15 >> 10) & 31) << 3;
    palette[palCount * 3 + 1] = ((c15 >> 5) & 31) << 3;
    palette[palCount * 3 + 2] = (c15 & 31) << 3;
    palCount++;
  }

  const lut = new Uint8Array(32768);
  const searchStart = hasAlpha ? 1 : 0;
  const totalColors = palLimit + searchStart;
  for (let c15 = 0; c15 < 32768; c15++) {
    if (counts[c15] === 0 && sorted.length > maxPal) continue;
    const r = ((c15 >> 10) & 31) << 3;
    const g = ((c15 >> 5) & 31) << 3;
    const b = (c15 & 31) << 3;
    let minD = Infinity;
    let best = searchStart;
    for (let j = searchStart; j < totalColors; j++) {
      const dr = r - palette[j * 3];
      const dg = g - palette[j * 3 + 1];
      const db = b - palette[j * 3 + 2];
      const d = dr * dr + dg * dg + db * db;
      if (d < minD) {
        minD = d;
        best = j;
        if (d === 0) break;
      }
    }
    lut[c15] = best;
  }

  const indices = new Uint8Array(w * h);
  for (let i = 0, p = 0; i < rgba.length; i += 4, p++) {
    if (hasAlpha && rgba[i + 3] < 128) {
      indices[p] = transparentIndex;
    } else {
      const c15 = ((rgba[i] >> 3) << 10) | ((rgba[i + 1] >> 3) << 5) | (rgba[i + 2] >> 3);
      indices[p] = lut[c15];
    }
  }

  const minCodeSize = 8;
  const clearCode = 1 << minCodeSize;
  const eoiCode = clearCode + 1;

  let codeSize = minCodeSize + 1;
  let nextCode = eoiCode + 1;

  const lzwBits: number[] = [];
  let curBits = 0;
  let curVal = 0;

  function writeBits(code: number) {
    curVal |= code << curBits;
    curBits += codeSize;
    while (curBits >= 8) {
      lzwBits.push(curVal & 0xff);
      curVal >>= 8;
      curBits -= 8;
    }
  }

  writeBits(clearCode);
  const table = new Map<number, number>();
  let prefix = indices[0] ?? 0;

  for (let i = 1; i < indices.length; i++) {
    const k = indices[i];
    const key = (prefix << 8) | k;
    if (table.has(key)) {
      prefix = table.get(key)!;
    } else {
      writeBits(prefix);
      if (nextCode < 4096) {
        table.set(key, nextCode++);
        if (nextCode === (1 << codeSize) + 1 && codeSize < 12) {
          codeSize++;
        }
      } else {
        writeBits(clearCode);
        table.clear();
        codeSize = minCodeSize + 1;
        nextCode = eoiCode + 1;
      }
      prefix = k;
    }
  }
  writeBits(prefix);
  writeBits(eoiCode);
  if (curBits > 0) lzwBits.push(curVal & 0xff);

  return { hasAlpha, transparentIndex, palette, minCodeSize, lzwBits };
}

export function encodeGif(w: number, h: number, rgba: Uint8ClampedArray): Uint8Array {
  const { hasAlpha, transparentIndex, palette, minCodeSize, lzwBits } = quantizeAndLzwFrame(
    w,
    h,
    rgba
  );

  const out: number[] = [
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, // GIF89a
    w & 0xff, (w >> 8) & 0xff,
    h & 0xff, (h >> 8) & 0xff,
    0xf7, 0, 0,
  ];
  for (let i = 0; i < 768; i++) out.push(palette[i]);
  out.push(0x21, 0xf9, 0x04, hasAlpha ? 1 : 0, 0, 0, hasAlpha ? transparentIndex : 0, 0);
  out.push(0x2c, 0, 0, 0, 0, w & 0xff, (w >> 8) & 0xff, h & 0xff, (h >> 8) & 0xff, 0);
  out.push(minCodeSize);
  for (let i = 0; i < lzwBits.length; i += 255) {
    const chunk = lzwBits.slice(i, i + 255);
    out.push(chunk.length, ...chunk);
  }
  out.push(0x00, 0x3b);

  return new Uint8Array(out);
}

/** Encodes the edited image for the given extension (png/jpg/jpeg/webp/gif/bmp/ico/cur/pdf). */
export async function encode(l: Layers, ext: string): Promise<Uint8Array> {
  const norm = ext.toLowerCase();
  const opaque =
    norm === "jpg" || norm === "jpeg" || norm === "pdf" || norm === "bmp" || norm === "dib";
  const c = flatten(l, opaque ? "#ffffff" : undefined);
  if (norm === "pdf") {
    const { jsPDF } = await import("jspdf");
    const w = c.width * 0.75,
      h = c.height * 0.75; // px @96dpi -> pt
    const doc = new jsPDF({
      orientation: w > h ? "landscape" : "portrait",
      unit: "pt",
      format: [w, h],
    });
    doc.addImage(c, "JPEG", 0, 0, w, h);
    return new Uint8Array(doc.output("arraybuffer"));
  }
  if (norm === "bmp" || norm === "dib") {
    return encodeBmp(c);
  }
  if (norm === "ico" || norm === "cur") {
    return encodeIco(c, norm === "cur");
  }
  if (norm === "gif") {
    const ctx = c.getContext("2d")!;
    const imgData = ctx.getImageData(0, 0, c.width, c.height);
    return encodeGif(c.width, c.height, imgData.data);
  }
  const mime =
    norm === "png" ? "image/png" : norm === "webp" ? "image/webp" : "image/jpeg";
  return new Uint8Array(await (await toBlob(c, mime)).arrayBuffer());
}

export function extOf(path: string) {
  const fileName = path.split(/[/\\]/).pop() ?? "";
  const dot = fileName.lastIndexOf(".");
  return dot > 0 ? fileName.slice(dot + 1).toLowerCase() : "";
}

export function writeFile(path: string, data: Uint8Array) {
  return invoke("write_file", data, { headers: { path: encodeURIComponent(path) } });
}

export async function editAnimatedGif(
  data: Uint8Array,
  ops: Op[],
  marksCanvas?: HTMLCanvasElement
): Promise<Uint8Array> {
  // Jika tidak ada perubahan sama sekali, kembalikan byte GIF asli
  if (ops.length === 0 && (!marksCanvas || marksCanvas.width === 0)) {
    return data;
  }

  // Gunakan WebCodecs ImageDecoder di WebView2 untuk memproses setiap frame GIF animasi
  if ("ImageDecoder" in window) {
    try {
      const ImgDec = (window as unknown as { ImageDecoder: any }).ImageDecoder;
      const decoder = new ImgDec({ data, type: "image/gif" });
      await decoder.tracks.ready;
      const frameCount: number = decoder.tracks.selectedTrack?.frameCount ?? 1;

      if (frameCount > 0) {
        const geomOps = ops.filter((o) => o.type === "rotate" || o.type === "crop" || o.type === "flipH");
        const strokeOps = ops.filter(
          (o): o is Extract<Op, { type: "stroke" }> => o.type === "stroke"
        );

        const out: number[] = [];
        let outW = 0;
        let outH = 0;

        for (let fi = 0; fi < frameCount; fi++) {
          const { image } = await decoder.decode({ frameIndex: fi });
          const durationUs: number = image.duration ?? 100_000;
          const delayCs = Math.max(2, Math.round(durationUs / 10_000));

          let base = canvas(image.displayWidth, image.displayHeight);
          base.getContext("2d")!.drawImage(image, 0, 0);
          image.close();

          for (const op of geomOps) {
            if (op.type === "rotate") base = rotate(base, op.deg);
            else if (op.type === "crop") base = crop(base, op);
            else if (op.type === "flipH") base = flipH(base);
          }

          if (marksCanvas && marksCanvas.width > 0 && marksCanvas.height > 0) {
            base.getContext("2d")!.drawImage(marksCanvas, 0, 0, base.width, base.height);
          } else if (strokeOps.length > 0) {
            const m = canvas(base.width, base.height);
            const mCtx = m.getContext("2d")!;
            for (const sOp of strokeOps) drawStroke(mCtx, sOp);
            base.getContext("2d")!.drawImage(m, 0, 0);
          }

          const w = base.width;
          const h = base.height;
          const rgba = base.getContext("2d")!.getImageData(0, 0, w, h).data;
          const { hasAlpha, transparentIndex, palette, minCodeSize, lzwBits } =
            quantizeAndLzwFrame(w, h, rgba);

          if (fi === 0) {
            outW = w;
            outH = h;
            out.push(
              0x47, 0x49, 0x46, 0x38, 0x39, 0x61, // GIF89a
              outW & 0xff, (outW >> 8) & 0xff,
              outH & 0xff, (outH >> 8) & 0xff,
              0xf7, 0, 0
            );
            for (let i = 0; i < 768; i++) out.push(palette[i]);
            // NETSCAPE2.0 Infinite Loop Extension
            out.push(
              0x21, 0xff, 0x0b,
              0x4e, 0x45, 0x54, 0x53, 0x43, 0x41, 0x50, 0x45, 0x32, 0x2e, 0x30,
              0x03, 0x01, 0x00, 0x00, 0x00
            );
          }

          // Graphic Control Extension (Disposal = 2 jika transparan agar frame tidak menumpuk)
          const gceFlags = hasAlpha ? (2 << 2) | 1 : 1 << 2;
          out.push(
            0x21, 0xf9, 0x04,
            gceFlags,
            delayCs & 0xff, (delayCs >> 8) & 0xff,
            hasAlpha ? transparentIndex : 0,
            0x00
          );

          // Image Descriptor (menggunakan Local Color Table untuk frame > 0)
          const useLct = fi > 0;
          out.push(
            0x2c,
            0, 0, 0, 0,
            w & 0xff, (w >> 8) & 0xff,
            h & 0xff, (h >> 8) & 0xff,
            useLct ? 0x87 : 0x00
          );
          if (useLct) {
            for (let i = 0; i < 768; i++) out.push(palette[i]);
          }
          out.push(minCodeSize);
          for (let i = 0; i < lzwBits.length; i += 255) {
            const chunk = lzwBits.slice(i, i + 255);
            out.push(chunk.length, ...chunk);
          }
          out.push(0x00);
        }

        out.push(0x3b); // GIF Trailer
        decoder.close();
        return new Uint8Array(out);
      }
    } catch {
      // Fallback ke Tauri backend jika ImageDecoder gagal
    }
  }

  const hasStrokes = ops.some((o) => o.type === "stroke");
  let marks = "";
  if (hasStrokes && marksCanvas && marksCanvas.width > 0 && marksCanvas.height > 0) {
    const dataUrl = marksCanvas.toDataURL("image/png");
    const comma = dataUrl.indexOf(",");
    if (comma !== -1) {
      marks = dataUrl.slice(comma + 1);
    }
  }

  const res = await invoke<ArrayBuffer>("edit_animated_gif", data, {
    headers: {
      ops: encodeURIComponent(JSON.stringify(ops)),
      marks,
    },
  });
  return new Uint8Array(res);
}

export async function blobToBitmap(blob: Blob): Promise<ImageBitmap> {
  if (blob.type !== "image/svg+xml") {
    try {
      return await createImageBitmap(blob);
    } catch {
      // Fall through to <img> decoder
    }
  }
  const url = URL.createObjectURL(blob);
  try {
    const el = new Image();
    el.src = url;
    await el.decode();
    const c = canvas(el.naturalWidth || 1024, el.naturalHeight || 1024);
    c.getContext("2d")!.drawImage(el, 0, 0, c.width, c.height);
    return await createImageBitmap(c);
  } finally {
    URL.revokeObjectURL(url);
  }
}

function isAnimated(ext: string, buf: ArrayBuffer): boolean {
  if (ext === "gif" || ext === "apng" || ext === "svg") return true;
  if (ext === "png" && buf.byteLength > 40) {
    // Cek chunk "acTL" pada file APNG yang berekstensi .png
    const u = new Uint8Array(buf, 8, Math.min(buf.byteLength - 8, 256));
    for (let i = 0; i + 4 <= u.length; i++) {
      if (u[i] === 0x61 && u[i + 1] === 0x63 && u[i + 2] === 0x54 && u[i + 3] === 0x4c) {
        return true;
      }
    }
  }
  if (ext === "webp" && buf.byteLength > 21) {
    const u = new Uint8Array(buf, 12, 9);
    return u[0] === 0x56 && u[1] === 0x50 && u[2] === 0x38 && u[3] === 0x58 && (u[8] & 2) !== 0;
  }
  return false;
}

export async function readImage(
  path: string
): Promise<{ bmp: ImageBitmap; size: number; blob: Blob | null }> {
  const ext = extOf(path);
  const mime = WEB_MIMES[ext];
  if (mime) {
    try {
      const buf = await invoke<ArrayBuffer>("read_file", { path });
      const blob = new Blob([buf], { type: mime });
      const bmp = await blobToBitmap(blob);
      return { bmp, size: buf.byteLength, blob: isAnimated(ext, buf) ? blob : null };
    } catch {
      // Fallback ke dekoder WIC Windows jika gagal di browser (mis. CMYK/16-bit/ICO khusus)
    }
  }
  const buf = await invoke<ArrayBuffer>("decode_image", { path });
  const dv = new DataView(buf);
  const w = dv.getUint32(0, true);
  const h = dv.getUint32(4, true);
  const size = Number(dv.getBigUint64(8, true));
  const bmp = await createImageBitmap(
    new ImageData(new Uint8ClampedArray(buf, 16, w * h * 4), w, h)
  );
  return { bmp, size, blob: null };
}
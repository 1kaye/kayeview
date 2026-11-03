<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    Printer,
    Plus,
    Minus,
    Copy,
    Trash2,
    Move,
    RotateCcw,
    RotateCw,
    LayoutGrid,
    Maximize2,
    ClipboardPaste,
    SlidersHorizontal,
    PanelLeftClose,
    PanelLeftOpen,
  } from "@lucide/svelte";

  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import * as Card from "$lib/components/ui/card";
  import * as Select from "$lib/components/ui/select";
  import * as ButtonGroup from "$lib/components/ui/button-group";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { toast } from "$lib/components/ui/sonner";

  type ShapeKey = "rect" | "circle" | "triangle" | "diamond" | "hexagon" | "star" | "custom";
  type ColorMode = "color" | "grayscale";

  type PrinterProps = {
    accepted: boolean;
    orientation: "portrait" | "landscape";
    paper_code: number;
    width_mm: number;
    height_mm: number;
    color_mode: ColorMode;
    dpi: number;
  };

  type Box = {
    x: number;
    y: number;
    w: number;
    h: number;
    zoom: number;
    ox: number;
    oy: number;
    rot: number;
    shape: ShapeKey;
    radius: number;
    customClip: string;
    outline: number;
    src: string;
    aspect: number;
  };

  let { src, aspect, onclose }: { src: string; aspect: number; onclose: () => void } = $props();

  const PX_PER_MM = 96 / 25.4;

  const PAPER_SIZES: Record<string, { label: string; w: number; h: number; code: number }> = {
    A3: { label: "A3 (29.7 × 42 cm)", w: 297, h: 420, code: 8 },
    A4: { label: "A4 (21 × 29.7 cm)", w: 210, h: 297, code: 9 },
    A5: { label: "A5 (14.8 × 21 cm)", w: 148, h: 210, code: 11 },
    A6: { label: "A6 (10.5 × 14.8 cm)", w: 105, h: 148, code: 70 },
    F4: { label: "F4 / Folio (21.5 × 33 cm)", w: 215, h: 330, code: 14 },
    Letter: { label: "Letter (21.6 × 27.9 cm)", w: 215.9, h: 279.4, code: 1 },
    Legal: { label: "Legal (21.6 × 35.6 cm)", w: 215.9, h: 355.6, code: 5 },
    "2R": { label: "Photo 2R (6 × 9 cm)", w: 60, h: 90, code: 256 },
    "3R": { label: "Photo 3R (8.9 × 12.7 cm)", w: 89, h: 127, code: 256 },
    "4R": { label: "Photo 4R (10.2 × 15.2 cm)", w: 102, h: 152, code: 256 },
    "5R": { label: "Photo 5R (12.7 × 17.8 cm)", w: 127, h: 178, code: 256 },
    "6R": { label: "Photo 6R (15.2 × 20.3 cm)", w: 152, h: 203, code: 256 },
    "8R": { label: "Photo 8R (20.3 × 25.4 cm)", w: 203, h: 254, code: 256 },
    "10R": { label: "Photo 10R (25.4 × 30.5 cm)", w: 254, h: 305, code: 256 },
    Custom: { label: "Custom (Ukuran Sendiri)", w: 210, h: 297, code: 256 },
  };

  const SHAPES: Record<ShapeKey, { label: string; clip: string }> = {
    rect: { label: "Persegi", clip: "none" },
    circle: { label: "Lingkaran", clip: "ellipse(50% 50% at 50% 50%)" },
    triangle: { label: "Segitiga", clip: "polygon(50% 0%, 0% 100%, 100% 100%)" },
    diamond: { label: "Ketupat", clip: "polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%)" },
    hexagon: {
      label: "Segi Enam",
      clip: "polygon(25% 0%, 75% 0%, 100% 50%, 75% 100%, 25% 100%, 0% 50%)",
    },
    star: {
      label: "Bintang",
      clip: "polygon(50% 0%, 61% 35%, 98% 35%, 68% 57%, 79% 91%, 50% 70%, 21% 91%, 32% 57%, 2% 35%, 39% 35%)",
    },
    custom: { label: "Kustom", clip: "polygon(20% 0%, 80% 0%, 100% 100%, 0% 100%)" },
  };

  let printers = $state<string[]>([]);
  let selectedPrinter = $state("");
  let driverDpi = $state(600);
  let propsBusy = $state(false);
  let printing = $state(false);
  let pastedUrls = new Set<string>();

  let mode = $state<"full" | "layout">("full");
  let sidebarOpen = $state(true);
  let paperKey = $state<string>("A4");
  let colorMode = $state<ColorMode>("color");
  let customW = $state(210);
  let customH = $state(297);
  // svelte-ignore state_referenced_locally
  let orientation = $state<"portrait" | "landscape">(aspect > 1 ? "landscape" : "portrait");

  let safeMargin = $state(4);
  let gapYPx = $state(10);
  let gapXPx = $state(10);

  let baseW = $derived(paperKey === "Custom" ? Math.max(30, customW) : PAPER_SIZES[paperKey].w);
  let baseH = $derived(paperKey === "Custom" ? Math.max(30, customH) : PAPER_SIZES[paperKey].h);
  let pageW = $derived(orientation === "portrait" ? Math.min(baseW, baseH) : Math.max(baseW, baseH));
  let pageH = $derived(orientation === "portrait" ? Math.max(baseW, baseH) : Math.min(baseW, baseH));

  let previewFilter = $derived(colorMode === "grayscale" ? "grayscale(100%)" : "none");

  let boxes = $state<Box[]>([]);
  let sel = $state(-1);
  let hoveredIdx = $state(-1);
  let undoStack: { boxes: Box[]; sel: number }[] = [];
  let preDragBoxes: Box[] | null = null;
  let wheelTimeout: ReturnType<typeof setTimeout> | null = null;

  function cloneBoxes(list: Box[]): Box[] {
    return list.map((b) => ({ ...b }));
  }

  /** Bersihkan Object URL yang sudah tidak direferensikan di boxes maupun di undoStack */
  function cleanupUnusedUrls() {
    if (pastedUrls.size === 0) return;
    const used = new Set<string>();
    for (const b of boxes) if (b.src) used.add(b.src);
    for (const entry of undoStack) {
      for (const b of entry.boxes) if (b.src) used.add(b.src);
    }
    if (preDragBoxes) {
      for (const b of preDragBoxes) if (b.src) used.add(b.src);
    }
    for (const u of pastedUrls) {
      if (!used.has(u)) {
        URL.revokeObjectURL(u);
        pastedUrls.delete(u);
      }
    }
  }

  function pushUndo() {
    undoStack.push({ boxes: cloneBoxes(boxes), sel });
    if (undoStack.length > 30) {
      undoStack.shift();
      cleanupUnusedUrls();
    }
  }

  function undo() {
    if (undoStack.length === 0) return;
    const prev = undoStack.pop()!;
    boxes = prev.boxes;
    sel = Math.min(prev.sel, boxes.length - 1);
    cleanupUnusedUrls();
  }

  async function openPrinterProps(prompt = true) {
    if (!selectedPrinter || (prompt && propsBusy)) return;
    const targetPrinter = selectedPrinter;
    if (prompt) propsBusy = true;
    try {
      const res = await invoke<PrinterProps>("open_printer_props", {
        printer: targetPrinter,
        pageWMm: pageW,
        pageHMm: pageH,
        paperCode: PAPER_SIZES[paperKey]?.code ?? 256,
        colorCode: colorMode === "color" ? 2 : 1,
        prompt,
      });
      if (selectedPrinter !== targetPrinter || !res.accepted) return;
      if (res.dpi > 0) driverDpi = res.dpi;
      if (prompt) {
        orientation = res.orientation === "landscape" ? "landscape" : "portrait";
        colorMode = res.color_mode === "grayscale" ? "grayscale" : "color";
        const sw = Math.min(res.width_mm, res.height_mm);
        const sh = Math.max(res.width_mm, res.height_mm);
        let matchedKey = "";
        for (const [k, v] of Object.entries(PAPER_SIZES)) {
          if (k === "Custom") continue;
          if (
            (v.code !== 256 && v.code === res.paper_code) ||
            (Math.abs(v.w - sw) <= 3 && Math.abs(v.h - sh) <= 3)
          ) {
            matchedKey = k;
            break;
          }
        }
        if (matchedKey) {
          paperKey = matchedKey;
        } else if (sw >= 30 && sh >= 30) {
          paperKey = "Custom";
          customW = Math.round(sw);
          customH = Math.round(sh);
        }
        clampAllBoxes();
        toast.success("Pengaturan printer diterapkan");
      }
    } catch (e) {
      if (prompt) toast.error(`Gagal membuka properti printer: ${e}`);
    } finally {
      if (prompt) propsBusy = false;
    }
  }

  onMount(async () => {
    try {
      const res = await invoke<{ printers: string[]; default: string }>("list_printers");
      printers = res.printers;
      selectedPrinter = res.default || res.printers[0] || "";
      if (selectedPrinter) await openPrinterProps(false);
    } catch (e) {
      toast.error(`Gagal memuat printer: ${e}`);
    }
  });

  onDestroy(() => {
    if (wheelTimeout) clearTimeout(wheelTimeout);
    for (const u of pastedUrls) URL.revokeObjectURL(u);
    pastedUrls.clear();
  });

  // Stage viewport & Zoom / Pan
  let cw = $state(0),
    ch = $state(0);
  let fitScale = $derived(
    cw && ch
      ? Math.min(
          Math.max(1, cw - 48) / (pageW * PX_PER_MM),
          Math.max(1, ch - 48) / (pageH * PX_PER_MM)
        )
      : 0.8
  );
  let viewZoom = $state(1);
  let viewPanX = $state(0);
  let viewPanY = $state(0);
  let spaceHeld = $state(false);
  let scale = $derived(Math.max(0.05, fitScale * viewZoom));

  // New box defaults
  let newW = $state(4.0);
  let newH = $state(6.0);
  let newShape = $state<ShapeKey>("rect");
  let newRadius = $state(0);
  let newCustomClip = $state("polygon(20% 0%, 80% 0%, 100% 100%, 0% 100%)");

  type DragState =
    | { kind: "move"; b: Box; sx: number; sy: number; x0: number; y0: number }
    | { kind: "pan-photo"; b: Box; sx: number; sy: number; x0: number; y0: number }
    | { kind: "pan-stage"; sx: number; sy: number; x0: number; y0: number }
    | {
        kind: "resize";
        b: Box;
        sx: number;
        sy: number;
        handle: "se" | "sw" | "ne" | "nw";
        x0: number;
        y0: number;
        w0: number;
        h0: number;
      }
    | {
        kind: "rotate";
        b: Box;
        cx: number;
        cy: number;
        startAngle: number;
        rot0: number;
      };

  let drag = $state<DragState | null>(null);

  function aabbSize(w: number, h: number, rot = 0) {
    const r = (rot * Math.PI) / 180;
    const c = Math.abs(Math.cos(r)),
      s = Math.abs(Math.sin(r));
    return { rw: w * c + h * s, rh: w * s + h * c };
  }

  function boundsFor(w: number, h: number, rot = 0) {
    const gx = Math.max(0, gapXPx || 0) / PX_PER_MM;
    const gy = Math.max(0, gapYPx || 0) / PX_PER_MM;
    const { rw, rh } = aabbSize(w, h, rot);
    const ox = (rw - w) / 2;
    const oy = (rh - h) / 2;
    const minX = safeMargin + ox;
    const minY = safeMargin + oy;
    const maxX = Math.max(minX, pageW - safeMargin - w - ox);
    const maxY = Math.max(minY, pageH - safeMargin - h - oy);
    return { minX, minY, maxX, maxY, stepX: gx, stepY: gy, rw, rh, ox, oy };
  }

  function clampBox(b: Box) {
    const availW = Math.max(5, pageW - safeMargin * 2);
    const availH = Math.max(5, pageH - safeMargin * 2);
    const { rw, rh } = aabbSize(b.w, b.h, b.rot);
    if (rw > availW || rh > availH) {
      const shrink = Math.min(availW / Math.max(1, rw), availH / Math.max(1, rh));
      b.w = Math.max(5, b.w * shrink);
      b.h = Math.max(5, b.h * shrink);
    }
    const { minX, minY, maxX, maxY } = boundsFor(b.w, b.h, b.rot);
    b.x = Math.max(minX, Math.min(maxX, b.x));
    b.y = Math.max(minY, Math.min(maxY, b.y));
  }

  function clampAllBoxes() {
    for (const b of boxes) clampBox(b);
  }

  function photoSize(b: Box) {
    const a = b.aspect || aspect || 1;
    const w = a > b.w / b.h ? b.h * a : b.w;
    return { w: w * b.zoom, h: (w / a) * b.zoom };
  }

  function clipOf(b: Box) {
    if (b.shape === "rect") return b.radius > 0 ? `inset(0 round ${b.radius}%)` : "none";
    if (b.shape === "custom") return b.customClip;
    return SHAPES[b.shape].clip;
  }

  function traceBoxShapePath(
    ctx: CanvasRenderingContext2D,
    b: Box,
    bx: number,
    by: number,
    bw: number,
    bh: number,
    expand = 0
  ) {
    const obx = bx - expand;
    const oby = by - expand;
    const obw = bw + expand * 2;
    const obh = bh + expand * 2;

    ctx.beginPath();
    if (b.shape === "rect") {
      if (b.radius > 0) {
        const r = (Math.min(bw, bh) * b.radius) / 100 + expand;
        ctx.roundRect(obx, oby, obw, obh, Math.max(0, r));
      } else {
        ctx.rect(obx, oby, obw, obh);
      }
    } else if (b.shape === "circle") {
      ctx.ellipse(
        bx + bw / 2,
        by + bh / 2,
        Math.max(1, bw / 2 + expand),
        Math.max(1, bh / 2 + expand),
        0,
        0,
        Math.PI * 2
      );
    } else {
      const poly = b.shape === "custom" ? b.customClip : SHAPES[b.shape].clip;
      const pts = [...poly.matchAll(/([\d.]+)%?\s+([\d.]+)%?/g)].map((m) => [
        obx + (parseFloat(m[1]) / 100) * obw,
        oby + (parseFloat(m[2]) / 100) * obh,
      ]);
      if (pts.length >= 3) {
        pts.forEach(([px, py], idx) => (idx ? ctx.lineTo(px, py) : ctx.moveTo(px, py)));
        ctx.closePath();
      } else {
        ctx.rect(obx, oby, obw, obh);
      }
    }
  }

  function applyCanvasClip(
    ctx: CanvasRenderingContext2D,
    b: Box,
    bx: number,
    by: number,
    bw: number,
    bh: number
  ) {
    traceBoxShapePath(ctx, b, bx, by, bw, bh, 0);
    ctx.clip();
  }

  function loadImageEl(url: string): Promise<HTMLImageElement> {
    return new Promise((res, rej) => {
      const im = new Image();
      im.onload = () => res(im);
      im.onerror = () => rej(new Error("Gagal memuat gambar"));
      im.src = url;
    });
  }

  async function applyPastedBlob(blob: Blob) {
    const url = URL.createObjectURL(blob);
    try {
      const im = await loadImageEl(url);
      pastedUrls.add(url);
      pushUndo();

      const a = im.naturalWidth / Math.max(1, im.naturalHeight);
      if (mode === "full") mode = "layout";

      if (sel >= 0 && boxes[sel]) {
        boxes[sel].src = url;
        boxes[sel].aspect = a;
        boxes[sel].ox = 0;
        boxes[sel].oy = 0;
        boxes[sel].zoom = 1;
        toast.success(`Gambar #${sel + 1} diganti`);
        sel = -1;
      } else {
        const maxW = Math.max(5, pageW - safeMargin * 2);
        const maxH = Math.max(5, pageH - safeMargin * 2);
        const wMm = Math.min(maxW, Math.max(5, newW * 10));
        const hMm = Math.min(maxH, Math.max(5, newH * 10));
        const { minX, minY } = boundsFor(wMm, hMm, 0);
        const last = boxes.at(-1);
        const pos = last ? nextTo(last, wMm, hMm, 0) : { x: minX, y: minY };

        boxes.push({
          ...pos,
          w: wMm,
          h: hMm,
          zoom: 1,
          ox: 0,
          oy: 0,
          rot: 0,
          shape: newShape,
          radius: newRadius,
          customClip: newCustomClip,
          outline: 0,
          src: url,
          aspect: a,
        });
        sel = -1;
        toast.success(`Gambar #${boxes.length} ditambahkan`);
      }
      cleanupUnusedUrls();
    } catch (e) {
      URL.revokeObjectURL(url);
      toast.error(`Gagal memuat gambar tempel: ${e}`);
    }
  }

  async function applyPastedUrl(url: string) {
    try {
      let blob: Blob;
      if (url.startsWith("data:image/")) {
        const res = await fetch(url);
        blob = await res.blob();
      } else {
        try {
          const res = await fetch(url);
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          blob = await res.blob();
        } catch {
          const buf = await invoke<ArrayBuffer>("download_url", { url });
          blob = new Blob([buf]);
        }
      }
      await applyPastedBlob(blob);
    } catch (e) {
      toast.error(`Gagal memuat URL gambar: ${e}`);
    }
  }

  async function pasteFromClipboard() {
    try {
      const items = await navigator.clipboard.read();
      for (const item of items) {
        const imgType = item.types.find((t) => t.startsWith("image/"));
        if (imgType) {
          const blob = await item.getType(imgType);
          await applyPastedBlob(blob);
          return;
        }
      }
    } catch {
      // Fallback ke pembacaan teks jika izin clipboard.read dibatasi
    }

    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (text.startsWith("data:image/") || /^https?:\/\/.+/i.test(text)) {
        await applyPastedUrl(text);
        return;
      }
      toast.info("Tidak ada gambar atau URL di clipboard");
    } catch {
      toast.info("Gunakan Ctrl+V untuk menempel gambar");
    }
  }

  async function onWindowPaste(e: ClipboardEvent) {
    if ((e.target as HTMLElement)?.closest("input, select, textarea")) return;
    const items = e.clipboardData?.items;
    if (items) {
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          const blob = item.getAsFile();
          if (blob) {
            e.preventDefault();
            await applyPastedBlob(blob);
            return;
          }
        }
      }
    }
    const text = e.clipboardData?.getData("text")?.trim();
    if (text) {
      if (text.startsWith("data:image/") || /^https?:\/\/.+/i.test(text)) {
        e.preventDefault();
        await applyPastedUrl(text);
        return;
      }
    }
  }

  async function onStageDrop(e: DragEvent) {
    const dtFiles = e.dataTransfer?.files;
    if (dtFiles && dtFiles.length > 0 && dtFiles[0].type.startsWith("image/")) {
      e.preventDefault();
      await applyPastedBlob(dtFiles[0]);
      return;
    }
    const uri = e.dataTransfer?.getData("text/uri-list") || e.dataTransfer?.getData("text/plain");
    if (uri && (uri.startsWith("data:image/") || /^https?:\/\/.+/i.test(uri.trim()))) {
      e.preventDefault();
      await applyPastedUrl(uri.trim());
    }
  }

  async function directPrint() {
    if (printing) return;
    if (!selectedPrinter) {
      toast.warning("Pilih printer terlebih dahulu");
      return;
    }
    if (mode === "layout" && boxes.length === 0) {
      toast.warning("Belum ada foto di halaman");
      return;
    }
    printing = true;
    toast.info("Menyiapkan halaman cetak...");
    await new Promise((r) => setTimeout(r, 0));
    try {
      const urls = new Set<string>([src, ...boxes.map((b) => b.src || src)]);
      const imgMap = new Map<string, HTMLImageElement>();
      for (const u of urls) {
        imgMap.set(u, await loadImageEl(u));
      }

      let cropX = 0,
        cropY = 0,
        cropW = pageW,
        cropH = pageH;
      if (mode === "full") {
        const safeAspect = aspect > 0 ? aspect : 1;
        const availW = Math.max(1, pageW - safeMargin * 2);
        const availH = Math.max(1, pageH - safeMargin * 2);
        let dw = availW;
        let dh = dw / safeAspect;
        if (dh > availH) {
          dh = availH;
          dw = dh * safeAspect;
        }
        cropX = safeMargin + (availW - dw) / 2;
        cropY = safeMargin + (availH - dh) / 2;
        cropW = dw;
        cropH = dh;
      } else {
        let minX = pageW,
          minY = pageH,
          maxX = 0,
          maxY = 0;
        for (const b of boxes) {
          const outMm = (b.outline || 0) / PX_PER_MM + 0.5;
          const { rw, rh } = aabbSize(b.w + outMm * 2, b.h + outMm * 2, b.rot);
          const cx = b.x + b.w / 2;
          const cy = b.y + b.h / 2;
          minX = Math.min(minX, cx - rw / 2);
          minY = Math.min(minY, cy - rh / 2);
          maxX = Math.max(maxX, cx + rw / 2);
          maxY = Math.max(maxY, cy + rh / 2);
        }
        cropX = Math.max(0, minX);
        cropY = Math.max(0, minY);
        cropW = Math.max(1, Math.min(pageW, maxX) - cropX);
        cropH = Math.max(1, Math.min(pageH, maxY) - cropY);
      }

      const maxDpiByMem = Math.sqrt(8_500_000 / Math.max(0.01, (cropW / 25.4) * (cropH / 25.4)));
      const renderDpi = Math.min(600, maxDpiByMem, Math.max(300, driverDpi || 300));
      const ppm = renderDpi / 25.4;
      const cwPx = Math.max(1, Math.round(cropW * ppm));
      const chPx = Math.max(1, Math.round(cropH * ppm));

      const c = document.createElement("canvas");
      c.width = cwPx;
      c.height = chPx;
      const ctx = c.getContext("2d")!;
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, cwPx, chPx);
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = "high";
      ctx.filter = previewFilter;

      if (mode === "full") {
        const im = imgMap.get(src)!;
        ctx.drawImage(im, 0, 0, cwPx, chPx);
      } else {
        ctx.translate(-cropX * ppm, -cropY * ppm);
        for (const b of boxes) {
          const im = imgMap.get(b.src || src)!;
          const p = photoSize(b);
          const bx = b.x * ppm;
          const by = b.y * ppm;
          const bw = b.w * ppm;
          const bh = b.h * ppm;
          ctx.save();
          if (b.rot) {
            ctx.translate(bx + bw / 2, by + bh / 2);
            ctx.rotate((b.rot * Math.PI) / 180);
            ctx.translate(-(bx + bw / 2), -(by + bh / 2));
          }
          applyCanvasClip(ctx, b, bx, by, bw, bh);
          const ix = bx + ((b.w - p.w) / 2 + b.ox) * ppm;
          const iy = by + ((b.h - p.h) / 2 + b.oy) * ppm;
          ctx.drawImage(im, ix, iy, p.w * ppm, p.h * ppm);
          ctx.restore();

          if ((b.outline ?? 0) > 0) {
            ctx.save();
            if (b.rot) {
              ctx.translate(bx + bw / 2, by + bh / 2);
              ctx.rotate((b.rot * Math.PI) / 180);
              ctx.translate(-(bx + bw / 2), -(by + bh / 2));
            }
            const strokeW = (b.outline * renderDpi) / 96;
            ctx.lineWidth = strokeW;
            ctx.lineJoin = "round";
            ctx.strokeStyle = "#000000";
            traceBoxShapePath(ctx, b, bx, by, bw, bh, strokeW / 2);
            ctx.stroke();
            ctx.restore();
          }
        }
      }

      const rgba = new Uint8Array(ctx.getImageData(0, 0, cwPx, chPx).data.buffer);
      c.width = 0;
      c.height = 0;

      const paperCode = PAPER_SIZES[paperKey]?.code ?? 256;
      const colorCode = colorMode === "color" ? 2 : 1;

      await new Promise((r) => setTimeout(r, 0));
      await invoke("print_page", rgba, {
        headers: {
          printer: encodeURIComponent(selectedPrinter),
          width: String(cwPx),
          height: String(chPx),
          page_w_mm: String(pageW),
          page_h_mm: String(pageH),
          crop_x_mm: String(cropX),
          crop_y_mm: String(cropY),
          crop_w_mm: String(cropW),
          crop_h_mm: String(cropH),
          paper_code: String(paperCode),
          color_code: String(colorCode),
        },
      });
      toast.success("Dokumen berhasil dikirim ke printer!");
    } catch (e) {
      toast.error(`Gagal mencetak: ${e}`);
    } finally {
      printing = false;
    }
  }

  function nextTo(b: Box, w: number, h: number, rot = 0) {
    const { minX, minY, maxX, maxY, stepX, stepY } = boundsFor(w, h, rot);
    const prevAabb = aabbSize(b.w, b.h, b.rot);
    const curAabb = aabbSize(w, h, rot);
    const prevRight = b.x + (b.w + prevAabb.rw) / 2;
    const prevBottom = b.y + (b.h + prevAabb.rh) / 2;
    const nx = prevRight + stepX + (curAabb.rw - w) / 2;
    if (nx <= maxX + 0.01) {
      return { x: nx, y: Math.max(minY, Math.min(maxY, b.y)) };
    }
    return { x: minX, y: Math.min(maxY, prevBottom + stepY + (curAabb.rh - h) / 2) };
  }

  function addBox(wCm: number, hCm: number, boxSrc = src, boxAspect = aspect) {
    pushUndo();
    const maxW = Math.max(5, pageW - safeMargin * 2);
    const maxH = Math.max(5, pageH - safeMargin * 2);
    const wMm = Math.min(maxW, Math.max(5, wCm * 10));
    const hMm = Math.min(maxH, Math.max(5, hCm * 10));
    const { minX, minY } = boundsFor(wMm, hMm, 0);
    const last = boxes.at(-1);
    const pos = last ? nextTo(last, wMm, hMm, 0) : { x: minX, y: minY };
    boxes.push({
      ...pos,
      w: wMm,
      h: hMm,
      zoom: 1,
      ox: 0,
      oy: 0,
      rot: 0,
      shape: newShape,
      radius: newRadius,
      customClip: newCustomClip,
      outline: 0,
      src: boxSrc,
      aspect: boxAspect,
    });
    sel = boxes.length - 1;
  }

  function duplicate() {
    if (sel < 0 || !boxes[sel]) return;
    pushUndo();
    const b = boxes[sel];
    boxes.push({ ...b, ...nextTo(boxes.at(-1)!, b.w, b.h, b.rot) });
    sel = boxes.length - 1;
  }

  function tileRow() {
    if (sel < 0 || !boxes[sel]) return;
    pushUndo();
    const b = boxes[sel];
    const { maxX, stepX } = boundsFor(b.w, b.h, b.rot);
    const { rw } = aabbSize(b.w, b.h, b.rot);
    let curX = b.x + rw + stepX;
    while (curX <= maxX + 0.01) {
      boxes.push({ ...b, x: curX, y: b.y });
      curX += rw + stepX;
    }
    sel = boxes.length - 1;
  }

  function relayoutBoxes() {
    if (!boxes.length) return;
    pushUndo();
    for (let i = 0; i < boxes.length; i++) {
      const b = boxes[i];
      clampBox(b);
      if (i === 0) {
        const { minX, minY } = boundsFor(b.w, b.h, b.rot);
        b.x = minX;
        b.y = minY;
      } else {
        const pos = nextTo(boxes[i - 1], b.w, b.h, b.rot);
        b.x = pos.x;
        b.y = pos.y;
      }
    }
  }

  function remove() {
    if (sel < 0) return;
    pushUndo();
    boxes.splice(sel, 1);
    sel = boxes.length ? Math.min(sel, boxes.length - 1) : -1;
    cleanupUnusedUrls();
  }

  function resetFullView() {
    viewZoom = 1;
    viewPanX = 0;
    viewPanY = 0;
  }

  function zoomStage(factor: number) {
    viewZoom = Math.min(8, Math.max(0.25, viewZoom * factor));
  }

  function onStageWheel(e: WheelEvent) {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    const oldZoom = viewZoom;
    const newZoom = Math.min(8, Math.max(0.25, oldZoom * factor));
    const ratio = newZoom / oldZoom;
    if (ratio !== 1) {
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      viewPanX = viewPanX - (mx - (cw / 2 + viewPanX)) * (ratio - 1);
      viewPanY = viewPanY - (my - (ch / 2 + viewPanY)) * (ratio - 1);
      viewZoom = newZoom;
    }
  }

  function onBoxWheel(e: WheelEvent, i: number) {
    if (e.altKey || e.ctrlKey) {
      e.preventDefault();
      e.stopPropagation();
      sel = i;
      if (!wheelTimeout) {
        pushUndo();
      } else {
        clearTimeout(wheelTimeout);
      }
      wheelTimeout = setTimeout(() => {
        wheelTimeout = null;
      }, 400);
      const b = boxes[i];
      const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
      const oldZoom = b.zoom;
      const newZoom = Math.min(10, Math.max(0.2, oldZoom * factor));
      const ratio = newZoom / oldZoom;
      if (ratio !== 1) {
        const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
        const scx = e.clientX - (r.left + r.width / 2);
        const scy = e.clientY - (r.top + r.height / 2);
        const rad = (-b.rot * Math.PI) / 180;
        const cos = Math.cos(rad),
          sin = Math.sin(rad);
        const ldx = scx * cos - scy * sin;
        const ldy = scx * sin + scy * cos;
        const k = 1 / (scale * PX_PER_MM);
        const curX_mm = ldx * k;
        const curY_mm = ldy * k;
        b.ox = b.ox - (curX_mm - b.ox) * (ratio - 1);
        b.oy = b.oy - (curY_mm - b.oy) * (ratio - 1);
        b.zoom = newZoom;
      }
    }
  }

  function onStagePointerDown(e: PointerEvent) {
    if (e.button === 0 || e.button === 1) {
      if (e.button === 0 && !spaceHeld) sel = -1;
      drag = { kind: "pan-stage", sx: e.clientX, sy: e.clientY, x0: viewPanX, y0: viewPanY };
    }
  }

  function startDrag(e: PointerEvent, i: number, kind: "move" | "pan-photo") {
    if (e.button === 1 || spaceHeld) {
      drag = { kind: "pan-stage", sx: e.clientX, sy: e.clientY, x0: viewPanX, y0: viewPanY };
      return;
    }
    e.stopPropagation();
    e.preventDefault();
    sel = i;
    preDragBoxes = cloneBoxes(boxes);
    const b = boxes[i];
    drag = {
      kind,
      b,
      sx: e.clientX,
      sy: e.clientY,
      x0: kind === "move" ? b.x : b.ox,
      y0: kind === "move" ? b.y : b.oy,
    };
  }

  function startResize(e: PointerEvent, i: number, handle: "se" | "sw" | "ne" | "nw") {
    e.stopPropagation();
    e.preventDefault();
    sel = i;
    preDragBoxes = cloneBoxes(boxes);
    const b = boxes[i];
    drag = {
      kind: "resize",
      b,
      handle,
      sx: e.clientX,
      sy: e.clientY,
      x0: b.x,
      y0: b.y,
      w0: b.w,
      h0: b.h,
    };
  }

  function startRotate(e: PointerEvent, i: number) {
    e.stopPropagation();
    e.preventDefault();
    sel = i;
    preDragBoxes = cloneBoxes(boxes);
    const b = boxes[i];
    const overlay = (e.currentTarget as HTMLElement).closest(".box-overlay")!;
    const r = overlay.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const startAngle = (Math.atan2(e.clientY - cy, e.clientX - cx) * 180) / Math.PI;
    drag = {
      kind: "rotate",
      b,
      cx,
      cy,
      startAngle,
      rot0: b.rot,
    };
  }

  function onMove(e: PointerEvent) {
    if (!drag) return;
    if (drag.kind === "pan-stage") {
      viewPanX = drag.x0 + (e.clientX - drag.sx);
      viewPanY = drag.y0 + (e.clientY - drag.sy);
      return;
    }

    if (drag.kind === "rotate") {
      const curAngle = (Math.atan2(e.clientY - drag.cy, e.clientX - drag.cx) * 180) / Math.PI;
      let deg = drag.rot0 + (curAngle - drag.startAngle);
      if (e.shiftKey) {
        deg = Math.round(deg / 15) * 15;
      } else {
        deg = Math.round(deg * 10) / 10;
      }
      drag.b.rot = (((deg + 180) % 360) + 360) % 360 - 180;
      clampBox(drag.b);
      return;
    }

    const k = 1 / (scale * PX_PER_MM);
    const dx = (e.clientX - drag.sx) * k;
    const dy = (e.clientY - drag.sy) * k;

    if (drag.kind === "move") {
      const { minX, minY, maxX, maxY } = boundsFor(drag.b.w, drag.b.h, drag.b.rot);
      drag.b.x = Math.max(minX, Math.min(maxX, drag.x0 + dx));
      drag.b.y = Math.max(minY, Math.min(maxY, drag.y0 + dy));
      return;
    }

    // Transformasikan delta layar ke sumbu lokal kotak
    const rad = (-drag.b.rot * Math.PI) / 180;
    const cos = Math.cos(rad),
      sin = Math.sin(rad);
    const ldx = dx * cos - dy * sin;
    const ldy = dx * sin + dy * cos;

    if (drag.kind === "pan-photo") {
      drag.b.ox = drag.x0 + ldx;
      drag.b.oy = drag.y0 + ldy;
    } else if (drag.kind === "resize") {
      const { b, handle, x0, y0, w0, h0 } = drag;
      const r0 = w0 / Math.max(1, h0);
      const maxW = Math.max(5, pageW - safeMargin * 2);
      const maxH = Math.max(5, pageH - safeMargin * 2);

      let nw = Math.max(5, handle.includes("w") ? w0 - ldx : w0 + ldx);
      let nh = Math.max(5, handle.includes("n") ? h0 - ldy : h0 + ldy);

      if (e.shiftKey) {
        if (Math.abs(nw - w0) * h0 >= Math.abs(nh - h0) * w0) {
          nh = nw / r0;
        } else {
          nw = nh * r0;
        }
        if (nw > maxW) {
          nw = maxW;
          nh = nw / r0;
        }
        if (nh > maxH) {
          nh = maxH;
          nw = nh * r0;
        }
        if (nw < 5) {
          nw = 5;
          nh = nw / r0;
        }
        if (nh < 5) {
          nh = 5;
          nw = nh * r0;
        }
      } else {
        nw = Math.min(maxW, nw);
        nh = Math.min(maxH, nh);
      }

      // Hitung pergeseran pusat kotak pada sumbu rotasi agar sudut berlawanan tetap terkunci
      const dcxLocal = (handle.includes("w") ? -(nw - w0) : nw - w0) / 2;
      const dcyLocal = (handle.includes("n") ? -(nh - h0) : nh - h0) / 2;
      const posRad = (b.rot * Math.PI) / 180;
      const pCos = Math.cos(posRad),
        pSin = Math.sin(posRad);
      const cx0 = x0 + w0 / 2;
      const cy0 = y0 + h0 / 2;
      const cxNew = cx0 + (dcxLocal * pCos - dcyLocal * pSin);
      const cyNew = cy0 + (dcxLocal * pSin + dcyLocal * pCos);

      b.w = nw;
      b.h = nh;
      b.x = cxNew - nw / 2;
      b.y = cyNew - nh / 2;
      clampBox(b);
    }
  }

  function onPointerUp() {
    if (drag && preDragBoxes && drag.kind !== "pan-stage") {
      const changed = JSON.stringify(preDragBoxes) !== JSON.stringify(boxes);
      if (changed) {
        undoStack.push({ boxes: preDragBoxes, sel });
        if (undoStack.length > 30) {
          undoStack.shift();
          cleanupUnusedUrls();
        }
      }
    }
    drag = null;
    preDragBoxes = null;
  }

  function onWindowBlur() {
    spaceHeld = false;
    onPointerUp();
  }

  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.closest("input, select, textarea")) return;
    if (e.code === "Space") {
      e.preventDefault();
      spaceHeld = e.type === "keydown";
      return;
    }
    if (e.type !== "keydown") return;
    const k = e.key.toLowerCase();
    if (e.ctrlKey) {
      if (k === "z") {
        e.preventDefault();
        undo();
      } else if (k === "b") {
        e.preventDefault();
        sidebarOpen = !sidebarOpen;
      } else if (k === "p") {
        e.preventDefault();
        directPrint();
      } else if (k === "+" || k === "=") {
        e.preventDefault();
        zoomStage(1.25);
      } else if (k === "-") {
        e.preventDefault();
        zoomStage(0.8);
      } else if (k === "0") {
        e.preventDefault();
        resetFullView();
      }
      return;
    }
    if (k === "escape") {
      e.preventDefault();
      onclose();
    } else if ((e.key === "Delete" || e.key === "Backspace") && sel >= 0) {
      e.preventDefault();
      remove();
    }
  }

  function cm(key: "w" | "h") {
    return [
      () => (boxes[sel] ? +(boxes[sel][key] / 10).toFixed(2) : 0),
      (v: number) => {
        if (!boxes[sel]) return;
        pushUndo();
        boxes[sel][key] = Math.max(0.5, v || 0) * 10;
        clampBox(boxes[sel]);
      },
    ] as const;
  }
  const [wg, ws] = cm("w"),
    [hg, hs] = cm("h");
</script>

<svelte:window
  onkeydown={onKey}
  onkeyup={onKey}
  onpaste={onWindowPaste}
  onpointermove={onMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onblur={onWindowBlur}
/>

<div class="print-view">
  <aside
    class="sidebar-panel screen-only transition-[margin-left] duration-300 ease-in-out w-[325px] shrink-0 h-full bg-card/90 backdrop-blur-md border-r border-border p-3.5 flex flex-col gap-2.5 overflow-hidden z-30"
    class:pointer-events-none={!sidebarOpen}
    style:margin-left={sidebarOpen ? "0px" : "-325px"}
  >
    <div class="flex items-center justify-between pb-2 border-b border-border/60 shrink-0">
      <div class="flex items-center gap-2">
        <div class="p-1.5 rounded-lg bg-primary/10 text-primary border border-primary/20">
          <Printer size={15} />
        </div>
        <span class="text-sm font-semibold text-foreground tracking-tight">Cetak Dokumen</span>
      </div>
      <Button
        variant="ghost"
        size="icon-sm"
        class="size-7 text-muted-foreground hover:text-foreground cursor-pointer"
        onclick={() => (sidebarOpen = false)}
        title="Sembunyikan Pengaturan (Ctrl+B)"
      >
        <PanelLeftClose size={15} />
      </Button>
    </div>

    <ButtonGroup.Root class="w-full shrink-0">
      <Button
        variant={mode === "full" ? "default" : "outline"}
        size="sm"
        class="flex-1 font-medium gap-1.5 {mode === 'full' ? 'shadow-xs' : 'text-muted-foreground'}"
        onclick={() => (mode = "full")}
      >
        <Maximize2 size={13} /> Penuh
      </Button>
      <Button
        variant={mode === "layout" ? "default" : "outline"}
        size="sm"
        class="flex-1 font-medium gap-1.5 {mode === 'layout' ? 'shadow-xs' : 'text-muted-foreground'}"
        onclick={() => (mode = "layout")}
      >
        <LayoutGrid size={13} /> Layout
      </Button>
    </ButtonGroup.Root>

    <div class="panel-content flex-1 overflow-y-auto overflow-x-hidden flex flex-col gap-3 min-h-0 pr-0.5">
      <Card.Root size="sm" class="bg-card/70 border-border/80 p-4 gap-3.5 shadow-xs rounded-xl shrink-0">
        <Card.Header class="p-0">
          <Card.Title class="text-xs font-semibold text-left">Printer &amp; Kertas</Card.Title>
        </Card.Header>
        <Card.Content class="p-0 flex flex-col gap-2.5">
          <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
            Printer
            <Select.Root type="single" bind:value={selectedPrinter} onValueChange={() => openPrinterProps(false)}>
              <Select.Trigger class="w-full h-8 text-xs bg-background">
                {selectedPrinter || "Pilih printer..."}
              </Select.Trigger>
              <Select.Content>
                {#if printers.length === 0}
                  <Select.Item value="" disabled class="text-xs">Tidak ada printer</Select.Item>
                {/if}
                {#each printers as p}
                  <Select.Item value={p} label={p} class="text-xs">{p}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </Label>

          <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
            Ukuran Kertas
            <Select.Root type="single" bind:value={paperKey} onValueChange={(v) => { if (v) { paperKey = v; clampAllBoxes(); } }}>
              <Select.Trigger class="w-full h-8 text-xs bg-background">
                {PAPER_SIZES[paperKey]?.label ?? paperKey}
              </Select.Trigger>
              <Select.Content>
                {#each Object.entries(PAPER_SIZES) as [k, v]}
                  <Select.Item value={k} label={v.label} class="text-xs">{v.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </Label>

          {#if paperKey === "Custom"}
            <div class="grid grid-cols-2 gap-2">
              <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
                Lebar (mm)
                <Input type="number" min="30" max="1000" bind:value={customW} onchange={clampAllBoxes} class="h-8 text-xs" />
              </Label>
              <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
                Tinggi (mm)
                <Input type="number" min="30" max="1000" bind:value={customH} onchange={clampAllBoxes} class="h-8 text-xs" />
              </Label>
            </div>
          {/if}

          <div class="grid grid-cols-2 gap-2">
            <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
              Orientasi
              <Select.Root type="single" bind:value={orientation} onValueChange={(v) => { if (v) { orientation = v as "portrait" | "landscape"; clampAllBoxes(); } }}>
                <Select.Trigger class="w-full h-8 text-xs bg-background">
                  {orientation === "portrait" ? "Portrait" : "Landscape"}
                </Select.Trigger>
                <Select.Content>
                  <Select.Item value="portrait" label="Portrait" class="text-xs">Portrait</Select.Item>
                  <Select.Item value="landscape" label="Landscape" class="text-xs">Landscape</Select.Item>
                </Select.Content>
              </Select.Root>
            </Label>
            <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
              Warna
              <Select.Root type="single" bind:value={colorMode} onValueChange={(v) => { if (v) { colorMode = v as ColorMode; } }}>
                <Select.Trigger class="w-full h-8 text-xs bg-background">
                  {colorMode === "color" ? "Berwarna" : "Abu-abu"}
                </Select.Trigger>
                <Select.Content>
                  <Select.Item value="color" label="Berwarna" class="text-xs">Berwarna</Select.Item>
                  <Select.Item value="grayscale" label="Abu-abu" class="text-xs">Abu-abu</Select.Item>
                </Select.Content>
              </Select.Root>
            </Label>
          </div>

          <div class="space-y-1.5 pt-0.5 text-left">
            <div class="flex items-center justify-between text-xs">
              <span class="text-muted-foreground font-normal">Margin Aman</span>
              <Badge variant="secondary" class="h-5 px-1.5 font-mono text-[11px] tabular-nums font-medium">{safeMargin} mm</Badge>
            </div>
            <input
              type="range"
              min="2"
              max="10"
              step="1"
              bind:value={safeMargin}
              oninput={clampAllBoxes}
              class="w-full h-1.5 accent-primary cursor-pointer bg-muted rounded-full"
            />
            <div class="flex justify-between text-[10px] text-muted-foreground font-mono">
              <span>2 mm</span>
              <span>10 mm</span>
            </div>
          </div>

          <Button
            variant="outline"
            size="sm"
            class="w-full mt-0.5"
            onclick={() => openPrinterProps(true)}
            disabled={!selectedPrinter || propsBusy}
          >
            <SlidersHorizontal size={14} /> Printer Properties
          </Button>
        </Card.Content>
      </Card.Root>

      {#if mode === "layout"}
        <Card.Root size="sm" class="bg-card/70 border-border/80 p-4 gap-3.5 shadow-xs rounded-xl shrink-0">
          <Card.Header class="p-0">
            <Card.Title class="text-xs font-semibold text-left">Jarak Bentuk</Card.Title>
          </Card.Header>
          <Card.Content class="p-0 flex flex-col gap-2.5">
            <div class="grid grid-cols-2 gap-2.5">
              <div class="flex flex-col items-start text-left gap-1 w-full">
                <span class="text-[11px] text-muted-foreground font-normal">Atas - Bawah</span>
                <ButtonGroup.Root class="w-full">
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => { gapYPx = Math.max(0, (gapYPx || 0) - 1); relayoutBoxes(); }}
                    disabled={gapYPx <= 0}
                    title="Kurangi 1px"
                  >
                    <Minus size={13} />
                  </Button>
                  <Input
                    type="number"
                    min="0"
                    max="200"
                    bind:value={gapYPx}
                    oninput={relayoutBoxes}
                    onchange={() => { if (!gapYPx || gapYPx < 0) gapYPx = 0; if (gapYPx > 200) gapYPx = 200; relayoutBoxes(); }}
                    class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => { gapYPx = Math.min(200, (gapYPx || 0) + 1); relayoutBoxes(); }}
                    disabled={gapYPx >= 200}
                    title="Tambah 1px"
                  >
                    <Plus size={13} />
                  </Button>
                </ButtonGroup.Root>
              </div>

              <div class="flex flex-col items-start text-left gap-1 w-full">
                <span class="text-[11px] text-muted-foreground font-normal">Kiri - Kanan</span>
                <ButtonGroup.Root class="w-full">
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => { gapXPx = Math.max(0, (gapXPx || 0) - 1); relayoutBoxes(); }}
                    disabled={gapXPx <= 0}
                    title="Kurangi 1px"
                  >
                    <Minus size={13} />
                  </Button>
                  <Input
                    type="number"
                    min="0"
                    max="200"
                    bind:value={gapXPx}
                    oninput={relayoutBoxes}
                    onchange={() => { if (!gapXPx || gapXPx < 0) gapXPx = 0; if (gapXPx > 200) gapXPx = 200; relayoutBoxes(); }}
                    class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => { gapXPx = Math.min(200, (gapXPx || 0) + 1); relayoutBoxes(); }}
                    disabled={gapXPx >= 200}
                    title="Tambah 1px"
                  >
                    <Plus size={13} />
                  </Button>
                </ButtonGroup.Root>
              </div>
            </div>
          </Card.Content>
        </Card.Root>

        <Card.Root size="sm" class="bg-card/70 border-border/80 p-4 gap-3.5 shadow-xs rounded-xl shrink-0">
          <Card.Header class="p-0">
            <Card.Title class="text-xs font-semibold text-left">Tambah Bentuk ({newW}×{newH})</Card.Title>
          </Card.Header>
          <Card.Content class="p-0 flex flex-col gap-2.5">
            <div class="grid grid-cols-2 gap-2">
              <div class="flex flex-col items-start text-left gap-1 w-full">
                <span class="text-[11px] text-muted-foreground font-normal">Lebar (cm)</span>
                <ButtonGroup.Root class="w-full">
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => (newW = Math.max(0.5, Math.round(((newW || 0) - 0.5) * 10) / 10))}
                    disabled={newW <= 0.5}
                    title="Kurangi 0.5 cm"
                  >
                    <Minus size={13} />
                  </Button>
                  <Input
                    type="number"
                    step="0.1"
                    min="0.5"
                    max="50"
                    bind:value={newW}
                    class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => (newW = Math.min(50, Math.round(((newW || 0) + 0.5) * 10) / 10))}
                    disabled={newW >= 50}
                    title="Tambah 0.5 cm"
                  >
                    <Plus size={13} />
                  </Button>
                </ButtonGroup.Root>
              </div>

              <div class="flex flex-col items-start text-left gap-1 w-full">
                <span class="text-[11px] text-muted-foreground font-normal">Tinggi (cm)</span>
                <ButtonGroup.Root class="w-full">
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => (newH = Math.max(0.5, Math.round(((newH || 0) - 0.5) * 10) / 10))}
                    disabled={newH <= 0.5}
                    title="Kurangi 0.5 cm"
                  >
                    <Minus size={13} />
                  </Button>
                  <Input
                    type="number"
                    step="0.1"
                    min="0.5"
                    max="50"
                    bind:value={newH}
                    class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => (newH = Math.min(50, Math.round(((newH || 0) + 0.5) * 10) / 10))}
                    disabled={newH >= 50}
                    title="Tambah 0.5 cm"
                  >
                    <Plus size={13} />
                  </Button>
                </ButtonGroup.Root>
              </div>
            </div>

            <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
              Bentuk
              <Select.Root type="single" bind:value={newShape} onValueChange={(v) => { if (v) { newShape = v as ShapeKey; } }}>
                <Select.Trigger class="w-full h-8 text-xs bg-background">
                  {SHAPES[newShape]?.label ?? newShape}
                </Select.Trigger>
                <Select.Content>
                  {#each Object.entries(SHAPES) as [k, v]}
                    <Select.Item value={k} label={v.label} class="text-xs">{v.label}</Select.Item>
                  {/each}
                </Select.Content>
              </Select.Root>
            </Label>

            {#if newShape === "rect"}
              <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1.5 font-normal w-full">
                <div class="flex justify-between w-full"><span>Sudut: {newRadius}%</span></div>
                <input type="range" class="accent-primary cursor-pointer w-full h-1" min="0" max="50" step="1" bind:value={newRadius} />
              </Label>
            {:else if newShape === "custom"}
              <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
                Clip-path
                <Input type="text" bind:value={newCustomClip} placeholder="polygon(50% 0%, 0% 100%, 100% 100%)" class="h-8 text-xs" />
              </Label>
            {/if}

            <div class="grid grid-cols-2 gap-1.5 mt-0.5">
              <Button variant="default" size="sm" onclick={() => addBox(newW, newH)}>
                <Plus size={14} /> Tambah
              </Button>
              <Button variant="outline" size="sm" onclick={pasteFromClipboard} title="Paste (Ctrl+V)">
                <ClipboardPaste size={14} /> Paste
              </Button>
            </div>
          </Card.Content>
        </Card.Root>

        {#if boxes[sel]}
          <Card.Root size="sm" class="bg-card/80 border-primary/50 shadow-xs ring-1 ring-primary/20 p-4 gap-3.5 rounded-xl shrink-0">
            <Card.Header class="p-0">
              <Card.Title class="text-xs font-semibold text-primary text-left">Terpilih (#{sel + 1})</Card.Title>
            </Card.Header>
            <Card.Content class="p-0 flex flex-col gap-2.5">
              <div class="grid grid-cols-2 gap-2">
                <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
                  Bentuk
                  <Select.Root
                    type="single"
                    value={boxes[sel].shape}
                    onValueChange={(v) => {
                      if (v && boxes[sel] && boxes[sel].shape !== v) {
                        pushUndo();
                        boxes[sel].shape = v as ShapeKey;
                      }
                    }}
                  >
                    <Select.Trigger class="w-full h-8 text-xs bg-background">
                      {SHAPES[boxes[sel].shape]?.label ?? boxes[sel].shape}
                    </Select.Trigger>
                    <Select.Content>
                      {#each Object.entries(SHAPES) as [k, v]}
                        <Select.Item value={k} label={v.label} class="text-xs">{v.label}</Select.Item>
                      {/each}
                    </Select.Content>
                  </Select.Root>
                </Label>

                <div class="flex flex-col items-start text-left gap-1 w-full">
                  <span class="text-[11px] text-muted-foreground font-normal">Rotasi (°)</span>
                  <ButtonGroup.Root class="w-full">
                    <Button
                      variant="outline"
                      size="sm"
                      class="h-8 px-2.5 shrink-0"
                      onclick={() => {
                        pushUndo();
                        boxes[sel].rot = Math.max(-180, (boxes[sel].rot || 0) - 15);
                        clampBox(boxes[sel]);
                      }}
                      disabled={boxes[sel].rot <= -180}
                      title="Putar -15°"
                    >
                      <Minus size={13} />
                    </Button>
                    <Input
                      type="number"
                      step="15"
                      min="-180"
                      max="180"
                      bind:value={boxes[sel].rot}
                      onfocus={() => pushUndo()}
                      oninput={() => clampBox(boxes[sel])}
                      class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                    />
                    <Button
                      variant="outline"
                      size="sm"
                      class="h-8 px-2.5 shrink-0"
                      onclick={() => {
                        pushUndo();
                        boxes[sel].rot = Math.min(180, (boxes[sel].rot || 0) + 15);
                        clampBox(boxes[sel]);
                      }}
                      disabled={boxes[sel].rot >= 180}
                      title="Putar +15°"
                    >
                      <Plus size={13} />
                    </Button>
                  </ButtonGroup.Root>
                </div>
              </div>

              {#if boxes[sel].shape === "rect"}
                <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1.5 font-normal w-full">
                  <div class="flex justify-between w-full"><span>Sudut: {boxes[sel].radius}%</span></div>
                  <input
                    type="range"
                    class="accent-primary cursor-pointer w-full h-1"
                    min="0"
                    max="50"
                    step="1"
                    bind:value={boxes[sel].radius}
                    onpointerdown={() => pushUndo()}
                  />
                </Label>
              {:else if boxes[sel].shape === "custom"}
                <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1 font-normal w-full">
                  Clip-path
                  <Input
                    type="text"
                    bind:value={boxes[sel].customClip}
                    onfocus={() => pushUndo()}
                    class="h-8 text-xs"
                  />
                </Label>
              {/if}

              <div class="grid grid-cols-2 gap-2">
                <div class="flex flex-col items-start text-left gap-1 w-full">
                  <span class="text-[11px] text-muted-foreground font-normal">Lebar (cm)</span>
                  <ButtonGroup.Root class="w-full">
                    <Button
                      variant="outline"
                      size="sm"
                      class="h-8 px-2.5 shrink-0"
                      onclick={() => ws(Math.max(0.5, Math.round((wg() - 0.5) * 10) / 10))}
                      disabled={wg() <= 0.5}
                      title="Kurangi 0.5 cm"
                    >
                      <Minus size={13} />
                    </Button>
                    <Input
                      type="number"
                      step="0.1"
                      min="0.5"
                      bind:value={wg, ws}
                      class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                    />
                    <Button
                      variant="outline"
                      size="sm"
                      class="h-8 px-2.5 shrink-0"
                      onclick={() => ws(Math.round((wg() + 0.5) * 10) / 10)}
                      title="Tambah 0.5 cm"
                    >
                      <Plus size={13} />
                    </Button>
                  </ButtonGroup.Root>
                </div>

                <div class="flex flex-col items-start text-left gap-1 w-full">
                  <span class="text-[11px] text-muted-foreground font-normal">Tinggi (cm)</span>
                  <ButtonGroup.Root class="w-full">
                    <Button
                      variant="outline"
                      size="sm"
                      class="h-8 px-2.5 shrink-0"
                      onclick={() => hs(Math.max(0.5, Math.round((hg() - 0.5) * 10) / 10))}
                      disabled={hg() <= 0.5}
                      title="Kurangi 0.5 cm"
                    >
                      <Minus size={13} />
                    </Button>
                    <Input
                      type="number"
                      step="0.1"
                      min="0.5"
                      bind:value={hg, hs}
                      class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                    />
                    <Button
                      variant="outline"
                      size="sm"
                      class="h-8 px-2.5 shrink-0"
                      onclick={() => hs(Math.round((hg() + 0.5) * 10) / 10)}
                      title="Tambah 0.5 cm"
                    >
                      <Plus size={13} />
                    </Button>
                  </ButtonGroup.Root>
                </div>
              </div>

              <Label class="text-[11px] text-muted-foreground flex flex-col items-start text-left gap-1.5 font-normal w-full">
                <div class="flex justify-between w-full">
                  <span>Zoom Foto</span><span>{Math.round(boxes[sel].zoom * 100)}%</span>
                </div>
                <input
                  type="range"
                  class="accent-primary cursor-pointer w-full h-1"
                  min="0.2"
                  max="5"
                  step="0.05"
                  bind:value={boxes[sel].zoom}
                  onpointerdown={() => pushUndo()}
                />
              </Label>

              <div class="flex flex-col items-start text-left gap-1 w-full">
                <span class="text-[11px] text-muted-foreground font-normal">Outline (px)</span>
                <ButtonGroup.Root class="w-full">
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => {
                      pushUndo();
                      boxes[sel].outline = Math.max(0, (boxes[sel].outline || 0) - 1);
                    }}
                    disabled={boxes[sel].outline <= 0}
                    title="Kurangi outline"
                  >
                    <Minus size={13} />
                  </Button>
                  <Input
                    type="number"
                    min="0"
                    max="10"
                    step="1"
                    bind:value={boxes[sel].outline}
                    onfocus={() => pushUndo()}
                    oninput={() => {
                      if (boxes[sel].outline < 0) boxes[sel].outline = 0;
                      if (boxes[sel].outline > 10) boxes[sel].outline = 10;
                    }}
                    class="h-8 text-center text-xs font-mono font-medium [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none px-1"
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-8 px-2.5 shrink-0"
                    onclick={() => {
                      pushUndo();
                      boxes[sel].outline = Math.min(10, (boxes[sel].outline || 0) + 1);
                    }}
                    disabled={boxes[sel].outline >= 10}
                    title="Tambah outline"
                  >
                    <Plus size={13} />
                  </Button>
                </ButtonGroup.Root>
              </div>

              <div class="grid grid-cols-2 gap-1.5 mt-0.5">
                <Button
                  variant="outline"
                  size="xs"
                  onclick={() => {
                    pushUndo();
                    Object.assign(boxes[sel], { zoom: 1, ox: 0, oy: 0, rot: 0, outline: 0 });
                    clampBox(boxes[sel]);
                  }}
                  title="Reset"
                >
                  <RotateCcw size={13} /> Reset
                </Button>
                <Button variant="outline" size="xs" onclick={duplicate} title="Duplikat">
                  <Copy size={13} /> Duplikat
                </Button>
                <Button variant="outline" size="xs" onclick={tileRow} title="Isi Baris">
                  <LayoutGrid size={13} /> Isi Baris
                </Button>
                <Button variant="destructive" size="xs" onclick={remove} title="Hapus (Del)">
                  <Trash2 size={13} /> Hapus
                </Button>
              </div>
            </Card.Content>
          </Card.Root>
        {/if}
      {/if}
    </div>

    <div class="pt-3 border-t border-border/60 flex items-center gap-2 shrink-0">
      <Button variant="outline" size="sm" class="flex-1" onclick={onclose}>
        Batal
      </Button>
      <Button variant="default" size="sm" class="flex-1 gap-1.5 font-medium shadow-xs" disabled={printing || !selectedPrinter} onclick={directPrint}>
        <Printer size={14} /> {printing ? "Mencetak..." : "Cetak"}
      </Button>
    </div>
  </aside>

  <div
    class="stage"
    class:panning={drag?.kind === "pan-stage" || spaceHeld}
    bind:clientWidth={cw}
    bind:clientHeight={ch}
    role="presentation"
    onwheel={onStageWheel}
    onpointerdown={onStagePointerDown}
    ondragover={(e) => e.preventDefault()}
    ondrop={onStageDrop}
  >
    {#if !sidebarOpen}
      <div class="stage-left-bar screen-only" onpointerdown={(e) => e.stopPropagation()} role="presentation">
        <Button
          variant="outline"
          size="sm"
          class="h-8 gap-2 bg-card/85 backdrop-blur-md shadow-xs border-border text-foreground hover:bg-accent font-medium cursor-pointer"
          onclick={() => (sidebarOpen = true)}
          title="Tampilkan Pengaturan Cetak (Ctrl+B)"
        >
          <PanelLeftOpen size={14} class="text-primary" />
          <span>Pengaturan</span>
        </Button>
      </div>
    {/if}

    <div
      class="stage-bottom-bar screen-only"
      onpointerdown={(e) => e.stopPropagation()}
      role="presentation"
    >
      <ButtonGroup.Root class="h-7 border border-border/70 rounded-lg bg-card/85 backdrop-blur-md shadow-xs">
        <Button
          variant="ghost"
          size="icon-sm"
          class="size-7 p-0 text-muted-foreground hover:text-foreground cursor-pointer rounded-r-none"
          onclick={() => zoomStage(0.8)}
          title="Perkecil Tampilan (Ctrl -)"
        >
          <Minus size={13} />
        </Button>
        <button
          type="button"
          class="h-7 px-2.5 text-xs font-mono tabular-nums text-foreground hover:text-primary transition-colors cursor-pointer border-x border-border/60 flex items-center justify-center bg-transparent"
          onclick={resetFullView}
          title="Reset Zoom / Pas Jendela (Ctrl 0)"
        >
          {Math.round(viewZoom * 100)}%
        </button>
        <Button
          variant="ghost"
          size="icon-sm"
          class="size-7 p-0 text-muted-foreground hover:text-foreground cursor-pointer rounded-l-none"
          onclick={() => zoomStage(1.25)}
          title="Perbesar Tampilan (Ctrl +)"
        >
          <Plus size={13} />
        </Button>
      </ButtonGroup.Root>
    </div>

    <div
      class="page-PanWrap"
      style="transform: translate({viewPanX}px, {viewPanY}px)"
    >
      <div class="page" style="width:{pageW}mm;height:{pageH}mm;transform:scale({scale})">
        <div
          class="safe-guide screen-only"
          style="left:{safeMargin}mm;top:{safeMargin}mm;right:{safeMargin}mm;bottom:{safeMargin}mm"
        ></div>

        {#if mode === "full"}
          <img
            class="full"
            {src}
            alt=""
            draggable="false"
            style="padding:{safeMargin}mm;box-sizing:border-box;filter:{previewFilter}"
          />
        {:else}
          {#each boxes as b, i}
            {@const p = photoSize(b)}
            {@const aabb = aabbSize(b.w, b.h, b.rot)}
            {@const outMm = (b.outline || 0) / PX_PER_MM}
            <div
              class="box"
              style="left:{b.x}mm;top:{b.y}mm;width:{b.w}mm;height:{b.h}mm;clip-path:{clipOf(b)};transform:rotate({b.rot}deg)"
              role="presentation"
              onpointerdown={(e) => startDrag(e, i, "pan-photo")}
              onwheel={(e) => onBoxWheel(e, i)}
              onpointerenter={() => (hoveredIdx = i)}
              onpointerleave={() => { if (hoveredIdx === i) hoveredIdx = -1; }}
            >
              <img
                src={b.src || src}
                alt=""
                draggable="false"
                style="width:{p.w}mm;height:{p.h}mm;left:{(b.w - p.w) / 2 + b.ox}mm;top:{(b.h - p.h) / 2 + b.oy}mm;filter:{previewFilter}"
              />
            </div>

            {#if (b.outline ?? 0) > 0}
              <div
                class="box-outline-frame"
                style="left:{b.x - outMm}mm;top:{b.y - outMm}mm;width:{b.w + outMm * 2}mm;height:{b.h + outMm * 2}mm;transform:rotate({b.rot}deg);clip-path:{clipOf(b)};"
              ></div>
            {/if}

            <div
              class="box-overlay screen-only"
              class:sel={i === sel}
              class:hovered={hoveredIdx === i || (drag?.kind === "rotate" && drag.b === b)}
              role="presentation"
              style="left:{b.x}mm;top:{b.y}mm;width:{b.w}mm;height:{b.h}mm;transform:rotate({b.rot}deg)"
              onpointerenter={() => (hoveredIdx = i)}
              onpointerleave={() => { if (hoveredIdx === i) hoveredIdx = -1; }}
            >
              {#if i === sel}
                <!-- Corner Scale + Hover Rotate Zones -->
                {#each ["nw", "ne", "se", "sw"] as const as corner}
                  <div class="corner-zone {corner}">
                    <div
                      class="res-handle"
                      title="Scale (Tahan Shift = Kunci Rasio)"
                      role="presentation"
                      onpointerdown={(e) => startResize(e, i, corner)}
                    ></div>
                    <div
                      class="rot-handle {corner}"
                      class:active={drag?.kind === "rotate" && drag.b === b}
                      title="Putar Bentuk (Tahan Shift = Snap 15°)"
                      role="presentation"
                      onpointerdown={(e) => startRotate(e, i)}
                    >
                      <RotateCw size={10} stroke-width={2.2} />
                    </div>
                  </div>
                {/each}
              {/if}
            </div>

            <!-- Unrotated HUD so Move handle always stays at the visual top-center even when rotated -->
            <div
              class="box-hud screen-only"
              class:sel={i === sel}
              class:hovered={hoveredIdx === i || (drag && 'b' in drag && drag.b === b)}
              role="presentation"
              style="left:{b.x + (b.w - aabb.rw) / 2}mm;top:{b.y + (b.h - aabb.rh) / 2}mm;width:{aabb.rw}mm;height:{aabb.rh}mm"
              onpointerenter={() => (hoveredIdx = i)}
              onpointerleave={() => { if (hoveredIdx === i) hoveredIdx = -1; }}
            >
              <div
                class="handle move-handle"
                role="presentation"
                title="Geser posisi bentuk"
                onpointerdown={(e) => startDrag(e, i, "move")}
              >
                <Move size={12} />
              </div>

              <span class="dim-badge">
                {(b.w / 10).toFixed(1)}×{(b.h / 10).toFixed(1)} cm{b.rot ? ` · ${Math.round(b.rot)}°` : ""}
              </span>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .print-view {
    position: fixed;
    inset: 0;
    display: flex;
    background: var(--color-background);
    z-index: 40;
    color: var(--color-foreground);
    font-family: 'Inter Variable', 'Inter', system-ui, -apple-system, sans-serif;
  }
  .stage-left-bar {
    position: absolute;
    top: 14px;
    left: 16px;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .panel-content {
    scrollbar-width: thin;
    scrollbar-color: var(--color-border) transparent;
  }
  .panel-content::-webkit-scrollbar {
    width: 5px;
  }
  .panel-content::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: 4px;
  }
  .stage {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    background: radial-gradient(circle at center, color-mix(in oklch, var(--color-background) 85%, white 3%) 0%, var(--color-background) 100%);
    cursor: grab;
  }
  .stage.panning {
    cursor: grabbing;
  }
  .stage-bottom-bar {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 20;
    display: flex;
    align-items: center;
    pointer-events: auto;
  }
  .page-PanWrap {
    display: flex;
    align-items: center;
    justify-content: center;
    will-change: transform;
  }
  .page {
    flex: none;
    position: relative;
    background: #fff;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.65);
    overflow: hidden;
    transform-origin: center center;
  }
  .safe-guide {
    position: absolute;
    border: 1px dashed rgba(59, 130, 246, 0.35);
    pointer-events: none;
    z-index: 5;
  }
  .full {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
    pointer-events: none;
  }
  .box {
    position: absolute;
    overflow: hidden;
    cursor: grab;
    transform-origin: center center;
    z-index: 2;
  }
  .box img {
    position: absolute;
    max-width: none;
    pointer-events: none;
    user-select: none;
  }
  .box-outline-frame {
    position: absolute;
    pointer-events: none;
    background: #000;
    transform-origin: center center;
    z-index: 1;
  }
  .box-overlay {
    position: absolute;
    pointer-events: none;
    outline: 1px dashed rgba(0, 0, 0, 0.25);
    transform-origin: center center;
    z-index: 3;
  }
  .box-overlay.sel {
    outline: 2px solid var(--color-primary);
    z-index: 10;
  }
  .box-hud {
    position: absolute;
    pointer-events: none;
    z-index: 9;
  }
  .box-hud.sel {
    z-index: 12;
  }
  .move-handle {
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.15s ease;
    position: absolute;
    left: 50%;
    top: -10px;
    transform: translateX(-50%);
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--color-primary);
    color: var(--color-primary-foreground);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: move;
    box-shadow: 0 2px 5px rgba(0, 0, 0, 0.4);
  }
  .box-hud.hovered .move-handle {
    pointer-events: auto;
    opacity: 1;
  }
  .dim-badge {
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.15s ease;
    position: absolute;
    bottom: -18px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(24, 24, 27, 0.85);
    color: #fff;
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 3px;
    white-space: nowrap;
  }
  .box-hud.hovered .dim-badge {
    opacity: 1;
  }
  .corner-zone {
    pointer-events: auto;
    position: absolute;
    width: 26px;
    height: 26px;
    z-index: 3;
  }
  .corner-zone.nw {
    top: -21px;
    left: -21px;
  }
  .corner-zone.ne {
    top: -21px;
    right: -21px;
  }
  .corner-zone.se {
    bottom: -21px;
    right: -21px;
  }
  .corner-zone.sw {
    bottom: -21px;
    left: -21px;
  }
  .res-handle {
    position: absolute;
    width: 10px;
    height: 10px;
    background: #fff;
    border: 2px solid var(--color-primary);
    border-radius: 2px;
    z-index: 2;
  }
  .corner-zone.nw .res-handle {
    bottom: 0;
    right: 0;
    cursor: nwse-resize;
  }
  .corner-zone.ne .res-handle {
    bottom: 0;
    left: 0;
    cursor: nesw-resize;
  }
  .corner-zone.se .res-handle {
    top: 0;
    left: 0;
    cursor: nwse-resize;
  }
  .corner-zone.sw .res-handle {
    top: 0;
    right: 0;
    cursor: nesw-resize;
  }
  .rot-handle {
    position: absolute;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    color: #60a5fa;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: grab;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.4);
    opacity: 0;
    pointer-events: none;
    transform: scale(0.75);
    transition: opacity 0.12s ease, transform 0.12s ease;
  }
  .box-overlay.hovered .rot-handle,
  .corner-zone:hover .rot-handle,
  .rot-handle.active {
    opacity: 1;
    pointer-events: auto;
    transform: scale(1);
  }
  .rot-handle:active,
  .rot-handle.active {
    cursor: grabbing;
    background: var(--color-primary);
    color: var(--color-primary-foreground);
  }
  .rot-handle.nw {
    top: 0;
    left: 0;
  }
  .rot-handle.ne {
    top: 0;
    right: 0;
  }
  .rot-handle.se {
    bottom: 0;
    right: 0;
  }
  .rot-handle.sw {
    bottom: 0;
    left: 0;
  }
</style>
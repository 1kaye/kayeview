<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import {
    FolderOpen,
    ChevronLeft,
    ChevronRight,
    RotateCw,
    Crop,
    PenLine,
    Pen,
    Highlighter,
    Eraser,
    Undo2,
    Copy,
    Save,
    FileDown,
    Printer,
    Check,
    ScanText,
    Image,
    Proportions,
    HardDrive,
    Trash2,
    ClipboardPaste,
    Minus,
    Plus,
  } from "@lucide/svelte";

  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import * as ButtonGroup from "$lib/components/ui/button-group";
  import * as Select from "$lib/components/ui/select";
  import * as Menubar from "$lib/components/ui/menubar";
  import { toast } from "$lib/components/ui/sonner";
  import PrintView from "$lib/PrintView.svelte";
  import {
    type Layers,
    type Op,
    type Rect,
    type Tool,
    OPEN_EXTS,
    SAVE_EXTS,
    applyOp,
    blobToBitmap,
    drawStroke,
    editAnimatedGif,
    encode,
    extOf,
    flatten,
    readImage,
    render,
    toBlob,
    writeFile,
  } from "$lib/editor";

  type Stroke = Extract<Op, { type: "stroke" }>;
  type Point = [number, number];

  const RATIOS: [string, number][] = [
    ["Bebas", 0],
    ["1:1", 1],
    ["3:4", 3 / 4],
    ["2:3", 2 / 3],
    ["4:3", 4 / 3],
    ["3:2", 3 / 2],
    ["16:9", 16 / 9],
    ["9:16", 9 / 16],
  ];

  const SAVE_FILTER_DEFS: { name: string; extensions: string[] }[] = [
    { name: "PNG (*.png)", extensions: ["png"] },
    { name: "JPEG (*.jpg, *.jpeg)", extensions: ["jpg", "jpeg"] },
    { name: "WebP (*.webp)", extensions: ["webp"] },
    { name: "GIF (*.gif)", extensions: ["gif"] },
    { name: "BMP (*.bmp, *.dib)", extensions: ["bmp", "dib"] },
    { name: "ICO (*.ico)", extensions: ["ico"] },
    { name: "CUR (*.cur)", extensions: ["cur"] },
    { name: "PDF (*.pdf)", extensions: ["pdf"] },
  ];

  type OcrWord = { text: string; x: number; y: number; w: number; h: number; pxH: number };
  type OcrLine = { words: OcrWord[] };

  let path = $state<string | null>(null);
  let fileSize = $state(0);
  let files = $state.raw<string[]>([]);
  let img = $state.raw<ImageBitmap | null>(null);
  let animUrl = $state<string | null>(null);
  let ops = $state.raw<Op[]>([]);
  let animStyle = $derived.by(() => {
    if (!img || !animUrl) return "";
    let rot = 0,
      rw = img.width,
      rh = img.height,
      vx = 0,
      vy = 0,
      vw = rw,
      vh = rh;
    for (const op of ops) {
      if (op.type === "crop") {
        vx += op.x;
        vy += op.y;
        vw = op.w;
        vh = op.h;
      } else if (op.type === "rotate") {
        const steps = Math.round((((op.deg % 360) + 360) % 360) / 90);
        for (let s = 0; s < steps; s++) {
          [rot, rw, rh, vx, vy, vw, vh] = [
            (rot + 90) % 360,
            rh,
            rw,
            rh - (vy + vh),
            vx,
            vh,
            vw,
          ];
        }
      }
    }
    const ox = rot === 90 || rot === 180 ? rw - vx : -vx;
    const oy = rot === 180 || rot === 270 ? rh - vy : -vy;
    return `width:${(img.width / vw) * 100}%;height:${(img.height / vh) * 100}%;left:${(ox / vw) * 100}%;top:${(oy / vh) * 100}%;transform:rotate(${rot}deg)`;
  });

  let rawBlob: Blob | null = null;
  function setAnim(blob: Blob | null) {
    rawBlob = blob;
    if (animUrl) URL.revokeObjectURL(animUrl);
    animUrl =
      blob && ["image/gif", "image/apng", "image/svg+xml", "image/webp"].includes(blob.type)
        ? URL.createObjectURL(blob)
        : null;
  }
  let savedLen = $state(0);
  let mode = $state<"view" | "crop" | "mark" | "ocr" | "print">("view");

  // OCR state
  let ocrLines = $state.raw<OcrLine[]>([]);
  let ocrBusy = $state(false);
  let ocrWordCount = $derived(ocrLines.reduce((acc, l) => acc + l.words.length, 0));
  let pastedName = $state<string | null>(null);
  let pastedFormat = $state<string | null>(null);
  let fileName = $derived(path ? (path.split(/[\\/]/).pop() ?? path) : (pastedName ?? "Gambar Web"));
  let fileFormat = $derived(path ? extOf(path).toUpperCase() : (pastedFormat ?? "PNG"));

  function formatBytes(bytes: number) {
    if (!bytes) return "0 B";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }

  // Tools
  let tool = $state<Tool>("pen");
  let color = $state("#ef4444");
  let size = $state(6);

  // Cropping
  let ratio = $state(0);
  let cropRect = $state<Rect | null>(null);

  // Print
  let printSrc = $state("");
  let printAspect = $state(1);

  // Canvas & Layers
  let view: HTMLCanvasElement;
  let temp: HTMLCanvasElement | undefined;
  let layers = $state.raw<Layers | null>(null);
  let dims = $state({ w: 0, h: 0 });

  // Stage viewport & Zoom / Pan
  let sw = $state(0),
    sh = $state(0);
  let zoomLevel = $state(1.0); // 1.0 = fit to window
  let panX = $state(0);
  let panY = $state(0);
  let isPanning = $state(false);
  let panStart = { x: 0, y: 0, px: 0, py: 0 };
  let spaceHeld = $state(false);

  // Fit scale: fits image within stage with 32px padding
  let fit = $derived(
    dims.w && dims.h ? Math.min(1, Math.max(1, sw - 32) / dims.w, Math.max(1, sh - 32) / dims.h) : 1,
  );
  let effectiveScale = $derived(fit * zoomLevel);
  let dirty = $derived(ops.length !== savedLen);

  // Floating Brush Cursor state for Mark tools
  let cursorX = $state(0);
  let cursorY = $state(0);
  let isHoveringImg = $state(false);

  function updateCursorPos(e: PointerEvent) {
    if (!view) return;
    const r = view.getBoundingClientRect();
    cursorX = e.clientX - r.left;
    cursorY = e.clientY - r.top;
    isHoveringImg =
      e.clientX >= r.left &&
      e.clientX <= r.right &&
      e.clientY >= r.top &&
      e.clientY <= r.bottom;
  }

  // Drag states
  let stroke: Stroke | null = null;
  type CropHandle = "nw" | "ne" | "se" | "sw" | "n" | "s" | "e" | "w" | "move";
  let cropDrag: {
    handle: CropHandle;
    startPoint: Point;
    origRect: Rect;
  } | null = null;

  function redraw(preview?: Stroke) {
    if (!layers || !view) return;
    const { base, marks } = layers;
    if (view.width !== base.width || view.height !== base.height) {
      [view.width, view.height] = [base.width, base.height];
    }
    const ctx = view.getContext("2d")!;
    ctx.clearRect(0, 0, view.width, view.height);
    if (!animUrl) ctx.drawImage(base, 0, 0);

    if (preview) {
      temp ??= document.createElement("canvas");
      if (temp.width !== marks.width || temp.height !== marks.height) {
        [temp.width, temp.height] = [marks.width, marks.height];
      }
      const t = temp.getContext("2d")!;
      t.clearRect(0, 0, temp.width, temp.height);
      t.drawImage(marks, 0, 0);
      drawStroke(t, preview);
      ctx.drawImage(temp, 0, 0);
    } else {
      ctx.drawImage(marks, 0, 0);
    }
    dims = { w: base.width, h: base.height };
  }

  function commit(op: Op) {
    if (!layers) return;
    ops = [...ops, op];
    layers = applyOp(layers, op);
    if (op.type !== "stroke") {
      ocrLines = [];
      mode = "view";
    }
    redraw();
  }

  function undo() {
    if (!img || !ops.length) return;
    ops = ops.slice(0, -1);
    layers = render(img, ops);
    ocrLines = [];
    if (mode !== "mark") mode = "view";
    redraw();
  }

  function resetZoom() {
    zoomLevel = 1.0;
    panX = 0;
    panY = 0;
  }

  function zoomStep(factor: number) {
    zoomLevel = Math.min(10, Math.max(0.1, zoomLevel * factor));
  }

  function resetInteractionState() {
    isPanning = false;
    spaceHeld = false;
    stroke = null;
    cropDrag = null;
  }

  // ---------- Coordinate Mapping ----------
  function toCanvasPoint(e: PointerEvent): Point {
    const r = view.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return [0, 0];
    const scaleX = view.width / r.width;
    const scaleY = view.height / r.height;
    const x = (e.clientX - r.left) * scaleX;
    const y = (e.clientY - r.top) * scaleY;
    return [
      Math.max(0, Math.min(view.width, x)),
      Math.max(0, Math.min(view.height, y)),
    ];
  }

  // ---------- File Operations ----------
  async function load(p: string) {
    try {
      const { bmp, size: bytes, blob } = await readImage(p);
      img?.close();
      setAnim(blob);
      pastedName = null;
      pastedFormat = null;
      [img, path, fileSize, ops, savedLen, ocrLines, mode] = [bmp, p, bytes, [], 0, [], "view"];
      resetZoom();
      layers = render(bmp, []);
      redraw();
      if (!files.includes(p)) {
        files = await invoke<string[]>("list_images", { path: p });
      }
    } catch (e) {
      toast.error(`Gagal membuka gambar: ${e}`);
    }
  }

  async function loadBlob(blob: Blob, name = "Gambar Web", format = "PNG", sourceUrl?: string) {
    if (!(await confirmDiscard())) return;
    try {
      const bmp = await blobToBitmap(blob);
      img?.close();
      setAnim(blob);
      pastedName = name;
      pastedFormat = format;
      [img, path, fileSize, ops, savedLen, ocrLines, mode] = [
        bmp,
        null,
        blob.size,
        [],
        0,
        [],
        "view",
      ];
      toast.success(`Gambar dimuat dari ${sourceUrl ? "internet" : "clipboard"}`);
      resetZoom();
      layers = render(bmp, []);
      redraw();
      files = [];
    } catch (e) {
      toast.error(`Gagal memuat gambar: ${e}`);
    }
  }

  async function loadFromUrl(url: string) {
    if (!(await confirmDiscard())) return;
    try {
      let blob: Blob;
      try {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        blob = await res.blob();
      } catch {
        const buf = await invoke<ArrayBuffer>("download_url", { url });
        blob = new Blob([buf]);
      }
      const rawName = url.split("?")[0].split("/").pop() || "web-image";
      const ext = rawName.includes(".") ? rawName.split(".").pop()!.toUpperCase() : "JPG";
      await loadBlob(blob, rawName, ext, url);
    } catch (e) {
      toast.error(`Gagal mengunduh gambar: ${e}`);
    }
  }

  async function pasteFromClipboard() {
    if (mode === "print") return;
    try {
      const items = await navigator.clipboard.read();
      for (const item of items) {
        const imgType = item.types.find((t) => t.startsWith("image/"));
        if (imgType) {
          const blob = await item.getType(imgType);
          await loadBlob(blob, "Salinan Gambar", imgType.split("/")[1]?.toUpperCase() || "PNG");
          return;
        }
      }
    } catch {
      // Fallback ke pembacaan teks jika izin clipboard.read dibatasi
    }

    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (text.startsWith("data:image/")) {
        const res = await fetch(text);
        const blob = await res.blob();
        const fmt = text.slice(5, text.indexOf(";")).split("/")[1]?.toUpperCase() || "PNG";
        await loadBlob(blob, "Data Image", fmt);
        return;
      } else if (/^https?:\/\/.+/i.test(text)) {
        await loadFromUrl(text);
        return;
      }
      toast.info("Tidak ada gambar atau tautan gambar di clipboard");
    } catch {
      toast.info("Gunakan Ctrl+V untuk menempel gambar");
    }
  }

  async function onWindowPaste(e: ClipboardEvent) {
    if ((e.target as HTMLElement)?.closest("input, select, textarea")) return;
    if (mode === "print") return;

    const items = e.clipboardData?.items;
    if (items) {
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          const blob = item.getAsFile();
          if (blob) {
            e.preventDefault();
            await loadBlob(blob, "Salinan Gambar", item.type.split("/")[1]?.toUpperCase() || "PNG");
            return;
          }
        }
      }
    }

    const text = e.clipboardData?.getData("text")?.trim();
    if (text) {
      if (text.startsWith("data:image/")) {
        e.preventDefault();
        try {
          const res = await fetch(text);
          const blob = await res.blob();
          const fmt = text.slice(5, text.indexOf(";")).split("/")[1]?.toUpperCase() || "PNG";
          await loadBlob(blob, "Data Image", fmt);
          return;
        } catch (err) {
          toast.error(`Gagal memuat data image: ${err}`);
        }
      } else if (/^https?:\/\/.+/i.test(text)) {
        e.preventDefault();
        await loadFromUrl(text);
        return;
      }
    }
  }

  async function onStageDrop(e: DragEvent) {
    if (mode === "print") return;
    const uri = e.dataTransfer?.getData("text/uri-list") || e.dataTransfer?.getData("text/plain");
    if (uri && /^https?:\/\/.+/i.test(uri.trim())) {
      e.preventDefault();
      await loadFromUrl(uri.trim());
    }
  }

  // Shadcn Alert Dialog Confirmation State
  let confirmState = $state<{
    open: boolean;
    title: string;
    description: string;
    confirmText: string;
    cancelText: string;
    destructive?: boolean;
    onConfirm: () => void;
    onCancel?: () => void;
  }>({
    open: false,
    title: "",
    description: "",
    confirmText: "Lanjutkan",
    cancelText: "Batal",
    onConfirm: () => {},
  });

  function confirmDialog(opts: {
    title: string;
    description: string;
    confirmText?: string;
    cancelText?: string;
    destructive?: boolean;
  }): Promise<boolean> {
    return new Promise((resolve) => {
      let settled = false;
      const finish = (result: boolean) => {
        if (settled) return;
        settled = true;
        confirmState.open = false;
        resolve(result);
      };
      confirmState = {
        open: true,
        title: opts.title,
        description: opts.description,
        confirmText: opts.confirmText ?? "Lanjutkan",
        cancelText: opts.cancelText ?? "Batal",
        destructive: opts.destructive ?? false,
        onConfirm: () => finish(true),
        onCancel: () => finish(false),
      };
    });
  }

  async function confirmDiscard() {
    if (!dirty) return true;
    return await confirmDialog({
      title: "Buang Perubahan?",
      description: "Perubahan belum disimpan. Apakah Anda yakin ingin membuang perubahan ini?",
      confirmText: "Buang Perubahan",
      cancelText: "Batal",
      destructive: true,
    });
  }

  async function openFile(p?: string | null) {
    if (!(await confirmDiscard())) return;
    if (!p) {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Gambar", extensions: OPEN_EXTS }],
      });
      p = typeof selected === "string" ? selected : null;
    }
    if (p) await load(p);
  }

  async function nav(d: number) {
    if (!path || files.length < 2) return;
    const i = files.indexOf(path);
    if (i === -1) return;
    const next = files[(i + d + files.length) % files.length];
    if (await confirmDiscard()) await load(next);
  }

  async function deleteFile() {
    if (!path) return;
    const name = fileName;
    const confirmed = await confirmDialog({
      title: "Hapus Foto",
      description: `Hapus "${name}" secara permanen? Tindakan ini tidak dapat dibatalkan.`,
      confirmText: "Hapus",
      cancelText: "Batal",
      destructive: true,
    });
    if (!confirmed) return;
    const cur = path;
    const idx = files.indexOf(cur);
    const remaining = files.filter((f) => f !== cur);
    try {
      await invoke("delete_file", { path: cur });
      if (remaining.length > 0) {
        await load(remaining[idx % remaining.length]);
      } else {
        img?.close();
        setAnim(null);
        pastedName = null;
        pastedFormat = null;
        [img, path, fileSize, files, ops, savedLen, ocrLines, layers, mode] = [
          null,
          null,
          0,
          [],
          [],
          0,
          [],
          null,
          "view",
        ];
        toast.success(`Foto ${name} berhasil dihapus`);
        dims = { w: 0, h: 0 };
      }
    } catch (e) {
      toast.error(`Gagal menghapus: ${e}`);
    }
  }

  async function saveTo(p: string) {
    if (!layers) return;
    try {
      const ext = extOf(p);
      const isSameFormat = (path && extOf(path) === ext) || pastedFormat?.toLowerCase() === ext;
      let data: Uint8Array;
      if (rawBlob && ops.length === 0 && isSameFormat) {
        data = new Uint8Array(await rawBlob.arrayBuffer());
      } else if (
        rawBlob &&
        ext === "gif" &&
        (rawBlob.type === "image/gif" || (path && extOf(path) === "gif"))
      ) {
        const origBytes = new Uint8Array(await rawBlob.arrayBuffer());
        try {
          data = await editAnimatedGif(origBytes, ops, layers.marks);
        } catch (err) {
          console.warn("Gagal encode animated GIF, fallback ke static frame", err);
          data = await encode(layers, ext);
        }
      } else {
        data = await encode(layers, ext);
      }
      await writeFile(p, data);

      const fname = p.split(/[\\/]/).pop() ?? p;
      toast.success(`Berhasil disimpan: ${fname}`);

      // Hanya update state file aktif jika format hasil simpan adalah format gambar (bukan PDF)
      if (OPEN_EXTS.includes(ext)) {
        path = p;
        pastedName = null;
        pastedFormat = null;
        fileSize = data.byteLength;
        savedLen = ops.length;
        if (!["gif", "apng", "webp", "svg"].includes(ext) && animUrl) {
          setAnim(null);
          redraw();
        }
        files = await invoke<string[]>("list_images", { path: p });
      }
    } catch (e) {
      toast.error(`Gagal menyimpan: ${e}`);
    }
  }

  async function save() {
    if (!layers) return;
    if (path && SAVE_EXTS.includes(extOf(path))) {
      await saveTo(path);
    } else {
      await saveAs();
    }
  }

  async function saveAs() {
    if (!layers) return;
    const rawExt = path ? extOf(path) : (pastedFormat?.toLowerCase() || "png");
    const defaultExt =
      rawExt === "jpe" || rawExt === "jfif"
        ? "jpg"
        : rawExt === "apng"
          ? "png"
          : rawExt === "rle"
            ? "bmp"
            : SAVE_EXTS.includes(rawExt)
              ? rawExt
              : "png";

    const base = path
      ? path.replace(/\.[^.\\/]+$/, "")
      : pastedName
        ? pastedName.replace(/\.[^.\\/]+$/, "")
        : "image";

    // Susun ulang filter agar format asli berada di indeks ke-0 (menjadi default di Windows Explorer)
    const matchedFilter =
      SAVE_FILTER_DEFS.find((f) => f.extensions.includes(defaultExt)) ?? SAVE_FILTER_DEFS[0];
    const primaryFilter = {
      name: matchedFilter.name,
      extensions: [defaultExt, ...matchedFilter.extensions.filter((e) => e !== defaultExt)],
    };
    const otherFilters = SAVE_FILTER_DEFS.filter((f) => f !== matchedFilter);

    let p = await saveDialog({
      defaultPath: `${base}.${defaultExt}`,
      filters: [
        primaryFilter,
        ...otherFilters,
        { name: "Semua Format Didukung", extensions: SAVE_EXTS },
      ],
    });
    if (!p) return;
    if (!SAVE_EXTS.includes(extOf(p))) p += `.${defaultExt}`;
    await saveTo(p);
  }

  async function copy() {
    if (!layers) return;
    try {
      const blob = await toBlob(flatten(layers));
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      toast.success("Gambar berhasil disalin ke clipboard");
    } catch (e) {
      toast.error(`Gagal menyalin: ${e}`);
    }
  }

  async function openPrint() {
    if (!layers) return;
    const c = flatten(layers);
    printAspect = c.width / c.height;
    if (printSrc) URL.revokeObjectURL(printSrc);
    printSrc = URL.createObjectURL(await toBlob(c));
    mode = "print";
  }

  function closePrint() {
    if (printSrc) URL.revokeObjectURL(printSrc);
    printSrc = "";
    mode = "view";
  }

  // ---------- Mode Switching & Windows Native OCR ----------
  function setMode(m: "crop" | "mark") {
    if (!img) return;
    if (mode === m) {
      mode = "view";
      return;
    }
    mode = m;
    if (mode === "crop") {
      ratio = 0;
      initCrop();
    }
  }

  async function toggleOcr() {
    if (!img || !layers || ocrBusy) return;
    if (mode === "ocr") {
      window.getSelection()?.removeAllRanges();
      mode = "view";
      return;
    }
    mode = "ocr";
    if (ocrLines.length > 0) return;
    ocrBusy = true;
    try {
      const base = layers.base;
      const maxDim = 4096;
      const scale = Math.min(1, maxDim / Math.max(base.width, base.height));
      const w = Math.max(1, Math.round(base.width * scale));
      const h = Math.max(1, Math.round(base.height * scale));
      let srcCanvas = base;
      if (scale < 1) {
        srcCanvas = document.createElement("canvas");
        srcCanvas.width = w;
        srcCanvas.height = h;
        srcCanvas.getContext("2d")!.drawImage(base, 0, 0, w, h);
      }
      const rgba = new Uint8Array(
        srcCanvas.getContext("2d")!.getImageData(0, 0, w, h).data.buffer,
      );
      const raw = await invoke<
        { words: { text: string; x: number; y: number; w: number; h: number }[] }[]
      >("ocr_image", rgba, { headers: { width: String(w), height: String(h) } });
      ocrLines = raw.map((l) => ({
        words: l.words.map((wd) => ({
          text: wd.text,
          x: wd.x / w,
          y: wd.y / h,
          w: wd.w / w,
          h: wd.h / h,
          pxH: wd.h / scale,
        })),
      }));
    } catch (e) {
      toast.error(`Gagal OCR: ${e}`);
    } finally {
      ocrBusy = false;
    }
  }

  async function copyOcrText() {
    const selected = window.getSelection()?.toString().trim();
    const full = ocrLines.map((l) => l.words.map((w) => w.text).join(" ")).join("\n");
    const text = selected || full;
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      toast.success(
        selected ? "Teks terpilih disalin ke clipboard" : "Semua teks OCR disalin ke clipboard",
      );
    } catch (e) {
      toast.error(`Gagal menyalin teks: ${e}`);
    }
  }

  // ---------- Rotation Actions ----------
  function rotateClockwise() {
    if (!img) return;
    commit({ type: "rotate", deg: 90 });
    resetZoom();
  }

  // ---------- Crop Actions ----------
  function initCrop() {
    const { w, h } = dims;
    if (w <= 0 || h <= 0) return;
    if (!ratio) {
      cropRect = {
        x: Math.round(w * 0.05),
        y: Math.round(h * 0.05),
        w: Math.round(w * 0.9),
        h: Math.round(h * 0.9),
      };
    } else {
      let cw = w * 0.85;
      let ch = cw / ratio;
      if (ch > h * 0.85) {
        ch = h * 0.85;
        cw = ch * ratio;
      }
      cropRect = {
        x: Math.round((w - cw) / 2),
        y: Math.round((h - ch) / 2),
        w: Math.round(cw),
        h: Math.round(ch),
      };
    }
  }

  function onRatioChange() {
    initCrop();
  }

  function applyCrop() {
    if (!cropRect) return;
    const r = {
      x: Math.round(Math.max(0, cropRect.x)),
      y: Math.round(Math.max(0, cropRect.y)),
      w: Math.round(Math.min(dims.w - cropRect.x, cropRect.w)),
      h: Math.round(Math.min(dims.h - cropRect.y, cropRect.h)),
    };
    if (r.w >= 2 && r.h >= 2) {
      commit({ type: "crop", ...r });
      resetZoom();
    }
  }

  // ---------- Stage Pointer & Wheel Events ----------
  function onStageWheel(e: WheelEvent) {
    if (!img) return;
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    const oldZoom = zoomLevel;
    const newZoom = Math.min(10, Math.max(0.1, oldZoom * factor));
    const ratioZoom = newZoom / oldZoom;
    if (ratioZoom !== 1) {
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      panX = panX - (mx - (sw / 2 + panX)) * (ratioZoom - 1);
      panY = panY - (my - (sh / 2 + panY)) * (ratioZoom - 1);
      zoomLevel = newZoom;
    }
  }

  function onStagePointerDown(e: PointerEvent) {
    updateCursorPos(e);
    // Middle button or Spacebar + drag triggers panning anywhere
    if (e.button === 1 || (e.button === 0 && (mode === "view" || spaceHeld))) {
      e.preventDefault();
      isPanning = true;
      panStart = { x: e.clientX, y: e.clientY, px: panX, py: panY };
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
      return;
    }

    if (e.button !== 0) return;

    if (mode === "mark") {
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
      const p = toCanvasPoint(e);
      stroke = {
        type: "stroke",
        tool,
        color,
        size: Math.max(1, size / effectiveScale),
        points: [p],
      };
      redraw(stroke);
    }
  }

  function onStagePointerMove(e: PointerEvent) {
    updateCursorPos(e);
    if (isPanning) {
      panX = panStart.px + (e.clientX - panStart.x);
      panY = panStart.py + (e.clientY - panStart.y);
      return;
    }

    if (stroke) {
      let p = toCanvasPoint(e);
      if (e.shiftKey) {
        const [x0, y0] = stroke.points[0];
        const dx = Math.abs(p[0] - x0),
          dy = Math.abs(p[1] - y0);
        p = dx * 0.4 > dy ? [p[0], y0] : dy * 0.4 > dx ? [x0, p[1]] : p;
        stroke.points = [[x0, y0], p];
        const r = view.getBoundingClientRect();
        if (view.width && view.height) {
          cursorX = (p[0] / view.width) * r.width;
          cursorY = (p[1] / view.height) * r.height;
        }
      } else {
        stroke.points.push(p);
      }
      redraw(stroke);
    } else if (cropDrag && cropRect) {
      handleCropDragMove(e);
    }
  }

  function onStagePointerUp() {
    if (isPanning) {
      isPanning = false;
      return;
    }

    if (stroke) {
      commit(stroke);
      stroke = null;
    }
    cropDrag = null;
  }

  // Crop Handle Drag
  function startCropHandleDrag(e: PointerEvent, handle: CropHandle) {
    e.stopPropagation();
    e.preventDefault();
    if (!cropRect) return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    cropDrag = {
      handle,
      startPoint: [e.clientX, e.clientY],
      origRect: { ...cropRect },
    };
  }

  function handleCropDragMove(e: PointerEvent) {
    if (!cropDrag || !cropRect) return;
    const { handle, startPoint, origRect } = cropDrag;
    const r = view.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return;
    const scaleX = view.width / r.width;
    const scaleY = view.height / r.height;
    const dx = (e.clientX - startPoint[0]) * scaleX;
    const dy = (e.clientY - startPoint[1]) * scaleY;

    const minSize = 10;

    if (handle === "move") {
      cropRect = {
        x: Math.max(0, Math.min(dims.w - origRect.w, origRect.x + dx)),
        y: Math.max(0, Math.min(dims.h - origRect.h, origRect.y + dy)),
        w: origRect.w,
        h: origRect.h,
      };
      return;
    }

    let left = origRect.x;
    let top = origRect.y;
    let right = origRect.x + origRect.w;
    let bottom = origRect.y + origRect.h;

    if (handle.includes("w")) left = Math.max(0, Math.min(right - minSize, left + dx));
    if (handle.includes("e")) right = Math.min(dims.w, Math.max(left + minSize, right + dx));
    if (handle.includes("n")) top = Math.max(0, Math.min(bottom - minSize, top + dy));
    if (handle.includes("s")) bottom = Math.min(dims.h, Math.max(top + minSize, bottom + dy));

    let w = right - left;
    let h = bottom - top;

    if (ratio > 0) {
      if (handle === "e" || handle === "w") {
        const maxH = dims.h;
        w = Math.min(w, maxH * ratio);
        h = w / ratio;
        if (handle === "w") left = right - w;
        const cy = origRect.y + origRect.h / 2;
        top = Math.max(0, Math.min(dims.h - h, cy - h / 2));
      } else if (handle === "n" || handle === "s") {
        const maxW = dims.w;
        h = Math.min(h, maxW / ratio);
        w = h * ratio;
        if (handle === "n") top = bottom - h;
        const cx = origRect.x + origRect.w / 2;
        left = Math.max(0, Math.min(dims.w - w, cx - w / 2));
      } else {
        // Corner handles (nw, ne, se, sw)
        if (w / ratio > h) {
          w = h * ratio;
        } else {
          h = w / ratio;
        }
        if (handle.includes("w")) left = right - w;
        if (handle.includes("n")) top = bottom - h;
      }
    }

    cropRect = { x: left, y: top, w, h };
  }

  // Keyboard Shortcuts
  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.closest("input, select, textarea")) return;
    const k = e.key.toLowerCase();

    if (e.code === "Space") {
      e.preventDefault();
      spaceHeld = e.type === "keydown";
      return;
    }

    if (e.type !== "keydown") return;

    if (mode === "print") return;

    if (e.ctrlKey) {
      if (k === "c" && (mode === "ocr" || window.getSelection()?.toString().trim())) {
        e.preventDefault();
        copyOcrText();
        return;
      }
      if (k === "c" && e.shiftKey) {
        e.preventDefault();
        setMode("crop");
        return;
      }
      const map: Record<string, () => unknown> = {
        o: () => openFile(),
        v: pasteFromClipboard,
        z: undo,
        c: copy,
        s: e.shiftKey ? saveAs : save,
        p: openPrint,
        r: rotateClockwise,
        m: () => setMode("mark"),
        t: toggleOcr,
        "+": () => zoomStep(1.25),
        "=": () => zoomStep(1.25),
        "-": () => zoomStep(0.8),
        "0": resetZoom,
      };
      if (map[k]) {
        e.preventDefault();
        map[k]();
      }
    } else if (mode === "view") {
      if (k === "r") {
        e.preventDefault();
        rotateClockwise();
      } else if (k === "c") {
        e.preventDefault();
        setMode("crop");
      } else if (k === "m") {
        e.preventDefault();
        setMode("mark");
      } else if (k === "t") {
        e.preventDefault();
        toggleOcr();
      } else if (k === "arrowleft") {
        nav(-1);
      } else if (k === "arrowright") {
        nav(1);
      } else if (k === "delete" && path) {
        e.preventDefault();
        deleteFile();
      }
    } else if (k === "escape") {
      window.getSelection()?.removeAllRanges();
      mode = "view";
    }
  }

  onMount(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop" && e.payload.paths[0]) openFile(e.payload.paths[0]);
    });
    invoke<string | null>("initial_file").then((p) => {
      if (p) load(p);
    });
    return () => void unlisten.then((f) => f());
  });
</script>

<svelte:window
  onkeydown={onKey}
  onkeyup={onKey}
  onpaste={onWindowPaste}
  onblur={resetInteractionState}
/>

{#if mode === "print"}
  <PrintView src={printSrc} aspect={printAspect} onclose={closePrint} />
{/if}

<main class:hidden={mode === "print"}>
  <header class="grid grid-cols-[1fr_auto_1fr] items-center gap-2 px-3 py-1.5 min-h-[42px] bg-card/75 backdrop-blur-md border-b border-border/80 text-foreground select-none">
    {#if mode === "mark"}
      <div class="flex items-center gap-1.5 justify-start">
        <Button variant="default" size="sm" onclick={() => (mode = "view")} title="Keluar Mode Mark">
          <PenLine size={14} /> Mode Mark
        </Button>
        <Button variant="default" size="sm" onclick={() => (mode = "view")} title="Terapkan Anotasi">
          <Check size={14} /> Terapkan
        </Button>
      </div>

      <div class="flex items-center justify-center gap-2">
        <label class="relative inline-flex items-center justify-center size-7 rounded-lg border border-input bg-card/60 shadow-2xs cursor-pointer transition-transform hover:scale-105" class:opacity-35={tool === "eraser"} class:pointer-events-none={tool === "eraser"} title="Pilih Warna">
          <span class="size-4 rounded-full border border-white/80 shadow-2xs" style:background-color={color}></span>
          <input type="color" bind:value={color} disabled={tool === "eraser"} class="absolute inset-0 opacity-0 cursor-pointer" />
        </label>

        <Select.Root type="single" value={tool} onValueChange={(v) => { if (v) tool = v as Tool; }}>
          <Select.Trigger class="h-8 text-xs bg-background min-w-[110px] gap-1.5">
            {#if tool === "pen"}
              <Pen size={14} /> <span>Pena</span>
            {:else if tool === "highlighter"}
              <Highlighter size={14} /> <span>Stabilo</span>
            {:else if tool === "eraser"}
              <Eraser size={14} /> <span>Hapus</span>
            {/if}
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="pen" label="Pena" class="text-xs">
              <div class="flex items-center gap-2">
                <Pen size={14} /> <span>Pena</span>
              </div>
            </Select.Item>
            <Select.Item value="highlighter" label="Stabilo" class="text-xs">
              <div class="flex items-center gap-2">
                <Highlighter size={14} /> <span>Stabilo</span>
              </div>
            </Select.Item>
            <Select.Item value="eraser" label="Hapus" class="text-xs">
              <div class="flex items-center gap-2">
                <Eraser size={14} /> <span>Hapus</span>
              </div>
            </Select.Item>
          </Select.Content>
        </Select.Root>

        <div class="flex items-center gap-2 h-8 px-2.5 rounded-lg border border-input bg-card/50 shadow-2xs">
          <input
            type="range"
            class="w-20 h-1 accent-primary cursor-pointer"
            min="2"
            max="120"
            bind:value={size}
            title="{size}px"
          />
          <Badge variant="secondary" class="h-5 px-1.5 text-[10px] font-mono tabular-nums">{size}px</Badge>
        </div>
      </div>

      <div class="flex items-center justify-end gap-1.5">
        {#if ops.length > 0}
          <Button variant="outline" size="sm" onclick={undo} title="Undo (Ctrl+Z)">
            <Undo2 size={14} /> Undo
          </Button>
        {/if}
      </div>
    {:else if mode === "crop"}
      <div class="flex items-center gap-1.5 justify-start">
        <Button variant="default" size="sm" onclick={() => (mode = "view")} title="Keluar Mode Crop">
          <Crop size={14} /> Mode Crop
        </Button>
        <Button variant="default" size="sm" onclick={applyCrop} title="Terapkan Crop">
          <Check size={14} /> Terapkan
        </Button>
      </div>

      <div class="flex items-center justify-center gap-2">
        <Select.Root type="single" value={String(ratio)} onValueChange={(v) => { if (v) { ratio = Number(v); onRatioChange(); } }}>
          <Select.Trigger class="h-8 text-xs bg-background min-w-[100px]">
            {RATIOS.find(([, val]) => String(val) === String(ratio))?.[0] ?? "Rasio"}
          </Select.Trigger>
          <Select.Content>
            {#each RATIOS as [label, val]}
              <Select.Item value={String(val)} label={label} class="text-xs">{label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>

      <div class="flex items-center justify-end gap-1.5">
        {#if ops.length > 0}
          <Button variant="outline" size="sm" onclick={undo} title="Undo (Ctrl+Z)">
            <Undo2 size={14} /> Undo
          </Button>
        {/if}
      </div>
    {:else if mode === "ocr"}
      <div class="flex items-center gap-1.5 justify-start">
        <Button variant="default" size="sm" onclick={toggleOcr} title="Keluar Mode OCR">
          <ScanText size={14} /> Mode OCR
        </Button>
        <Button variant="default" size="sm" onclick={toggleOcr} title="Selesai OCR">
          <Check size={14} /> Selesai
        </Button>
      </div>

      <div class="flex items-center justify-center gap-1.5">
        <Badge variant="secondary" class="h-8 text-xs px-3">
          {ocrBusy
            ? "Memindai..."
            : ocrWordCount > 0
              ? `${ocrWordCount} kata terdeteksi · Blok teks untuk salin`
              : "Teks tidak ditemukan"}
        </Badge>
      </div>

      <div class="flex items-center justify-end gap-1.5">
        {#if ocrWordCount > 0}
          <Button variant="default" size="sm" onclick={copyOcrText} title="Salin Teks (Ctrl+C)">
            <Copy size={14} /> Salin Teks
          </Button>
        {/if}
      </div>
    {:else}
      <!-- Left: Desktop Menubar & Quick Nav -->
      <div class="flex items-center gap-2 justify-start">
        <Menubar.Root class="bg-background/60 border-border/70 shadow-2xs backdrop-blur-sm">
          <Menubar.Menu>
            <Menubar.Trigger class="text-xs font-medium cursor-pointer">Berkas</Menubar.Trigger>
            <Menubar.Content class="min-w-[190px]">
              <Menubar.Item onSelect={() => openFile()}>
                <FolderOpen class="size-4" />
                <span>Buka</span>
                <Menubar.Shortcut>Ctrl+O</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item onSelect={pasteFromClipboard}>
                <ClipboardPaste class="size-4" />
                <span>Tempel</span>
                <Menubar.Shortcut>Ctrl+V</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item disabled={!layers} onSelect={save}>
                <Save class="size-4" />
                <span>Simpan</span>
                <Menubar.Shortcut>Ctrl+S</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item disabled={!layers} onSelect={saveAs}>
                <FileDown class="size-4" />
                <span>Ekspor...</span>
                <Menubar.Shortcut>Ctrl+Shift+S</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Separator />
              <Menubar.Item disabled={!layers} onSelect={openPrint}>
                <Printer class="size-4" />
                <span>Cetak</span>
                <Menubar.Shortcut>Ctrl+P</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Separator />
              <Menubar.Item variant="destructive" disabled={!path} onSelect={deleteFile}>
                <Trash2 class="size-4" />
                <span>Hapus Foto</span>
                <Menubar.Shortcut>Del</Menubar.Shortcut>
              </Menubar.Item>
            </Menubar.Content>
          </Menubar.Menu>

          <Menubar.Menu>
            <Menubar.Trigger class="text-xs font-medium cursor-pointer">Edit</Menubar.Trigger>
            <Menubar.Content class="min-w-[190px]">
              <Menubar.Item disabled={ops.length === 0} onSelect={undo}>
                <Undo2 class="size-4" />
                <span>Urungkan</span>
                <Menubar.Shortcut>Ctrl+Z</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item disabled={!layers} onSelect={copy}>
                <Copy class="size-4" />
                <span>Salin Gambar</span>
                <Menubar.Shortcut>Ctrl+C</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Separator />
              <Menubar.Item disabled={!img} onSelect={rotateClockwise}>
                <RotateCw class="size-4" />
                <span>Putar 90°</span>
                <Menubar.Shortcut>R</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item disabled={!img} onSelect={() => setMode("crop")}>
                <Crop class="size-4" />
                <span>Crop</span>
                <Menubar.Shortcut>C</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item disabled={!img} onSelect={() => setMode("mark")}>
                <PenLine class="size-4" />
                <span>Mark</span>
                <Menubar.Shortcut>M</Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item disabled={!img || ocrBusy} onSelect={toggleOcr}>
                <ScanText class="size-4" />
                <span>OCR</span>
                <Menubar.Shortcut>T</Menubar.Shortcut>
              </Menubar.Item>
            </Menubar.Content>
          </Menubar.Menu>
        </Menubar.Root>

        {#if files.length > 1}
          <ButtonGroup.Root>
            <Button variant="ghost" size="icon-sm" onclick={() => nav(-1)} title="Foto Sebelumnya (←)">
              <ChevronLeft size={14} />
            </Button>
            <Button variant="ghost" size="icon-sm" onclick={() => nav(1)} title="Foto Berikutnya (→)">
              <ChevronRight size={14} />
            </Button>
          </ButtonGroup.Root>
        {/if}
      </div>

      <!-- Center (Empty in default view) -->
      <div></div>

      <!-- Right: Main CTA & State Actions -->
      <div class="flex items-center justify-end gap-1.5">
        {#if ops.length > 0}
          <Button variant="outline" size="sm" onclick={undo} title="Undo (Ctrl+Z)">
            <Undo2 size={14} /> Undo
          </Button>
        {/if}
      </div>
    {/if}
  </header>

  <!-- Main Viewport Stage -->
  <section
    class="stage"
    bind:clientWidth={sw}
    bind:clientHeight={sh}
    onwheel={onStageWheel}
    onpointerdown={onStagePointerDown}
    onpointermove={onStagePointerMove}
    onpointerup={onStagePointerUp}
    onpointercancel={onStagePointerUp}
    onpointerleave={() => (isHoveringImg = false)}
    ondragover={(e) => e.preventDefault()}
    ondrop={onStageDrop}
    role="presentation"
  >
    {#if !img}
      <div class="flex flex-col items-center justify-center text-center select-none pointer-events-none animate-in fade-in zoom-in-95 duration-200 max-w-md w-full px-6">
        <div class="size-14 rounded-2xl bg-gradient-to-b from-card to-card/50 border border-border/80 shadow-md flex items-center justify-center backdrop-blur-md mb-3 text-muted-foreground">
          <Image class="size-6 opacity-80" strokeWidth={1.5} />
        </div>
        <p class="text-xs font-semibold tracking-wider text-foreground/90 uppercase mb-4">
          Tidak ada gambar yang dimuat
        </p>

        <div class="flex flex-col gap-2 text-xs text-muted-foreground w-full">
          <div class="flex items-center justify-between gap-3">
            <span>Buka file foto</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl + O</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Tempel langsung (Paste)</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl + V</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Simpan / Simpan Sebagai</span>
            <div class="flex items-center gap-1">
              <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl + S</kbd>
              <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl + Shift + S</kbd>
            </div>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Cetak Dokumen</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl + P</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Salin Gambar</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl + C</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Putar 90°</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">R</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Potong Gambar (Crop)</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">C</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Anotasi / Coret (Mark)</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">M</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Ekstrak Teks (OCR)</span>
            <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">T</kbd>
          </div>
          <div class="flex items-center justify-between gap-3">
            <span>Zoom &amp; Pas Jendela</span>
            <div class="flex items-center gap-1">
              <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl +/-</kbd>
              <kbd class="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono text-foreground border border-border shadow-2xs">Ctrl + 0</kbd>
            </div>
          </div>
        </div>
      </div>
    {/if}

    <div
      class="viewport"
      class:hidden={!img}
      style:transform="translate({panX}px, {panY}px)"
      class:panning={isPanning || spaceHeld}
    >
      <div
        class="imgwrap"
        class:tool-mark={mode === "mark"}
        style:width="{dims.w * effectiveScale}px"
        style:height="{dims.h * effectiveScale}px"
      >
        {#if animUrl}
          {#key animUrl}
            <div class="anim-clip">
              <img class="base-img" src={animUrl} style={animStyle} alt="" draggable="false" />
            </div>
          {/key}
        {/if}
        <canvas
          bind:this={view}
          style:width="{dims.w * effectiveScale}px"
          style:height="{dims.h * effectiveScale}px"
        ></canvas>

        <!-- Windows Native OCR Selectable Text Overlay -->
        {#if mode === "ocr" && ocrLines.length > 0 && !spaceHeld}
          <div class="ocr-layer" role="presentation" onpointerdown={(e) => e.stopPropagation()}>
            {#each ocrLines as line}
              <div class="ocr-line">
                {#each line.words as word, wi}
                  <span
                    class="ocr-word"
                    style:left="{word.x * 100}%"
                    style:top="{word.y * 100}%"
                    style:width="{word.w * 100}%"
                    style:height="{word.h * 100}%"
                    style:font-size="{Math.max(7, word.pxH * effectiveScale * 0.78)}px"
                    style:line-height="{word.pxH * effectiveScale}px"
                  >{word.text}{wi < line.words.length - 1 ? " " : ""}</span>
                {/each}
              </div>
            {/each}
          </div>
        {/if}

        <!-- Floating Custom Brush Cursor for Pen (dot), Highlighter (bar), and Eraser (circle) -->
        {#if mode === "mark" && isHoveringImg && !isPanning && !spaceHeld}
          {@const brushW = tool === "highlighter" ? size * 0.25 : size}
          {@const brushH = size}
          <div
            class="brush-cursor"
            class:is-pen={tool === "pen"}
            class:is-eraser={tool === "eraser"}
            class:is-highlighter={tool === "highlighter"}
            style:left="{cursorX}px"
            style:top="{cursorY}px"
            style:width="{brushW}px"
            style:height="{brushH}px"
            style:background-color={tool !== "eraser" ? color : null}
          ></div>
        {/if}

        <!-- Interactive Manual Crop Overlay with 8 Handles and 3x3 Grid -->
        {#if mode === "crop" && cropRect && dims.w > 0 && dims.h > 0}
          {@const leftPct = (cropRect.x / dims.w) * 100}
          {@const topPct = (cropRect.y / dims.h) * 100}
          {@const widthPct = (cropRect.w / dims.w) * 100}
          {@const heightPct = (cropRect.h / dims.h) * 100}

          <div
            class="crop-box"
            style="left:{leftPct}%;top:{topPct}%;width:{widthPct}%;height:{heightPct}%"
            role="presentation"
            onpointerdown={(e) => startCropHandleDrag(e, "move")}
          >
            <!-- 3x3 Grid Lines -->
            <div class="grid-line gh-1"></div>
            <div class="grid-line gh-2"></div>
            <div class="grid-line gv-1"></div>
            <div class="grid-line gv-2"></div>

            <!-- Dimension badge -->
            <span class="crop-badge">{Math.round(cropRect.w)} × {Math.round(cropRect.h)}</span>

            <!-- 8 Resizing Handles -->
            <div class="handle c-nw" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "nw")}></div>
            <div class="handle c-ne" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "ne")}></div>
            <div class="handle c-se" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "se")}></div>
            <div class="handle c-sw" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "sw")}></div>
            <div class="handle c-n" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "n")}></div>
            <div class="handle c-s" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "s")}></div>
            <div class="handle c-e" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "e")}></div>
            <div class="handle c-w" role="presentation" onpointerdown={(e) => startCropHandleDrag(e, "w")}></div>
          </div>
        {/if}
      </div>
    </div>
  </section>

  <footer class="grid grid-cols-3 items-center px-3 py-1 text-xs text-muted-foreground bg-card/60 backdrop-blur-md border-t border-border select-none min-h-[32px]">
    {#if img}
      <!-- Kiri: Nama dan Format -->
      <div class="flex items-center gap-2 min-w-0 justify-start overflow-hidden">
        <span class="inline-flex items-center gap-1.5 max-w-[260px] truncate text-foreground font-medium text-xs" title={path ?? fileName}>
          <Image size={13} class="text-muted-foreground shrink-0" />
          <span class="truncate">{fileName}</span>
        </span>
        <span class="text-muted-foreground/40 select-none">|</span>
        <Badge variant="outline" class="h-4 px-1.5 text-[10px] font-mono uppercase tracking-wider shrink-0" title="Format Foto">
          {fileFormat}
        </Badge>
      </div>

      <!-- Tengah: - Nilai Zoom + -->
      <div class="flex items-center justify-center">
        <ButtonGroup.Root class="h-6 border border-border/60 rounded-md bg-background/50 shadow-2xs">
          <Button
            variant="ghost"
            size="icon"
            class="h-6 w-6 p-0 text-muted-foreground hover:text-foreground cursor-pointer rounded-r-none"
            onclick={() => zoomStep(0.8)}
            title="Perkecil (Ctrl -)"
          >
            <Minus size={12} />
          </Button>
          <button
            type="button"
            class="h-6 px-2 text-[11px] font-mono tabular-nums text-foreground hover:text-primary transition-colors cursor-pointer border-x border-border/60 flex items-center justify-center bg-transparent"
            onclick={resetZoom}
            title="Pas Jendela / Reset 100% (Ctrl 0)"
          >
            {Math.round(zoomLevel * 100)}%
          </button>
          <Button
            variant="ghost"
            size="icon"
            class="h-6 w-6 p-0 text-muted-foreground hover:text-foreground cursor-pointer rounded-l-none"
            onclick={() => zoomStep(1.25)}
            title="Perbesar (Ctrl +)"
          >
            <Plus size={12} />
          </Button>
        </ButtonGroup.Root>
      </div>

      <!-- Kanan: Dimensi, Ukuran File, dan Belum Disimpan -->
      <div class="flex items-center gap-2 justify-end shrink-0">
        <span class="inline-flex items-center gap-1.5 text-xs font-mono tabular-nums text-muted-foreground" title="Ukuran Pixel Foto">
          <Proportions size={13} class="shrink-0" />
          <span>{dims.w} × {dims.h} px</span>
        </span>
        <span class="text-muted-foreground/40 select-none">|</span>
        <span class="inline-flex items-center gap-1.5 text-xs font-mono tabular-nums text-muted-foreground" title="Ukuran File Foto">
          <HardDrive size={13} class="shrink-0" />
          <span>{formatBytes(fileSize)}</span>
        </span>
        {#if dirty || !path}
          <span class="text-muted-foreground/40 select-none">|</span>
          <Badge variant="destructive" class="h-4 px-1.5 text-[10px]">Belum disimpan</Badge>
        {/if}
      </div>
    {/if}
  </footer>
</main>

<AlertDialog.Root
  bind:open={confirmState.open}
  onOpenChange={(isOpen) => {
    if (!isOpen) confirmState.onCancel?.();
  }}
>
  <AlertDialog.Content size="sm">
    <AlertDialog.Header>
      <AlertDialog.Title>{confirmState.title}</AlertDialog.Title>
      <AlertDialog.Description>{confirmState.description}</AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel onclick={() => confirmState.onCancel?.()}>
        {confirmState.cancelText}
      </AlertDialog.Cancel>
      <AlertDialog.Action
        variant={confirmState.destructive ? "destructive" : "default"}
        onclick={() => confirmState.onConfirm()}
      >
        {confirmState.confirmText}
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

<style>
  :global(html, body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
    background: var(--color-background);
    color: var(--color-foreground);
    font-family: 'Inter Variable', 'Inter', system-ui, -apple-system, sans-serif;
    font-size: 13px;
    user-select: none;
  }
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .hidden {
    display: none !important;
  }
  .stage {
    flex: 1;
    min-height: 0;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    background: radial-gradient(circle at center, color-mix(in oklch, var(--color-background) 85%, white 3%) 0%, var(--color-background) 100%);
  }
  .viewport {
    position: relative;
    will-change: transform;
    cursor: default;
  }
  .viewport.panning {
    cursor: grab;
  }
  .imgwrap {
    position: relative;
    line-height: 0;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.7);
    background: repeating-conic-gradient(#222 0 25%, #1a1a1a 0 50%) 0 0 / 16px 16px;
  }
  .anim-clip {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
  .base-img {
    position: absolute;
    transform-origin: 0 0;
    max-width: none;
    max-height: none;
  }
  .imgwrap.tool-mark,
  .imgwrap.tool-mark canvas {
    cursor: none !important;
  }
  .brush-cursor {
    position: absolute;
    transform: translate(-50%, -50%);
    pointer-events: none;
    z-index: 50;
    box-sizing: border-box;
    transition: width 0.05s ease-out, height 0.05s ease-out;
  }
  .brush-cursor.is-pen {
    border-radius: 50%;
    box-shadow: 0 0 0 1.5px #ffffff, 0 0 0 2.5px #000000;
  }
  .brush-cursor.is-eraser {
    border-radius: 50%;
    border: 2px solid #ffffff;
    box-shadow: 0 0 0 1px #000000;
    background: rgba(255, 255, 255, 0.25);
  }
  .brush-cursor.is-highlighter {
    border-radius: 1px;
    box-shadow: 0 0 0 1.5px #ffffff, 0 0 0 2.5px #000000;
    opacity: 0.55;
  }
  canvas {
    position: relative;
    display: block;
  }

  /* Manual Crop Overlay & 8 Handles */
  .crop-box {
    position: absolute;
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.65);
    border: 1.5px solid #ffffff;
    cursor: move;
  }
  .grid-line {
    position: absolute;
    pointer-events: none;
    background: rgba(255, 255, 255, 0.35);
  }
  .gh-1 {
    left: 0;
    right: 0;
    top: 33.333%;
    height: 1px;
  }
  .gh-2 {
    left: 0;
    right: 0;
    top: 66.666%;
    height: 1px;
  }
  .gv-1 {
    top: 0;
    bottom: 0;
    left: 33.333%;
    width: 1px;
  }
  .gv-2 {
    top: 0;
    bottom: 0;
    left: 66.666%;
    width: 1px;
  }
  .crop-badge {
    position: absolute;
    top: -15px;
    left: -5px;
    color: #fff;
    font-size: 11px;
    white-space: nowrap;
    pointer-events: none;
  }
  .handle {
    position: absolute;
    background: #ffffff;
    border: 2px solid var(--color-primary);
    border-radius: 2px;
    z-index: 10;
  }
  /* Corner handles (12x12) */
  .c-nw,
  .c-ne,
  .c-se,
  .c-sw {
    width: 12px;
    height: 12px;
  }
  .c-nw {
    top: -6px;
    left: -6px;
    cursor: nwse-resize;
  }
  .c-ne {
    top: -6px;
    right: -6px;
    cursor: nesw-resize;
  }
  .c-se {
    bottom: -6px;
    right: -6px;
    cursor: nwse-resize;
  }
  .c-sw {
    bottom: -6px;
    left: -6px;
    cursor: nesw-resize;
  }
  /* Edge handles (rectangles) */
  .c-n {
    top: -5px;
    left: 50%;
    transform: translateX(-50%);
    width: 20px;
    height: 8px;
    cursor: ns-resize;
  }
  .c-s {
    bottom: -5px;
    left: 50%;
    transform: translateX(-50%);
    width: 20px;
    height: 8px;
    cursor: ns-resize;
  }
  .c-w {
    left: -5px;
    top: 50%;
    transform: translateY(-50%);
    width: 8px;
    height: 20px;
    cursor: ew-resize;
  }
  .c-e {
    right: -5px;
    top: 50%;
    transform: translateY(-50%);
    width: 8px;
    height: 20px;
    cursor: ew-resize;
  }

  .ocr-layer {
    position: absolute;
    inset: 0;
    z-index: 25;
    user-select: text;
    cursor: text;
  }
  .ocr-word {
    position: absolute;
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    background: rgba(250, 204, 21, 0.22);
    border: 1px solid rgba(250, 204, 21, 0.78);
    border-radius: 3px;
    color: transparent;
    white-space: pre;
    overflow: hidden;
    user-select: text;
    cursor: text;
  }
  .ocr-word:hover {
    background: rgba(250, 204, 21, 0.35);
    border-color: #facc15;
  }
  .ocr-word::selection {
    background: rgba(37, 99, 235, 0.85);
    color: #ffffff;
  }
</style>
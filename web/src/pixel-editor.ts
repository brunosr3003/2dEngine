// Pixel Editor — 32x32 canvas, paleta, ferramentas, IA via Gemini.
// Auth por cookie; todo request de API passa credentials: "include".

const SIZE = 32;
const CELL = 16; // cada pixel logico = 16 CSS px -> canvas 512x512

// Paleta dark-fantasy inicial (user pode adicionar via color picker)
const DEFAULT_PALETTE: string[] = [
  "#00000000", // transparente
  "#000000ff", "#1a1a22ff", "#2a2a36ff", "#4a4a56ff",
  "#8a8a96ff", "#c0c0c8ff", "#e3e3eaff", "#ffffffff",
  "#5a2e1aff", "#8c4a1eff", "#b87333ff", "#d4a53cff",
  "#6b1f1fff", "#a33030ff", "#d4523cff", "#ff7a2eff",
  "#1a3a6bff", "#2f5aa6ff", "#4d8ad4ff", "#7acbffff",
  "#1f4a2aff", "#2f7a3dff", "#4cae5aff", "#8aef6dff",
  "#3b1f4fff", "#6b2f8cff", "#a854c6ff", "#cf8cf0ff",
  "#5c4a1aff", "#8a6f2eff", "#c7a346ff", "#f5d470ff",
];

type Tool = "brush" | "eraser" | "fill" | "eyedrop";

class Editor {
  grid: string[][] = [];
  history: string[][][] = [];
  tool: Tool = "brush";
  current_color: string = "#ffffffff";
  grid_on: boolean = true;
  c: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  palette: string[] = [...DEFAULT_PALETTE];
  drawing: boolean = false;

  constructor() {
    this.c = document.getElementById("c") as HTMLCanvasElement;
    this.ctx = this.c.getContext("2d")!;
    for (let y = 0; y < SIZE; y++) {
      this.grid.push(new Array(SIZE).fill("#00000000"));
    }
    this.render();
  }

  push_history() {
    this.history.push(this.grid.map(row => [...row]));
    if (this.history.length > 50) this.history.shift();
  }

  undo() {
    const prev = this.history.pop();
    if (prev) { this.grid = prev; this.render(); }
  }

  set_pixel(x: number, y: number, color: string) {
    if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
    this.grid[y][x] = color;
  }

  apply_tool(x: number, y: number) {
    if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
    switch (this.tool) {
      case "brush":   this.set_pixel(x, y, this.current_color); break;
      case "eraser":  this.set_pixel(x, y, "#00000000"); break;
      case "fill":    this.flood_fill(x, y, this.current_color); break;
      case "eyedrop": {
        this.current_color = this.grid[y][x];
        update_alpha_from_color();
        highlight_palette();
        break;
      }
    }
    this.render();
  }

  flood_fill(sx: number, sy: number, target: string) {
    const source = this.grid[sy][sx];
    if (source === target) return;
    const stack: [number, number][] = [[sx, sy]];
    while (stack.length) {
      const [x, y] = stack.pop()!;
      if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) continue;
      if (this.grid[y][x] !== source) continue;
      this.grid[y][x] = target;
      stack.push([x + 1, y], [x - 1, y], [x, y + 1], [x, y - 1]);
    }
  }

  clear() {
    this.push_history();
    for (let y = 0; y < SIZE; y++) {
      for (let x = 0; x < SIZE; x++) this.grid[y][x] = "#00000000";
    }
    this.render();
  }

  load_pixels(pixels: string[][]) {
    this.push_history();
    for (let y = 0; y < SIZE; y++) {
      for (let x = 0; x < SIZE; x++) {
        const v = pixels[y]?.[x] ?? "#00000000";
        this.grid[y][x] = normalize_hex(v);
      }
    }
    this.render();
  }

  render() {
    const ctx = this.ctx;
    ctx.clearRect(0, 0, this.c.width, this.c.height);
    for (let y = 0; y < SIZE; y++) {
      for (let x = 0; x < SIZE; x++) {
        const col = this.grid[y][x];
        if (col === "#00000000" || col === "#00000000ff") continue;
        ctx.fillStyle = col;
        ctx.fillRect(x * CELL, y * CELL, CELL, CELL);
      }
    }
    if (this.grid_on) {
      ctx.strokeStyle = "rgba(60,60,80,0.45)";
      ctx.lineWidth = 1;
      for (let i = 0; i <= SIZE; i++) {
        ctx.beginPath();
        ctx.moveTo(i * CELL + 0.5, 0);
        ctx.lineTo(i * CELL + 0.5, this.c.height);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(0, i * CELL + 0.5);
        ctx.lineTo(this.c.width, i * CELL + 0.5);
        ctx.stroke();
      }
    }
  }

  /// Exporta PNG 32x32 exato (sem scale).
  to_png_base64(): string {
    const off = document.createElement("canvas");
    off.width = SIZE; off.height = SIZE;
    const octx = off.getContext("2d")!;
    for (let y = 0; y < SIZE; y++) {
      for (let x = 0; x < SIZE; x++) {
        const c = this.grid[y][x];
        if (c === "#00000000") continue;
        octx.fillStyle = c;
        octx.fillRect(x, y, 1, 1);
      }
    }
    const dataUrl = off.toDataURL("image/png");
    return dataUrl.split(",")[1];
  }
}

function normalize_hex(s: string): string {
  s = s.trim().toLowerCase();
  if (!s.startsWith("#")) s = "#" + s;
  // #rgb -> #rrggbbff
  if (s.length === 4) {
    s = "#" + s[1]+s[1]+s[2]+s[2]+s[3]+s[3]+"ff";
  } else if (s.length === 7) {
    s = s + "ff";
  } else if (s.length === 9) {
    // ok
  } else {
    return "#00000000";
  }
  return s;
}

// ────────────── State / init ──────────────

let editor: Editor;

function highlight_palette() {
  const swatches = document.querySelectorAll<HTMLDivElement>(".sw");
  swatches.forEach(sw => {
    sw.classList.toggle("active", sw.dataset.color === editor.current_color);
  });
}

function build_palette() {
  const pal = document.getElementById("palette")!;
  pal.innerHTML = "";
  editor.palette.forEach(hex => {
    const sw = document.createElement("div");
    sw.className = "sw";
    if (hex === "#00000000") sw.classList.add("transparent");
    else sw.style.background = hex;
    sw.dataset.color = hex;
    sw.addEventListener("click", () => {
      editor.current_color = hex;
      update_alpha_from_color();
      highlight_palette();
    });
    pal.appendChild(sw);
  });
  highlight_palette();
}

function update_alpha_from_color() {
  const alpha_input = document.getElementById("alpha") as HTMLInputElement;
  const alpha_val = document.getElementById("alpha-val")!;
  const custom = document.getElementById("custom-color") as HTMLInputElement;
  const c = editor.current_color;
  if (c.length === 9) {
    const a = parseInt(c.slice(7, 9), 16);
    alpha_input.value = String(a);
    alpha_val.textContent = String(a);
    custom.value = "#" + c.slice(1, 7);
  }
}

function setup_canvas_events() {
  const c = editor.c;
  const coord_el = document.getElementById("coord")!;
  const getXY = (ev: MouseEvent): [number, number] => {
    const rect = c.getBoundingClientRect();
    const x = Math.floor((ev.clientX - rect.left) * (SIZE / rect.width));
    const y = Math.floor((ev.clientY - rect.top)  * (SIZE / rect.height));
    return [x, y];
  };
  c.addEventListener("mousedown", (ev) => {
    editor.drawing = true;
    editor.push_history();
    const [x, y] = getXY(ev);
    editor.apply_tool(x, y);
  });
  c.addEventListener("mousemove", (ev) => {
    const [x, y] = getXY(ev);
    coord_el.textContent = `${x}, ${y}`;
    if (editor.drawing && (editor.tool === "brush" || editor.tool === "eraser")) {
      editor.apply_tool(x, y);
    }
  });
  window.addEventListener("mouseup", () => { editor.drawing = false; });
}

function setup_tool_buttons() {
  document.querySelectorAll<HTMLButtonElement>(".tool-btns button").forEach(btn => {
    btn.addEventListener("click", () => {
      const t = btn.dataset.tool as Tool;
      editor.tool = t;
      document.querySelectorAll(".tool-btns button").forEach(b => b.classList.remove("active"));
      btn.classList.add("active");
    });
  });

  const custom = document.getElementById("custom-color") as HTMLInputElement;
  const alpha_input = document.getElementById("alpha") as HTMLInputElement;
  const alpha_val = document.getElementById("alpha-val")!;
  const update_color = () => {
    const a = parseInt(alpha_input.value) & 0xff;
    alpha_val.textContent = String(a);
    const hex = custom.value.toLowerCase() + a.toString(16).padStart(2, "0");
    editor.current_color = hex;
    // Se essa cor nao esta na paleta, adiciona
    if (!editor.palette.includes(hex)) {
      editor.palette.push(hex);
      build_palette();
    }
    highlight_palette();
  };
  custom.addEventListener("input", update_color);
  alpha_input.addEventListener("input", update_color);

  document.getElementById("btn-clear")!.addEventListener("click", () => editor.clear());
  document.getElementById("btn-undo")!.addEventListener("click", () => editor.undo());
  document.getElementById("btn-grid")!.addEventListener("click", () => {
    editor.grid_on = !editor.grid_on;
    editor.render();
  });
}

function setup_keyboard() {
  window.addEventListener("keydown", (ev) => {
    if ((ev.target as HTMLElement)?.tagName === "INPUT") return;
    if ((ev.target as HTMLElement)?.tagName === "TEXTAREA") return;
    if (ev.key === "b") set_tool("brush");
    if (ev.key === "e") set_tool("eraser");
    if (ev.key === "f") set_tool("fill");
    if (ev.key === "i") set_tool("eyedrop");
    if ((ev.ctrlKey || ev.metaKey) && ev.key === "z") {
      ev.preventDefault();
      editor.undo();
    }
  });
}

function set_tool(t: Tool) {
  editor.tool = t;
  document.querySelectorAll(".tool-btns button").forEach(b => {
    b.classList.toggle("active", (b as HTMLElement).dataset.tool === t);
  });
}

// ────────────── API ──────────────

async function api_auth(password: string): Promise<boolean> {
  const r = await fetch("/api/pixel/auth", {
    method: "POST",
    headers: {"Content-Type": "application/json"},
    credentials: "include",
    body: JSON.stringify({ password }),
  });
  return r.ok;
}

async function api_generate(prompt: string): Promise<string[][]> {
  const r = await fetch("/api/pixel/generate", {
    method: "POST",
    headers: {"Content-Type": "application/json"},
    credentials: "include",
    body: JSON.stringify({ prompt, size: SIZE }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error || `http ${r.status}`);
  return body.pixels;
}

async function api_save(name: string, png_base64: string): Promise<string> {
  const r = await fetch("/api/pixel/save", {
    method: "POST",
    headers: {"Content-Type": "application/json"},
    credentials: "include",
    body: JSON.stringify({ name, png_base64 }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error || `http ${r.status}`);
  return body.saved as string;
}

async function api_list(): Promise<string[]> {
  const r = await fetch("/api/pixel/list", {
    method: "POST",
    credentials: "include",
  });
  const body = await r.json();
  if (!r.ok) return [];
  return body.sprites as string[];
}

function refresh_list() {
  api_list().then(names => {
    const ul = document.getElementById("sprites-list")!;
    ul.innerHTML = "";
    names.forEach(n => {
      const li = document.createElement("li");
      li.textContent = n;
      ul.appendChild(li);
    });
  });
}

// ────────────── Wire IA / Save / Auth ──────────────

function setup_ia() {
  const btn = document.getElementById("btn-generate") as HTMLButtonElement;
  const prompt_el = document.getElementById("ia-prompt") as HTMLTextAreaElement;
  const status = document.getElementById("ia-status")!;
  btn.addEventListener("click", async () => {
    const p = prompt_el.value.trim();
    if (!p) { status.textContent = "descreva o sprite"; status.className = "status err"; return; }
    btn.disabled = true;
    status.className = "status";
    status.textContent = "gerando...";
    try {
      const pixels = await api_generate(p);
      editor.load_pixels(pixels);
      status.className = "status ok";
      status.textContent = "pronto";
    } catch (e: any) {
      status.className = "status err";
      status.textContent = e.message || "erro";
    } finally {
      btn.disabled = false;
    }
  });
}

function setup_save() {
  const btn_save = document.getElementById("btn-save") as HTMLButtonElement;
  const btn_dl = document.getElementById("btn-download") as HTMLButtonElement;
  const name_el = document.getElementById("sprite-name") as HTMLInputElement;
  const status = document.getElementById("save-status")!;
  btn_save.addEventListener("click", async () => {
    const name = name_el.value.trim();
    if (!name) { status.textContent = "nome vazio"; status.className = "status err"; return; }
    status.className = "status";
    status.textContent = "salvando...";
    try {
      const b64 = editor.to_png_base64();
      const path = await api_save(name, b64);
      status.className = "status ok";
      status.textContent = `salvo: ${path}`;
      refresh_list();
    } catch (e: any) {
      status.className = "status err";
      status.textContent = e.message || "erro";
    }
  });
  btn_dl.addEventListener("click", () => {
    const name = (name_el.value.trim() || "sprite") + ".png";
    const b64 = editor.to_png_base64();
    const a = document.createElement("a");
    a.href = "data:image/png;base64," + b64;
    a.download = name;
    a.click();
  });
}

function setup_auth() {
  const form = document.getElementById("auth-form") as HTMLFormElement;
  const err = document.getElementById("auth-err")!;
  form.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const data = new FormData(form);
    const pw = data.get("password") as string;
    err.textContent = "";
    const ok = await api_auth(pw);
    if (ok) {
      document.getElementById("auth-gate")!.hidden = true;
      document.getElementById("editor")!.hidden = false;
      boot_editor();
    } else {
      err.textContent = "senha invalida";
    }
  });
}

function boot_editor() {
  editor = new Editor();
  build_palette();
  setup_canvas_events();
  setup_tool_buttons();
  setup_keyboard();
  setup_ia();
  setup_save();
  refresh_list();
}

// Init: liga o form de login imediatamente. Se ja tiver cookie valido,
// a primeira chamada autenticada (list) deve ir direto.
setup_auth();

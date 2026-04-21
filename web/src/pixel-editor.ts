// Pixel Editor — 32x32 canvas, paleta, ferramentas, IA via Gemini.
// Auth por cookie; todo request de API passa credentials: "include".

// Tamanho configuravel: 32, 64 ou 128. Muda o canvas + API size.
let SIZE = 64;
// CELL escolhido pra canvas ficar ~512-1024 px (click-friendly)
function cell_for(size: number): number {
  if (size <= 32) return 16;    // canvas 512
  if (size <= 64) return 8;     // canvas 512
  return 6;                     // 128 -> canvas 768
}
let CELL = cell_for(SIZE);

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

type Tool = "brush" | "eraser" | "fill" | "eyedrop" | "select";

interface SelRect { x: number; y: number; w: number; h: number; }
interface ClipData { pixels: string[][]; w: number; h: number; }

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
  /// Seleção retangular ativa (null = sem seleção).
  selection: SelRect | null = null;
  /// Durante drag: "create" = criando novo retângulo; "move" = movendo.
  sel_dragging: "create" | "move" | null = null;
  /// Ponto inicial do drag (em celulas).
  sel_start: { x: number; y: number } | null = null;
  /// Pixels capturados no inicio do movimento (pra restaurar se cancelar).
  sel_pixels_snap: string[][] | null = null;
  /// Posicao (canto sup-esq) do conteudo movido atualmente.
  sel_move_pos: { x: number; y: number } | null = null;
  clipboard: ClipData | null = null;

  constructor() {
    this.c = document.getElementById("c") as HTMLCanvasElement;
    this.ctx = this.c.getContext("2d")!;
    this.resize_to(SIZE);
  }

  /// Redefine o tamanho logico do grid e o canvas. Limpa tudo.
  resize_to(new_size: number) {
    SIZE = new_size;
    CELL = cell_for(SIZE);
    this.c.width  = SIZE * CELL;
    this.c.height = SIZE * CELL;
    this.grid = [];
    for (let y = 0; y < SIZE; y++) {
      this.grid.push(new Array(SIZE).fill("#00000000"));
    }
    this.history = [];
    const lbl = document.getElementById("size-label");
    if (lbl) lbl.textContent = `${SIZE}×${SIZE}`;
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
      case "select":  /* tratado via eventos */ break;
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
    // Auto-resize se a matriz vem de tamanho diferente
    const got = pixels.length;
    if (got > 0 && got !== SIZE) {
      console.log(`[pixel] load_pixels: resize ${SIZE}->${got}`);
      this.resize_to(got);
      const sel = document.getElementById("ia-size") as HTMLSelectElement | null;
      if (sel && [32, 64, 128].includes(got)) sel.value = String(got);
    }
    this.push_history();
    for (let y = 0; y < SIZE; y++) {
      for (let x = 0; x < SIZE; x++) {
        const v = pixels[y]?.[x] ?? "#00000000";
        this.grid[y][x] = normalize_hex(v);
      }
    }
    this.render();
  }

  /// Inicia criação de nova seleção (click num ponto).
  start_selection(x: number, y: number) {
    this.sel_dragging = "create";
    this.sel_start = { x, y };
    this.selection = { x, y, w: 1, h: 1 };
  }

  /// Atualiza a seleção enquanto o mouse arrasta.
  update_selection(x: number, y: number) {
    if (!this.sel_start) return;
    const sx = Math.max(0, Math.min(this.sel_start.x, x));
    const sy = Math.max(0, Math.min(this.sel_start.y, y));
    const ex = Math.min(SIZE - 1, Math.max(this.sel_start.x, x));
    const ey = Math.min(SIZE - 1, Math.max(this.sel_start.y, y));
    this.selection = { x: sx, y: sy, w: ex - sx + 1, h: ey - sy + 1 };
  }

  /// Começa a mover o conteúdo da seleção. Captura pixels e limpa a área original.
  start_move(cursor_x: number, cursor_y: number) {
    if (!this.selection) return;
    this.sel_dragging = "move";
    this.sel_start = { x: cursor_x, y: cursor_y };
    // Captura snapshot dos pixels da seleção
    this.sel_pixels_snap = [];
    for (let iy = 0; iy < this.selection.h; iy++) {
      const row: string[] = [];
      for (let ix = 0; ix < this.selection.w; ix++) {
        const gx = this.selection.x + ix;
        const gy = this.selection.y + iy;
        row.push(this.grid[gy][gx]);
        // Limpa do grid (preview mostra conteudo flutuando)
        this.grid[gy][gx] = "#00000000";
      }
      this.sel_pixels_snap.push(row);
    }
    this.sel_move_pos = { x: this.selection.x, y: this.selection.y };
    this.push_history();
  }

  /// Atualiza a posição do conteúdo durante o movimento.
  update_move(cursor_x: number, cursor_y: number) {
    if (!this.selection || !this.sel_start || !this.sel_move_pos) return;
    const dx = cursor_x - this.sel_start.x;
    const dy = cursor_y - this.sel_start.y;
    this.sel_move_pos = {
      x: this.selection.x + dx,
      y: this.selection.y + dy,
    };
  }

  /// Termina o movimento: escreve os pixels na nova posição.
  commit_move() {
    if (!this.sel_pixels_snap || !this.sel_move_pos || !this.selection) return;
    for (let iy = 0; iy < this.sel_pixels_snap.length; iy++) {
      for (let ix = 0; ix < this.sel_pixels_snap[iy].length; ix++) {
        const gx = this.sel_move_pos.x + ix;
        const gy = this.sel_move_pos.y + iy;
        const c = this.sel_pixels_snap[iy][ix];
        if (c === "#00000000") continue; // nao sobrescreve com transparente
        if (gx < 0 || gy < 0 || gx >= SIZE || gy >= SIZE) continue;
        this.grid[gy][gx] = c;
      }
    }
    this.selection = {
      x: this.sel_move_pos.x,
      y: this.sel_move_pos.y,
      w: this.selection.w,
      h: this.selection.h,
    };
    this.sel_pixels_snap = null;
    this.sel_move_pos = null;
    this.sel_dragging = null;
    this.sel_start = null;
  }

  /// Verifica se um ponto está dentro da seleção atual.
  is_inside_selection(x: number, y: number): boolean {
    const s = this.selection;
    if (!s) return false;
    return x >= s.x && x < s.x + s.w && y >= s.y && y < s.y + s.h;
  }

  /// Copia os pixels da seleção pro clipboard.
  copy_selection(): boolean {
    if (!this.selection) return false;
    const pixels: string[][] = [];
    for (let iy = 0; iy < this.selection.h; iy++) {
      const row: string[] = [];
      for (let ix = 0; ix < this.selection.w; ix++) {
        row.push(this.grid[this.selection.y + iy][this.selection.x + ix]);
      }
      pixels.push(row);
    }
    this.clipboard = { pixels, w: this.selection.w, h: this.selection.h };
    return true;
  }

  /// Recorta: copia + limpa a área da seleção.
  cut_selection(): boolean {
    if (!this.copy_selection()) return false;
    if (!this.selection) return false;
    this.push_history();
    for (let iy = 0; iy < this.selection.h; iy++) {
      for (let ix = 0; ix < this.selection.w; ix++) {
        this.grid[this.selection.y + iy][this.selection.x + ix] = "#00000000";
      }
    }
    return true;
  }

  /// Cola o clipboard numa posição (canto sup-esq) e ativa seleção lá.
  paste_clipboard(tx?: number, ty?: number) {
    if (!this.clipboard) return false;
    const px = tx ?? Math.max(0, Math.floor((SIZE - this.clipboard.w) / 2));
    const py = ty ?? Math.max(0, Math.floor((SIZE - this.clipboard.h) / 2));
    this.push_history();
    for (let iy = 0; iy < this.clipboard.h; iy++) {
      for (let ix = 0; ix < this.clipboard.w; ix++) {
        const gx = px + ix;
        const gy = py + iy;
        if (gx < 0 || gy < 0 || gx >= SIZE || gy >= SIZE) continue;
        const c = this.clipboard.pixels[iy][ix];
        if (c === "#00000000") continue;
        this.grid[gy][gx] = c;
      }
    }
    this.selection = { x: px, y: py, w: this.clipboard.w, h: this.clipboard.h };
    return true;
  }

  clear_selection() {
    this.selection = null;
    this.sel_dragging = null;
    this.sel_start = null;
    this.sel_pixels_snap = null;
    this.sel_move_pos = null;
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
    // Preview do conteudo em movimento (seleção flutuante)
    if (this.sel_dragging === "move" && this.sel_pixels_snap && this.sel_move_pos) {
      for (let iy = 0; iy < this.sel_pixels_snap.length; iy++) {
        for (let ix = 0; ix < this.sel_pixels_snap[iy].length; ix++) {
          const c = this.sel_pixels_snap[iy][ix];
          if (c === "#00000000") continue;
          const gx = this.sel_move_pos.x + ix;
          const gy = this.sel_move_pos.y + iy;
          if (gx < 0 || gy < 0 || gx >= SIZE || gy >= SIZE) continue;
          ctx.fillStyle = c;
          ctx.fillRect(gx * CELL, gy * CELL, CELL, CELL);
        }
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
    // Desenha retângulo de seleção (marching ants simulado via 2 cores)
    if (this.selection) {
      const s = this.selection;
      // Se movendo, desenha no sel_move_pos em vez do rect original
      const rx = (this.sel_dragging === "move" && this.sel_move_pos)
        ? this.sel_move_pos.x : s.x;
      const ry = (this.sel_dragging === "move" && this.sel_move_pos)
        ? this.sel_move_pos.y : s.y;
      const px = rx * CELL + 0.5;
      const py = ry * CELL + 0.5;
      const pw = s.w * CELL;
      const ph = s.h * CELL;
      // Fundo translúcido dentro da seleção
      ctx.fillStyle = "rgba(255,210,50,0.15)";
      ctx.fillRect(px, py, pw, ph);
      // Borda dupla: preta + amarela pontilhada (efeito de marching ants)
      ctx.lineWidth = 2;
      ctx.strokeStyle = "#000";
      ctx.setLineDash([]);
      ctx.strokeRect(px, py, pw, ph);
      ctx.lineWidth = 1;
      ctx.strokeStyle = "#f0d030";
      ctx.setLineDash([4, 4]);
      ctx.lineDashOffset = (Date.now() / 100) % 8;
      ctx.strokeRect(px, py, pw, ph);
      ctx.setLineDash([]);
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
    const [x, y] = getXY(ev);
    if (editor.tool === "select") {
      // Se clicou dentro de uma seleção existente → move
      if (editor.selection && editor.is_inside_selection(x, y)) {
        editor.start_move(x, y);
      } else {
        editor.start_selection(x, y);
      }
      editor.drawing = true;
      editor.render();
      return;
    }
    editor.drawing = true;
    editor.push_history();
    editor.apply_tool(x, y);
  });
  c.addEventListener("mousemove", (ev) => {
    const [x, y] = getXY(ev);
    coord_el.textContent = `${x}, ${y}`;
    if (!editor.drawing) return;
    if (editor.tool === "select") {
      if (editor.sel_dragging === "create") {
        editor.update_selection(x, y);
      } else if (editor.sel_dragging === "move") {
        editor.update_move(x, y);
      }
      editor.render();
      return;
    }
    if (editor.tool === "brush" || editor.tool === "eraser") {
      editor.apply_tool(x, y);
    }
  });
  window.addEventListener("mouseup", () => {
    if (editor.drawing && editor.tool === "select") {
      if (editor.sel_dragging === "create") {
        editor.sel_dragging = null;
        editor.sel_start = null;
        // Se seleção ficou 1x1 e é clique único, desseleciona
        if (editor.selection && editor.selection.w === 1 && editor.selection.h === 1) {
          editor.clear_selection();
        }
      } else if (editor.sel_dragging === "move") {
        editor.commit_move();
      }
      editor.render();
    }
    editor.drawing = false;
  });
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
  document.getElementById("btn-logout")!.addEventListener("click", () => {
    set_token(null);
    location.reload();
  });

  // Clipboard operations
  document.getElementById("btn-copy")!.addEventListener("click", () => {
    if (editor.copy_selection()) console.log("[pixel] copiado");
  });
  document.getElementById("btn-cut")!.addEventListener("click", () => {
    if (editor.cut_selection()) {
      editor.render();
      console.log("[pixel] recortado");
    }
  });
  document.getElementById("btn-paste")!.addEventListener("click", () => {
    if (editor.paste_clipboard()) {
      editor.render();
      console.log("[pixel] colado");
    }
  });
  document.getElementById("btn-deselect")!.addEventListener("click", () => {
    editor.clear_selection();
    editor.render();
  });
}

function setup_keyboard() {
  window.addEventListener("keydown", (ev) => {
    if ((ev.target as HTMLElement)?.tagName === "INPUT") return;
    if ((ev.target as HTMLElement)?.tagName === "TEXTAREA") return;

    const ctrl = ev.ctrlKey || ev.metaKey;

    // Clipboard (Ctrl+C/X/V)
    if (ctrl && ev.key.toLowerCase() === "c") {
      ev.preventDefault();
      editor.copy_selection();
      return;
    }
    if (ctrl && ev.key.toLowerCase() === "x") {
      ev.preventDefault();
      if (editor.cut_selection()) editor.render();
      return;
    }
    if (ctrl && ev.key.toLowerCase() === "v") {
      ev.preventDefault();
      if (editor.paste_clipboard()) editor.render();
      return;
    }
    if (ctrl && ev.key.toLowerCase() === "z") {
      ev.preventDefault();
      editor.undo();
      return;
    }
    if (ctrl && ev.key.toLowerCase() === "a") {
      ev.preventDefault();
      editor.selection = { x: 0, y: 0, w: SIZE, h: SIZE };
      editor.render();
      return;
    }
    if (ev.key === "Escape") {
      if (editor.selection) {
        editor.clear_selection();
        editor.render();
        ev.preventDefault();
      }
      return;
    }
    // Tools
    if (ev.key === "b") set_tool("brush");
    if (ev.key === "e") set_tool("eraser");
    if (ev.key === "f") set_tool("fill");
    if (ev.key === "p" || ev.key === "i") set_tool("eyedrop");
    if (ev.key === "s") set_tool("select");
    if (ev.key === "Delete" || ev.key === "Backspace") {
      if (editor.selection) {
        editor.cut_selection();
        editor.render();
      }
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

const TOKEN_KEY = "pix_jwt";

function get_token(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}
function set_token(t: string | null) {
  if (t) localStorage.setItem(TOKEN_KEY, t);
  else   localStorage.removeItem(TOKEN_KEY);
}
function auth_headers(): Record<string, string> {
  const t = get_token();
  return t ? { Authorization: `Bearer ${t}` } : {};
}

async function api_auth(password: string): Promise<boolean> {
  const r = await fetch("/api/pixel/auth", {
    method: "POST",
    headers: {"Content-Type": "application/json"},
    body: JSON.stringify({ password }),
  });
  if (!r.ok) return false;
  const body = await r.json();
  if (body.token) set_token(body.token);
  return true;
}

async function api_verify(): Promise<boolean> {
  if (!get_token()) return false;
  const r = await fetch("/api/pixel/verify", {
    method: "POST",
    headers: { ...auth_headers() },
  });
  if (r.status === 401) { set_token(null); return false; }
  return r.ok;
}

async function api_generate(prompt: string, model: string): Promise<string[][]> {
  const r = await fetch("/api/pixel/generate", {
    method: "POST",
    headers: {"Content-Type": "application/json", ...auth_headers()},
    body: JSON.stringify({ prompt, size: SIZE, model }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error || `http ${r.status}`);
  return body.pixels;
}

async function api_generate_svg(prompt: string, model: string): Promise<string> {
  const r = await fetch("/api/pixel/generate-svg", {
    method: "POST",
    headers: {"Content-Type": "application/json", ...auth_headers()},
    body: JSON.stringify({ prompt, size: SIZE, model }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error || `http ${r.status}`);
  return body.svg as string;
}

/// Normaliza o SVG: extrai o bloco <svg>..</svg> e garante xmlns/width/height
/// que sao necessarios pro browser carregar como imagem.
function sanitize_svg(svg_text: string): string {
  let svg = svg_text.trim();
  // Tira markdown fences, lixo antes/depois
  svg = svg.replace(/```(svg|xml|html)?/gi, "");
  const start = svg.indexOf("<svg");
  const end = svg.lastIndexOf("</svg>");
  if (start !== -1 && end !== -1 && end > start) {
    svg = svg.slice(start, end + "</svg>".length);
  }
  // Desescape se vier com \" escapado
  if (svg.includes('\\"')) svg = svg.replace(/\\"/g, '"');
  if (svg.includes("\\/")) svg = svg.replace(/\\\//g, "/");
  // Garante xmlns
  if (!/\sxmlns=/.test(svg)) {
    svg = svg.replace(/<svg\b/i, `<svg xmlns="http://www.w3.org/2000/svg"`);
  }
  // Garante viewBox
  if (!/\sviewBox=/.test(svg)) {
    svg = svg.replace(/<svg\b/i, `<svg viewBox="0 0 ${SIZE} ${SIZE}"`);
  }
  // Garante width/height explicitos (alguns browsers se recusam sem isso)
  if (!/\swidth=/.test(svg))  svg = svg.replace(/<svg\b/i, `<svg width="${SIZE}"`);
  if (!/\sheight=/.test(svg)) svg = svg.replace(/<svg\b/i, `<svg height="${SIZE}"`);
  return svg;
}

/// Rasteriza SVG string num canvas SIZExSIZE e extrai matriz de hex.
async function rasterize_svg_to_matrix(svg_text: string): Promise<string[][]> {
  const svg = sanitize_svg(svg_text);
  if (!svg.includes("</svg>")) {
    throw new Error("SVG truncado (sem </svg>). O modelo estourou limite de tokens — tenta 2.5-flash ou 64×64.");
  }
  const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  try {
    const img = new Image();
    img.crossOrigin = "anonymous";
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = (ev) => {
        console.error("[pixel] svg load err. content:", svg.slice(0, 500));
        reject(new Error(`falha ao carregar SVG (${ev}). Ver console pro SVG bruto.`));
      };
      img.src = url;
    });
    const c = document.createElement("canvas");
    c.width = SIZE; c.height = SIZE;
    const ctx = c.getContext("2d")!;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(img, 0, 0, SIZE, SIZE);
    const data = ctx.getImageData(0, 0, SIZE, SIZE).data;
    const to_hex = (n: number) => n.toString(16).padStart(2, "0");
    const pixels: string[][] = [];
    for (let y = 0; y < SIZE; y++) {
      const row: string[] = [];
      for (let x = 0; x < SIZE; x++) {
        const i = (y * SIZE + x) * 4;
        const r = data[i], g = data[i+1], b = data[i+2], a = data[i+3];
        if (a < 10) row.push("#00000000");
        else row.push(`#${to_hex(r)}${to_hex(g)}${to_hex(b)}${to_hex(a)}`);
      }
      pixels.push(row);
    }
    return pixels;
  } finally {
    URL.revokeObjectURL(url);
  }
}

/// Rasteriza uma imagem (PNG/JPG/SVG) ja carregada num objeto File/Blob.
async function rasterize_file_to_matrix(file: File): Promise<string[][]> {
  const name = file.name.toLowerCase();
  if (name.endsWith(".svg") || file.type.includes("svg")) {
    const txt = await file.text();
    return await rasterize_svg_to_matrix(txt);
  }
  // PNG/JPG/etc: desenha e extrai
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error(`falha ao carregar ${file.name}`));
      img.src = url;
    });
    const c = document.createElement("canvas");
    c.width = SIZE; c.height = SIZE;
    const ctx = c.getContext("2d")!;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(img, 0, 0, SIZE, SIZE);
    const data = ctx.getImageData(0, 0, SIZE, SIZE).data;
    const to_hex = (n: number) => n.toString(16).padStart(2, "0");
    const pixels: string[][] = [];
    for (let y = 0; y < SIZE; y++) {
      const row: string[] = [];
      for (let x = 0; x < SIZE; x++) {
        const i = (y * SIZE + x) * 4;
        const r = data[i], g = data[i+1], b = data[i+2], a = data[i+3];
        if (a < 10) row.push("#00000000");
        else row.push(`#${to_hex(r)}${to_hex(g)}${to_hex(b)}${to_hex(a)}`);
      }
      pixels.push(row);
    }
    return pixels;
  } finally {
    URL.revokeObjectURL(url);
  }
}

async function api_save(name: string, pixels: string[][]): Promise<{txt: string; png: string}> {
  const r = await fetch("/api/pixel/save", {
    method: "POST",
    headers: {"Content-Type": "application/json", ...auth_headers()},
    body: JSON.stringify({ name, pixels }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error || `http ${r.status}`);
  return { txt: body.saved_txt, png: body.saved_png };
}

async function api_load(name: string): Promise<string[][]> {
  const r = await fetch("/api/pixel/load", {
    method: "POST",
    headers: {"Content-Type": "application/json", ...auth_headers()},
    body: JSON.stringify({ name }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error || `http ${r.status}`);
  return body.pixels as string[][];
}

interface SpriteEntry { name: string; has_txt: boolean; has_png: boolean; }
async function api_list(): Promise<SpriteEntry[]> {
  const r = await fetch("/api/pixel/list", {
    method: "POST",
    headers: { ...auth_headers() },
  });
  if (!r.ok) return [];
  const body = await r.json();
  return body.sprites as SpriteEntry[];
}

function refresh_list() {
  api_list().then(sprites => {
    const ul = document.getElementById("sprites-list")!;
    ul.innerHTML = "";
    sprites.forEach(s => {
      const li = document.createElement("li");
      li.className = "sprite-item";
      // Label: nome + tags de formato
      const label = document.createElement("span");
      label.className = "sprite-label";
      label.textContent = s.name;
      const tags = document.createElement("span");
      tags.className = "sprite-tags";
      if (s.has_txt) {
        const t = document.createElement("em"); t.className = "tag t-txt"; t.textContent = "TXT";
        tags.appendChild(t);
      }
      if (s.has_png) {
        const t = document.createElement("em"); t.className = "tag t-png"; t.textContent = "PNG";
        tags.appendChild(t);
      }
      li.appendChild(label);
      li.appendChild(tags);
      // Click no item carrega no canvas
      if (s.has_txt) {
        li.style.cursor = "pointer";
        li.title = "Clique pra carregar no canvas";
        li.addEventListener("click", async () => {
          try {
            const pixels = await api_load(s.name);
            editor.load_pixels(pixels);
            const name_el = document.getElementById("sprite-name") as HTMLInputElement;
            name_el.value = s.name;
            const st = document.getElementById("save-status")!;
            st.className = "status ok";
            st.textContent = `carregado ${s.name}.txt`;
          } catch (e: any) {
            console.error("[pixel] load err:", e);
          }
        });
      }
      ul.appendChild(li);
    });
  });
}

// ────────────── Wire IA / Save / Auth ──────────────

function setup_ia() {
  const btn       = document.getElementById("btn-generate") as HTMLButtonElement;
  const btn_svg   = document.getElementById("btn-generate-svg") as HTMLButtonElement;
  const prompt_el = document.getElementById("ia-prompt") as HTMLTextAreaElement;
  const status    = document.getElementById("ia-status")!;

  /// Fluxo compartilhado: mostra contador, chama getter, aplica no editor.
  const run = async (
    label: string,
    fn: () => Promise<string[][]>,
  ) => {
    const p = prompt_el.value.trim();
    if (!p) { status.textContent = "descreva o sprite"; status.className = "status err"; return; }
    btn.disabled = true;
    btn_svg.disabled = true;
    status.className = "status";
    const t0 = Date.now();
    const tick = setInterval(() => {
      const s = Math.floor((Date.now() - t0) / 1000);
      status.textContent = `${label}... ${s}s`;
    }, 500);
    try {
      const pixels = await fn();
      editor.load_pixels(pixels);
      status.className = "status ok";
      const s = Math.floor((Date.now() - t0) / 1000);
      status.textContent = `pronto em ${s}s (${label})`;
    } catch (e: any) {
      status.className = "status err";
      status.textContent = e.message || "erro";
    } finally {
      clearInterval(tick);
      btn.disabled = false;
      btn_svg.disabled = false;
    }
  };

  btn.addEventListener("click", async () => {
    if (SIZE >= 128) {
      status.className = "status err";
      status.textContent = "matriz nao suporta 128×128 (use SVG) — limite do Gemini";
      return;
    }
    const model = (document.getElementById("ia-model") as HTMLSelectElement).value;
    const p = prompt_el.value.trim();
    await run(`matriz/${model}`, () => api_generate(p, model));
  });

  btn_svg.addEventListener("click", async () => {
    const model = (document.getElementById("ia-model") as HTMLSelectElement).value;
    const p = prompt_el.value.trim();
    await run(`svg/${model}`, async () => {
      const svg = await api_generate_svg(p, model);
      return await rasterize_svg_to_matrix(svg);
    });
  });

  // Dropdown de tamanho: recria canvas
  const size_el = document.getElementById("ia-size") as HTMLSelectElement;
  size_el.addEventListener("change", () => {
    const new_size = parseInt(size_el.value, 10);
    if (new_size === SIZE) return;
    if (!confirm(`Trocar pra ${new_size}×${new_size}? O canvas atual sera limpo.`)) {
      size_el.value = String(SIZE);
      return;
    }
    editor.resize_to(new_size);
  });
}

function setup_upload() {
  const btn = document.getElementById("btn-upload") as HTMLButtonElement;
  const input = document.getElementById("upload-input") as HTMLInputElement;
  const status = document.getElementById("upload-status")!;
  btn.addEventListener("click", () => input.click());
  input.addEventListener("change", async () => {
    const file = input.files?.[0];
    if (!file) return;
    status.className = "status";
    status.textContent = `processando ${file.name}...`;
    try {
      const pixels = await rasterize_file_to_matrix(file);
      editor.load_pixels(pixels);
      status.className = "status ok";
      status.textContent = `importado: ${file.name} (${Math.round(file.size / 1024)}KB)`;
      // Sugere nome a partir do arquivo
      const name_el = document.getElementById("sprite-name") as HTMLInputElement;
      if (!name_el.value) {
        const base = file.name.replace(/\.[^.]+$/, "").replace(/[^a-z0-9_-]/gi, "_");
        name_el.value = base;
      }
    } catch (e: any) {
      console.error("[pixel] upload err:", e);
      status.className = "status err";
      status.textContent = e.message || "erro";
    } finally {
      input.value = "";
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
      const res = await api_save(name, editor.grid);
      status.className = "status ok";
      status.textContent = `salvo: ${res.txt.split("/").slice(-1)[0]} + ${res.png.split("/").slice(-1)[0]}`;
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
  console.log("[pixel] setup_auth called");
  const btn = document.getElementById("auth-btn") as HTMLButtonElement | null;
  const input = document.getElementById("auth-password") as HTMLInputElement | null;
  const err = document.getElementById("auth-err");
  if (!btn || !input) { console.error("[pixel] auth elements not found"); return; }
  console.log("[pixel] button listener attached");
  const do_submit = async () => {
    console.log("[pixel] auth click");
    const pw = input.value;
    console.log("[pixel] pw length:", pw?.length);
    if (err) err.textContent = "tentando...";
    try {
      const ok = await api_auth(pw);
      console.log("[pixel] auth result:", ok);
      if (ok) {
        document.getElementById("auth-gate")!.hidden = true;
        document.getElementById("editor")!.hidden = false;
        boot_editor();
      } else {
        if (err) err.textContent = "senha invalida";
      }
    } catch (e: any) {
      console.error("[pixel] auth fetch err:", e);
      if (err) err.textContent = "erro: " + (e?.message || String(e));
    }
  };
  btn.addEventListener("click", do_submit);
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") do_submit();
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
  setup_upload();
  refresh_list();
  // Animacao das "marching ants" da seleção
  setInterval(() => {
    if (editor.selection) editor.render();
  }, 120);
}

// Init: valida token existente (se tiver). Token OK -> boot direto;
// caso contrario mostra tela de login.
console.log("[pixel] boot script");
async function boot() {
  if (await api_verify()) {
    console.log("[pixel] token valido, boot direto");
    document.getElementById("auth-gate")!.hidden = true;
    document.getElementById("editor")!.hidden = false;
    boot_editor();
  } else {
    console.log("[pixel] sem token, mostrando login");
    setup_auth();
  }
}
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", boot);
} else {
  boot();
}

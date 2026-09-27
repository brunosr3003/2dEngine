// Visao de espectador: mesma geometria de camera e mesmo relevo do cliente.
// Os modelos de entidades sao marcadores simples; o retrato administrativo nao
// transporta animacoes, vestuario nem o estado completo da cena do cliente.
const cameraCanvas = document.getElementById('camera3d');
const cameraControles = document.getElementById('camera-controles');
const cameraAviso = document.getElementById('camera-aviso');
let cameraGl = null;
let cameraPrograma = null;
let cameraBuffer = null;
let cameraQtdTerreno = 0;
let cameraMalha = null;
let cameraCarregando = false;
let cameraPedido = 0;
let cameraAtiva = false;
let cameraYaw = 0;
let cameraZoom = 1;
let cameraPitch = Math.atan2(14, 10);
let cameraArrasto = null;
let cameraCentro = null;
let cameraQuadroAnterior = 0;

function cameraErro(msg) {
  cameraAviso.textContent = msg;
  cameraAviso.classList.toggle('ativo', Boolean(msg));
}

function cameraShader(gl, tipo, fonte) {
  const shader = gl.createShader(tipo);
  gl.shaderSource(shader, fonte);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
  return shader;
}

function prepararCamera() {
  if (cameraGl) return true;
  const gl = cameraCanvas.getContext('webgl2', { antialias: true, alpha: false, preserveDrawingBuffer: true });
  if (!gl) { cameraErro('Este navegador não oferece WebGL 2 para a visão 3D.'); return false; }
  try {
    const vertice = cameraShader(gl, gl.VERTEX_SHADER, `#version 300 es
      in vec3 aPos; in vec3 aCor;
      uniform mat4 uProj; uniform mat4 uView;
      out vec3 vCor; out float vDist;
      void main() {
        vec4 vista = uView * vec4(aPos, 1.0);
        vDist = length(vista.xyz);
        vCor = aCor;
        gl_Position = uProj * vista;
      }`);
    const fragmento = cameraShader(gl, gl.FRAGMENT_SHADER, `#version 300 es
      precision mediump float;
      in vec3 vCor; in float vDist;
      out vec4 cor;
      void main() {
        float nevoa = smoothstep(65.0, 125.0, vDist);
        cor = vec4(mix(vCor, vec3(0.48, 0.62, 0.72), nevoa), 1.0);
      }`);
    const programa = gl.createProgram();
    gl.attachShader(programa, vertice); gl.attachShader(programa, fragmento);
    gl.linkProgram(programa);
    if (!gl.getProgramParameter(programa, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(programa));
    cameraPrograma = {
      p: programa,
      pos: gl.getAttribLocation(programa, 'aPos'),
      cor: gl.getAttribLocation(programa, 'aCor'),
      proj: gl.getUniformLocation(programa, 'uProj'),
      view: gl.getUniformLocation(programa, 'uView'),
    };
    cameraBuffer = gl.createBuffer();
    gl.enable(gl.DEPTH_TEST);
    gl.disable(gl.CULL_FACE);
    cameraGl = gl;
    return true;
  } catch (e) {
    cameraErro('Falha ao iniciar a câmera 3D: ' + e.message);
    return false;
  }
}

function fecharCamera() {
  cameraAtiva = false;
  cameraPedido++;
  cameraCanvas.classList.remove('ativa');
  cameraControles.classList.remove('ativo');
  cameraErro('');
  document.getElementById('hud').hidden = false;
}
document.getElementById('camera-fechar').onclick = () => { fecharCamera(); seguindo = null; };

function abrirCamera() {
  if (!seguindo || !prepararCamera()) return;
  cameraAtiva = true;
  cameraCanvas.classList.add('ativa');
  cameraControles.classList.add('ativo');
  document.getElementById('hud').hidden = true;
  cameraMalha = null;
  cameraQtdTerreno = 0;
  cameraCentro = null;
  cameraErro('Carregando terreno do jogo…');
}

function cameraAltura(p) {
  if (!cameraMalha) return p.y || 0;
  const { x, z, lado, passo, alturas } = cameraMalha;
  const ix = Math.round((p.x - x) / passo), iz = Math.round((p.z - z) / passo);
  return ix >= 0 && iz >= 0 && ix < lado && iz < lado ? alturas[iz * lado + ix] : (p.y || 0);
}

async function carregarTerreno(r, j) {
  if (cameraCarregando) return;
  cameraCarregando = true;
  const pedido = ++cameraPedido;
  const zona = r.zona;
  try {
    const url = BASE + 'api/terreno/' + encodeURIComponent(zona) +
      '?x=' + encodeURIComponent(j.x.toFixed(1)) + '&z=' + encodeURIComponent(j.z.toFixed(1));
    const resp = await fetch(url, { credentials: 'same-origin', cache: 'no-store' });
    if (!resp.ok) throw new Error('terreno indisponível (' + resp.status + ')');
    const dados = await resp.json();
    if (pedido !== cameraPedido || !cameraAtiva || retrato()?.zona !== zona) return;
    cameraMalha = { ...dados, zona };
    cameraQtdTerreno = 0;
    const vertices = [];
    const { x, z, passo, lado, alturas, cores } = dados;
    const vertice = (ix, iz, brilho) => {
      const i = iz * lado + ix;
      const cor = cores[i];
      vertices.push(x + ix * passo, alturas[i], z + iz * passo,
        cor[0] / 255 * brilho, cor[1] / 255 * brilho, cor[2] / 255 * brilho);
    };
    for (let iz = 0; iz < lado - 1; iz++) for (let ix = 0; ix < lado - 1; ix++) {
      const i = iz * lado + ix;
      const brilho = Math.max(.56, Math.min(1.18,
        .88 + (alturas[i] - alturas[i + lado + 1]) * .045));
      vertice(ix, iz, brilho); vertice(ix + 1, iz, brilho); vertice(ix, iz + 1, brilho);
      vertice(ix + 1, iz, brilho); vertice(ix + 1, iz + 1, brilho); vertice(ix, iz + 1, brilho);
    }
    const gl = cameraGl;
    gl.bindBuffer(gl.ARRAY_BUFFER, cameraBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices.length * 4 + 2_000_000, gl.DYNAMIC_DRAW);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Float32Array(vertices));
    cameraQtdTerreno = vertices.length / 6;
    cameraErro('');
  } catch (e) {
    if (pedido === cameraPedido) cameraErro('Não foi possível carregar a cena: ' + e.message);
  } finally {
    cameraCarregando = false;
  }
}

function cameraVista(eye, alvo) {
  const norm = a => { const n = Math.hypot(...a) || 1; return a.map(v => v / n); };
  const cross = (a, b) => [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]];
  const dot = (a, b) => a[0]*b[0]+a[1]*b[1]+a[2]*b[2];
  const f = norm(alvo.map((v, i) => v - eye[i]));
  const direita = norm(cross(f, [0, 1, 0]));
  const cima = cross(direita, f);
  return new Float32Array([
    direita[0], cima[0], -f[0], 0,
    direita[1], cima[1], -f[1], 0,
    direita[2], cima[2], -f[2], 0,
    -dot(direita, eye), -dot(cima, eye), dot(f, eye), 1,
  ]);
}

function cameraProjecao(aspecto) {
  const f = 1 / Math.tan(Math.PI / 8), perto = .01, longe = 1000;
  return new Float32Array([
    f / aspecto, 0, 0, 0, 0, f, 0, 0,
    0, 0, (longe + perto) / (perto - longe), -1,
    0, 0, 2 * longe * perto / (perto - longe), 0,
  ]);
}

function cameraCaixa(vertices, x, y, z, largura, altura, cor) {
  const a = x - largura / 2, b = x + largura / 2;
  const c = y, d = y + altura;
  const e = z - largura / 2, f = z + largura / 2;
  const faces = [
    [[a,c,e],[b,c,e],[b,d,e],[a,d,e], .72],
    [[b,c,f],[a,c,f],[a,d,f],[b,d,f], .91],
    [[b,c,e],[b,c,f],[b,d,f],[b,d,e], .80],
    [[a,c,f],[a,c,e],[a,d,e],[a,d,f], .65],
    [[a,d,e],[b,d,e],[b,d,f],[a,d,f], 1.12],
  ];
  for (const [v0,v1,v2,v3,luz] of faces) {
    for (const v of [v0,v1,v2,v0,v2,v3]) vertices.push(...v, ...cor.map(c => c*luz));
  }
}

function desenharCamera() {
  requestAnimationFrame(desenharCamera);
  if (!cameraAtiva || abaAtual !== 'mundo' || !cameraGl) return;
  const r = retrato();
  const j = r?.jogadores.find(j => j.id === seguindo?.id);
  if (!j || canalAtivo !== seguindo.canal) {
    cameraErro('Jogador saiu deste canal.');
    fecharCamera();
    return;
  }
  document.getElementById('camera-nome').textContent = j.nome + ' · ' + r.zona +
    (j.instancia ? ' · instância' : '');
  if (j.instancia) {
    cameraErro('Cena da instância indisponível nesta visão de espectador.');
    return;
  }
  if (!cameraMalha || r.zona !== cameraMalha.zona ||
      Math.abs(j.x - (cameraMalha.x + 64)) > 24 ||
      Math.abs(j.z - (cameraMalha.z + 64)) > 24) {
    if (!cameraCarregando) carregarTerreno(r, j);
  }
  if (!cameraQtdTerreno) return;
  const gl = cameraGl, p = cameraPrograma;
  const agora = performance.now();
  const dt = Math.min(.1, Math.max(0, (agora - cameraQuadroAnterior) / 1000));
  cameraQuadroAnterior = agora;
  if (!cameraCentro) cameraCentro = { x: j.x, z: j.z, y: j.y };
  const seguimento = 1 - Math.exp(-12 * dt);
  cameraCentro.x += (j.x - cameraCentro.x) * seguimento;
  cameraCentro.z += (j.z - cameraCentro.z) * seguimento;
  cameraCentro.y += (j.y - cameraCentro.y) * seguimento;
  const dpr = Math.min(devicePixelRatio || 1, 2);
  const largura = Math.max(1, Math.round(cameraCanvas.clientWidth * dpr));
  const altura = Math.max(1, Math.round(cameraCanvas.clientHeight * dpr));
  if (cameraCanvas.width !== largura || cameraCanvas.height !== altura) {
    cameraCanvas.width = largura; cameraCanvas.height = altura;
  }
  gl.viewport(0, 0, largura, altura);
  gl.clearColor(.48, .62, .72, 1);
  gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
  gl.useProgram(p.p);
  const apoio = Number.isFinite(cameraCentro.y) ? cameraCentro.y : cameraAltura(j);
  const distancia = Math.hypot(14, 10) * cameraZoom;
  const recuo = distancia * Math.cos(cameraPitch), alto = distancia * Math.sin(cameraPitch);
  const olho = [cameraCentro.x - Math.sin(cameraYaw) * recuo, apoio + alto,
    cameraCentro.z + Math.cos(cameraYaw) * recuo];
  gl.uniformMatrix4fv(p.proj, false, cameraProjecao(largura / altura));
  gl.uniformMatrix4fv(p.view, false, cameraVista(olho, [cameraCentro.x, apoio, cameraCentro.z]));
  gl.bindBuffer(gl.ARRAY_BUFFER, cameraBuffer);
  gl.enableVertexAttribArray(p.pos); gl.enableVertexAttribArray(p.cor);
  gl.vertexAttribPointer(p.pos, 3, gl.FLOAT, false, 24, 0);
  gl.vertexAttribPointer(p.cor, 3, gl.FLOAT, false, 24, 12);
  gl.drawArrays(gl.TRIANGLES, 0, cameraQtdTerreno);
  const entidades = [];
  for (const outro of r.jogadores) {
    if (Math.hypot(outro.x-j.x, outro.z-j.z) > 70) continue;
    cameraCaixa(entidades, outro.x, outro.y, outro.z, .55, 1.65,
      outro.id === j.id ? [.32,.68,1] : [.56,.76,.98]);
  }
  for (const mob of r.mobs) {
    if (mob.estado === 'morto' || Math.hypot(mob.x-j.x, mob.z-j.z) > 70) continue;
    cameraCaixa(entidades, mob.x, mob.y, mob.z, mob.chefe ? 1.5 : .8,
      mob.chefe ? 2.4 : 1.15, mob.chefe ? [.95,.75,.18] : [.85,.38,.32]);
  }
  gl.bufferSubData(gl.ARRAY_BUFFER, cameraQtdTerreno * 24, new Float32Array(entidades));
  gl.drawArrays(gl.TRIANGLES, cameraQtdTerreno, entidades.length / 6);
}
requestAnimationFrame(desenharCamera);

cameraCanvas.addEventListener('pointerdown', e => {
  cameraArrasto = { x: e.clientX, y: e.clientY, yaw: cameraYaw, pitch: cameraPitch };
  cameraCanvas.setPointerCapture(e.pointerId);
});
cameraCanvas.addEventListener('pointermove', e => {
  if (!cameraArrasto) return;
  cameraYaw = cameraArrasto.yaw + (e.clientX - cameraArrasto.x) * .008;
  cameraPitch = Math.max(.471239, Math.min(1.308997,
    cameraArrasto.pitch + (e.clientY - cameraArrasto.y) * .005));
});
cameraCanvas.addEventListener('pointerup', () => { cameraArrasto = null; });
cameraCanvas.addEventListener('pointercancel', () => { cameraArrasto = null; });
cameraCanvas.addEventListener('wheel', e => {
  e.preventDefault();
  cameraZoom = Math.max(.55, Math.min(1.5, cameraZoom * (e.deltaY > 0 ? 1.1 : 1 / 1.1)));
  const t = (cameraZoom - .55) / .95;
  const piso = .471239 + (.593412 - .471239) * t;
  cameraPitch = piso + (1.308997 - piso) * (.18 + .34 * t);
}, { passive: false });

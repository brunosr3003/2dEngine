import { useEffect, useRef, useState } from 'react';
import { auth } from './api';

type Props = {
  path: string | null;
  col: number;
  row: number;
  onPath: (p: string | null) => void;
  onGrid: (c: number, r: number) => void;
};

let _iconsCache: { configured: boolean; icons: string[]; prefix: string } | null = null;

async function fetchIcons() {
  if (_iconsCache) return _iconsCache;
  const r = await fetch('/api/econ/icons', { headers: { 'Authorization': 'Bearer ' + auth.get() } });
  const d = await r.json();
  _iconsCache = {
    configured: !!d.configured,
    icons: d.icons || [],
    prefix: (d.resources_prefix as string) || 'Items',
  };
  return _iconsCache;
}

/// Picker de ícone: lista PNGs em `ICONS_DIR` (config no server) com
/// thumbnails clicáveis. Cai pra drop-zone manual + col/row se a pasta não
/// estiver configurada.
export function IconPicker({ path, col, row, onPath, onGrid }: Props) {
  const [previewBlob, setPreviewBlob] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const [icons, setIcons] = useState<string[]>([]);
  const [configured, setConfigured] = useState(false);
  const [prefix, setPrefix] = useState<string>('Items');
  const [filter, setFilter] = useState('');
  const [showGrid, setShowGrid] = useState(true);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    fetchIcons().then(({ configured, icons, prefix }) => {
      setConfigured(configured); setIcons(icons); setPrefix(prefix);
    }).catch(() => {});
  }, []);

  useEffect(() => () => {
    if (previewBlob) URL.revokeObjectURL(previewBlob);
  }, [previewBlob]);

  const onFile = (file: File) => {
    if (!/\.png$/i.test(file.name)) {
      alert('Só PNG. Salva o sprite em MMORPG/Assets/_Project/Resources/Items/ antes.');
      return;
    }
    const base = file.name.replace(/\.png$/i, '');
    onPath(`${prefix}/${base}`);
    if (previewBlob) URL.revokeObjectURL(previewBlob);
    setPreviewBlob(URL.createObjectURL(file));
  };

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const f = e.dataTransfer.files?.[0];
    if (f) onFile(f);
  };

  // basename derivado do path atual ("Icons/sliced/sword" → "sword") pra highlight.
  // Aceita tanto o prefixo do server quanto o legado "Items/" pra retro-compat.
  const currentBase = (() => {
    if (!path) return null;
    if (prefix && path.startsWith(prefix + '/')) return path.slice(prefix.length + 1);
    if (path.startsWith('Items/')) return path.slice('Items/'.length);
    return null;
  })();

  const filtered = filter
    ? icons.filter(n => n.toLowerCase().includes(filter.toLowerCase()))
    : icons;

  // URL pra thumbnail (server retorna PNG bytes com cache)
  const thumbUrl = (name: string) => `/api/econ/icons/${encodeURIComponent(name)}?t=${auth.get().slice(-8)}`;

  return (
    <div className="col">
      <div className="grid2" style={{ alignItems: 'start' }}>
        <div className="col">
          <label>Path (Resources)
            <input
              value={path ?? ''}
              placeholder='ex: Items/sword (vazio = usa col/row)'
              onChange={(e) => onPath(e.target.value || null)}
            />
          </label>
          <div
            className={'drop-zone' + (dragging ? ' dragging' : '')}
            onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
            onDragLeave={() => setDragging(false)}
            onDrop={onDrop}
            onClick={() => fileRef.current?.click()}
          >
            Arrasta um PNG aqui<br />
            <span className="muted">(ou clica pra selecionar)</span>
          </div>
          <input
            ref={fileRef}
            type="file"
            accept="image/png"
            style={{ display: 'none' }}
            onChange={(e) => { const f = e.target.files?.[0]; if (f) onFile(f); }}
          />
          <div className="row" style={{ gap: 8 }}>
            <label style={{ flex: 1 }}>Icon Col (fallback)
              <input type="number" min={0} max={15} value={col}
                onChange={(e) => onGrid(Number(e.target.value || 0), row)} />
            </label>
            <label style={{ flex: 1 }}>Icon Row (fallback)
              <input type="number" min={0} max={136} value={row}
                onChange={(e) => onGrid(col, Number(e.target.value || 0))} />
            </label>
          </div>
        </div>
        <div className="col" style={{ alignItems: 'center' }}>
          <div className="muted" style={{ fontSize: 11 }}>Preview</div>
          <div className="icon-preview">
            {previewBlob
              ? <img src={previewBlob} alt="" />
              : currentBase
                ? <img src={thumbUrl(currentBase)} alt="" onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }} />
                : <span className="muted small">col {col}, row {row}<br/>(spritesheet)</span>}
          </div>
        </div>
      </div>

      {configured && (
        <div className="icon-grid-wrap">
          <div className="row" style={{ gap: 8, marginBottom: 6, alignItems: 'center' }}>
            <strong style={{ fontSize: 12 }}>Sprites disponíveis ({icons.length})</strong>
            <input
              placeholder="filtrar..."
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              style={{ width: 200 }}
            />
            <button className="ghost" type="button" onClick={() => setShowGrid(!showGrid)}>
              {showGrid ? 'Ocultar' : 'Mostrar'}
            </button>
          </div>
          {showGrid && (
            <div className="icon-grid">
              {filtered.map(name => (
                <button
                  key={name}
                  type="button"
                  className={'icon-cell-btn' + (currentBase === name ? ' selected' : '')}
                  title={name}
                  onClick={() => onPath(`${prefix}/${name}`)}
                >
                  <img src={thumbUrl(name)} alt={name} />
                  <span>{name}</span>
                </button>
              ))}
              {filtered.length === 0 && <span className="muted small">nenhum sprite encontrado</span>}
            </div>
          )}
        </div>
      )}
      {!configured && (
        <div className="muted small" style={{ marginTop: 6 }}>
          Picker desabilitado: <code>ICONS_DIR</code> não configurado no servidor.
          Use drop-zone ou digite o path manualmente.
        </div>
      )}
    </div>
  );
}

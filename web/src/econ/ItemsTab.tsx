import { useMemo, useState } from 'react';
import { api, auth } from './api';
import { Modal } from './Modal';
import { IconPicker } from './IconPicker';
import { type Item, SLOTS } from './types';

type Props = { items: Item[]; reload: () => Promise<void> };

const EMPTY: Item = {
  id: 0, name: '', sell_price: 0, buy_price: null, shop_order: null, stack_max: 1,
  equip_slot: null, item_level: 1, icon_col: 0, icon_row: 0, icon_path: null,
  active: true,
  hp_min: 0, hp_max: 0, mp_min: 0, mp_max: 0, atk_min: 0, atk_max: 0,
  def_min: 0, def_max: 0, dex_min: 0, dex_max: 0, wis_min: 0, wis_max: 0,
};

export function ItemsTab({ items, reload }: Props) {
  const [filter, setFilter] = useState('');
  const [editing, setEditing] = useState<Item | null>(null);
  const [isNew, setIsNew] = useState(false);

  const filtered = useMemo(() => {
    const f = filter.trim().toLowerCase();
    if (!f) return items;
    return items.filter(it =>
      String(it.id).includes(f) || it.name.toLowerCase().includes(f)
    );
  }, [items, filter]);

  const range = (a: number, b: number) => (a || b) ? `${a}-${b}` : '—';

  const openNew = () => {
    const nextId = items.length === 0 ? 100 : Math.max(...items.map(it => it.id)) + 1;
    setEditing({ ...EMPTY, id: nextId });
    setIsNew(true);
  };

  const openEdit = (it: Item) => { setEditing({ ...it }); setIsNew(false); };

  const onDelete = async (it: Item) => {
    if (!confirm(`Remover item ${it.id} — ${it.name}?`)) return;
    try {
      await api('DELETE', '/items/' + it.id);
      await reload();
    } catch (e) { alert((e as Error).message); }
  };

  const toggleActive = async (it: Item) => {
    const next: Item = { ...it, active: !it.active };
    try {
      await api('PUT', '/items/' + it.id, next);
      await reload();
    } catch (e) { alert((e as Error).message); }
  };

  return (
    <section>
      <div className="toolbar">
        <input
          placeholder="buscar id ou nome..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <button className="primary" onClick={openNew}>+ Novo Item</button>
        <span className="muted">{filtered.length} / {items.length} itens</span>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>ID</th><th>Nome</th><th></th><th>Status</th><th>Slot</th><th>Lvl</th>
              <th>Sell</th><th>Buy</th><th>Stack</th>
              <th>Icon</th>
              <th>HP</th><th>MP</th><th>ATK</th><th>DEF</th><th>DEX</th><th>WIS</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map(it => (
              <tr key={it.id} className={it.active ? '' : 'row-inactive'}>
                <td>{it.id}</td>
                <td>{it.name}</td>
                <td><ItemThumb path={it.icon_path} /></td>
                <td>
                  <button
                    className={'badge-toggle ' + (it.active ? 'on' : 'off')}
                    onClick={() => toggleActive(it)}
                    title="Click pra alternar"
                  >{it.active ? 'Ativo' : 'Inativo'}</button>
                </td>
                <td>{it.equip_slot ? <span className="badge">{it.equip_slot}</span> : <span className="muted">—</span>}</td>
                <td>{it.item_level}</td>
                <td>{it.sell_price}</td>
                <td>{it.buy_price ?? <span className="muted">—</span>}</td>
                <td>{it.stack_max}</td>
                <td className="icon-cell">{it.icon_col},{it.icon_row}</td>
                <td>{range(it.hp_min, it.hp_max)}</td>
                <td>{range(it.mp_min, it.mp_max)}</td>
                <td>{range(it.atk_min, it.atk_max)}</td>
                <td>{range(it.def_min, it.def_max)}</td>
                <td>{range(it.dex_min, it.dex_max)}</td>
                <td>{range(it.wis_min, it.wis_max)}</td>
                <td>
                  <button onClick={() => openEdit(it)}>Editar</button>
                  <button className="danger" onClick={() => onDelete(it)}>×</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ItemEditor
        editing={editing}
        isNew={isNew}
        onClose={() => setEditing(null)}
        onSaved={async () => { setEditing(null); await reload(); }}
      />
    </section>
  );
}

function ItemThumb({ path }: { path: string | null }) {
  if (!path) return <span className="muted small">—</span>;
  // basename é o que o backend serve (ele resolve dentro do ICONS_DIR).
  const base = path.split('/').pop() || path;
  // Token entra como cache-buster — quando você reloga, miniaturas atualizam.
  const url = `/api/econ/icons/${encodeURIComponent(base)}?t=${auth.get().slice(-8)}`;
  return (
    <img
      className="row-thumb"
      src={url}
      alt={base}
      title={path}
      onError={(e) => { (e.target as HTMLImageElement).style.opacity = '0'; }}
    />
  );
}

type EditorProps = {
  editing: Item | null;
  isNew: boolean;
  onClose: () => void;
  onSaved: () => Promise<void>;
};

function ItemEditor({ editing, isNew, onClose, onSaved }: EditorProps) {
  const [draft, setDraft] = useState<Item | null>(null);
  const [err, setErr] = useState('');
  const [saving, setSaving] = useState(false);

  // Sync draft when editing changes (open/close)
  useEffectSync(editing, setDraft, setErr);

  if (!editing || !draft) return null;
  const set = <K extends keyof Item>(k: K, v: Item[K]) => setDraft({ ...draft, [k]: v });

  const onSave = async () => {
    setErr('');
    if (!draft.name.trim()) { setErr('nome obrigatório'); return; }
    if (draft.id <= 0) { setErr('id inválido'); return; }
    setSaving(true);
    try {
      const payload = { ...draft };
      if (isNew) await api('POST', '/items', payload);
      else       await api('PUT',  '/items/' + draft.id, payload);
      await onSaved();
    } catch (e) { setErr((e as Error).message); }
    finally { setSaving(false); }
  };

  const N = (k: keyof Item, label: string) => (
    <label>{label}
      <input
        type="number"
        value={draft[k] as number ?? 0}
        onChange={(e) => set(k, (e.target.value === '' ? 0 : Number(e.target.value)) as Item[typeof k])}
      />
    </label>
  );

  return (
    <Modal open title={isNew ? 'Novo Item' : `Editar #${draft.id} — ${draft.name}`}
      err={err} saving={saving} onClose={onClose} onSave={onSave}>
      <div className="grid3">
        <label>ID{isNew ? '' : ' (não editável)'}
          <input type="number" disabled={!isNew} value={draft.id}
            onChange={(e) => set('id', Number(e.target.value || 0))} />
        </label>
        <label>Nome
          <input value={draft.name} onChange={(e) => set('name', e.target.value)} />
        </label>
        <label>Slot
          <select value={draft.equip_slot ?? ''}
            onChange={(e) => set('equip_slot', e.target.value || null)}>
            {SLOTS.map(s => <option key={s} value={s}>{s || '—'}</option>)}
          </select>
        </label>
        <label className="checkbox-label">
          <input type="checkbox" checked={draft.active}
            onChange={(e) => set('active', e.target.checked)} />
          <span>Ativo (drop / equip / use). Inativo: só vender ou guardar.</span>
        </label>
        {N('item_level', 'Item Level')}
        {N('sell_price', 'Sell Price')}
        <label>Buy Price (vazio = não vende)
          <input type="number" value={draft.buy_price ?? ''}
            onChange={(e) => set('buy_price', e.target.value === '' ? null : Number(e.target.value))} />
        </label>
        <label>Shop Order
          <input type="number" value={draft.shop_order ?? ''}
            onChange={(e) => set('shop_order', e.target.value === '' ? null : Number(e.target.value))} />
        </label>
        {N('stack_max', 'Stack Max')}
      </div>
      <h3 style={{ marginTop: 16 }}>Ícone</h3>
      <IconPicker
        path={draft.icon_path}
        col={draft.icon_col}
        row={draft.icon_row}
        onPath={(p) => set('icon_path', p)}
        onGrid={(c, r) => { set('icon_col', c); set('icon_row', r); }}
      />
      <h3 style={{ marginTop: 16 }}>Stat Ranges (rolls no drop com instance)</h3>
      <div className="grid3">
        {N('hp_min', 'HP Min')} {N('hp_max', 'HP Max')} <span />
        {N('mp_min', 'MP Min')} {N('mp_max', 'MP Max')} <span />
        {N('atk_min', 'ATK Min')} {N('atk_max', 'ATK Max')} <span />
        {N('def_min', 'DEF Min')} {N('def_max', 'DEF Max')} <span />
        {N('dex_min', 'DEX Min')} {N('dex_max', 'DEX Max')} <span />
        {N('wis_min', 'WIS Min')} {N('wis_max', 'WIS Max')} <span />
      </div>
    </Modal>
  );
}

// Helper to sync external `editing` -> internal draft state. Inline so we don't
// need a separate file. Resets `err` on open.
import { useEffect, type Dispatch, type SetStateAction } from 'react';
function useEffectSync<T>(
  src: T | null,
  setDraft: Dispatch<SetStateAction<T | null>>,
  setErr: (s: string) => void,
) {
  useEffect(() => {
    setDraft(src ? ({ ...(src as object) } as T) : null);
    setErr('');
  }, [src]); // eslint-disable-line react-hooks/exhaustive-deps
}

import { useEffect, useMemo, useState } from 'react';
import { api } from './api';
import { Modal } from './Modal';
import type { FarmDrop, FarmKind, Item } from './types';

type Props = { farmDrops: FarmDrop[]; items: Item[]; reload: () => Promise<void> };

const KINDS: FarmKind[] = ['Tree', 'Rock', 'Flower'];
const KIND_LABEL: Record<FarmKind, string> = {
  Tree: 'Árvore', Rock: 'Rocha', Flower: 'Flor',
};

const EMPTY = (kind: FarmKind, tier: number, firstItemId: number): FarmDrop => ({
  id: 0, kind, tier, item_id: firstItemId, qty_min: 1, qty_max: 1, chance: 1.0,
});

export function FarmDropsTab({ farmDrops, items, reload }: Props) {
  const [filterKind, setFilterKind] = useState<FarmKind | ''>('');
  const [filterTier, setFilterTier] = useState<number | ''>('');
  const [editing, setEditing] = useState<FarmDrop | null>(null);
  const [isNew, setIsNew] = useState(false);

  const filtered = useMemo(() => farmDrops.filter(d =>
    (filterKind === '' || d.kind === filterKind) &&
    (filterTier === '' || d.tier === filterTier)
  ), [farmDrops, filterKind, filterTier]);

  const itemName = (id: number) => items.find(x => x.id === id)?.name ?? '?';

  const onDelete = async (d: FarmDrop) => {
    if (!confirm(`Remover farm-drop ${d.id}?`)) return;
    try { await api('DELETE', '/farm-drops/' + d.id); await reload(); }
    catch (e) { alert((e as Error).message); }
  };

  const newKind = (filterKind || 'Tree') as FarmKind;
  const newTier = (filterTier || 1) as number;
  const firstItem = items[0]?.id ?? 1;

  return (
    <section>
      <div className="toolbar">
        <label>Recurso:&nbsp;
          <select value={filterKind}
            onChange={(e) => setFilterKind(e.target.value as FarmKind | '')}
            style={{ width: 160 }}>
            <option value="">— todos —</option>
            {KINDS.map(k => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}
          </select>
        </label>
        <label>Tier:&nbsp;
          <select value={filterTier === '' ? '' : String(filterTier)}
            onChange={(e) => setFilterTier(e.target.value === '' ? '' : Number(e.target.value))}
            style={{ width: 100 }}>
            <option value="">— todos —</option>
            {[1, 2, 3, 4].map(t => <option key={t} value={t}>T{t}</option>)}
          </select>
        </label>
        <button className="primary"
          onClick={() => { setEditing(EMPTY(newKind, newTier, firstItem)); setIsNew(true); }}>
          + Novo Drop
        </button>
        <span className="muted">{filtered.length} drops</span>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>ID</th><th>Recurso</th><th>Tier</th><th>Item</th>
              <th>Qty Min</th><th>Qty Max</th><th>Chance</th><th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map(d => (
              <tr key={d.id}>
                <td>{d.id}</td>
                <td>{KIND_LABEL[d.kind]}</td>
                <td>T{d.tier}</td>
                <td>{d.item_id} — {itemName(d.item_id)}</td>
                <td>{d.qty_min}</td>
                <td>{d.qty_max}</td>
                <td>{(d.chance * 100).toFixed(1)}%</td>
                <td>
                  <button onClick={() => { setEditing({ ...d }); setIsNew(false); }}>Editar</button>
                  <button className="danger" onClick={() => onDelete(d)}>×</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <FarmDropEditor
        editing={editing} isNew={isNew} items={items}
        onClose={() => setEditing(null)}
        onSaved={async () => { setEditing(null); await reload(); }} />
    </section>
  );
}

function FarmDropEditor({
  editing, isNew, items, onClose, onSaved,
}: {
  editing: FarmDrop | null; isNew: boolean; items: Item[];
  onClose: () => void; onSaved: () => Promise<void>;
}) {
  const [draft, setDraft] = useState<FarmDrop | null>(null);
  const [err, setErr] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(editing ? { ...editing } : null);
    setErr('');
  }, [editing]);

  if (!editing || !draft) return null;
  const set = <K extends keyof FarmDrop>(k: K, v: FarmDrop[K]) => setDraft({ ...draft, [k]: v });

  const onSave = async () => {
    setErr('');
    if (draft.qty_max < draft.qty_min) { setErr('qty_max < qty_min'); return; }
    setSaving(true);
    try {
      const { id: _, ...payload } = draft;
      if (isNew) await api('POST', '/farm-drops', payload);
      else       await api('PUT',  '/farm-drops/' + draft.id, payload);
      await onSaved();
    } catch (e) { setErr((e as Error).message); }
    finally { setSaving(false); }
  };

  return (
    <Modal open title={isNew ? 'Novo Farm Drop' : `Editar farm-drop #${draft.id}`}
      err={err} saving={saving} onClose={onClose} onSave={onSave}>
      <div className="grid2">
        <label>Recurso
          <select value={draft.kind} onChange={(e) => set('kind', e.target.value as FarmKind)}>
            {KINDS.map(k => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}
          </select>
        </label>
        <label>Tier
          <select value={draft.tier} onChange={(e) => set('tier', Number(e.target.value))}>
            {[1, 2, 3, 4].map(t => <option key={t} value={t}>T{t}</option>)}
          </select>
        </label>
        <label>Item
          <select value={draft.item_id} onChange={(e) => set('item_id', Number(e.target.value))}>
            {items.map(it => <option key={it.id} value={it.id}>{it.id} — {it.name}</option>)}
          </select>
        </label>
        <label>Chance (0..1) <input type="number" step={0.01} min={0} max={1} value={draft.chance}
          onChange={(e) => set('chance', Number(e.target.value || 0))} /></label>
        <label>Qty Min <input type="number" min={0} value={draft.qty_min}
          onChange={(e) => set('qty_min', Number(e.target.value || 0))} /></label>
        <label>Qty Max <input type="number" min={0} value={draft.qty_max}
          onChange={(e) => set('qty_max', Number(e.target.value || 0))} /></label>
      </div>
    </Modal>
  );
}

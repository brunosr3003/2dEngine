import { useEffect, useMemo, useState } from 'react';
import { api } from './api';
import { Modal } from './Modal';
import type { Drop, Item, Enemy } from './types';

type Props = { drops: Drop[]; items: Item[]; enemies: Enemy[]; reload: () => Promise<void> };

const EMPTY = (firstKind: number): Drop => ({
  id: 0, enemy_kind: firstKind, item_id: 1, qty_min: 1, qty_max: 1, chance: 0.1,
});

export function DropsTab({ drops, items, enemies, reload }: Props) {
  const [filterKind, setFilterKind] = useState<number | ''>('');
  const [editing, setEditing] = useState<Drop | null>(null);
  const [isNew, setIsNew] = useState(false);

  const filtered = useMemo(() =>
    filterKind === '' ? drops : drops.filter(d => d.enemy_kind === filterKind)
  , [drops, filterKind]);

  const itemName = (id: number) => items.find(x => x.id === id)?.name ?? '?';
  const enemyName = (k: number) => enemies.find(x => x.kind === k)?.name ?? '?';

  const onDelete = async (d: Drop) => {
    if (!confirm(`Remover drop ${d.id}?`)) return;
    try { await api('DELETE', '/drops/' + d.id); await reload(); }
    catch (e) { alert((e as Error).message); }
  };

  return (
    <section>
      <div className="toolbar">
        <label>Filtrar por mob:&nbsp;
          <select value={filterKind === '' ? '' : String(filterKind)}
            onChange={(e) => setFilterKind(e.target.value === '' ? '' : Number(e.target.value))}
            style={{ width: 240 }}>
            <option value="">— todos —</option>
            {enemies.map(e => <option key={e.kind} value={e.kind}>{e.kind} — {e.name}</option>)}
          </select>
        </label>
        <button className="primary"
          onClick={() => { setEditing(EMPTY(enemies[0]?.kind ?? 0)); setIsNew(true); }}>
          + Novo Drop
        </button>
        <span className="muted">{filtered.length} drops</span>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>ID</th><th>Mob</th><th>Item</th><th>Qty Min</th><th>Qty Max</th><th>Chance</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map(d => (
              <tr key={d.id}>
                <td>{d.id}</td>
                <td>{d.enemy_kind} — {enemyName(d.enemy_kind)}</td>
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
      <DropEditor
        editing={editing} isNew={isNew} items={items} enemies={enemies}
        onClose={() => setEditing(null)}
        onSaved={async () => { setEditing(null); await reload(); }} />
    </section>
  );
}

function DropEditor({
  editing, isNew, items, enemies, onClose, onSaved,
}: {
  editing: Drop | null; isNew: boolean; items: Item[]; enemies: Enemy[];
  onClose: () => void; onSaved: () => Promise<void>;
}) {
  const [draft, setDraft] = useState<Drop | null>(null);
  const [err, setErr] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(editing ? { ...editing } : null);
    setErr('');
  }, [editing]);

  if (!editing || !draft) return null;
  const set = <K extends keyof Drop>(k: K, v: Drop[K]) => setDraft({ ...draft, [k]: v });

  const onSave = async () => {
    setErr('');
    if (draft.qty_max < draft.qty_min) { setErr('qty_max < qty_min'); return; }
    setSaving(true);
    try {
      const { id: _, ...payload } = draft;
      if (isNew) await api('POST', '/drops', payload);
      else       await api('PUT',  '/drops/' + draft.id, payload);
      await onSaved();
    } catch (e) { setErr((e as Error).message); }
    finally { setSaving(false); }
  };

  return (
    <Modal open title={isNew ? 'Novo Drop' : `Editar drop #${draft.id}`}
      err={err} saving={saving} onClose={onClose} onSave={onSave}>
      <div className="grid2">
        <label>Mob (kind)
          <select value={draft.enemy_kind} onChange={(e) => set('enemy_kind', Number(e.target.value))}>
            {enemies.map(e => <option key={e.kind} value={e.kind}>{e.kind} — {e.name}</option>)}
          </select>
        </label>
        <label>Item
          <select value={draft.item_id} onChange={(e) => set('item_id', Number(e.target.value))}>
            {items.map(it => <option key={it.id} value={it.id}>{it.id} — {it.name}</option>)}
          </select>
        </label>
        <label>Qty Min <input type="number" min={0} value={draft.qty_min}
          onChange={(e) => set('qty_min', Number(e.target.value || 0))} /></label>
        <label>Qty Max <input type="number" min={0} value={draft.qty_max}
          onChange={(e) => set('qty_max', Number(e.target.value || 0))} /></label>
        <label>Chance (0..1) <input type="number" step={0.01} min={0} max={1} value={draft.chance}
          onChange={(e) => set('chance', Number(e.target.value || 0))} /></label>
      </div>
    </Modal>
  );
}

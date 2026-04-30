import { useEffect, useState } from 'react';
import { api } from './api';
import { Modal } from './Modal';
import type { Enemy } from './types';

type Props = { enemies: Enemy[]; reload: () => Promise<void> };

export function EnemiesTab({ enemies, reload }: Props) {
  const [editing, setEditing] = useState<Enemy | null>(null);

  return (
    <section>
      <div className="toolbar">
        <span className="muted">Edita kinds existentes (kind é fixo). Loot Lvl = level dos drops desse mob.</span>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Kind</th><th>Nome</th><th>HP</th><th>Speed</th><th>ATK</th>
              <th>CD</th><th>Det</th><th>Range</th><th>Kite</th><th>Proj</th>
              <th>XP</th><th>DEF</th><th>Size</th><th>Loot Lvl</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {enemies.map(e => (
              <tr key={e.kind}>
                <td>{e.kind}</td>
                <td>{e.name}</td>
                <td>{e.hp_max}</td>
                <td>{e.speed.toFixed(1)}</td>
                <td>{e.attack_damage}</td>
                <td>{e.attack_cooldown.toFixed(1)}</td>
                <td>{e.detect_range.toFixed(0)}</td>
                <td>{e.attack_range.toFixed(1)}</td>
                <td>{e.kite_dist?.toFixed?.(0) ?? '—'}</td>
                <td>{e.proj_count}</td>
                <td>{e.xp_reward}</td>
                <td>{e.defense}</td>
                <td>{e.size_scale.toFixed(2)}</td>
                <td>{e.loot_item_level ?? <span className="muted">—</span>}</td>
                <td><button onClick={() => setEditing({ ...e })}>Editar</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <EnemyEditor
        editing={editing}
        onClose={() => setEditing(null)}
        onSaved={async () => { setEditing(null); await reload(); }} />
    </section>
  );
}

function EnemyEditor({ editing, onClose, onSaved }: {
  editing: Enemy | null; onClose: () => void; onSaved: () => Promise<void>;
}) {
  const [draft, setDraft] = useState<Enemy | null>(null);
  const [err, setErr] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(editing ? { ...editing } : null);
    setErr('');
  }, [editing]);

  if (!editing || !draft) return null;
  const set = <K extends keyof Enemy>(k: K, v: Enemy[K]) => setDraft({ ...draft, [k]: v });

  const onSave = async () => {
    setErr(''); setSaving(true);
    try {
      const { kind, ...payload } = draft;
      await api('PUT', '/enemies/' + kind, payload);
      await onSaved();
    } catch (e) { setErr((e as Error).message); }
    finally { setSaving(false); }
  };

  const N = (k: keyof Enemy, label: string, step = 1) => (
    <label>{label}
      <input
        type="number"
        step={step}
        value={draft[k] as number ?? 0}
        onChange={(e) => set(k, (e.target.value === '' ? 0 : Number(e.target.value)) as Enemy[typeof k])}
      />
    </label>
  );

  return (
    <Modal open title={`Editar mob #${draft.kind} — ${draft.name}`}
      err={err} saving={saving} onClose={onClose} onSave={onSave}>
      <div className="grid2">
        <label>Nome
          <input value={draft.name} onChange={(e) => set('name', e.target.value)} />
        </label>
        {N('hp_max', 'HP Max')}
        {N('speed', 'Speed', 0.1)}
        {N('attack_damage', 'Attack Damage')}
        {N('attack_cooldown', 'Attack CD', 0.1)}
        {N('detect_range', 'Detect Range', 0.1)}
        {N('attack_range', 'Attack Range', 0.1)}
        <label>Kite Dist (vazio = melee)
          <input type="number" step={0.1} value={draft.kite_dist ?? ''}
            onChange={(e) => set('kite_dist', e.target.value === '' ? null : Number(e.target.value))} />
        </label>
        {N('proj_count', 'Proj Count')}
        {N('xp_reward', 'XP Reward')}
        {N('defense', 'Defense')}
        {N('size_scale', 'Size Scale', 0.05)}
        <label>Loot Item Level (vazio = usa items.item_level)
          <input type="number" min={1} value={draft.loot_item_level ?? ''}
            onChange={(e) => set('loot_item_level', e.target.value === '' ? null : Number(e.target.value))} />
        </label>
        {N('tint_r', 'Tint R', 0.05)}
        {N('tint_g', 'Tint G', 0.05)}
        {N('tint_b', 'Tint B', 0.05)}
        {N('tint_a', 'Tint A', 0.05)}
      </div>
    </Modal>
  );
}

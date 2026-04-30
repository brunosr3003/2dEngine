import { useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';
import { api, auth } from './api';
import { Modal } from './Modal';
import { IconPicker } from './IconPicker';
import { type Skill, PROFS, TARGET_TYPES } from './types';

type Props = { skills: Skill[]; reload: () => Promise<void> };

const EMPTY: Skill = {
  id: 0, name: '', description: '',
  prof: 'Sword', tier: 1, is_passive: false, path: null,
  unlock_char_lvl: 1, unlock_prof_lvl: 1, usable_with: null,
  cost_mp: 0, cost_stamina: 0, cooldown_s: 0, cast_time_s: 0,
  target_type: 'projectile', range_tiles: 0, radius_tiles: 0,
  base_damage: 0, base_heal: 0,
  scaling_atk: 0, scaling_wis: 0, scaling_dex: 0,
  per_rank_dmg_pct: 0.10, per_rank_cd_pct: 0, per_rank_cost_pct: 0,
  icon_path: null, vfx_id: null, active: true,
};

export function SkillsTab({ skills, reload }: Props) {
  const [filter, setFilter] = useState('');
  const [profFilter, setProfFilter] = useState<string>('');
  const [editing, setEditing] = useState<Skill | null>(null);
  const [isNew, setIsNew] = useState(false);

  const filtered = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return skills.filter(s => {
      if (profFilter && s.prof !== profFilter) return false;
      if (!f) return true;
      return String(s.id).includes(f) || s.name.toLowerCase().includes(f)
          || (s.description || '').toLowerCase().includes(f);
    });
  }, [skills, filter, profFilter]);

  const openNew = () => {
    const nextId = skills.length === 0 ? 2000 : Math.max(...skills.map(s => s.id)) + 1;
    setEditing({ ...EMPTY, id: nextId });
    setIsNew(true);
  };

  const openEdit = (s: Skill) => { setEditing({ ...s }); setIsNew(false); };

  const onDelete = async (s: Skill) => {
    if (!confirm(`Remover skill ${s.id} — ${s.name}?`)) return;
    try {
      await api('DELETE', '/skills/' + s.id);
      await reload();
    } catch (e) { alert((e as Error).message); }
  };

  const toggleActive = async (s: Skill) => {
    try {
      await api('PUT', '/skills/' + s.id, { ...s, active: !s.active });
      await reload();
    } catch (e) { alert((e as Error).message); }
  };

  return (
    <section>
      <div className="toolbar">
        <input placeholder="buscar id/nome/descrição..." value={filter}
               onChange={e => setFilter(e.target.value)} />
        <select value={profFilter} onChange={e => setProfFilter(e.target.value)}>
          <option value="">Todas profs</option>
          {PROFS.map(p => <option key={p} value={p}>{p}</option>)}
        </select>
        <button className="primary" onClick={openNew}>+ Nova Skill</button>
        <span className="muted">{filtered.length} / {skills.length}</span>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Ações</th>
              <th>ID</th><th>Prof</th><th>T</th><th>A/P</th><th>Nome</th>
              <th></th>
              <th>Status</th><th>Path</th>
              <th>Char/Prof</th>
              <th>Cost</th><th>CD</th>
              <th>Target</th><th>Range</th><th>Radius</th>
              <th>Base dmg/heal</th>
              <th>Scaling</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map(s => (
              <tr key={s.id} className={s.active ? '' : 'row-inactive'}>
                <td>
                  <button onClick={() => openEdit(s)}>Editar</button>
                  <button className="danger" onClick={() => onDelete(s)}>×</button>
                </td>
                <td>{s.id}</td>
                <td><span className="badge">{s.prof}</span></td>
                <td>T{s.tier}</td>
                <td>{s.is_passive ? <span className="muted">P</span> : <span style={{color:'#5cd'}}>A</span>}</td>
                <td>{s.name}</td>
                <td><SkillThumb path={s.icon_path} /></td>
                <td>
                  <button className={'badge-toggle ' + (s.active ? 'on' : 'off')}
                          onClick={() => toggleActive(s)} title="Click pra alternar">
                    {s.active ? 'Ativo' : 'Inativo'}
                  </button>
                </td>
                <td>{s.path || <span className="muted">—</span>}</td>
                <td>L{s.unlock_char_lvl} / {s.unlock_prof_lvl}</td>
                <td>{s.cost_mp ? `${s.cost_mp}mp` : ''}{s.cost_stamina ? ` ${s.cost_stamina}st` : ''}</td>
                <td>{s.cooldown_s.toFixed(1)}s</td>
                <td>{s.target_type}</td>
                <td>{s.range_tiles || '—'}</td>
                <td>{s.radius_tiles || '—'}</td>
                <td>{s.base_damage > 0 ? `dmg ${s.base_damage}` : ''}{s.base_heal > 0 ? `heal ${s.base_heal}` : ''}</td>
                <td className="small">
                  {s.scaling_atk > 0 && `atk×${s.scaling_atk} `}
                  {s.scaling_wis > 0 && `wis×${s.scaling_wis} `}
                  {s.scaling_dex > 0 && `dex×${s.scaling_dex}`}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <SkillEditor
        editing={editing}
        isNew={isNew}
        onClose={() => setEditing(null)}
        onSaved={async () => { setEditing(null); await reload(); }}
      />
    </section>
  );
}

function SkillThumb({ path }: { path: string | null }) {
  if (!path || !path.startsWith('Items/')) return <span className="muted small">—</span>;
  const base = path.slice('Items/'.length);
  const url = `/api/econ/icons/${encodeURIComponent(base)}?t=${auth.get().slice(-8)}`;
  return (
    <img className="row-thumb" src={url} alt={base} title={path}
         onError={e => { (e.target as HTMLImageElement).style.opacity = '0'; }} />
  );
}

type EditorProps = {
  editing: Skill | null;
  isNew: boolean;
  onClose: () => void;
  onSaved: () => Promise<void>;
};

function SkillEditor({ editing, isNew, onClose, onSaved }: EditorProps) {
  const [draft, setDraft] = useState<Skill | null>(null);
  const [err, setErr] = useState('');
  const [saving, setSaving] = useState(false);
  useEffectSync(editing, setDraft, setErr);

  if (!editing || !draft) return null;
  const set = <K extends keyof Skill>(k: K, v: Skill[K]) => setDraft({ ...draft, [k]: v });

  const onSave = async () => {
    setErr('');
    if (!draft.name.trim()) { setErr('nome obrigatório'); return; }
    if (draft.id <= 0) { setErr('id inválido'); return; }
    setSaving(true);
    try {
      const payload = { ...draft };
      if (isNew) await api('POST', '/skills', payload);
      else       await api('PUT',  '/skills/' + draft.id, payload);
      await onSaved();
    } catch (e) { setErr((e as Error).message); }
    finally { setSaving(false); }
  };

  const N = (k: keyof Skill, label: string, step?: number) => (
    <label>{label}
      <input type="number" step={step ?? 1} value={(draft[k] as number) ?? 0}
        onChange={e => set(k, (e.target.value === '' ? 0 : Number(e.target.value)) as Skill[typeof k])} />
    </label>
  );

  // Toggle prof in usable_with (null = all profs).
  const toggleProf = (p: string) => {
    const cur = draft.usable_with ?? [];
    const next = cur.includes(p) ? cur.filter(x => x !== p) : [...cur, p];
    set('usable_with', next.length === 0 ? null : next);
  };

  return (
    <Modal open title={isNew ? 'Nova Skill' : `Editar #${draft.id} — ${draft.name}`}
           err={err} saving={saving} onClose={onClose} onSave={onSave}>
      <div className="grid3">
        <label>ID{isNew ? '' : ' (não editável)'}
          <input type="number" disabled={!isNew} value={draft.id}
                 onChange={e => set('id', Number(e.target.value || 0))} />
        </label>
        <label>Nome
          <input value={draft.name} onChange={e => set('name', e.target.value)} />
        </label>
        <label>Prof
          <select value={draft.prof} onChange={e => set('prof', e.target.value)}>
            {PROFS.map(p => <option key={p} value={p}>{p}</option>)}
          </select>
        </label>
        <label>Tier
          <select value={draft.tier} onChange={e => set('tier', Number(e.target.value))}>
            {[1,2,3,4].map(t => <option key={t} value={t}>T{t}</option>)}
          </select>
        </label>
        <label>Path
          <input value={draft.path ?? ''} placeholder="duelist / tank / fire / ..."
                 onChange={e => set('path', e.target.value || null)} />
        </label>
        <label className="checkbox-label">
          <input type="checkbox" checked={draft.is_passive}
                 onChange={e => set('is_passive', e.target.checked)} />
          <span>Passiva (sempre-ativa, não ocupa slot da bar)</span>
        </label>
        <label className="checkbox-label">
          <input type="checkbox" checked={draft.active}
                 onChange={e => set('active', e.target.checked)} />
          <span>Ativa no DB (False = removida do catálogo)</span>
        </label>
      </div>

      <h3 style={{ marginTop: 16 }}>Descrição</h3>
      <textarea
        rows={2}
        style={{ width: '100%', resize: 'vertical', fontFamily: 'inherit' }}
        value={draft.description}
        onChange={e => set('description', e.target.value)}
      />

      <h3 style={{ marginTop: 16 }}>Unlock</h3>
      <div className="grid3">
        {N('unlock_char_lvl', 'Char Level mín')}
        {N('unlock_prof_lvl', 'Prof Level mín')}
        <span />
      </div>

      <h3 style={{ marginTop: 16 }}>Cross-weapon (vazio = qualquer arma)</h3>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
        {PROFS.map(p => {
          const on = (draft.usable_with ?? []).includes(p);
          return (
            <button key={p} type="button"
                    className={'badge-toggle ' + (on ? 'on' : 'off')}
                    onClick={() => toggleProf(p)}>{p}</button>
          );
        })}
      </div>

      <h3 style={{ marginTop: 16 }}>Cast (ativas)</h3>
      <div className="grid3">
        <label>Target Type
          <select value={draft.target_type} onChange={e => set('target_type', e.target.value)}>
            {TARGET_TYPES.map(t => <option key={t} value={t}>{t}</option>)}
          </select>
        </label>
        {N('cooldown_s', 'Cooldown (s)', 0.1)}
        {N('cast_time_s', 'Cast Time (s)', 0.05)}
        {N('cost_mp', 'Cost MP')}
        {N('cost_stamina', 'Cost Stamina')}
        <span />
        {N('range_tiles', 'Range (tiles)', 0.5)}
        {N('radius_tiles', 'Radius (tiles)', 0.5)}
        <span />
      </div>

      <h3 style={{ marginTop: 16 }}>Damage / Heal</h3>
      <div className="grid3">
        {N('base_damage', 'Base Damage')}
        {N('base_heal', 'Base Heal')}
        <span />
        {N('scaling_atk', 'Scaling × ATK', 0.05)}
        {N('scaling_wis', 'Scaling × WIS', 0.05)}
        {N('scaling_dex', 'Scaling × DEX', 0.05)}
      </div>

      <h3 style={{ marginTop: 16 }}>Per Rank (% por rank, 1..10)</h3>
      <div className="grid3">
        {N('per_rank_dmg_pct', 'Dmg/Heal %', 0.01)}
        {N('per_rank_cd_pct', 'CD redux %', 0.01)}
        {N('per_rank_cost_pct', 'Cost redux %', 0.01)}
      </div>

      <h3 style={{ marginTop: 16 }}>VFX (animação)</h3>
      <div className="grid3">
        <label>VFX ID (string livre — futuro hook em SpriteSheetVfx)
          <input value={draft.vfx_id ?? ''} placeholder="ex: fireball_explosion"
                 onChange={e => set('vfx_id', e.target.value || null)} />
        </label>
      </div>

      <h3 style={{ marginTop: 16 }}>Ícone</h3>
      <IconPicker
        path={draft.icon_path}
        col={0} row={0}
        onPath={p => set('icon_path', p)}
        onGrid={() => {/* skills usam só icon_path */}}
      />
    </Modal>
  );
}

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

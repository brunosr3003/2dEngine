import { useEffect, useMemo, useState } from 'react';
import { api, auth } from './api';
import { ItemDetailModal } from './ItemDetailModal';
import type { Item, Enemy } from './types';

type Summary = {
  gold_total: number; players: number;
  inv_slots: number; vault_slots: number; equipped: number;
  drops_total: number; drops_24h: number;
};

type ItemReport = {
  item_id: number; name: string | null;
  in_inventory: number; in_vault: number; in_equipment: number;
  dropped_total: number; dropped_qty: number;
};

type PlayerReport = {
  name: string; level: number; gold: number;
  inv_count: number; vault_count: number; equipment_count: number;
};

type DropLog = {
  id: number; ts: string; enemy_kind: number; item_id: number;
  qty: number; rarity: number; item_level: number; refinement: number;
};

const RARITY_NAMES = ['Comum', 'Mágico', 'Raro', 'Épico', 'Lendário'];
const RARITY_COLORS = ['#bfbfbf', '#5577ff', '#ffd84d', '#aa55ff', '#ff7733'];

type Props = { items: Item[]; enemies: Enemy[] };

export function ReportTab({ items, enemies }: Props) {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [itemRows, setItemRows] = useState<ItemReport[]>([]);
  const [playerRows, setPlayerRows] = useState<PlayerReport[]>([]);
  const [drops, setDrops] = useState<DropLog[]>([]);
  const [filterItem, setFilterItem] = useState<number | ''>('');
  const [filterKind, setFilterKind] = useState<number | ''>('');
  const [filterRarity, setFilterRarity] = useState<number | ''>('');
  const [loading, setLoading] = useState(false);
  const [detailFor, setDetailFor] = useState<number | null>(null);

  const itemName = useMemo(() => {
    const m = new Map<number, string>();
    for (const it of items) m.set(it.id, it.name);
    return (id: number) => m.get(id) || `#${id}`;
  }, [items]);

  const itemPath = useMemo(() => {
    const m = new Map<number, string | null>();
    for (const it of items) m.set(it.id, it.icon_path);
    return (id: number) => m.get(id) ?? null;
  }, [items]);

  const enemyName = useMemo(() => {
    const m = new Map<number, string>();
    for (const e of enemies) m.set(e.kind, e.name);
    return (k: number) => m.get(k) || `#${k}`;
  }, [enemies]);

  const reload = async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams();
      params.set('limit', '300');
      if (filterItem !== '')   params.set('item',   String(filterItem));
      if (filterKind !== '')   params.set('kind',   String(filterKind));
      if (filterRarity !== '') params.set('rarity', String(filterRarity));
      const [s, i, p, d] = await Promise.all([
        api<Summary>('GET', '/report/summary'),
        api<{ items: ItemReport[] }>('GET', '/report/items'),
        api<{ players: PlayerReport[] }>('GET', '/report/players'),
        api<{ drops: DropLog[] }>('GET', '/report/drops?' + params.toString()),
      ]);
      setSummary(s);
      setItemRows(i.items);
      setPlayerRows(p.players);
      setDrops(d.drops);
    } catch (e) { alert((e as Error).message); }
    finally { setLoading(false); }
  };

  useEffect(() => { reload(); /* eslint-disable-line react-hooks/exhaustive-deps */ }, []);
  useEffect(() => { reload(); /* eslint-disable-line react-hooks/exhaustive-deps */ }, [filterItem, filterKind, filterRarity]);

  const thumb = (path: string | null) => {
    if (!path || !path.startsWith('Items/')) return null;
    const base = path.slice('Items/'.length);
    return <img className="row-thumb" style={{ width: 22, height: 22 }}
      src={`/api/econ/icons/${encodeURIComponent(base)}?t=${auth.get().slice(-8)}`} alt="" />;
  };

  return (
    <section>
      <div className="toolbar">
        <button className="primary" onClick={reload} disabled={loading}>
          {loading ? 'Carregando…' : 'Atualizar'}
        </button>
      </div>

      {summary && (
        <div className="report-cards">
          <Card label="Gold em circulação" value={summary.gold_total.toLocaleString('pt-BR')} accent />
          <Card label="Jogadores"          value={summary.players} />
          <Card label="Slots usados (inv)" value={summary.inv_slots} />
          <Card label="Slots usados (vault)" value={summary.vault_slots} />
          <Card label="Itens equipados"    value={summary.equipped} />
          <Card label="Drops registrados"  value={summary.drops_total} />
          <Card label="Drops 24h"          value={summary.drops_24h} />
        </div>
      )}

      <h3 style={{ marginTop: 24 }}>Itens no jogo (snapshot atual + total dropado)</h3>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>ID</th><th></th><th>Nome</th>
              <th>Inventário</th><th>Vault</th><th>Equipados</th>
              <th>Total atual</th>
              <th>Drops (rolls)</th><th>Drops (qty total)</th>
            </tr>
          </thead>
          <tbody>
            {itemRows.map(r => (
              <tr key={r.item_id} className="row-clickable" onClick={() => setDetailFor(r.item_id)}>
                <td>{r.item_id}</td>
                <td>{thumb(itemPath(r.item_id))}</td>
                <td>{r.name ?? itemName(r.item_id)}</td>
                <td>{r.in_inventory.toLocaleString('pt-BR')}</td>
                <td>{r.in_vault.toLocaleString('pt-BR')}</td>
                <td>{r.in_equipment.toLocaleString('pt-BR')}</td>
                <td><strong>{(r.in_inventory + r.in_vault + r.in_equipment).toLocaleString('pt-BR')}</strong></td>
                <td>{r.dropped_total.toLocaleString('pt-BR')}</td>
                <td>{r.dropped_qty.toLocaleString('pt-BR')}</td>
              </tr>
            ))}
            {itemRows.length === 0 && <tr><td colSpan={9} className="muted">vazio</td></tr>}
          </tbody>
        </table>
      </div>

      <h3 style={{ marginTop: 24 }}>Jogadores</h3>
      <div className="table-wrap">
        <table>
          <thead>
            <tr><th>Nome</th><th>Level</th><th>Gold</th><th>Slots Inv</th><th>Slots Vault</th><th>Equipados</th></tr>
          </thead>
          <tbody>
            {playerRows.map(p => (
              <tr key={p.name}>
                <td>{p.name}</td>
                <td>{p.level}</td>
                <td><strong style={{ color: '#ffd84d' }}>{p.gold.toLocaleString('pt-BR')}</strong></td>
                <td>{p.inv_count}</td>
                <td>{p.vault_count}</td>
                <td>{p.equipment_count}</td>
              </tr>
            ))}
            {playerRows.length === 0 && <tr><td colSpan={6} className="muted">nenhum personagem</td></tr>}
          </tbody>
        </table>
      </div>

      <h3 style={{ marginTop: 24 }}>Log de drops</h3>
      <div className="toolbar">
        <select value={filterKind === '' ? '' : String(filterKind)}
          onChange={(e) => setFilterKind(e.target.value === '' ? '' : Number(e.target.value))}>
          <option value="">— qualquer mob —</option>
          {enemies.map(e => <option key={e.kind} value={e.kind}>{e.kind} — {e.name}</option>)}
        </select>
        <select value={filterItem === '' ? '' : String(filterItem)}
          onChange={(e) => setFilterItem(e.target.value === '' ? '' : Number(e.target.value))}>
          <option value="">— qualquer item —</option>
          {items.map(it => <option key={it.id} value={it.id}>{it.id} — {it.name}</option>)}
        </select>
        <select value={filterRarity === '' ? '' : String(filterRarity)}
          onChange={(e) => setFilterRarity(e.target.value === '' ? '' : Number(e.target.value))}>
          <option value="">— qualquer raridade —</option>
          {RARITY_NAMES.map((n, i) => <option key={i} value={i}>{n}</option>)}
        </select>
        <span className="muted">{drops.length} drops (300 mais recentes filtrados)</span>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Quando</th><th>Mob</th><th></th><th>Item</th><th>Qty</th>
              <th>Raridade</th><th>iLvl</th><th>+Ref</th>
            </tr>
          </thead>
          <tbody>
            {drops.map(d => (
              <tr key={d.id}>
                <td>{new Date(d.ts).toLocaleString('pt-BR')}</td>
                <td>{d.enemy_kind} — {enemyName(d.enemy_kind)}</td>
                <td>{thumb(itemPath(d.item_id))}</td>
                <td>{d.item_id} — {itemName(d.item_id)}</td>
                <td>{d.qty}</td>
                <td><span style={{ color: RARITY_COLORS[d.rarity] || '#fff' }}>
                  {RARITY_NAMES[d.rarity] || `#${d.rarity}`}
                </span></td>
                <td>{d.item_level}</td>
                <td>{d.refinement > 0 ? `+${d.refinement}` : '—'}</td>
              </tr>
            ))}
            {drops.length === 0 && <tr><td colSpan={8} className="muted">nenhum drop ainda</td></tr>}
          </tbody>
        </table>
      </div>

      <ItemDetailModal
        itemId={detailFor}
        itemName={detailFor != null ? itemName(detailFor) : undefined}
        iconPath={detailFor != null ? itemPath(detailFor) : null}
        onClose={() => setDetailFor(null)}
      />
    </section>
  );
}

function Card({ label, value, accent }: { label: string; value: number | string; accent?: boolean }) {
  return (
    <div className={'report-card' + (accent ? ' accent' : '')}>
      <div className="report-card-label">{label}</div>
      <div className="report-card-value">{value}</div>
    </div>
  );
}

import { useEffect, useState } from 'react';
import { api, auth } from './api';

type Holding = {
  character: string;
  location: 'inventory' | 'vault' | 'equipment' | string;
  slot: string;
  qty: number;
  rarity: number | null;
  refinement: number | null;
  item_level: number | null;
  hp_max: number; mp_max: number;
  attack: number; defense: number;
  dex: number; wis: number;
  sockets: number;
  affix_count: number;
};

type Detail = {
  item_id: number;
  holdings: Holding[];
  rarity_counts: number[];          // 5 buckets
  refinement_counts: Record<string, number>;
  item_level_hist: Record<string, number>;
  total_qty: number;
  inv_qty: number;
  vault_qty: number;
  equip_qty: number;
  no_instance: number;
  by_player_qty: Record<string, number>;
};

const RARITY_NAMES = ['Comum', 'Mágico', 'Raro', 'Épico', 'Lendário'];
const RARITY_COLORS = ['#bfbfbf', '#5577ff', '#ffd84d', '#aa55ff', '#ff7733'];

type Props = {
  itemId: number | null;
  itemName?: string;
  iconPath?: string | null;
  onClose: () => void;
};

export function ItemDetailModal({ itemId, itemName, iconPath, onClose }: Props) {
  const [data, setData] = useState<Detail | null>(null);
  const [err, setErr] = useState('');
  const [loading, setLoading] = useState(false);
  const [tab, setTab] = useState<'overview' | 'holders' | 'all'>('overview');

  useEffect(() => {
    if (itemId == null) { setData(null); return; }
    setLoading(true); setErr('');
    api<Detail>('GET', '/report/items/' + itemId)
      .then(setData)
      .catch((e) => setErr((e as Error).message))
      .finally(() => setLoading(false));
  }, [itemId]);

  if (itemId == null) return null;

  const thumb = iconPath?.startsWith('Items/')
    ? `/api/econ/icons/${encodeURIComponent(iconPath.slice('Items/'.length))}?t=${auth.get().slice(-8)}`
    : null;

  return (
    <div className="modal-bg" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal" style={{ minWidth: 820, maxWidth: '95vw' }}>
        <div className="row" style={{ alignItems: 'center', gap: 12 }}>
          {thumb && <img src={thumb} alt="" style={{ width: 40, height: 40, imageRendering: 'pixelated', background: '#0a0a12', borderRadius: 4 }} />}
          <h2 style={{ margin: 0 }}>#{itemId} — {itemName ?? '?'}</h2>
          <span className="muted" style={{ marginLeft: 'auto' }}>
            {loading ? 'carregando…' : data ? `${data.total_qty} no jogo` : ''}
          </span>
        </div>

        {err && <div className="err">{err}</div>}

        {data && (
          <>
            <div className="report-cards" style={{ marginTop: 16 }}>
              <Card label="Total no jogo" value={data.total_qty} accent />
              <Card label="Em inventário" value={data.inv_qty} />
              <Card label="Em vault"      value={data.vault_qty} />
              <Card label="Equipados"     value={data.equip_qty} />
              <Card label="Sem instance (legacy/stack)" value={data.no_instance} />
            </div>

            <div className="row" style={{ marginTop: 16, gap: 0, borderBottom: '1px solid var(--border)' }}>
              <TabBtn active={tab==='overview'} onClick={() => setTab('overview')}>Visão geral</TabBtn>
              <TabBtn active={tab==='holders'}  onClick={() => setTab('holders')}>Por jogador</TabBtn>
              <TabBtn active={tab==='all'}      onClick={() => setTab('all')}>Todas as instâncias</TabBtn>
            </div>

            {tab === 'overview' && (
              <div style={{ marginTop: 16 }}>
                <h3>Distribuição por raridade</h3>
                <div className="rarity-bars">
                  {data.rarity_counts.map((n, i) => (
                    <RarityBar key={i} idx={i} count={n} total={data.total_qty - data.no_instance} />
                  ))}
                </div>

                <h3 style={{ marginTop: 16 }}>Refinamento</h3>
                <Histogram entries={Object.entries(data.refinement_counts).map(([k, v]) => [`+${k}`, v])} />

                <h3 style={{ marginTop: 16 }}>Item Level</h3>
                <Histogram entries={Object.entries(data.item_level_hist).map(([k, v]) => [`iLvl ${k}`, v])} />
              </div>
            )}

            {tab === 'holders' && (
              <div style={{ marginTop: 16 }}>
                <table>
                  <thead><tr><th>Personagem</th><th>Quantidade</th></tr></thead>
                  <tbody>
                    {Object.entries(data.by_player_qty).sort((a,b) => b[1]-a[1]).map(([n, q]) => (
                      <tr key={n}><td>{n}</td><td><strong>{q}</strong></td></tr>
                    ))}
                    {Object.keys(data.by_player_qty).length === 0 && (
                      <tr><td colSpan={2} className="muted">ninguém tem esse item</td></tr>
                    )}
                  </tbody>
                </table>
              </div>
            )}

            {tab === 'all' && (
              <div style={{ marginTop: 16, maxHeight: '50vh', overflow: 'auto' }}>
                <table>
                  <thead>
                    <tr>
                      <th>Personagem</th><th>Onde</th><th>Slot</th><th>Qty</th>
                      <th>Raridade</th><th>+Ref</th><th>iLvl</th>
                      <th>HP</th><th>MP</th><th>ATK</th><th>DEF</th><th>DEX</th><th>WIS</th>
                      <th>Sockets</th><th>Affixes</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.holdings.map((h, i) => (
                      <tr key={i}>
                        <td>{h.character}</td>
                        <td><span className={'badge loc-' + h.location}>{h.location}</span></td>
                        <td className="muted small">{h.slot}</td>
                        <td>{h.qty}</td>
                        <td>{h.rarity != null
                          ? <span style={{ color: RARITY_COLORS[h.rarity] }}>{RARITY_NAMES[h.rarity]}</span>
                          : <span className="muted">—</span>}</td>
                        <td>{h.refinement != null && h.refinement > 0 ? `+${h.refinement}` : '—'}</td>
                        <td>{h.item_level ?? '—'}</td>
                        <td>{h.hp_max || '—'}</td>
                        <td>{h.mp_max || '—'}</td>
                        <td>{h.attack || '—'}</td>
                        <td>{h.defense || '—'}</td>
                        <td>{h.dex || '—'}</td>
                        <td>{h.wis || '—'}</td>
                        <td>{h.sockets || '—'}</td>
                        <td>{h.affix_count || '—'}</td>
                      </tr>
                    ))}
                    {data.holdings.length === 0 && (
                      <tr><td colSpan={15} className="muted">nenhuma instância</td></tr>
                    )}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )}

        <div className="modal-actions">
          <button className="primary" onClick={onClose}>Fechar</button>
        </div>
      </div>
    </div>
  );
}

function Card({ label, value, accent }: { label: string; value: number; accent?: boolean }) {
  return (
    <div className={'report-card' + (accent ? ' accent' : '')}>
      <div className="report-card-label">{label}</div>
      <div className="report-card-value">{value.toLocaleString('pt-BR')}</div>
    </div>
  );
}

function TabBtn({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      style={{
        background: 'transparent', border: 'none', padding: '8px 16px',
        color: active ? 'var(--accent)' : 'var(--muted)',
        borderBottom: '2px solid ' + (active ? 'var(--accent)' : 'transparent'),
      }}
    >{children}</button>
  );
}

function RarityBar({ idx, count, total }: { idx: number; count: number; total: number }) {
  const pct = total > 0 ? (count / total) * 100 : 0;
  return (
    <div className="rarity-row">
      <span style={{ color: RARITY_COLORS[idx], width: 80, fontWeight: 600 }}>{RARITY_NAMES[idx]}</span>
      <div className="rarity-bg">
        <div className="rarity-fill" style={{ width: `${pct}%`, background: RARITY_COLORS[idx] }} />
      </div>
      <span className="muted small" style={{ width: 90, textAlign: 'right' }}>
        {count.toLocaleString('pt-BR')} ({pct.toFixed(1)}%)
      </span>
    </div>
  );
}

function Histogram({ entries }: { entries: [string, number][] }) {
  if (entries.length === 0) return <div className="muted small">sem dados</div>;
  const max = Math.max(...entries.map(([, v]) => v));
  return (
    <div className="hist">
      {entries.map(([k, v]) => (
        <div className="hist-row" key={k}>
          <span className="hist-label">{k}</span>
          <div className="hist-bg">
            <div className="hist-fill" style={{ width: max > 0 ? `${(v/max)*100}%` : 0 }} />
          </div>
          <span className="hist-count">{v}</span>
        </div>
      ))}
    </div>
  );
}

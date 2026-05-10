import { useCallback, useEffect, useState } from 'react';
import { api, auth, login as loginApi, AuthError } from './api';
import { ItemsTab } from './ItemsTab';
import { DropsTab } from './DropsTab';
import { FarmDropsTab } from './FarmDropsTab';
import { EnemiesTab } from './EnemiesTab';
import { ReportTab } from './ReportTab';
import { SkillsTab } from './SkillsTab';
import type { Drop, Enemy, FarmDrop, Item, Skill } from './types';

type Tab = 'items' | 'skills' | 'drops' | 'farm-drops' | 'enemies' | 'report';

export function App() {
  const [authed, setAuthed] = useState<boolean | null>(null); // null = checking
  const [tab, setTab] = useState<Tab>('items');
  const [items, setItems] = useState<Item[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [enemies, setEnemies] = useState<Enemy[]>([]);
  const [drops, setDrops] = useState<Drop[]>([]);
  const [farmDrops, setFarmDrops] = useState<FarmDrop[]>([]);
  const [status, setStatus] = useState('');

  // Boot: verify token
  useEffect(() => {
    if (!auth.get()) { setAuthed(false); return; }
    api('POST', '/verify')
      .then(() => setAuthed(true))
      .catch(() => setAuthed(false));
  }, []);

  const loadAll = useCallback(async () => {
    setStatus('carregando…');
    try {
      const [i, e, d, sk, fd] = await Promise.all([
        api<{ items: Item[] }>('GET', '/items'),
        api<{ enemies: Enemy[] }>('GET', '/enemies'),
        api<{ drops: Drop[] }>('GET', '/drops'),
        api<{ skills: Skill[] }>('GET', '/skills'),
        api<{ farm_drops: FarmDrop[] }>('GET', '/farm-drops'),
      ]);
      setItems(i.items);
      setEnemies(e.enemies);
      setDrops(d.drops);
      setSkills(sk.skills);
      setFarmDrops(fd.farm_drops);
      setStatus('');
    } catch (err) {
      if (err instanceof AuthError) { setAuthed(false); return; }
      setStatus('erro: ' + (err as Error).message);
    }
  }, []);

  useEffect(() => { if (authed) void loadAll(); }, [authed, loadAll]);

  if (authed === null) return <div className="boot">…</div>;
  if (!authed) return <Gate onLogin={() => setAuthed(true)} />;

  return (
    <>
      <header>
        <h1 style={{ margin: 0, fontSize: 18 }}>Admin Econ</h1>
        <nav>
          {(['items', 'skills', 'drops', 'farm-drops', 'enemies', 'report'] as const).map(t => (
            <button key={t} className={tab === t ? 'active' : ''} onClick={() => setTab(t)}>
              {t === 'items' ? 'Itens'
                : t === 'skills' ? 'Skills'
                : t === 'drops' ? 'Drops'
                : t === 'farm-drops' ? 'Farm Drops'
                : t === 'enemies' ? 'Mobs'
                : 'Relatório'}
            </button>
          ))}
        </nav>
        <span className="muted" style={{ marginLeft: 'auto' }}>{status}</span>
        <button className="ghost" onClick={() => { auth.clear(); setAuthed(false); }}>Sair</button>
      </header>
      <main>
        {tab === 'items'   && <ItemsTab items={items} reload={loadAll} />}
        {tab === 'skills'  && <SkillsTab skills={skills} reload={loadAll} />}
        {tab === 'drops'   && <DropsTab drops={drops} items={items} enemies={enemies} reload={loadAll} />}
        {tab === 'farm-drops' && <FarmDropsTab farmDrops={farmDrops} items={items} reload={loadAll} />}
        {tab === 'enemies' && <EnemiesTab enemies={enemies} reload={loadAll} />}
        {tab === 'report'  && <ReportTab items={items} enemies={enemies} />}
      </main>
    </>
  );
}

function Gate({ onLogin }: { onLogin: () => void }) {
  const [pwd, setPwd] = useState('');
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    setErr(''); setBusy(true);
    try {
      await loginApi(pwd);
      onLogin();
    } catch (e) { setErr((e as Error).message); }
    finally { setBusy(false); }
  };

  return (
    <div id="gate">
      <form id="gate-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <h1>Admin — Economia</h1>
        <p className="muted">Senha admin (ECON_ADMIN_PASSWORD ou PIXEL_ADMIN_PASSWORD).</p>
        <input
          type="password"
          autoFocus
          autoComplete="current-password"
          placeholder="senha"
          value={pwd}
          onChange={(e) => setPwd(e.target.value)}
          disabled={busy}
        />
        <button className="primary" type="submit" disabled={busy || !pwd}>
          {busy ? 'Entrando…' : 'Entrar'}
        </button>
        {err && <div className="err">{err}</div>}
      </form>
    </div>
  );
}

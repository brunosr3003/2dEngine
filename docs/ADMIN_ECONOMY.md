# Admin de Economia — Fluxo Completo

> **Estado:** a metade do SERVIDOR está viva — o painel web edita items,
> drops e mobs no Postgres e o servidor de jogo recarrega em até 5 s
> (`POST /api/econ/login`, hot-reload em `economy::spawn_hot_reload`).
>
> A metade do CLIENTE descrita abaixo é do cliente Unity, que **não existe
> mais**. Tudo que fala de `MMORPG/Assets/...`, `.meta`, Editor e
> `ItemsConfigCache.cs` é história: o cliente Rust ainda não consome o
> broadcast de `ItemsConfig`. Está aqui porque o desenho do fluxo continua
> valendo — só falta o outro lado.

Painel web pra editar **items / drops / mobs** sem rebuild do server. Mudanças
caem no Postgres, o game server detecta via versionamento e recarrega o cache
em até 5s. O cliente Unity recebe um broadcast de `ItemsConfig` e troca
nomes/ícones ao vivo.

```
   Browser (econ.html)
        │  REST /api/econ/*  + JWT cookie
        ▼
  ┌──────────────┐                ┌──────────────────┐
  │  web crate   │  UPDATE/INSERT │  Postgres        │
  │  :8090 / 3020│ ─────────────▶ │  items, drops,   │
  └──────────────┘                │  enemy_kinds,    │
        ▲                         │  economy_version │ ◀── bump após cada write
        │                         └──────────────────┘
        │ static files                    │
        │ (Vite build em web/dist)        │ poll a cada 5s
        │                                 ▼
                                  ┌──────────────────┐
                                  │  game server     │
                                  │  :9000 (ws)      │
                                  └──────────────────┘
                                          │
                                          │ ServerMessage::ItemsConfig
                                          ▼
                                  ┌──────────────────┐
                                  │  Cliente Unity   │
                                  │  ItemsConfigCache│
                                  └──────────────────┘
```

---

## Stack

| Camada       | Onde                                                   | Roda em        |
|--------------|--------------------------------------------------------|----------------|
| Game server  | `2dEngine/crates/server` (`server` bin)                | `ws://:9000`   |
| Web/admin    | `2dEngine/crates/web` (`web` bin)                      | `http://:8090` (local) / `:3020` (prod, atrás de nginx) |
| Frontend     | `2dEngine/web/src/econ/*` (React 19 + TS, Vite)        | static em `web/dist/econ.html` |
| DB           | Postgres                                               | local: docker `solar-dev-db`. prod: `mmo_dev` |
| Cliente      | `MMORPG/Assets/_Project/Scripts/UI/ItemsConfigCache.cs`| Unity build    |

---

## Tabelas envolvidas

### `items` — definição de cada item
Colunas relevantes pra edição:
- `id, name, sell_price, buy_price, shop_order, stack_max` (existentes)
- `equip_slot` — `Weapon|Armor|Helm|Legs|Boots|Gloves|Belt|Cape|Necklace|Ring|Offhand|NULL`
- `item_level` — usado se enemy não tiver `loot_item_level`
- `icon_col, icon_row` — fallback do spritesheet `raven_icons.png`
- `icon_path` — `Items/<slug>` (Resources do cliente). Tem prioridade sobre col/row.
- `hp_min/max, mp_min/max, atk_min/max, def_min/max, dex_min/max, wis_min/max` —
  ranges rolados em `ItemInstance::roll_with_template` quando o mob dropa.

### `loot_drops` — quem dropa o quê
`(id, enemy_kind, item_id, qty_min, qty_max, chance)`

### `enemy_kinds` — stats dos mobs
HP, dano, speed, XP… + `loot_item_level` (sobrescreve `items.item_level`).

### `economy_version` — sentinela de hot-reload
Linha única `(id=1, version, updated_at)`. Cada write do admin faz `version++`.
Game server faz `SELECT version FROM economy_version` a cada 5s (em
`spawn_hot_reload`). Se mudou, recarrega tudo via `load_from_db`.

---

## Hot-reload — caminho completo

```
Admin clica Salvar
    │
    ▼
PUT /api/econ/items/:id  (econ_admin.rs)
    │  UPDATE items SET ...
    │  UPDATE economy_version SET version = version + 1
    ▼
(volta pro admin: { ok: true, version: N })

Em até 5s — game server tick:
    │  economy::current_version() != world.last_econ_version
    │
    │  world.step():
    │    last_econ_version = v
    │    for s in sessions { send ItemsConfig { item_configs: snapshot } }
    │
    ▼
Cliente Unity:
    │  ItemsConfigCache.OnMessage(ItemsConfig)
    │    _byId[id] = entry  (atualiza nome/icone/slot)
    │    _spriteCache.Clear()
    │
    ▼
Próxima vez que ItemInfo.NameOf(id) ou IconOf(id) é chamado, devolve
o valor novo (sem rebuild do cliente).
```

---

## Auth

- Senha master via env var:
  - `ECON_ADMIN_PASSWORD` (preferido)
  - cai pra `PIXEL_ADMIN_PASSWORD` se não setada
  - default só pra dev: `Nop1nop2!`
- JWT HS256 — secret derivada (`econ-jwt-{senha}`) ou via `ECON_JWT_SECRET`.
- Sessão dura 7 dias. Token salvo em `localStorage` (key `econ_token`) +
  cookie `econ_auth` (Path=/, SameSite=Lax).
- Toda rota `/api/econ/*` exceto `/login` checa Authorization Bearer ou cookie.

### `ICONS_DIR` — pasta dos sprites

Setando `ICONS_DIR` no env do `web`, o picker do admin lista todos os PNGs
dela (gerando thumbnails via `GET /api/econ/icons/:name`). Sem essa env,
o picker some — sobra a drop-zone manual.

```
ICONS_DIR=/Users/bruno/MMORPG/Assets/_Project/Resources/Items
```

---

## Rodando local

### Pré-requisitos

- Docker rodando o Postgres `solar-dev-db` (porta 5432 → `mmo_dev`).
- Rust (`~/.cargo/bin/cargo`).
- Node + pnpm (pra rebuildar o frontend admin).

### Subir tudo

```bash
# 1) Game server (bind ws://:9000)
cd /Users/bruno/2dEngine
cargo run --release --bin server

# 2) Web crate (bind http://:8090) — em outra aba
cd /Users/bruno/2dEngine
ECON_ADMIN_PASSWORD=admin123 \
WEB_BIND=127.0.0.1:8090 \
ICONS_DIR=/Users/bruno/MMORPG/Assets/_Project/Resources/Items \
cargo run --release --bin web
```

Boot do server faz a migration (ALTER TABLE com IF NOT EXISTS), seed dos 44
items + backfill de `icon_path = Items/<slug>` em rows que ainda não foram
editadas, e seed do `loot_item_level` por kind. Idempotente — pode rodar
quantas vezes quiser.

### Acessar

- Admin: <http://127.0.0.1:8090/econ.html>
- Senha: `admin123` (ou o valor de `ECON_ADMIN_PASSWORD`)

### Rebuild do front após mexer em `web/src/econ/*`

```bash
cd /Users/bruno/2dEngine/web
pnpm install   # 1ª vez
pnpm run build # gera web/dist/econ.html + assets
```

Web crate serve `web/dist/` direto (`fallback_service` com `ServeDir`).
Hard-refresh (`Ctrl+Shift+R`) no navegador pra invalidar cache.

---

## Rodando em produção

### Setup inicial (uma vez)

1. **Build local cross-platform**: `cargo build --release` em `2dEngine`.
   `pnpm run build` no `web/` antes pra ter o `dist/` atualizado.
2. **Push pro VPS** (`mmo-vps` alias `~/.ssh/mmo_vps_ed25519`):
   ```bash
   rsync -avz --delete \
     --exclude target --exclude node_modules \
     /Users/bruno/2dEngine/ mmo-vps:/opt/mmorpg/2dEngine/
   ```
3. **Build no VPS**: `ssh mmo-vps`, `cd /opt/mmorpg/2dEngine && cargo build --release`.
4. **Configurar env do systemd** (`/etc/systemd/system/mmorpg-web.service`):
   ```ini
   [Service]
   Environment=DATABASE_URL=postgres://...
   Environment=WEB_BIND=127.0.0.1:3020
   Environment=ECON_ADMIN_PASSWORD=<senha-forte>
   Environment=ECON_JWT_SECRET=<secret-aleatorio>
   Environment=ICONS_DIR=/opt/mmorpg/icons
   ExecStart=/opt/mmorpg/2dEngine/target/release/web
   ```
4b. **Mirror dos sprites** (`/opt/mmorpg/icons/` no VPS) — o admin precisa
    poder ler os PNGs pra mostrar o picker:
   ```bash
   rsync -avz --delete \
     /Users/bruno/MMORPG/Assets/_Project/Resources/Items/ \
     mmo-vps:/opt/mmorpg/icons/
   ```
   Re-rodar quando adicionar sprites novos no Unity.
5. **Nginx** já tá roteando `/api/` → `127.0.0.1:3020`. O fallback `/` serve
   `web/dist` — basta apontar `WEB_STATIC=/opt/mmorpg/2dEngine/web/dist` se
   o cwd não for `/opt/mmorpg/2dEngine`.
6. **Reload nginx + restart serviços**:
   ```bash
   systemctl daemon-reload
   systemctl restart mmorpg-web mmorpg-server
   ```

### Acessar em prod

- Admin: <https://mmo.brunji.com.br/econ.html>
- Senha: o `ECON_ADMIN_PASSWORD` que você setou no systemd.

### Deploy de uma mudança no admin

```bash
# 1) Local: rebuild front + push
cd /Users/bruno/2dEngine/web && pnpm run build
rsync -avz dist/ mmo-vps:/opt/mmorpg/2dEngine/web/dist/

# 2) Se mexeu em código Rust:
rsync -avz --delete --exclude target --exclude node_modules \
  /Users/bruno/2dEngine/crates/ mmo-vps:/opt/mmorpg/2dEngine/crates/
ssh mmo-vps 'cd /opt/mmorpg/2dEngine && cargo build --release && \
  systemctl restart mmorpg-web mmorpg-server'
```

> Cloudflare cacheia static. Após push do `dist/`, considere **purge cache**
> no painel CF — senão usuários antigos pegam JS velho.

---

## Pipeline pra adicionar sprite novo

1. **Coloca o PNG** (16×16 ou múltiplo, qualquer tamanho serve mas o ItemSlot
   no inventário renderiza ~44px com `image-rendering: pixelated`):
   ```
   MMORPG/Assets/_Project/Resources/Items/<seu_item>.png
   ```
2. **Abre o Unity Editor** (importa o asset; o `.meta` é gerado automático
   se não existir; pode editar pra `Filter Mode: Point`, `Pixels Per Unit: 16`).
3. **Admin** → aba Itens → Editar Item → na seção **Sprites disponíveis**,
   o novo PNG aparece automático (server lê a pasta). Clica nele → preenche
   `icon_path = Items/<seu_item>`. Alternativamente arrasta na drop-zone ou
   digita o path manualmente.
4. **Salvar**. Server bumpa version, broadcast, cliente recebe e o ícone
   troca ao vivo no inventário (próxima abertura ou Refresh).

> Drag-drop no browser só captura **nome do arquivo**, não o path absoluto
> (limite do HTML5 File API). Por isso a convenção: PNGs sempre em
> `Resources/Items/`. Admin grava `Items/<basename-sem-extensão>` em
> `icon_path`.

### Sem path (modo legacy)

Se preferir usar o spritesheet `raven_icons.png` (16×137 sub-sprites,
`Items/icon_col`, `Items/icon_row`), deixe `icon_path` vazio e ajuste os
campos `Icon Col` / `Icon Row` no editor. Cliente cai no
`ItemIconLibrary.GetByGrid(col, row)` quando `icon_path` é `null`.

---

## Endpoints `/api/econ/*`

```
POST   /login          { password }                          → 200 { ok, token }
POST   /verify                                               → 200 { ok }
GET    /items                                                → { items: [...] }
POST   /items          { ItemPayload }                       → upsert  + bump
PUT    /items/:id      { ItemPayload }                       → update  + bump
DELETE /items/:id                                            → delete  + bump
GET    /enemies                                              → { enemies: [...] }
PUT    /enemies/:kind  { EnemyPayload }                      → update  + bump
GET    /drops?kind=N   (kind opcional)                       → { drops: [...] }
POST   /drops          { DropPayload }                       → insert  + bump
PUT    /drops/:id      { DropPayload }                       → update  + bump
DELETE /drops/:id                                            → delete  + bump
GET    /icons                                                → { configured, icons: [...] }
GET    /icons/:name                                          → image/png (cache 5min)
```

> `/icons` lista basenames (sem extensão) dos PNGs em `ICONS_DIR`.
> `/icons/:name` serve o arquivo. Anti-traversal: nome aceita só
> `[A-Za-z0-9._-]` e rejeita `..`.

Toda mutation termina com `UPDATE economy_version SET version = version + 1`.

---

## Wire — `ServerMessage::ItemsConfig`

Enviado pelo game server:
- 1× quando o cliente loga (junto com `InventoryUpdate` e `StatsUpdate`).
- A cada hot-reload (broadcast pra todos os clientes logados).

```jsonc
{
  "type": "ItemsConfig",
  "item_configs": [
    {
      "id": 3,
      "name": "Espada",
      "icon_path": "Items/sword",
      "icon_col": 1,
      "icon_row": 90,
      "equip_slot": "Weapon"
    }
  ]
}
```

> Campo `item_configs` (não `items`) pra não colidir com `ShopOpen.items`
> que reusa o mesmo C# `ServerMessage` deserializer.

Cliente: `ItemsConfigCache` (em `MMORPG/Assets/_Project/Scripts/UI/`) faz
auto-subscribe via `RuntimeInitializeOnLoadMethod` e popula um
`Dictionary<uint, Entry>`. `ItemInfo.NameOf` e `ItemInfo.IconOf` consultam
ele antes de cair pro hardcoded.

---

## Troubleshooting

| Sintoma | Causa provável | Fix |
|---------|----------------|-----|
| `Address already in use (os error 48)` no boot do web | Porta 8080 ocupada (talvez Pixel Editor) | Use `WEB_BIND=127.0.0.1:8090` |
| `senha invalida` no admin local | env var não setada antes do `cargo run` | `export ECON_ADMIN_PASSWORD=admin123` ou prefixe na linha |
| `não autenticado` em chamadas após login | Token expirou (7 dias) ou JWT secret mudou | Logout + login. Se mudou senha admin, precisa re-logar (secret é derivado da senha) |
| Edit não aparece no jogo | Game server não detectou bump | Aguarde 5s. Se persistir, ver log `economy hot-reload: vN`. Reinicie `mmorpg-server` em último caso |
| Ícone não troca após drop do PNG | Cliente não tem o asset no Resources | Confirme PNG em `Assets/_Project/Resources/Items/<slug>.png`. Cliente faz `Resources.Load<Sprite>("Items/<slug>")` — sem extensão |
| Build falha com erro do `@vitejs/plugin-react` | Plugin v6 só com Vite 6+ | Já fixado em `^4` no `package.json`. `pnpm install` se faltar |

### Logs úteis

```bash
# Game server (local)
tail -f /private/tmp/claude-501/.../tasks/<id>.output | grep -E "hot-reload|Error"

# Game server (prod)
ssh mmo-vps 'tail -f /opt/mmorpg/logs/server.log | grep -E "hot-reload|Error"'

# Web (prod)
ssh mmo-vps 'journalctl -u mmorpg-web -f'

# DB direto
docker exec solar-dev-db psql -U solar -d mmo_dev -c \
  "SELECT version FROM economy_version; SELECT id, name, icon_path FROM items LIMIT 5;"
```

---

## Limites conhecidos

- **Adicionar item novo (id > 44) via admin** funciona no DB, mas o cliente
  Unity ainda tem `IconGridOf` hardcoded em `ItemInfo.cs`. O `ItemsConfigCache`
  cobre nome+ícone, mas tooltip/`BonusOf` cai pro hardcoded → mostra 0 stats
  pra item novo sem instance. Pra suporte completo, expor `BonusOf` via wire
  também (próxima fase).
- **Admin não cria sprite** — só referencia. Coloca o PNG no `Resources/Items/`
  manualmente antes.
- **Sem audit log** — admin sobrescreve direto. Se quiser histórico, adicionar
  uma `economy_audit` table com diffs.
- **Senha única** — não tem multi-user. Tudo vai como `econ-admin`.

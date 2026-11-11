# Release 197 — checklist (prepared 11/11/2026, not shipped yet)

Everything is committed on the release branch; the builds wait for brunjilinux
(test server, Android and Windows/Linux builds run there).

## What goes out
- Market buy orders (`mercado_pedidos` on the central DB, `PedirCompra` / `VenderAoPedido` / cancel).
- Economy: Darksteel only from rocks (+ login calendar, shop, gifts); monsters drop craft
  materials at 30% (-70%, `shared::escassez`); no Darksteel in dungeon/field-boss chests.
- Keys: purple 40-69, legendary 70+ (0.3% dungeon, 0.1% world boss); field-boss chest key
  10/25/50% by chest color, area color, max purple (`forte::chave_do_bau`).
- Combine guarantee every 25th attempt per color for keys, pets and mounts
  (`combinar::GARANTIA`, counters in `characters.dungeon_json`).
- XP penalty for monsters more than 5 levels below: -8%/level, floor 20% (`combat::fator_xp_por_nivel`).
- Caverns out of the game (`dungeon::no_jogo`); their tables still feed field-boss chests.
- Bag "Help" tab (`shared::ajuda`, `client/ajuda_ui.rs`).
- Server list shows fullness color instead of player count.
- Character fixes: case-insensitive unique names at creation; client re-enters the played
  character by exact name; account list prunes characters gone from the DB.
- Latency log (`server::latencia`, table `latencias`, Panopticon tab "rede"): network RTT measured by the
  server (WebSocket ping), the ping the player sees, FPS, platform and real IP (nginx `X-Real-IP`).
- Honest on-screen ping: the server answers `Ping` in the connection task (no 30 Hz tick wait) and the
  client times the `Pong` on arrival in the network thread (no frame wait).
- Dungeon reconnect grace: 90 s to come back into the same run (`RECONEXAO_DG_S`).
- Rate-us hourly notice moved to Panopticon schedules in prod (cron removed) — already live.
- Panopticon: flash-offer campaigns tab; country column in 'rede' (needs /opt/tempest-prod/geoip/pais.mmdb, already on mmo-vps; test server needs a copy or PANOPTICO_GEOIP).
- Protocol 197.

## Order (as always)
1. Test server + Tempest TESTE iPhone, owner tests.
2. AAB to Google Play (production) and wait for approval.
3. Maintenance with announcements; during the cutover, with ALL `tempest-prod-*` units STOPPED:
   `ssh mmo-vps 'bash -s' < scripts/renomear-personagem.sh Hatuna Hatuna2`
   (account 25 "Hatuna", created 11/11, clashes with account 2 "hatuna").
4. Server/Panopticon binaries, site downloads + `releases.json`, iPhone prod.

## Known pre-existing test failures (not from this release)
magica::testes_da_ilhota_limpa, balanceamento::metas_dos_chefes,
balanceamento::todo_telegrafico_e_esquivavel, loot_mobs::nenhum_bicho_do_bestiario_cai_sem_nada,
personagens::nenhum_render_target_com_profundidade_no_cliente, vox::todo_npc_tem_as_dez_pecas_e_cabe_no_lote.

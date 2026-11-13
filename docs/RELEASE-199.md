# Release 199 — pronta pra subir (preparada em 13/11/2026, NÃO está no ar)

Tudo commitado no branch `release-2026-11-09`, com tag `v199-pre` e push no GitHub.
Prod continua na 198 (src 19ad434). Nada foi buildado nem publicado.

- Protocolo **199** · build do cliente **2026111301** · Android **versionCode 17**
- Patch notes (inglês) em `docs/PATCHNOTES.txt`, selados. Changelog do time em `docs/CHANGELOG.md`.

## O que entra

### Prime Merchant (NPC novo na praça de cada vila)
- Loja **só de ouro**. Preço = preço recomendado do mercado **+15%**, arredondado pra cima. O ouro é destruído (sink).
- Catálogo e limite diário por personagem: XP Potion 5 · Energia 20 · Darksteel 50.000 · chaves cinza→roxa 3 · materiais cinza→azul 500.
- Ficam de fora de propósito: chave lendária, material roxo e Glittering Powder.
- O quadro mostra o histórico de preço das últimas 2 h.
- **Poção de XP só nas diárias**, até 3 por dia. Saiu das missões de história, secundárias e avulsas.
- Onde mexer: `crates/shared/src/prime.rs` (`LINHAS`, `MARKUP_PCT`), servidor em `server/prime.rs` + `server/world/prime.rs`, tela em `client/prime_ui.rs`.

### Clã
- Tag **[TAG]** de 2 a 5 letras, única e com filtro de palavrões. Aparece no nome, no chat e no grupo.
- Lista de online em qualquer canal, com ilha, canal e "visto por último".
- Cargo novo de **oficial**: convida, expulsa membro comum e edita o aviso.
- Janela do clã refeita.

### PK
- Modos **Livre / Grupo / Clã / Pacífico**.
- Quem está com PK ligado ou nome vermelho pode ser atacado por qualquer um, sem penalidade.
- Ponto de PK só ao matar um Pacífico limpo.
- Perda de XP na morte: 10% + 5% por ponto, até 50%. Só os 10% base podem ser recuperados.
- −1 ponto por hora online, ou perdão por ouro (1.000 × nível por ponto).
- Na Ilha Mágica, fora do porto de chegada, o PK fica travado em Grupo e não gera pontos.

### PvP e combate
- Dano em jogador: skill ×0.10, básico ×0.15. Penalidade por diferença de nível, sem teto por golpe e sem execute da katana em jogador.
- Skills seguem o intervalo base da arma: attack speed acelera só o básico (build de pistola com DEX dava 2× o previsto nas skills).
- Life Drain cura sobre o dano que entra.
- Simulador de PvP: `pvp_sim`, `/pvp` no endpoint admin e aba **pvp** no Panopticon.
- Removidas skills e mecânicas mortas: Hunter's Mark, Sword Dance, Bloodthirst, Riposte, Quick Draw, bloqueio, parry e Master's Counter.

### Mercado e bolsa
- Quantidade digitada em todo seletor, tier no nome do item, listas roladas com o dedo.
- Aviso vermelho em anúncio ±50% fora do valor de mercado, com **Adjust price / Adjust all**.
- Atalhos na bolsa: View on market / Sell / Buy order.

### Outros
- Ward Charm protege refino só até +7.
- Nenhum monstro nasce a menos de 12 m de veio roxo.
- Telemetria `magica_extensao` (extensões da Ilha Mágica feitas lá de dentro).
- `enviar_kit`: KIT_ITEM / KIT_QTD / KIT_SUFIXO.

## Testes (rodados na junção, Mac)
- server: 334 passaram, 3 falhas já conhecidas
- shared: 450 passaram, 1 falha já conhecida
- client: 558 passaram, 2 falhas já conhecidas

Falhas que já vinham de antes (não são desta release): `magica::testes_da_ilhota_limpa`,
`balanceamento::metas_dos_chefes`, `balanceamento::todo_telegrafico_e_esquivavel`,
`loot_mobs::nenhum_bicho_do_bestiario_cai_sem_nada`,
`personagens::nenhum_render_target_com_profundidade_no_cliente`,
`vox::todo_npc_tem_as_dez_pecas_e_cabe_no_lote`.

## Pra subir (quando o Bruno der o ok)
1. Servidor de teste + Tempest TESTE no iPhone. O dono testa: Prime, clã, PK e mercado.
2. AAB (versionCode 17) pra produção na Google Play, e **esperar a aprovação**. Servidor antes da Play deixa o jogador Android travado sem update.
3. Manutenção com avisos (`/opt/tempest-prod/avisos/aviso.sh`). Na mmo-vps: `git archive` deste commit pra `/opt/tempest-prod/src`, build do `server`+`supervisor` e, separado, do `web`+`panoptico`. Backup em `/opt/tempest-prod/backups/release-199`.
4. As migrações rodam sozinhas no boot (`ALTER TABLE characters ADD COLUMN IF NOT EXISTS`): `diario_json` (contadores diários do Prime) e `pk_modo`, `pk_decai_s`, `pk_hostil`, `pk_points` (PK). Depois de subir, conferir no `tempest_sa01` se elas existem.
5. Site: downloads + `releases.json` (protocol 199) para Windows, Mac e Linux. Depois o iPhone de prod.

# NAVEGAÇÃO — o mar como sistema (plano, 15/11/2026)

> Status: **FASES 1, 2 E 4 FEITAS (15-16/11/2026, versão de TESTE, protocolo
> 199 ainda não lançado), e o visual do mar REFEITO (ver "Remake visual").** A fase 3 (comércio) foi cortada; a 5 fica para
> depois. A fase 4 está em "Fase 4 — como ficou". O dono escolheu a "opção
> C": navio próprio, mar navegável entre as ilhas, upgrades e PvP do
> Corsário. O que a fase 1 virou de verdade está em "Fase 1 — como ficou",
> logo abaixo das fases.

## A lição de setembro

O jogo já teve um mar aberto completo (commits `1b5ea33`…`3cd9ff4`, removido
em `95c5d3a`, 21/09/2026; o desenho antigo está em
`git show 95c5d3a^:docs/MAR_ABERTO.md`). Ele caiu por **um** motivo: trocar
de ilha **exigia** navegar. Vários minutos de pedágio numa coisa que se faz
toda hora — *"essa mecânica tá furada"*.

O que funcionou e volta:

- **Navegar é andar, só que na água.** Mesmo direcional, mesma câmera, mesma
  rota no mapa. O barco com leme e acelerador (20/09) perdia "o referencial de
  frente e trás" e foi cortado.
- **A ilha vista do mar é uma silhueta.** Um domo por ilha e um cais
  marcando a entrada, sem gerar o relevo inteiro, o que fica barato no
  celular.
- **No mar quem apanha é o casco.** Bicho do mar rói o casco: a resposta é
  desviar, correr ou atirar, e é isso que dá sentido ao reparo e ao upgrade.
- **O tesouro nunca é destruído.** Afundar quem carrega faz o baú boiar;
  matar em PvP o entrega ao vencedor. Negar o prêmio nunca pode ser a jogada
  vencedora.
- **Corredor da Capitania:** perto do cais ninguém apanha. A perseguição é o
  jogo; a porta não é.

## A regra nova que rege tudo

> **O mar é opcional. Nada de progressão obrigatória passa por ele.**

- O **Capitão do Porto continua levando de graça** de uma ilha pra outra,
  como hoje.
- O mar existe porque **rende**: recursos, baús, carga, chefes e PvP que só
  existem lá.
- Material exclusivo do mar serve só pra coisa **do mar** (navio, cosméticos)
  ou é atalho, nunca a única fonte de algo que a progressão de terra pede.

## Fases

### Fase 1 — O mar e o primeiro navio (fundação)

- **Zona nova `mar_aberto`**, um processo, centrada na própria caixa. O fio
  aguenta ±4096 u de posição e ~15,9 u/s de velocidade; há teste que falha se
  o mar não couber. As 4 ilhas com porto (Bosque, Geleira, Ermo, Planalto)
  aparecem como silhuetas com cais.
- **Zarpar e atracar:** no cais, o Capitão ganha "Zarpar com o meu navio"
  além da viagem grátis. No mar, chegar a um cais oferece "Atracar" →
  reconexão na zona da ilha (o mesmo `TrocarZona` de hoje). Se o processo do
  mar cair, o login volta pra ilha de onde zarpou (`zona_volta`).
- **O navio é um item** num slot próprio (um navio ativo, como a montaria). O
  primeiro, a **Chalupa**, sai de uma missão secundária do Bosque, nunca da
  história principal.
- **O que fazer no mar:**
  - **destroços boiando** (coleta: madeira, aço, darksteel, energia, na
    faixa de nível do trecho);
  - **cardumes** (pesca simples: AUTO COLETA no barco);
  - **baús à deriva** que nascem de tempos em tempos.
- **Perigo:** bichos do mar que roem o casco (tubarão, enguia) e **recifes**
  em volta das silhuetas. Casco a zero → o navio fica "avariado", você
  reaparece no último cais e conserta lá (em ouro).
- **Reaproveitado:** `agua.rs` (oceano 3D com ondas e anel do horizonte, já
  no jogo), `terreno::mover_casco`/`Ilha::mover_no_mar` (o casco só anda na
  água), `rumo_do_porto`.
- **Tamanho:** grande. É a fase que cria zona, processo, unidade systemd,
  rota nginx `/z/mar_aberto` e o modo de render do mar.

### Fase 1 — como ficou (FEITA, 15/11/2026)

**Zona `mar_aberto`** (`shared::mar`, `server/world/navio.rs`).

- É uma CARTA, não o mapa-mundi: as 4 ilhas com porto ficam em `ESCALA`
  0,25 das posições do `ARQUIPELAGO`, centradas na própria caixa. O mar
  inteiro é um quadrado de ±700 u (`RAIO_BLOCOS` 1400, o mesmo tamanho de
  Abyssia: campo de altura de ~16 MB no servidor), navegável até ±680 u
  (`BORDA`). Em 1:1 seriam 5.200 u e 216 MB, e minutos por travessia — o
  pedágio que matou o mar de setembro. Uma travessia vizinha leva 30-60 s.
- Silhueta analítica (`mar::bloco_da_coluna`, `RelevoDesenhado::Mar` no
  `Gerador`): domo por ilha (raio = 15% do raio real, entre 50 e 130 u;
  Bosque e Planalto 120, Geleira e Ermo 60), plataforma rasa de 30 u em
  volta e o cais (22 × 5 u) no rumo de `DefIlha::rumo_do_porto`. Sem Perlin,
  sem árvore, sem vila. O ancoradouro é a água 3,5 u depois da ponta do cais.
- Teste do fio `o_mar_inteiro_cabe_no_orcamento_do_fio` (±700 u contra
  ±4.095 u do `i16` a `POS_SCALE` 8, e cada ilha dentro da `BORDA`).
- Entra no jogo por `terreno::def_da_zona` (`mar::DEF`), não pelo
  `ARQUIPELAGO`: servidor, terreno do cliente, minimapa e construções leem
  o mesmo `bloco_da_coluna`. Cache de altura `-mar{REVISAO}`.

**O navio é um item.** `item_id::NAVIO_CHALUPA` = 470 (Sloop / Chalupa;
471-473 reservados pros cascos da fase 2), slot próprio `EquipSlot::Navio`
(coluna `equipment.slot = 'navio'`), um navio ativo. O estado
(`NavioData { casco, avariado }`) mora em `ItemInstance::navio`, último
campo, `#[serde(default)]`. Não refina nem ascende.

- **Chalupa:** casco 2.400, 10 u/s (o andar é 5; o teto do fio é 15,9 u/s,
  teste `nenhum_casco_passa_do_teto_do_fio`), raio do casco 1,4 u.
- **Conserto:** 0,1 cobre por ponto de casco, ou seja 240 cobre do zero ao
  cheio, com o Capitão do Porto.
- **História:** o passo 719 "Bound for the Glacier"
  (`historia::PASSO_DA_CHALUPA`) entrega a Chalupa ao ser concluído
  (`navio::dar_chalupa`: no slot, senão bolsa, senão Correio). Não é
  `reward_item`: quem já tem navio em qualquer lugar (slot, bolsa, banco,
  Correio) não ganha outro.
- **Antigos:** migração `chalupa_dos_antigos_v1` (`migracoes_de_dados`): todo
  personagem com o marcador da história além do passo 719 e sem navio
  (equipamento, bolsa, banco ou Correio) recebe uma Chalupa no Correio,
  motivo `navio::MOTIVO_CORREIO_NAVIO` (8, "Your first ship").

**Capitão do Porto.**

- A viagem rápida cobra COBRE (`navio::preco_da_viagem`): 50 × (índice da
  ilha + 1) — Bosque 50, Geleira 100, Ermo 150, Planalto 200, Skyreach 250
  (o Pergaminho de Teleporte custa 100). O preço vai no botão ("Board · N
  copper"); sem cobre, recusa dizendo quanto falta; destino fora do ar não
  cobra. Ônibus e submarino seguem de graça. A viagem do passo VIAGEM ativo
  da história é grátis (pelo diálogo e pelo menu, que o AUTO usa), pra
  ninguém travar a história por falta de cobre.
- O menu do Capitão ganha a linha "My ship": "Sail" (zarpa pro ancoradouro
  desta ilha no mar) e "Repair · N copper". Navio avariado não zarpa.

**No mar.**

- Navegar é andar: o mesmo direcional; o servidor move TODO corpo com
  `terreno::mover_casco` contra `mar::navegavel` (terra e a borda da carta
  são parede), a velocidade é a do casco, e o cliente desenha um casco de
  voxels (`client::mar_ui::desenha_casco`) no lugar do boneco.
- "Dock at <ilha>" aparece a até 14 u do ancoradouro (`PERTO_DO_CAIS`) e
  leva ao porto daquela ilha (`TrocarZona`), se a história já abriu a ilha.
- `characters.zona_volta`: a ilha de onde zarpou. Se o processo do mar
  estiver fora do ar no login, o login vai pra ela (e cai no porto dela).
  Some ao pisar em terra.
- Corredor da Capitania: 40 u em volta de cada ancoradouro, ninguém apanha
  (nem de bicho nem de jogador) e não nasce bicho.
- PvP: os modos de PK de hoje valem sem mudança (`can_damage_player`); só o
  corredor protege.
- Bichos do mar: o Tubarão de Recife (70) e a Enguia-Tigre (72) de Abyssia,
  em ninhos a 1/3 e 2/3 de cada rota entre ancoradouros (12 ninhos, 3 vagas),
  no nível do trecho (`mar::nivel_em`: a faixa da ilha mais perto, subindo
  da costa até 250 u mar adentro). A mordida vai pro CASCO, não pra vida.
  Casco a 30% avisa; a zero o navio fica avariado e o jogador volta pro cais
  de onde zarpou (ou o mais perto que esteja no ar).
- Destroços boiando (`mar::destroco_da_coluna`): um em cada 5 células de
  20 u de água livre (~900 no mar); 30% madeira, 50% caixote de Aço da cor
  do trecho (rende como pedra: Aço + Darksteel), 20% Energia. Coleta pelo
  mesmo pipeline das pedras e árvores (esgota e volta).
- Baús à deriva: 1 por rota (6), um pouco fora da linha; dão cobre
  (60 + 6 × nível), Darksteel (10 + 2 × nível), Aço e madeira da cor do
  trecho; voltam em 180 s.

**Gancho da fase 2:** `navio::CASCOS` e `navio::proximo_casco` — usados na
fase 2 (ver "Fase 2 — como ficou").

**Cortado da fase 1:** cardumes/pesca no barco, recifes em volta das
silhuetas (a costa do domo já é parede), arte de casco em arquivo `.vox` (o
casco é de cubos), bicho do mar próprio (reusa os de Abyssia), "metas do
mar" com simulação de quanto o casco aguenta.

**Servidor de TESTE (brunjilinux):** unidade `tempest-teste-mar.service`
(`MMO_ZONA=mar_aberto`, `MMO_CANAL=mar1`, `MMO_CANAL_UNICO=1`,
`BIND_ADDR=127.0.0.1:19106`, `PANOPTICO_BIND=127.0.0.1:10106`,
`MMO_HOST_PUBLICO=100.64.0.23:8095/z/mar_aberto`) e a rota
`/z/mar_aberto* → 127.0.0.1:19106` no `~/tempest-teste/Caddyfile`. Roda o
`~/tempest-teste/bin/server`: o mar só existe depois que esse binário for
trocado por um com esta fase.

**Produção (a fazer na mmo-vps, NÃO feito):**

1. `/opt/tempest-prod/units` (e `/etc/systemd`): `tempest-prod-mar.service`
   copiado da Geleira com `MMO_ZONA=mar_aberto`, um `MMO_CANAL` próprio,
   `MMO_CANAL_UNICO=1`, uma porta livre (ex.: 9106) e
   `MMO_HOST_PUBLICO=mmo.brunji.com.br/z/mar_aberto`; habilitar e subir junto
   com o resto da release.
2. nginx: `location /z/mar_aberto` → a porta nova (o mesmo bloco de
   websocket das outras ilhas); se houver túnel por porta, o
   `tempest-prod-tunel-mar` e o `-R` correspondentes.
3. Na primeira subida: a coluna `characters.zona_volta`, o item 470
   ("Sloop") e a migração `chalupa_dos_antigos_v1` rodam sozinhos. Subir com
   TODOS os processos parados (como o `bau_da_ilha_correio_v1`): um processo
   antigo no ar gravaria o `dungeon_json` da memória por cima do Correio.
4. Cache de altura: o primeiro boot gera `data/ilhas/<semente>-1400-mar1.alt`
   (~1 s).

### Fase 2 — O navio como progressão

- **Cascos por classe:** Chalupa → Escuna → Galeão → Nau, feitos no Estaleiro
  (craft com madeira T1–T4, aço e um material exclusivo do mar, a **Prancha
  de Naufrágio**).
- **Upgrades em eixos:** Casco (vida), Vela (velocidade), Canhão (dano) e
  Porão (espaço de carga). Usar o mesmo padrão de **estrelas** da ascensão
  (`shared::essencia`) para não inventar uma terceira progressão.
- **Skills do navio:** canhonaço (frente), bordada (lados), reparo de
  emergência e fumaça (fuga). Usam a barra de skills enquanto se navega.
- **Cosméticos:** velas, figura de proa e rastro, à venda na loja de TP e na
  de Essência. Sem arte nova pesada: cor da vela e um punhado de
  figuras-de-proa em voxel.

### Fase 2 — como ficou (FEITA, 16/11/2026, versão de TESTE, protocolo 199)

Tudo em `shared::navio` (números puros, cliente e servidor leem a mesma
tabela), `server/world/navio.rs` e `client/mar_ui.rs` + `client/viagem_ui.rs`.
Protocolo: `PedidoNavio::{Construir, Melhorar{eixo}, Habilidade{id}}` no FIM
do enum; `NavioData` ganhou `estrelas: [u8; 3]`, `vela`, `figura` no FIM, com
`#[serde(default)]` (navio da fase 1 carrega com zero estrela — teste
`navio_antigo_carrega_e_estrelas_persistem`). `PROTOCOL_VERSION` segue 199.

**Cascos por classe** (`navio::CASCOS`, itens 471-473, que eram linhas mortas
de montaria antiga no banco: nome, slot `navio` e nível forçados no boot).
O nível é o de CONSTRUIR no Estaleiro (interno; a tela mostra pela escala nova).

| Classe | Item | Casco | Vel. (u/s) | Nível (interno → tela) | Canhão | Mastros |
|---|---|---|---|---|---|---|
| Chalupa (Sloop) | 470 | 2.400 | 10,0 | 1 → 1 | ×1,00 | 1 |
| Escuna (Schooner) | 471 | 3.600 | 11,2 | 15 → 30 | ×1,15 | 2 |
| Galeão (Galleon) | 472 | 5.200 | 12,4 | 30 → 50 | ×1,30 | 3 |
| Nau (Man-o'-War) | 473 | 7.200 | 13,4 | 42 → 70 | ×1,50 | 3 + portinholas |

Velocidade com 5 estrelas de vela: 11,5 / 12,9 / 14,3 / 15,4 u/s. Teto
`navio::VELOCIDADE_MAX` 15,5 (a Fumaça também é cortada nele), abaixo do
`TETO_DO_FIO` 15,875 — teste `nenhum_casco_passa_do_teto_do_fio`. Uma
travessia de 600 u fica entre 60 s (Chalupa) e ~39 s (o mais rápido possível).
O raio de colisão é o mesmo 1,4 u pra todas: a classe só muda o desenho
(escala 1,0 → 1,38).

**O Estaleiro** é uma seção do painel do Capitão do Porto (botão "Shipwright"
na linha "My ship"): mesma janela, sem NPC novo, sem receita no Craft. Ele
age no navio do espaço Navio. Construir CONSOME o casco anterior e devolve o
novo com casco cheio, levando as estrelas e os cosméticos (nada investido se
perde). Receitas (`navio::RECEITAS`):

| Para | Consome | Madeira | Aço | Prancha de Naufrágio |
|---|---|---|---|---|
| Escuna | Chalupa | T1 ×60 + T2 ×40 | verde ×40 | ×20 |
| Galeão | Escuna | T2 ×80 + T3 ×50 | azul ×80 | ×50 |
| Nau | Galeão | T3 ×100 + T4 ×60 | roxo ×120 | ×100 |

**Estrelas do navio** (`custo_da_estrela`, `NavioData::estrelas`): três eixos,
0 a 5 cada, sempre dão certo, no Estaleiro. NUNCA custam Essência do
Renascimento (regra do dono: Essência é só da estrela de item). Ficam na
instância do item, então troca, mercado, banco e Correio levam junto.

| Estrela | Cobre | Darksteel | Pranchas |
|---|---|---|---|
| 0 → 1 | 1.000 | 500 | 5 |
| 1 → 2 | 2.000 | 1.000 | 10 |
| 2 → 3 | 4.000 | 2.000 | 15 |
| 3 → 4 | 8.000 | 4.000 | 20 |
| 4 → 5 | 16.000 | 8.000 | 25 |
| eixo cheio | 31.000 | 15.500 | 75 |

| Eixo | Por estrela | 5 estrelas |
|---|---|---|
| Casco (Hull) | +8% do casco | +40% |
| Vela (Sails) | +3% de velocidade | +15% |
| Canhão (Cannon) | +8% do dano das skills | +40% |

A estrela de casco sobe o casco atual pelo que ela acrescenta (casco
avariado continua avariado). O conserto do Capitão cobra pelo casco com
estrelas (0,1 cobre/ponto). Sem Porão: o comércio foi cortado.

**Prancha de Naufrágio** (item 478, `item_id::PRANCHA_DE_NAUFRAGIO`, só do
mar, só pra navio): 1 por destroço coletado (madeira ou caixote de Aço; o de
Energia não), 4 + nível/15 por baú à deriva, 25% de chance de 1 por bicho do
mar morto e 6 garantidas pelo elite.

**Skills do navio** (`navio::HABILIDADES`): no mar, os slots 1-4 da barra
viram do navio (o 4º ocupa o lugar do pulo; as skills da arma e as ativas da
árvore esperam em terra). Instantâneas, sem mana, recarga própria em
`skill_cds` com ids 300-303 (o cliente lê no `SkillsState` de sempre). O
dano é `ladder::attack(nível do capitão) × mult da skill × canhão da classe
× estrelas de canhão`; contra bicho vira `pending_skill_hits` (defesa,
diferença de nível e crédito da morte como qualquer golpe); contra jogador
vai direto no CASCO (casco contra casco, `PVP_CANHAO` 1,0), só se
`can_damage_player` deixar (modos de PK de hoje) e fora do corredor da
Capitania.

| Slot | Skill | Efeito | Recarga | Alcance | Dano | AUTO |
|---|---|---|---|---|---|---|
| 1 | Bordada (Broadside) | todo inimigo a até 12 m nos costados (proa e popa cegas, \|cos\| ≤ 0,8; parado continua atirando só dos costados, pelo rumo do leme) | 6 s | 12 m | ×1,5 | sim |
| 2 | Canhão Longo (Long Gun; era o Canhão de Proa até 16/11) | um tiro no alvo selecionado, **só com ele num costado** | 5 s | 22 m | ×0,6 | sim |
| 3 | Reparo de Emergência | +30% do casco cheio | 60 s | — | — | não |
| 4 | Fumaça (Smoke) | invisível/intocável 3 s (a invisibilidade das ativas) + 30% de velocidade por 4 s | 30 s | — | — | não |

AUTO: arrastar pra cima liga (só os canhões), o mesmo gesto da barra da
arma; dispara quando o alvo selecionado está no alcance. Teste de sentido
`os_bichos_do_mar_aguentam_alguns_tiros`: o Tubarão (perfil do lobo) e a
Enguia (do tigre) no nível do capitão levam 2-6 Bordadas de uma Chalupa
crua, menos de uma Nau com 5 estrelas de canhão, e o elite 8-30.

**Elite do mar:** um por rota — o primeiro lugar do ninho a 2/3 de cada rota
(`e_ninho_de_elite`: zona ímpar) nasce "[ELITE] Dread …", ×5 de vida, ×1,4
de mordida, ×4 de XP, 6 Pranchas pra quem afunda, e volta em 180 s.

**Cosméticos** (itens da loja de TP, aba Skins › Ship, `loja::ITENS_DA_LOJA`
ids 2-9; nunca por Essência). Usar da bolsa ("Apply to ship") aplica no navio
do espaço Navio e gasta o item. Os outros jogadores veem: no mar o visual
(classe, estrelas, vela, figura) vai nos bytes livres 5-7 de
`EntityMeta::skins` (`navio::Visual::no_fio`).

| Item | Nome | TP |
|---|---|---|
| 580 | Crimson Sails (Velas Carmesim) | 150 |
| 581 | Black Sails (Velas Negras) | 200 |
| 582 | Royal Blue Sails (Velas Azul-Real) | 150 |
| 583 | Emerald Sails (Velas Esmeralda) | 150 |
| 584 | Golden Sails (Velas Douradas) | 300 |
| 586 | Mermaid Figurehead (Sereia) | 250 |
| 587 | Dragon Figurehead (Dragão) | 300 |
| 588 | Eagle Figurehead (Águia) | 250 |

Arte: sem arquivo novo. O casco de cubos ganhou classe (tamanho, 1-3
mastros, convés de popa e portinholas a partir do Galeão, flâmula), a vela
recolorida, amurada que vira latão com as estrelas e três figuras de proa
de cubos e esferas. Prévia `MMO_PREVIA_MAR=1` grava também `mar-classes.png`
e `mar-vestidos.png`.

**Cortado da fase 2:** Porão (comércio cortado), rastro como cosmético,
venda por Essência, recolorir de volta pra vela de lona sem item, exigência
de mirar a proa no Canhão de Proa (ele atira em qualquer direção), arte
`.vox` própria e VFX de som pros canhões (usam o som genérico de skill).

### Fase 3 — Carga e comércio entre ilhas (CORTADA pelo dono, 15/11/2026: "não precisa ter a parte do comércio, faz só a navegação e PvP")

- Cada porto compra e vende **mercadorias** (madeira de lei, peles, gelo,
  especiarias…) com preço próprio. Comprar barato numa ilha e vender caro em
  outra dá lucro.
- A carga vai no **Porão** do navio (bolsa, banco e correio recusam): é o que
  faz "leve até a outra ilha" ser verdade.
- **Economia:** o lucro tem que ser medido como fonte de ouro. A proposta é
  que o NPC compre com um teto diário por personagem e que parte do valor
  seja taxa do porto (sumidouro). Há teste de "ouro por hora no comércio ≤ X
  do farm de terra".

### Fase 4 — Corsário e PvP naval (FEITA, ver "Fase 4 — como ficou")

- **Quem carrega, arrisca:** navio com carga ou tesouro fica **marcado**
  (atacável por qualquer um, mesmo PK Pacífico). Sem carga, o mar segue os
  modos de PK de hoje (`shared::pk`).
- **Afundar quem carrega:** a carga boia nos destroços (de quem chegar
  primeiro). Vencer em PvP direto entrega a carga ao vencedor. O navio nunca
  é perdido, só avariado.
- **Baú do Colosso:** chefes do mundo deixam um tesouro que só cabe no
  convés. Abrir onde caiu paga ×1,0; levar até outra ilha paga mais (até
  ×3), e a travessia é a chance de perder.
- **Zonas sem lei:** poucas e com motivo (naufrágios no meio das rotas).
- **Sinergia com o caminho Corsário** (`shared::arvore`): passivas do mar
  (+dano de canhão, +carga, +saque de destroço, abordagem mais rápida).
  Hoje o Corsário é só loot e coleta; o mar dá o tema que faltava.
- **Karma** pra quem mata inocente sem carga: reaproveitar os pontos de PK
  atuais em vez de criar um sistema novo.

### Fase 4 — como ficou (FEITA, 16/11/2026, versão de TESTE, protocolo 199)

Sem comércio (fase 3 cortada), a única carga é o TESOURO. Tudo em
`shared::navio` (números puros), `shared::mar` (zonas sem lei), `shared::pk`
(marca por cima dos modos), `shared::arvore` (nós do mar),
`server/world/navio.rs` e `client/mar_ui.rs` + `client/mapa.rs`.
Protocolo: `PedidoNavio::AbrirTesouro` no FIM; `NavioData::conves:
Option<Tesouro>` e `PkState::marcado` no FIM, com `#[serde(default)]`.
`PROTOCOL_VERSION` segue 199.

**Baú do Colosso** (`items::Tesouro { chefe, nivel, origem, ilhas }`, vive
SÓ em `NavioData::conves`, um por convés).

- Todo chefe de CAMPO (`bosses::CHEFES` fora de dungeon) larga um, garantido,
  para QUEM MATOU (o crédito do abate, o mesmo jogador do `BossRewards`) —
  escolha: o maior contribuidor exigiria contabilidade de dano por chefe que
  o servidor não tem. Vai direto pro convés do navio do espaço Navio, em
  qualquer ilha (`dar_bau_do_colosso`). Os baús de chão do chefe continuam.
- **Sem navio, ou convés já ocupado:** abre na hora a ×1,0 (prêmio justo de
  chefe pra quem não quer PvP; nada se perde).
- **Recusas:** navio com tesouro (`navio::preso_ao_conves`) não sai do
  espaço Navio (`handle_inventory_swap` recusa tirar ou trocar), e bolsa
  (`add_to_inventory`), banco (`handle_vault_deposit`), mercado
  (`mercado_anunciar`, lê como vinculado), troca (`troca::negociavel`) e
  Correio (`DadosDungeon::postar`) recusam. O Estaleiro leva o baú pro
  casco novo (`reconstruido`).
- **Multiplicador** (`mult_do_tesouro`): ×1,0 onde caiu; +0,6 por ilha
  DISTINTA em que se atracou pelo mar com ele no convés — ×1,6 / ×2,2 /
  ×2,8 —, teto ×3,0; a passiva do Corsário soma por cima do teto (até
  +0,2). Uma ilha só conta depois de uma TRAVESSIA de verdade
  (`conta_atracacao`): atracar de volta na ilha de onde zarpou
  (`zona_volta`) não conta; voltar à ilha de origem vindo de outra conta.
  Viagem do Capitão e pergaminho não contam.
- **Abrir:** botão "Open the Colossus Chest · ×N" na HUD, em terra (no mar
  recusa: "leve pra terra"). Conteúdo (`conteudo_do_colosso`): a tabela do
  baú do chefe (`forte::conteudo_do_chefe`, estágio verde) com todo
  empilhável ESCALADO pelo multiplicador (ouro, cobre, Darksteel, Energia,
  material, Marcas), a peça que a tabela rolar (uma, sem escala), chave de
  craft a 50% (a do baú roxo; segunda rolagem a partir de ×2,0) e 10 ×
  mult Pranchas de Naufrágio.

**A marca** (`navio::marcado`, DERIVADA, nada guardado no item): no mar, com
tesouro no convés, OU nos 60 s de rastro depois que o convés esvaziou no mar
(`Session::marca_ate`, relógio do processo do mar; `TRILHA_DA_MARCA_S`), OU
com a pedra-chave Código Pirata. Marcado = atacável por QUALQUER um no mar
(`pk::pode_atacar_no_mar`: lê o alvo como em modo PK, então Pacífico ataca e
a proteção por poder não vale), menos o próprio grupo/clã; o corredor da
Capitania (40 u) protege sempre. Na tela: bandeira preta com caveira de
cubos no mastro (`mar_ui::desenha_marca`, via `PkState::marcado`), nome em
vermelho e a linha vermelha "You carry treasure — anyone can attack you"
(ou "Marked — …" no rastro/Código, "Lawless waters — …" nas zonas) abaixo
da barra do casco.

**As duas mortes** (`dano_no_casco(…, atacante)`, `tesouro_perdido`):

| | navio | tesouro |
|---|---|---|
| casco a zero pelo mar (bicho) | avariado, volta ao cais | BOIA nos destroços (`TesouroBoiandoTag`, `Npc(7)`): quem chegar primeiro iça (4 s parado a até 6 u, barra "Hauling") |
| afundado por canhão de jogador / morto em PvP | avariado | vai DIRETO pro convés do matador se livre; senão boia |

Boia 120 s nos destroços (`BOIA_S`); ninguém pegou, ela deriva pro baú à
deriva mais perto e espera lá sem prazo. O navio nunca se perde, o tesouro
nunca é destruído (só some se o processo do mar reiniciar com ele boiando).

**Zonas sem lei** (`mar::zonas_sem_lei`, `RAIO_SEM_LEI` 45 u): 3, nos ninhos
de elite (2/3 do caminho) das rotas que NÃO tocam o Bosque — Geleira–Ermo,
Geleira–Planalto, Ermo–Planalto. Entram em `pk::zona_pvp_aberto`, então
funcionam como as ilhotas da Ilha Mágica: modo travado em Grupo, PvP aberto,
sem ponto de PK, aviso "Lawless waters: …" ao entrar/sair. Desenhadas em
vermelho no mapa e no minimapa (`mapa::aguas_sem_lei`). Nunca tocam um
corredor (teste).

**Pontos de PK** (`pk::pontos_por_abate_no_mar`): os de hoje. Afundar (ou
matar) um Pacífico NÃO marcado no mar dá ponto como em terra; marcado, ou em
zona sem lei, não dá. Sem karma novo.

**Corsário no mar** (`arvore::MAR`, ids 173-179 no FIM, anel 3, lado do
Guardião do braço Corsário):

| Id | Nó | Tipo | Custo | Efeito | Liga em |
|---|---|---|---|---|---|
| 173 | — | menor | 3 | canhão +4% | Deep Miner (R8) |
| 174 | — | menor | 3 | casco +5% | 173 |
| 175 | Boarding Party | notável | 4 | içar +30%, prêmio +×0,1 | 174 |
| 176 | — | menor | 3 | casco +5% | Sea Legs (ponte Corsário│Guardião) |
| 177 | — | menor | 3 | içar +15% | 176 |
| 178 | Plunderer | notável | 4 | prêmio +×0,1, canhão +5% | 177 |
| 179 | Pirate's Code | pedra-chave | 6 | +25% de canhão contra navio marcado; SEMPRE marcado no mar | 175 e 178 |

"Casco +X%" = o casco toma X% menos dano (o mesmo que X% a mais de casco,
sem mexer no número da barra). Teia: 180 nós (13 pedras-chave, 38 notáveis,
116 menores), custo total **486** (era 460).

**Cortado da fase 4:** carga de comércio (fase 3), o "+carga" e o "+saque de
destroço" do Corsário (sem Porão; o saque virou o prêmio do Baú), baú
esperando 60 s no chão pra quem não tem navio (abre na hora), contribuição
por dano (vai pra quem matou), rastro da marca entre processos (vive no
processo do mar), punição de quem desloga no mar com tesouro, arte `.vox` da
bandeira.

### Refazendo a jogabilidade do mar (16/11/2026, versão de TESTE, protocolo 199)

O veredito do dono: *"o mar aberto está um lixo ... tá sem auto path ... não
mostra a doca, não dá nem pra entender ... o combate tá péssimo"*. Refeito o
JOGO do mar (o visual — ilhas, docas, água, cascos, céu — é o remake visual,
em `mar_cena`/`mar_casco`/`agua`). Exigência do dono no meio do caminho: *"o
mar precisa ser o mais otimizado possível pra rodar no celular"*. Protocolo
sem variante nova; `PROTOCOL_VERSION` segue 199.

**1. Auto path no mar, igual à terra.**

- O servidor tem um **A\* de água** (`server/world/navio.rs`,
  `caminho_no_mar`): grade grossa de 4 u (`CELULA_DA_ROTA`) sobre
  `mar::navegavel`, uma célula é livre se há água no centro e num anel de
  `FOLGA_DA_ROTA` = raio do casco + 0,8 u em volta (o casco nunca raspa a
  costa por onde foi roteado). Construída UMA vez por processo (`OnceLock`,
  aquecida no `povoar_mar`), ~190 mil células, ~190 KB de `bool`. Lê o relevo
  em tempo de execução: mudar escala ou posição das ilhas não pede código
  aqui. Busca 8-vizinhos com custo octile, sem cortar quina, buffers
  reutilizados (carimbo de geração, nada se zera por pedido), orçamento de
  150 mil células; a reta livre (o caso comum) nem busca. Depois a rota é
  esticada (só os pontos onde ela vira): uma travessia sai com poucos pontos.
- `handle_mover_para` no mar chama `rota_no_mar` (sem o teto de 1.200 u da
  terra): o MESMO `SeguidorDeRota` conduz o casco, o MESMO
  `ServerMessage::Rota` desenha o tracejado — agora em cima da água
  (`mar_combate_ui::altura_do_tracejado`). Destino em terra (toque numa
  ilha) termina na água mais perto dele.
- Cliente `mar_rota.rs`: no mar todo "ir pra lá" vira UM `MoverPara` com o
  destino (o toque no mapa e no minimapa, o "Ir" do mapa, o auto coleta, o
  auto missão); reenvia só parado 2 s sem rota viva, desiste depois de 6
  pedidos sem progresso. **Toque numa ilha (ou ao lado do cais) = ir pro
  cais dela e atracar**; chegando ao alcance do cais que era o destino,
  `Atracar` sai sozinho (`Passo::Atracar`). Faixa "Sailing to Glacier · 320
  m · 32 s" (distância pela rota que falta, ETA pela velocidade do navio).
- **"Sail to…"** embaixo do painel do navio: uma linha por cais (nome,
  distância, ETA, botão "Sail"). O "Ir" de missão no mar (`PedirDestino`
  do auto missão) navega pro cais da ilha da missão (`mar_rota::ilha_da_missao`:
  o passo de VIAGEM ou a zona da missão; qualquer ilha → o cais mais perto),
  atraca e o auto missão retoma em terra (`retomar_auto_missao`). A ilha do
  mapa-múndi (`Objetivo::Embarcar`) no mar também vira "navegar até o cais".
- **Auto combate no mar:** varredura a 4 Hz (`mar_rota::VARREDURA_S`, em
  terra é todo quadro); escolhe o bicho mais perto (a mesma regra de
  prioridade); aproxima até `navio::DISTANCIA_DE_COMBATE` 8,5 m (dentro da
  Bordada de 12, fora da mordida); com o AUTO COMBATE ligado os canhões
  disparam sozinhos (a Bordada só com o alvo no costado ou parado, pra não
  gastar a recarga na água); sem bicho à vista navega pro ninho mais perto
  (`mar::ninhos`, a mesma função de onde o servidor povoa), pulando por 40 s
  o ninho que achou vazio.
- **Auto coleta no mar:** o servidor oferece o destroço com um ponto de
  aproximação NA ÁGUA e alcance pelo A\* de água (`no_de_coleta_em`; o A\* de
  terra via o mar inteiro como parede e não oferecia nada).
- No mar a arma descansa: o ataque básico de jogador não sai
  (`if ir.wants_attack && !self.no_mar()`); quem luta são os canhões.

**2. Combate que dá pra ler** (`client/mar_combate_ui.rs`).

- **Balas voam:** a skill não acerta ao apertar; o servidor lança uma
  `BalaDeCanhao` (entidade do ECS SEM `NetId`, nenhum snapshot a carrega)
  mirada onde o alvo vai estar (`navio::mira_com_avanco`, um passo de
  avanço pela velocidade) e resolve ao pousar (`pousar_balas`, no
  `tick_mar`): acerta quem ainda está a `RAIO_DO_ACERTO` 2,4 u (+ o raio do
  casco se for navio) do ponto; senão é espirro — quem virou o leme
  esquivou. Tempo de voo `navio::tempo_de_voo` = 0,3 s + dist/30, teto 1,1 s;
  cliente e servidor leem o mesmo número. Um `SkillImpactFx` por bala
  (`tier` = índice na rajada); dano em navio entra no `damage_this_tick`
  (número de dano de terra).
- **Na tela:** anel de alvo na água (com marcas de "travado") e barra de
  vida; os DOIS cones da Bordada na água quando o alvo está perto — verde no
  lado que acertaria, cinza no outro (parado: o anel de alcance em volta);
  linha de mira e anel de alcance de 22 m do Canhão de Proa; clarão nas
  portinholas, fumaça, a bala no arco pelo tempo real de voo, espirro no
  erro e lascas de madeira no acerto; tremor de câmera pra quem atira;
  clarão vermelho no casco atingido; aviso "Low hull — head to a dock!" a
  30%.
- **Bichos do mar:** barbatana cortando a água e esteira (tubarão), corcovas
  rolando (enguia); a mordida é ANUNCIADA: `navio::MORDIDA_PREPARO_S` 0,9 s
  (em terra 0,46) de anel vermelho que fecha, com a cunha apontando a presa
  (lido do `MobAttackFx` que já existia, `impact_s` exato); sair do alcance
  antes de fechar esquiva (teste). Lascas e espirro no casco ao morder. O
  elite ("[ELITE] Dread …") sai 1,6× maior, com anel vermelho e plaqueta
  ELITE.
- **Sons** (`assets/audio/mar/`, `scripts/gera-sons-mar.sh`, sox, mono 22
  kHz, Vorbis q3): `canhao` 5,5 KB, `agua` 6,7 KB, `impacto` 4,8 KB;
  `Som::CanhaoDisparo`, `CanhaoAgua`, `CanhaoImpacto` no fim do enum.
- **Balanceamento** (`shared::navio`): Bordada ×1,4 → ×1,5; Canhão de Proa
  ×2,2 / 4 s → ×0,6 / 5 s ("A long-range shot", o abridor e o caçador, não a
  arma principal: a 2,2 ele dava três vezes o dano da Bordada e a luta
  durava 4 s); bicho do mar nasce com ×1,6 de vida (`VIDA_DO_BICHO_DO_MAR`,
  `preparar_bicho_do_mar`; o elite multiplica por cima). Resultado: um bicho
  do nível leva 3-5 Bordadas e a luta com os dois canhões dura 10-18 s, do
  nível 1 ao 60 (`os_bichos_do_mar_aguentam_alguns_tiros`,
  `a_luta_no_mar_dura_dez_a_vinte_e_cinco_s`); elite 15-25 Bordadas; uma
  Chalupa crua afunda outra em ~8 Bordadas.

**3. HUD do mar.** Painel do navio no topo (`painel_do_navio`): barra do
casco com números (pisca a 30%), velocidade, zona — "Safe harbour" (verde,
corredor), "Open waters", "Lawless waters" (vermelho) —, tesouro no convés,
"Nearest dock: Glacier · 230 m"; embaixo a linha de perigo da fase 4 e o
"Sail to…". O nome e a seta de borda de cada cais são do remake visual
(`mar_cena::rotulos`). A barra de skills do navio ganhou ícones vetoriais
claros (`mar_ui::icone_da_habilidade`: casco disparando dos dois costados,
canhão com bala, tábua + martelo + cruz verde, navio sumindo na nuvem), com
a varredura de recarga de sempre.

**Custo (celular).**

| O quê | Custo |
|---|---|
| Efeitos de combate | piscinas FIXAS: 24 balas, 192 partículas; efeito novo sobrescreve o mais velho; nenhuma alocação por quadro |
| Desenho | todos os decalques de água (anéis, cones, linha, telegrafia, barbatanas) numa malha reutilizada + partículas/balas em quads virados pra câmera numa segunda: 2 draw calls; nada de `draw_sphere` (o FX antigo montava uma esfera por chamada) |
| Gráficos leves | `config_graficos` com forração < 65% ou distância "Near": metade das partículas |
| A\* de água | só no servidor, só quando um destino é pedido (toque limitado a 0,2 s; aproximação de combate a 0,6 s); grade montada uma vez no boot; o cliente nunca roda A\* |
| Auto modos | varredura de alvo a 4 Hz no mar |
| HUD | linhas de texto em cache, refeitas só quando o valor muda; o cais mais perto sem montar lista |
| Sons | 3 arquivos, 17 KB no total, ≤ 0,6 s |

**Testes:** shared `a_luta_no_mar_dura_dez_a_vinte_e_cinco_s`,
`a_bala_voa_e_a_mira_adianta`, `os_bichos_do_mar_aguentam_alguns_tiros`
(agora 3-5); servidor `a_rota_na_agua_nunca_passa_por_terra_e_chega_a_todo_cais`
(de todo cais a todo cais, cada trecho na água, e o casco navegando com o
seguidor e o `mover_casco` reais chega ao alcance de atracar; dos ninhos a
todo cais; toque numa ilha termina na água), `o_toque_no_mar_vira_rota_e_tracejado`,
`a_bala_voa_e_acerta_o_alvo_certo` (nada antes de pousar, só o alvo,
esquiva = espirro, avanço no alvo que anda), `a_mordida_e_anunciada_e_da_pra_fugir`,
`o_bicho_do_mar_tem_couro_grosso`, `a_coleta_no_mar_acha_destroco_alcancavel`;
cliente `chegar_ao_cais_atraca_sozinho`, `tocar_na_ilha_vai_pro_cais_dela`,
`parado_pede_de_novo_e_desiste`, `eta_e_faixa`,
`o_auto_combate_no_mar_pega_o_bicho_mais_perto`, `a_caca_no_mar_vai_ao_ninho_mais_perto`,
`os_efeitos_vivem_em_piscinas_fixas`, `a_linha_do_hud_so_refaz_quando_muda`,
`os_sons_do_mar_existem_e_sao_pequenos`. Prévia: `MMO_PREVIA_MAR=1` grava
também `mar-combate-bordada.png`, `-proa`, `-tubarao` e `-rota`
(`MMO_PREVIA_SO=combate` só elas).

**Cortado:** A\* no cliente (o servidor já tem o relevo), seta de borda
própria pro cais (a do remake visual já existe), barra de casco de navio
inimigo (o casco dos outros não vai no fio; o alvo mostra a vida),
cosmético de bala.

### Auto combate no mar, refeito (16/11/2026, versão de TESTE, protocolo 199)

O dono: *"o auto combat no barco tá muito ruim"*. Medido sem tela
(`server/world/piloto_do_mar.rs`, testes): o auto antigo (no cliente) ia até
8,5 m do alvo, **parava** e ficava ali atirando enquanto os tubarões
nadavam até o casco e mordiam a cada 2 s — 24,5 s parado, 22 mordidas,
26% do casco num ninho de três (44 s). E o achado de fundo: no mar quem
**foge** da mordida (o que a telegrafia pede) fazia o bicho desistir em 5 s
sem morder e voltar pra casa com a vida cheia (o casco anda ao dobro do
tubarão provocado): um ninho de três curou quatro vezes e passou de 120 s.

- **Piloto no servidor.** O cliente liga o AUTO (`PedidoNavio::Piloto`,
  variante no fim; reenviada a cada 3 s enquanto ligado) e segue escolhendo
  o alvo; o servidor conduz o casco direto no `frame.move_dir` (joystick
  parado, sem rota), 10 decisões/s, sem ida e volta de `MoverPara`.
- **Bordejar e dar a bordada:** rumo tangente em volta do alvo a
  `navio::PILOTO_RAIO` 7,5 m (dentro da visão do bicho, que continua
  perseguindo; longe da mordida), o costado virado pra ele. Bordada só com o
  alvo (ou dois bichos) no costado; com bicho ao alcance na proa/popa e
  nenhuma mordida vindo, **para uma decisão** (parado atira em volta) e
  pega o bando todo. Canhão de Proa sempre que o alvo está nos 22 m.
  *(Mudou no mesmo dia: sem parar, sem canhão de proa — ver "Inércia de
  giro e tudo de lado".)*
- **Lê a telegrafia:** mordida vindo nele = rumo direto pra longe do bicho
  até ela fechar; bicho a menos de 6 m empurra o casco.
- **Casco:** Reparo de Emergência abaixo de 35%; abaixo de 20% sem reparo,
  Fumaça e rota de água até o corredor do cais mais perto, volta acima de
  50% (`navio::piloto_recua`, a mesma regra no cliente, que para de caçar).
- Terra e corredor viram parede (sondagem a 2 e 4 m, gira até livrar, curva
  forte troca o sentido); parado 1,5 s troca o sentido. Rota de caça que
  termina no alvo o piloto assume; toque do jogador em outro lugar é dele.
- PK: escolha própria só bicho; jogador só se o cliente escolheu (o
  `auto_pvp` dele) e `can_damage_player` deixa. Nada no corredor.
- **Bicho do mar desiste em 15 s** (`navio::DESISTE_NO_MAR_S`; em terra 5)
  sem morder; levar tiro continua zerando a conta.

| Mesmo ninho (nível, 3 bichos) | Antes | Piloto |
|---|---|---|
| Tempo | 44,2 s | 44,5 s (ninhos 2 e 4: 30,8 e 26,0 s) |
| Casco perdido | 26% | 0% |
| Tiros / acertos | 18 / 18 | 19 / 19 |
| Mordidas (anunciadas) | 22 (24) | 0 (0) |
| Maior tempo parado | 24,5 s | 1,0 s |

Ninho do elite (elite + 2): antes **afundava** (32 mordidas, 23,7 s parado,
14/16 acertos); piloto 112 s, 0% de casco, 56/60 acertos, 0 de 20 mordidas
anunciadas. Testes: `o_piloto_vence_o_auto_antigo_no_mesmo_ninho`,
`o_piloto_afunda_o_ninho_do_nivel_sem_perder_metade_do_casco` (20-60 s,
< 50% casco, > 60% acerto, nunca parado 3 s), `o_piloto_aguenta_o_ninho_do_elite`,
`com_o_casco_baixo_o_piloto_repara_e_recua`, `o_piloto_respeita_o_corredor_e_o_pk`;
cliente `o_piloto_segue_o_auto_e_recua_com_o_casco`.

### Inércia de giro e tudo de lado (16/11/2026, versão de TESTE, protocolo 199)

O dono: *"do nada ele vira pro mob e volta… tem algum ataque que sai da
frente do barco? se sim tira, tudo tem que sair de lado"* e *"a rotação do
barco tem que ser um pouco mais blocada pra dar sensação de barco… quanto
maior mais demora a virar"*.

- **Tudo sai de lado.** O Canhão de Proa virou **Canhão Longo** (Long Gun):
  mesmo alcance (22 m), só com o alvo num costado (`navio::no_costado`); o
  servidor recusa com "Turn your side to the target to fire.", a barra do
  cliente também. O piloto não "para pra atirar em volta" mais. O casco no
  mar nunca vira pra mira (`ent.mira` ignorado, `rumo` dos outros ignorado).
- **Leme com peso** (`shared::navio::Leme`, um por sessão no mar,
  `Session::leme_do_mar`): o joystick, a rota ou o piloto dizem pra onde; o
  rumo gira no máximo `giro_max` rad/s, a velocidade vai ao alvo numa rampa
  de `arranque_s` (subindo e descendo) e cai em curva fechada:
  `v × (1 − 0,6·|ângulo que falta|/π)` (meia-volta a 40%). A velocidade
  enviada é sempre `rumo × vel` (nunca passa de `VELOCIDADE_MAX`, abaixo do
  fio). Parado, o navio guarda o rumo: Bordada e Canhão Longo continuam só
  dos costados (`rumo_do_casco` no servidor, `mar_ui::rumo_do_casco` pelo
  yaw no cliente). Só um casco que nunca navegou atira em volta (e isso é
  no corredor do cais, onde não se atira).

| Casco | Giro (rad/s) | Arranque | Rótulo | Órbita do piloto | Vel. na órbita |
|---|---|---|---|---|---|
| Chalupa | 1,5 | 0,6 s | Agile | 7,5 m | 10,0 |
| Escuna | 1,2 | 0,75 s | Steady | 9,3 m | 10,3 |
| Galeão | 0,9 | 0,9 s | Heavy | 10 m | 8,3 |
| Nau | 0,7 | 1,05 s | Heavy | 10 m | 6,4 |

  Estrela de vela: +3% no giro (como na velocidade). O cartão do navio
  (bolsa) e o Estaleiro mostram "Turning: Agile/Steady/Heavy".
- **Rota com raio de giro** (`navio::direcao_na_rota`): ponto a `d`, `θ`
  fora da proa só é alcançável se `d ≥ 2·r·sen θ` (`r = v/giro`): o
  acelerador cai até caber. Curva da rota de `φ` é feita a `giro·4/sen(φ/2)`
  freando a tempo; ponto intermediário conta como alcançado a 2,5 m
  (`FOLGA_DA_ROTA`); virando (> ~30°), o seguidor não se dá por travado.
  Casco que raspa a costa desliza ±20°/±40° a meia força
  (`navio::mover_raspando`) em vez de empacar (a Nau empacava num pontal na
  rota Geleira → Ilha Mágica).
- **Piloto:** órbita `raio_do_piloto` (raio de giro a toda, entre 7,5 e
  10 m) na velocidade `velocidade_do_piloto` (92% de `giro·r`); aproxima e
  foge de mordida a toda. Sem alvo atirável (bicho na carência ou voltando
  pra casa) segue o mais perto em vez de ficar parado; com o alvo a mais de
  8 m do ninho, a órbita puxa pro ninho (a coleira: o Galeão arrastava o
  bicho, ele desistia, voltava e curava). Fora da área de 24 m, pega o mais
  perto até 40 m.
- **Cliente:** yaw do casco segue a velocidade com suavização própria
  (`YAW_DO_CASCO_K` 7) e abaixo de 1 u/s guarda o rumo (o degrau de 1/8 u/s
  do fio apontava pra qualquer lado); adernar segue o giro (~9° Chalupa,
  ~4° Nau). Sem previsão local (o cliente não prevê movimento).

| Piloto, ninho do nível (3 bichos) | ninho 0 | ninho 2 | ninho 4 |
|---|---|---|---|
| Chalupa | 32,4 s | 30,8 s | 27,0 s |
| Escuna | 26,4 s | 25,8 s | 26,4 s |
| Galeão | 26,7 s | 26,0 s | 26,6 s |
| Nau | 32,7 s | 46,5 s | 21,0 s |

Todos: 0% de casco, 100% de acerto, 0 mordidas, nunca parado mais de 1 s.
Testes: shared `o_navio_vira_com_peso`, `a_rota_nao_fica_orbitando_o_ponto`;
servidor `o_piloto_afunda_o_ninho_do_nivel_sem_perder_metade_do_casco`
(agora as quatro classes), `a_rota_na_agua_nunca_passa_por_terra_e_chega_a_todo_cais`
(as quatro classes saindo do cais de proa pra ilha), `canhonaco_precisa_de_alvo_no_alcance`
(alvo na proa recusado mesmo parado).

### Remake visual (16/11/2026, versão de TESTE, protocolo 199)

O dono, no primeiro mar: *"o mar aberto está um lixo inacreditável ... as
ilhas em miniatura estão uma merda, não mostra a doca nem nada, não dá nem
pra entender, o barco tá horrível ... o mar em si tá muito muito feio"*. Foi
refeito o visual inteiro, sem mexer no protocolo (`PROTOCOL_VERSION` 199) e
com o servidor lendo o MESMO relevo do cliente (`shared::mar`).

**Escala nova.** `ESCALA` 0,25 → **0,36**, `RAIO_BLOCOS` 1.400 → **1.800**
(o mar é ±900 u, campo de altura de ~26 MB no servidor), `BORDA` 680 → 880;
raio das ilhas `(raio_m × 0,2).clamp(105, 160)` (Bosque e Planalto 160,
Geleira e Ermo 105). O fio aguenta ±4.095 u (teste
`o_mar_inteiro_cabe_no_orcamento_do_fio`). Travessias de ancoradouro a
ancoradouro, Chalupa a 10 u/s (teste `as_travessias_levam_de_meio_a_um_minuto_e_meio`):
Bosque–Geleira 552 u (55 s), Bosque–Ermo 555 u (55 s), Geleira–Ermo 543 u
(54 s), Geleira–Planalto 541 u (54 s), Ermo–Planalto 544 u (54 s) e a longa
Bosque–Planalto 952 u (95 s); com o casco mais rápido (15,4 u/s), 35-62 s.
Era 30-60 s com ilhas-domo de 60-120 u. Cache de altura `-mar2`.

**As ilhas parecem as ilhas** (`mar::Feitio`, `terra_em`, `bloco_da_coluna`):
perfil desenhado por ilha com três oitavas de value noise (hash por ponto da
grade, smoothstep entre eles: barato e igual nos dois lados), costa com cabos
e baías (`IlhaVista::costa`, 84-100% do raio):

| Ilha | Silhueta | Paleta |
|---|---|---|
| Bosque | praia larga, morros ondulados até ~25 u, mata | areia, três verdes |
| Geleira | faixa de gelo, falésia branca de ~15-20 u a 3-7 u da costa, campo de neve até ~30 u, **blocos de gelo** boiando na plataforma | neve, gelo azulado, rocha escura no pé |
| Ermo | praia, dunas em cristas paralelas, **três mesetas** de 15-24 u | areia, arenito em estratos |
| Planalto | sem praia: falésia escura direto da água (~25-30 u), topo serrilhado até ~45 u, **pilares de rocha** no mar | rocha abissal, rocha escura, musgo |

Paredes em estratos por altura absoluta (`mar::material_da_parede`); topo por
`mar::material_do_topo` (via `Gerador::material_desenhado`). Gelo e pilares
ficam na plataforma (nunca além de `raio + 14`, onde começa a `agua_livre`) e
nunca na frente do cais (teste `o_cais_e_de_tabua_e_o_caminho_esta_livre`:
60 u de água livre saindo do ancoradouro). Árvores e pedras são SÓ do
cliente (navio não chega em terra): modelos low-poly de 20-60 quads
(`mar_cena::props`), um candidato a cada 3,5 u (`prop_da_coluna`), assados
no pedaço do terreno (custo zero por quadro), ralos com Gráficos → Forração.

**A doca é impossível de não ver** (`mar_cena::Doca`, construída uma vez por
ilha): píer de tábuas sobre estacas (26 u + cabeça em T de 11 u), lampiões,
caixotes e barris, cais de pedra plano na raiz (`ALTURA_DO_MOLHE`, cavado no
que a ilha for), galpão de telhado vermelho, **farol** listrado de vermelho e
branco de ~20 u com lanterna, halo que nunca fica menor que uns pixels (vira
estrela no horizonte) e facho girando, bandeiras na cor da ilha, dois barcos
atracados balançando, **anel brilhante** no ancoradouro (e o tracejado dos
14 u do "Dock"), rótulo flutuante "Bosque · Dock · N m" sobre o farol e
**marcador na borda da tela** apontando cada doca fora de vista
(`mar_cena::rotulos`), e âncora + nome no minimapa e no mapa (`mapa::aguas_sem_lei`).
O mapa do mar pinta cada ilha com a paleta dela.

**De longe** (`mar_cena::Lod`): uma malha de altura por ilha, 3-5 u por
célula, com cada vértice no chão MAIS BAIXO em volta (fica sempre embaixo do
voxel: perto, os pedaços cobrem; além deles, é a silhueta na névoa). No mar a
névoa vai bem mais longe (`mar_cena::neblina`: 380/520/640 u por qualidade).
Céu em domo com gradiente e sol (`desenha_ceu`), a mesma cor que a água
reflete e que a névoa usa.

**O navio parece navio** (`client/mar_casco.rs`): modelo de voxel por visual
(classe, vela, figura, latão), montado UMA vez pelo malhador da vegetação
(`vegetacao::malha_com`, com paleta por "slot"), voxel de 0,125 u: proa
afilada, tosado subindo nas pontas, quilha, fundo pintado, faixa por classe,
convés de tábuas, amurada (latão com as estrelas), tombadilho com janelas de
popa acesas e castelo a partir do Galeão, portinholas com canhão (Escuna 2,
Galeão 4, Nau 2×5 por bordo), gurupés, mastros, vergas e cesto da gávea,
lanterna de popa, figura de proa de voxel. Velas numa malha por mastro
(latinas na Chalupa e Escuna, redondas no Galeão e na Nau, mais a bujarrona)
que **enfunam** por uma escala na matriz (zero trabalho de vértice), cordame
em linhas, flâmula. Anda NAS ondas (`agua::onda_em`, as mesmas três de
Gerstner do shader: sobe, arfa, balança), aderna na curva e deixa **esteira**
de espuma que alarga e some em 3,2 s.

**A água** (`agua.rs`, `Estilo` > 0 só no mar; as ilhas seguem com a água de
antes): normal da própria onda de Gerstner (as derivadas saem dos mesmos três
senos), fresnel com o céu, rastro de brilho do sol, luz atravessando as
cristas (turquesa), carneirinhos no alto das cristas, arrebentação em faixas
em TODA costa (no mar a espuma é "tem terra do lado", porque a plataforma cai
um bloco na costa), fundo opaco e névoa na cor do céu. Prévia ao meio-dia e
ao entardecer (`mar_cena::define_entardecer`; o mar não tem ciclo de dia).

**Destroços e baús** boiam nas ondas (`mar_cena::boia`, `jangada`): destroço
com toco de mastro e pano rasgado, baú numa jangada com boia e bandeirinha.

**Orçamento do celular** (medido na prévia e nos testes):

Qualidade = Gráficos → Forração (Esparsa = baixa, Média, Cheia = alta;
`mar_cena::qualidade`). Ondas desligadas em Gráficos = mar parado, como antes.

| | Baixa | Média | Alta |
|---|---|---|---|
| Névoa no mar (fim) | 620 u | 850 u | 1.100 u |
| Ilhas de longe (4 malhas, célula) | 5 u, ~15 mil vértices | 4 u, ~24 mil | 3 u, ~43 mil (~41 mil triângulos) |
| Árvores/pedras nas ilhas (assadas no pedaço) | 30% | 65% | 100% (Bosque: >1.000 árvores na ilha inteira, 30-120 quads cada) |
| Ondas no vértice | 2 | 3 | 3 |
| Fragmento da água | fresnel + sol + arrebentação | + 1 seno de marola + carneirinhos | + 3 senos de marola |
| Destroços desenhados até | 70 u | 110 u | 110 u |

Fixos em qualquer qualidade: doca ~15 mil vértices cada (60.784 nas quatro),
desenhada até 360 u, 1 chamada por doca; ilha de longe 1 chamada inteira, ou
ladrilhos de 48 u (só os que os pedaços carregados não cobrem) quando o
jogador está perto dela. Navio: Chalupa ~6,7 mil vértices, Escuna ~9,2 mil,
Galeão ~17 mil, Nau ~21 mil (teste `os_cascos_saem_bonitos_e_baratos`, teto
24 mil); perto são casco + uma vela por mastro + bujarrona (3-5 chamadas),
longe de 55 u casco + velas juntas (2 chamadas), sem cordame. Por quadro, o
que é refeito na CPU é pouco: céu ~210 vértices, anel do cais ~300, halos e
fachos, bandeiras de 14 vértices, esteira ≤ ~200 vértices por navio; nada
estático é refeito. Malha nova só na primeira vez que um visual de navio
aparece (alguns ms) e uma ilha/doca por quadro ao entrar no mar.

Medição de FPS no aparelho fica com o benchmark do primeiro login
(`benchmark::cena_mar`, que agora desenha este mar).

**Prévia:** `MMO_PREVIA_MAR=1` (`MMO_PREVIA_SO=<parte>` filtra) grava
`mar-largo`, `mar-largo-bosque`, `mar-cais`, `mar-cais-geleira/ermo/planalto`, `mar-classe-0..3`,
`mar-jogo`, `mar-jogo-longe`, `mar-navegando`, `mar-meio-dia`,
`mar-entardecer` e `mar-alto` em `MMO_PREVIA_SAIDA`. O benchmark do primeiro
login (`benchmark::cena_mar`) desenha o mar por `mar_cena::desenha_mundo`, o
mesmo passe da prévia.

### Toda ilha no mar (16/11/2026, versão de TESTE, protocolo 199)

O dono: *"pode fazer qualquer npc de teleporte entre ilhas ter a opção de
sail porque eu quero poder navegar para qualquer ilha do jogo"*.

**Carta nova** (`shared::mar`). As 4 ilhas com porto continuam em `ESCALA`
0,36 do mapa-mundi e com o cais virado pro mesmo lado (`rumo_do_porto`); as
4 sem porto em terra entram por `mar::NOVAS`, postas à mão no mesmo
referencial, ao norte (onde o mapa-mundi as tem), com o cais virado pro
centróide das ilhas com que têm rota. A caixa toda é recentrada: as 4
antigas se deslocam juntas (≈ +87 u em x, +565 u em z), sem mudar distância
entre elas. `RAIO_BLOCOS` 1.800 → **2.300** (±1.150 u, campo de altura de
~42 MB no servidor), `BORDA` 880 → **1.130**, cache `-mar3` (`REVISAO` 3).
Fio: ±1.150 contra ±4.095 u (teste `o_mar_inteiro_cabe_no_orcamento_do_fio`).

| Ilha (perfil) | Centro no mar (x, z) | Raio | Faixa do mar | Zona do cais |
|---|---|---|---|---|
| Bosque | (735, 565) | 160 | 1-15 | `ilha_inicial` |
| Geleira | (87, 169) | 105 | 15-30 | `ilha_gelo` |
| Ermo | (87, 961) | 105 | 28-42 | `ilha_deserto` |
| Planalto | (-561, 565) | 160 | 40-60 | `ilha_planalto` |
| Ilha Mágica (`Magica`) | (135, -416) | 105 | 20-80 (os degraus) | `ilha_magica` (degrau pelo poder) |
| Skyreach (`Celeste`) | (-565, -316) | 120 | 60-80 | `ilha_celeste` |
| Kōgen-tō (`Neon`) | (-765, -936) | 130 | 80-100 | `ilha_kogen` |
| Abyssia (`Abissal`) | (535, -886) | 110 | 100-120 | `ilha_abissal` |

**Rotas** (`mar::pares_das_rotas`, 12): os 6 pares entre portos, como antes,
mais Geleira–Mágica, Mágica–Skyreach, Planalto–Skyreach, Skyreach–Kōgen,
Kōgen–Abyssia e Mágica–Abyssia (`ROTAS_NOVAS`). Ninhos (1/3 e 2/3, elite no
de 2/3), baú à deriva e destroços seguem as rotas e o `nivel_em` da ilha
mais perto; `povoar_mar` agora deixa a faixa subir até 120. Águas sem lei:
as 3 entre portos longe do Bosque e **uma nova longe, Kōgen–Abyssia**
(`SEM_LEI_NOVA`). Travessias (ancoradouro a ancoradouro, Chalupa 10 u/s,
teste `as_travessias_levam_de_meio_a_um_minuto_e_meio`: 30-100 s entre
portos, 45-120 s nas rotas novas): Geleira–Mágica 649 u (65 s), Mágica–Skyreach
472 u (47 s), Planalto–Skyreach 841 u (84 s), Skyreach–Kōgen 629 u (63 s),
Mágica–Abyssia 638 u (64 s) e a longa Kōgen–Abyssia 1.045 u (105 s); as 6
entre portos não mudaram (54-95 s).

**Silhuetas novas** (`Feitio`, `terra_da`, `material_do_topo/parede`):
Skyreach = costa de mármore e bancos de nuvem; Kōgen-tō = muralha de
concreto e porto plano com a grade de ruas (asfalto, faixa tracejada);
Abyssia = **atol** de coral em anel em volta de uma lagoa
(`FRACAO_DA_LAGOA` 0,46) funda no meio, onde gira o redemoinho — o anel é
fechado (teste) e a lagoa não é água livre; Ilha Mágica = praias claras,
morros verdes e coroa de cristal violeta. O cais, o molhe plano e os 60 u
de entrada livre valem pra todas (testes de sempre).

**Atracar** (`server/world/navio.rs`): cada cais novo leva à zona certa
(`mar::zona_do_indice`; a Mágica é o índice `mar::ILHA_MAGICA` = 7, o
primeiro depois do `ARQUIPELAGO`, que cabe no bit 7 do `Tesouro::ilhas`).
Skyreach, Kōgen-tō e Abyssia: a MESMA chegada das viagens de Capitão,
ônibus e submarino (`porto_da_ilha`: o porto, ou a cidade, onde ficam os
NPCs de viagem). Ilha Mágica: `magica::CHEGADA`, no maior degrau que o
poder abre, pelo mesmo `entrar_na_magica` do Guia (retoma o tempo ativo ou
gasta uma entrada: as grátis do dia, depois passe); sair da ilha
(`sair_da_magica`) com `magica_volta` = mar devolve o navio ao cais da
Mágica (sem navio que navegue, ou mar fora do ar: o Bosque). **Destino
fechado recusa atracar com o motivo** ("You haven't unlocked Kōgen-tō yet:
reach the story step "…""; a Mágica: "it opens at N power (you: M)");
navegar até lá pra olhar é livre.

**Zarpar de todo NPC de viagem.** O Capitão do Porto, o Professor do Ônibus
Voador e o Capitão do Submarino (onde estiverem) mandam o estado do navio
junto com o menu, e a linha "My ship" (Sail / Repair · N copper /
Shipwright) aparece pros três (`abrir_menu_viagem`,
`perto_do_capitao_do_porto` = qualquer `Meio`). Zarpar de uma ilha sem
porto em terra põe o navio no cais NOVO dela; atracar lá devolve à chegada
da ilha. As viagens de ônibus, submarino e Capitão seguem iguais.

**Visual** (`client/mar_temas.rs`, chamado por `mar_cena`): cada ilha nova
é a malha de altura de longe (`constroi_lod`) + UMA malha low-poly dos
marcos (`malha_de_longe`, sempre desenhada no alcance da névoa porque fica
onde nenhum pedaço cobre) + a doca em voxel (`pecas`) até 360 u.

| Ilha | Marcos de longe | Doca (perto) | Farol equivalente |
|---|---|---|---|
| Skyreach | 5 ilhas flutuantes (domo de grama, aro de mármore, cone de rocha), árvores douradas | píer de mármore, **elevador do céu** até a ilha flutuante sobre o cais, coreto de cúpula dourada, nuvens | luz dourada no topo da ilha do elevador (43,5 u); **cachoeira** caindo da ilha no mar, com espuma |
| Kōgen-tō | skyline de torres com faixas neon e luz de topo piscando, 2 guindastes | píer de concreto com placas neon, **guindaste pórtico** e pilhas de contêineres | mastro de sinal com anéis neon e placa (23,6 u) |
| Abyssia | pilares de coral com ponta brilhante | píer de grade e latão, pilares de coral, lanternas de concha | **sino de mergulho** no guindaste (16,4 u) e **redemoinho** brilhante na lagoa com coluna de luz |
| Ilha Mágica | cristais gigantes (até ~46 u) com lascas | píer encantado sob **arco de runas**, pedras rúnicas, cristais | **espigão de cristal** violeta (22 u) |

Cor de cada ilha (bandeira, anel, rótulo, âncora no mapa): Skyreach
dourado, Kōgen-tō rosa-neon, Abyssia água, Mágica lilás. Props novos
assados no terreno (`mar_cena::props`, 27 modelos ≤ 120 quads): árvores do
céu, cristais, corais e alga, ralos com a Forração. Rótulo "<ilha> · Dock",
marcador de borda, minimapa e mapa do mar saem de `mar::ilhas()` e já
mostram as 8. Vértices medidos (testes `mar_temas::as_docas_novas_sao_baratas` e
`a_ilha_de_longe_fica_por_baixo_e_e_barata`, célula 3 u): Skyreach
relevo 8.649 + marcos 1.820 + doca 16.028; Kōgen-tō 10.000 + 5.024 + 18.440
(18 luzes de topo); Abyssia 7.569 + 952 + 14.828; Mágica 6.889 + 1.008 +
14.840. Na prévia (alta): ilhas de longe 218 mil índices nas 8, docas
124.920 vértices nas 8 (cada doca 1 chamada, só até 360 u; os marcos, 1
chamada por ilha). Por quadro: halos, a fita da cachoeira (~70 vértices) e
o redemoinho (~230).

**Prévia:** `MMO_PREVIA_MAR=1` grava também `mar-cais-celeste/kogen/abissal/magica`,
`mar-chegada-celeste/kogen/abissal/magica` e `mar-todo` (o mar inteiro do
alto, névoa afastada só nessa foto).

**Testes:** `mar::toda_ilha_do_jogo_esta_no_mar`,
`cada_doca_nova_leva_a_ilha_certa`, `as_docas_novas_tem_o_seu_tema`,
`a_ilha_e_parede_e_o_cais_e_a_porta` (anel do atol fechado), travessias e
águas sem lei (4); servidor `a_rota_na_agua_nunca_passa_por_terra_e_chega_a_todo_cais`
(todo cais a partir de todo cais, 56 rotas), `todo_npc_de_viagem_oferece_zarpar_e_zarpa_da_doca_da_ilha`,
`toda_doca_nova_atraca_na_ilha_certa`, `destino_bloqueado_recusa_atracar_com_o_motivo`;
cliente `mar_temas::as_docas_novas_sao_baratas`, `cada_tema_tem_os_seus_marcos`.

**Produção (NÃO feito):** nada novo além do que a fase 1 lista; o cache de
altura `-mar3` (~42 MB) é gerado no primeiro boot do processo do mar.

### Fase 5 — Conteúdo do mar

- **Mapas do tesouro** (drop raro) que apontam ilhotas secretas só
  alcançáveis por navio.
- **Evento do Navio Fantasma** (horário, como o fim de semana).
- **Chefe do mar, o Leviatã:** morde o casco num ritmo fixo; a luta é dano
  contra resistência do navio, não desvio de golpe. Fica fora de
  `bosses::CHEFES` de propósito, porque aquela lista exige golpe esquivável.

## Restrições técnicas

- **Cada ilha é uma zona e um processo.** Trocar de zona é reconectar
  (`mandar_para_zona` → `ServerMessage::TrocarZona`). O mar é mais uma zona,
  não um mundo contínuo com as ilhas.
- **Fio:** posição `i16` com `POS_SCALE` 8 (±4096 u) e velocidade `i8`
  (±15,9 u/s). O casco mais rápido fica abaixo disso, com teste.
- **Celular:** silhuetas analíticas, nada de relevo Perlin das ilhas no mar.
- **Skyreach, Kōgen-tō e Abyssia não têm porto em terra** (`tem_porto`),
  mas desde 16/11/2026 têm **cais no mar** (`mar::NOVAS`), como a Ilha
  Mágica — ver "Toda ilha no mar".
- **Dados:** o navio vive na instância do item (como pet e montaria),
  então trocar e vender no mercado leva o que foi investido.

## Decisões do dono (15/11/2026)

1. **Capitão:** a viagem rápida entre ilhas **passa a custar cobre**. Ela
   continua existindo, mas o mar ganha motivo.
2. **PvP no mar:** o **mar inteiro segue os modos de PK** de hoje
   (`shared::pk`: Pacífico, Grupo, Clã, Livre). A "marca" de quem carrega
   carga ou tesouro da fase 4 é por cima disso.
3. **Primeiro navio:** a **missão da história** que tira o jogador da
   primeira ilha entrega a Chalupa. **Jogadores antigos** (que já passaram
   desse passo) recebem um barco de graça por migração. O **Estaleiro** faz
   o craft e os upgrades dos cascos seguintes.
4. **Ordem das fases:** **1 → 2 → 4**. A fase 3 (comércio) foi cortada; a 5 (conteúdo do mar) fica para depois. Sem comércio, a "carga" da fase 4 é só o tesouro (Baú do Colosso e baús do mar).
5. Material exclusivo do mar: por padrão, só pra navio e cosmético (não foi
   perguntado; vale a regra "o mar é opcional").

Com a Chalupa vindo da história, a regra "o mar é opcional" continua de pé
porque o Capitão segue levando (agora por cobre): ninguém é obrigado a
navegar pra avançar.

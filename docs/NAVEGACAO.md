# NAVEGAÇÃO — o mar como sistema (plano, 15/11/2026)

> Status: **FASES 1 E 2 FEITAS (15-16/11/2026, versão de TESTE, protocolo
> 199 ainda não lançado).** A fase 4 segue como plano. O dono escolheu a "opção
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
| 1 | Bordada (Broadside) | todo inimigo a até 12 m nos costados (proa e popa cegas, \|cos\| ≤ 0,8; parado atira em volta) | 6 s | 12 m | ×1,4 | sim |
| 2 | Canhão de Proa (Bow Cannon) | um tiro no alvo selecionado | 4 s | 22 m | ×2,2 | sim |
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

### Fase 4 — Corsário e PvP naval

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
- **Skyreach, Kōgen-tō e Abyssia não têm porto** (`tem_porto`): ficam fora do
  mar, como hoje.
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

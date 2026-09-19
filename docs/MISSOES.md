# Missões

O **Mestre de Missões** fica na praça de cada cidade, ao lado do poço, olhando
pro centro (`shared::vila`, papel `Missoes`). No servidor ele é
`EntityKind::Npc(10)`; clicar nele (de longe o personagem anda até ele) abre a
janela com o que dá pra aceitar e o que está em andamento.

## A cadeia da ilha inicial

Giver `GIVER_MESTRE_DA_ILHA` (121). Cada uma só aparece depois da anterior
**entregue** (`QuestDef::requires`).

| Id | Missão | Objetivo | Recompensa |
|---|---|---|---|
| 501 | Conheça o Alquimista | falar com o Alquimista | 30 ouro, 40 XP, 3 poções de vida |
| 502 | Lobos na estrada | derrotar 6 lobos | 80 ouro, 120 XP, 2 poções de vida |
| 503 | Cobre pra forja | juntar 30 de Cobre | 120 ouro, 150 XP, 3 poções de mana |
| 504 | Ursos na mata | derrotar 3 ursos (nível 3) | 160 ouro, 220 XP, 3 poções de vida |
| 505 | Visite o Porto | falar com o Capitão do Porto | 250 ouro, 300 XP, 2 poções de vida maiores |
| 506 | Lenha para o cais | derrubar 8 árvores | 200 ouro, 350 XP, 3 poções de vida, 1 Poção de Experiência |
| 507 | O que a pedra guarda | juntar 20 de Darksteel | 260 ouro, 450 XP, 10 de Aço, 1 Poção de Experiência |
| 508 | Quintessência | juntar 6 de Quintessência (nível 8) | 320 ouro, 600 XP, 12 de Aço, 2 poções de vida maiores |
| 509 | O berloque do owlbear | juntar 6 de Berloque de Exorcismo (nível 12) | 400 ouro, 900 XP, **1 Couro** (chave de armadura), 15 de Aço |
| 510 | O que há sob o naufrágio | concluir uma dungeon (nível 6) | 500 ouro, 1.200 XP, 3 poções de vida maiores, 20 de Aço |

**Por que a cadeia vai até 509 — "a oficina do Mestre".** Uma peça cinza custa
1 chave + 30 de Aço + 10 de Quintessência + 10 de Berloque + 200 de Darksteel
+ 300 de Cobre, e o início só apresentava pedra e cobre: o jogador recebia
"crie seu primeiro equipamento" sem nunca ter sido informado de onde sai o
resto. Cada uma das quatro novas apresenta uma fonte — árvore (a única madeira
do jogo), Darksteel da pedra, Quintessência do tigre, Berloque do owlbear — e
a última paga em chave, que é o único desses itens que não se farma na ilha
(só cai de chefe, ver [LOOT_DOS_MOBS.md](LOOT_DOS_MOBS.md)).

## O resto do Bosque (511–538): seis cadeias, de NPC em NPC

O capítulo I da história acaba no nível 15 e a cadeia de cima no 12; os três
chefes de campo, a maioria dos bichos, as duas dungeons e a oficina ficavam
sem missão nenhuma. Agora são seis **linhas de subquests em sequência**, e
cada passo se pega — e se entrega — com um **NPC diferente da vila**
(19/09/2026: eram todas do Mestre, que oferecia as seis de uma vez).

- **Quem dá:** todo ofício da vila é um giver, `quests::giver_do_papel`
  (`140 + papel`; o Mestre segue com o 121). Armeiro de armas e de armaduras são
  o mesmo NPC (`giver_do_npc`).
- **Falar com o NPC:** se ele tem missão pra oferecer ou uma pronta pra
  receber, isso vem ANTES da loja/forja/cofre dele (`tem_missao_com`). Sem
  nada, o clique segue normal.
- **"!" e "?"** aparecem sobre cada NPC com missão, não só sobre o Mestre.
- **Menu de missões por LINHAS e ABAS** (19/09/2026): a história e cada
  cadeia são UMA entrada, com o passo atual ("Passo 2 de 5 · título") e o que
  fazer agora ("pegar com: X", "Em andamento 1/3", "entregar: X", o motivo da
  trava). As abas são Em andamento, Disponíveis, Bloqueadas e Concluídas, pelo
  estado do passo atual, cada uma com a contagem. Tocar na linha abre os
  passos dela; o **Ir** age no passo atual e, na disponível, leva até quem dá
  (pela vila gerada, mesmo fora da área carregada) e fala com ele.
- **Entrega:** a auto missão pronta vai até quem deu.

| cadeia | quem dá, na ordem |
|---|---|
| Chefes | Treinador → Capitão do Porto → Cartógrafo → Treinador → Mestre |
| Bestiário | Banqueiro → Alfaiate → Capitão → Banqueiro → Identificador → Cartógrafo / Treinador |
| Oficina | Ferreiro → Armeiro → Ferreiro → Ferreiro → Armeiro |
| Coleta | Banqueiro → Ferreiro → Alquimista |
| Dungeons | Capitão → Taberneiro → Mestre |
| Vila | Mestre → Taberneiro → Cartógrafo → Banqueiro → Identificador (cada um manda falar com o próximo) |

| cadeia | ids | o que pede | níveis |
|---|---|---|---|
| Chefes | 511–515 | Lobo Alfa, Barba-Tormenta, Urso Ancião; depois 3 e 5 chefes quaisquer | 8–15 |
| Bestiário | 516–522 | lobo, urso, pistoleiro, tigre, mago, owlbear; 80 de qualquer | 2–15 |
| Oficina | 523–527 | refinar 3, criar 2, refinar 5, 150 Darksteel, 60 Aço | 5–13 |
| Coleta | 528–530 | 20 árvores, 30 pedras, 60 coletas | 3–11 |
| Dungeons | 531–533 | Porão, Adega, 3 quaisquer | 8–14 |
| Vila | 534–538 | Taberneiro, Cartógrafo, Banqueiro, Identificador, Alfaiate | 1–10 |

XP na escala do nível pedido (~1/5 do nível), para o jogador de 14–16 ter
o que fazer entre as travas da história.

**Missão de chefe.** `KILL` com `obj_target` = `alvo_de_mob(kind do chefe)`
(10, 11, 12 no Bosque) ou `ALVO_QUALQUER_CHEFE` (1000). A morte de chefe conta
para **todo mundo a até 40 unidades** na mesma instância, não só para quem deu
o último golpe e a party dele — chefe de campo é luta de muitos (a XP continua
só para matador e party). A auto missão vai até a vaga do chefe, preferindo um
vivo; o auto combate luta, mas **não desvia** dos telegráficos — isso é do
jogador. Há também a diária **Chefe do dia** em cada ilha (608, 618, 628, 638).

## Regras (todas no servidor)

- **Aceitar**: `pode_aceitar` — nível, facção, pré-requisito, estado e
  cooldown; e perto do Mestre.
- **Matar**: KILL com `obj_target = alvo_de_mob(kind)` conta só aquele bicho da
  tabela (`0` continua sendo qualquer um).
- **Falar com** (`objective_kind::TALK`, `obj_target = Papel as u16`): interagir
  com o NPC da vila daquele papel deixa a missão pronta. O Alquimista abre a
  loja e conclui a conversa no mesmo clique.
- **Juntar**: conferido e consumido na entrega.
- **Entregar**: só perto do Mestre; recusa com o motivo no chat ("faltam 20 de
  30"). Entregou, a próxima da cadeia já aparece na janela.
- Estado em `character_quests`, como sempre.

## Cliente

- Janela do Mestre: **Disponíveis** (descrição, recompensa, Aceitar) e **Em
  andamento** (progresso, Entregar quando pronta, Abandonar). Fecha com Esc, X
  ou se afastando.
- **J** abre o diário (só o andamento).
- Rastreador das 3 primeiras ativas abaixo do minimapa.
- Sobre o Mestre: **!** tem missão nova, **?** tem entrega pronta.

## Diálogo

Falar com quem dá ou recebe a missão abre uma **caixa de diálogo** com as
falas escritas pra ela (`shared::quests::falas`), avançadas com **Próximo**:

- **Oferta** (Mestre): no fim, **Aceitar** ou **Agora não** (que abre a janela).
- **Fale com** (Alquimista, Capitão…): no fim, **Concluir**. A missão só conta
  quando o diálogo termina — o cliente manda `ConcluirConversa { npc_eid }` e o
  servidor confere a distância (`quests::pode_concluir_conversa`). Clicar no
  NPC (`Interact`) não conta mais. Sem missão "fale com" pendente, o clique
  segue abrindo a loja como sempre.
- **Entrega** (Mestre): mostra a recompensa e **Receber** (`TurnInQuest`).

Esc fecha o diálogo.

## Auto missão

**Clicar numa missão do rastreador** (ou **Ir** no diário, J) liga a auto
missão daquela missão. O personagem anda sozinho; o jogador só aperta
**Próximo** e **Receber**.

1. O cliente pede `QuestDestino { quest_id }`. O **servidor** responde onde fica
   o objetivo (`quests::destino_tipo`), porque ele conhece zonas e esgotados:
   - **fale com** → o NPC da vila daquele papel (`NPC`, com o `npc_eid`);
   - **derrotar** → a zona de spawn mais perto **onde aquele bicho nasce**
     (`quests::zona_do_bicho`: a chance de cada kind sai da mesma regra de
     `economy::kind_para_nivel` — um kind novo a cada três níveis — e a zona
     precisa ter chance ≥ 15% e nível que o jogador aguenta) → `COMBATE`;
   - **juntar** item que cai de mob (o Cobre cai de todos) → a zona desses
     bichos → `COMBATE`; item de coleta (tronco/pedra, `farm_node_drops`) → o
     melhor spot de coleta → `COLETA`;
   - objetivo cumprido → o Mestre de Missões → `ENTREGA`.
2. Vai pela **viagem do mapa** (etapas ≤ 180 u). Chegando:
   - NPC → anda até ele e abre o diálogo;
   - zona → **auto combate** centrado nela, religado se cair, até cumprir;
   - spot → **auto coleta**.
3. Cumpriu → desliga combate/coleta, volta ao Mestre, diálogo de entrega;
   **Receber** → a próxima da cadeia é aceita sozinha e a auto missão segue.
4. Para com: andar no teclado, clicar no mundo, clicar no mapa, Esc, ligar o
   auto combate à mão, cair. Missão sem destino na ilha avisa no chat.

HUD: faixa "AUTO MISSÃO · nome · etapa" acima da do auto combate.

## Diárias

Cada ilha tem oito diárias do Mestre de Missões (ids 601–608 Bosque, 611–618
Geleira, 621–628 Ermo, 631–638 Planalto), na escala da faixa da ilha:

| tarefa | objetivo | como conta | auto missão |
|---|---|---|---|
| Pedreira do dia | quebrar 20/25/30/35 pedras | `GATHER` — ato de coletar pedra | vai ao veio e liga a auto coleta |
| Caça do dia | derrotar 30/40/50/60 bichos | `KILL` qualquer mob | vai à zona de mob e liga o auto combate |
| Mãos à obra | criar 1 equipamento | `CRAFT` — craft com sucesso | abre o painel de Craft |
| Forja quente | tentar refinar 1 vez | `REFINE` — tentativa, suba ou não | abre a Forja |
| Encanto do dia | encantar | `ENCHANT` | **Em breve** (cadeado) |
| Porão do dia | concluir uma dungeon (Porão ou Gruta) | `DUNGEON` | conta na vitória (docs/DUNGEONS_E_RAIDS.md) |
| Caçada do dia | derrotar o chefe da Caçada | `RAID` | **Em breve** (cadeado) |
| Chefe do dia | derrotar 1 chefe de campo da ilha | `KILL` `ALVO_QUALQUER_CHEFE` — conta pra quem estava na luta | vai à vaga do chefe e liga o auto combate |

- **Reset:** meia-noite UTC. Entregue, a diária volta no dia seguinte. Aceita e
  não entregue até a meia-noite, **expira** e sai do log (o servidor confere a
  cada segundo). O menu (L) mostra a seção "Diárias · reset em Xh YYmin".
- **Ilha:** o Mestre só oferece e aceita as missões da ilha em que está.
- **Em breve:** aparece no menu com cadeado e o motivo no hover; o servidor não
  deixa aceitar.

## Poção de Experiência

Missão **de área** (matar ou juntar numa zona) paga também **1 Poção de
Experiência**: as diárias Pedreira e Caça, e na cadeia "Lobos na estrada",
"Cobre pra forja" e "Ursos na mata". A poção dá +30% de XP por 1 hora (ver
ITENS.md); o HUD mostra o ícone com os minutos restantes.

## História (missão principal)

Além da cadeia do Mestre e das diárias, existe a **história**: uma linha de
missões que nunca acaba, dada e avançada pelo servidor sem aceitar nem
entregar, com travas só por nível e pela rota entre ilhas. Detalhes em
[HISTORIA.md](HISTORIA.md).

- Objetivos novos: `LUGAR` (ponto-chave da ilha), `NIVEL` (trava) e `VIAGEM`
  (embarcar com o Capitão do Porto).
- Destinos novos da auto missão: `LUGAR` (ir e esperar no ponto) e `TRAVA`
  (parar com aviso).
- A história não pode ser abandonada; o rastreador a mantém no topo.

### Recompensas extras das diárias

Além da recompensa própria, cada diária paga uma poção: as de área (quebrar pedra, caçar) dão **Poção de Experiência**, a de criar item dá **Poção de Fortuna** e a de refinar dá **Poção de Sorte** (ver `docs/ITENS.md`).

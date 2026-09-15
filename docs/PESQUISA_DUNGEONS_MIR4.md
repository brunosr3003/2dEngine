# Pesquisa: dungeons e conteúdo instanciado do MIR4

> Documento de **pesquisa**, não de desenho. Complementa a seção 1 de
> `DUNGEONS_E_RAIDS.md`. Pesquisado em set/2026 com as fontes listadas no fim.
>
> Legenda: **(confirmado)** = fonte oficial (fórum/patch note da Wemade) ·
> **(guia)** = guia de terceiro · **(incerto)** = uma fonte só, snippet de
> busca ou fontes divergentes · **(não encontrado)** = procurado e sem
> evidência.
>
> Limitação: a MIR4 Wiki (`mir4.wiki`) respondeu HTTP 500 em todas as páginas
> durante a pesquisa, e alguns guias (GameWith de raid, pinoygamer, devtrackers)
> bloquearam. Onde só havia resumo de busca, está marcado **(incerto)**.

---

## 0. Resumo em uma tabela

| conteúdo | onde entra | grupo | tempo | entradas | ticket extra | abre em | entre servidores |
|---|---|---|---|---|---|---|---|
| **Raid** | Menu > Raid | solo possível; até **5** (incerto) | limite existe, valor não publicado (incerto) | **2 grátis/dia** | gold, **2 recargas/dia**; +entradas pelo prédio *Torre da Vitória* | nível 20 (incerto) + Entry Power Score | **sim**, lista de recrutamento da **região** |
| **Boss Raid** | Menu > Raid | até **15** (incerto) | idem | **1 grátis/dia** | gold, 2/dia | nível 30 (incerto) + poder por chefe | sim, região |
| **Hell Raid** | Menu > Raid | até **15** | **15 min** | **0 grátis** | ticket do *baú de tarefa diária* + 1 request de NPC | nível **90** | sim, região |
| **Competitive Raid** | Menu > Raid | 10v10 a 15v15, times sorteados | **10 min** | **1 grátis/dia** | — | (não encontrado) | busca automática (sem detalhe) |
| **Path of Fiery Battle** | Menu > Raid | solo ou até **5** | limite por run (valor não publicado) | ticket consumível | — | nível **120** | não dito |
| **Magic Square** (Praça Mágica) | Portal | solo em mapa compartilhado com **PvP** | **30 min** por entrada, extensível | **2 grátis/dia** (cresce com o prédio *Portal*) | gold 150/200 (2/dia), ticket, até 99 acumuladas | missões do Labirinto 1F + poder por andar | não (servidor próprio) |
| **Secret Peak** (Pico Secreto) | Portal | idem | 30 min, extensível | 2 grátis/dia | idem | poder por andar | não |
| **Fissured MS/SP 11F** | Portal | idem | 30 min | **só ticket** | craft, loja de Mileage/Clã (2–3×/semana, incerto), drop 8–10F | 270k / 275k de poder | não |
| **Clan Challenge** | clã | até **50** do clã | **30 min** para 5 chefes | 1 estágio por semana (domingo) | fundo do clã | pesquisa de clã | não |
| **Clan Expedition** | clã | clã | sábado 00–24h | 2 participações/semana por pessoa | fundo + darksteel + energy do clã | — | não |
| **Expedition** (invasão) | Castelo de Bicheon | 300 por servidor | 21:00–01:00 | 5 vidas | gold, 4 tickets/semana | nível 70 + clã top 20 | **sim**, mesma região |
| **Attack of the Living Wraiths** | 4º andar dos vales | aberto | quinta 22–23h | evento | — | acesso ao vale | não |
| **Hydra's Depths 5F** | área de Expedição | aberto | — | — | — | Conquest *Sanctuary of Hydra* estágio 13 | sim (área de expedição) |
| **Heist** | cofre do Castelo de Bicheon | clãs | sexta | evento | recursos do clã | — | nativos e expedicionários |

**Não existem no MIR4** (procurados sem resultado, provavelmente nomes de
outros jogos): *Mirror Dungeon*, *Dark Nest*, *Hell Valley*, *Wraith
Dungeon*, *Gathering dungeon*. O que existe com nome parecido:
*Crimson Dragon Nest* é uma **raid**, *Hell Raid* é a raid de topo, *Wraiths*
é um **evento** num andar de vale, e "Hidden Valley/Secret Valley" são **mapas
abertos** com andares (1F–4F), não instâncias.

---

## 1. Conteúdo por tipo

### 1.1 Raid (instância de grupo com vários inimigos)

| item | valor | status |
|---|---|---|
| Entrada | Menu > aba **Raid** > escolher Raid ou Boss Raid | confirmado |
| Desbloqueio | Entry Power Score por raid; nível 20 como porta geral | poder: confirmado; nível: **incerto** (resumo de wiki) |
| Grupo | "até 5 jogadores", auto-start quando 5 entram | **incerto** (snippet de busca) |
| Solo | "You can attempt a Raid alone, but being in a party is recommended" | guia (gameplay.tips FAQ) |
| Entradas | **2 grátis/dia**, recarga com gold **2×/dia** | confirmado (fórum post/42) |
| Entradas extras | promover o prédio **Conquest > Tower of Victory** dá mais entradas de raid. Torre abre no nível 34 + 1 raid concluída | confirmado (post/1887) |
| Reset | 04:00 (guias antigos) **ou** 00:00 UTC+8 (fórum) | **conflitante**; 00:00 UTC+8 = 16:00 UTC, que é o reset regional atual |
| Estágios | cada raid tem **estágios numerados** (dificuldade). Ex.: *Vipergeist Prison* = **14º estágio** da Raid, mobs nível 165 | confirmado (PR Newswire) |
| Morte | **não perde EXP** dentro de raid | guia |
| Saída | ícone de **porta abaixo do timer** sai no meio; não dá para trocar de área durante a raid | guia |
| Recompensa de 1ª vitória | **por correio**, não direto na bolsa. Ex.: estátua do Dragão Azul Épica, pedra Divine Dragon Rara, pedra Mystic Lendária | confirmado |
| Drops normais | por chance, sem garantia. Bolsa cheia → vai para o correio | guia |
| Impedimento | não entra em raid durante Clan Expedition/Challenge, Cerco, Heist, Sabuk | confirmado |

Raids conhecidas (lista **incompleta**, ordem aproximada): *Wailing Dead Mine*
(nível 115), *Crimson Dragon Nest*, *Hidden Altar*, *Sabuk Execution Ground*,
*Forgotten Arena*, *Vipergeist Prison* (14º estágio, 165).

### 1.2 Boss Raid (um chefe forte)

| item | valor | status |
|---|---|---|
| Grupo | até **15** | **incerto** (snippet); o fórum oficial não dá o número |
| Entradas | **1 grátis/dia**, recarga com gold 2×/dia | confirmado |
| Desbloqueio | poder por chefe; nível 30 | nível **incerto** |
| Estágios | *Claydoh GEN* = **10º estágio** do Boss Raid, nível 160 | confirmado |
| Chefe exemplo | *Nefariox King* (nível 105) dá montaria *Hell Horse* | guia (Pocket Gamer) |
| Mecânica | chefe com golpe de área + invocação de adds + tempestade "tudo de uma vez"; "precisa de muito HP e lutar junto" | confirmado (press release) |

### 1.3 Hell Raid (topo)

Patch de 04/out/2022 (cs.mir4global.com/post/1046):

| item | valor |
|---|---|
| Abre | **nível 90**, e cada Hell Raid pede um Required Power Score |
| Grupo | **até 15**; "quanto mais participantes, melhor a recompensa" |
| Tempo | **15 min** |
| Entrada | **sem entrada grátis**. Ticket vem do **Daily Task Reward Box** (tarefas diárias) + 1 request no NPC do Castelo de Bicheon |
| Níveis do chefe | **100, 120, 140**; bônus por matar o de **140 dentro dos 15 min** |
| Ressurreição | cooldown **10 s** na 1ª morte, **+10 s por morte** seguinte |
| 1ª vitória | 2 chefes dão pedra e estátua **Épicas**; *Inferno Chief* dá as **Lendárias** |
| Drops | arma secundária e brinco **Épicos tier 1**, Divine Oil, tributos |

### 1.4 Competitive Raid (PvE com dois times)

Fórum post/2401 (confirmado):

- Dois times **sorteados na hora**, mínimo 20 (10v10), máximo 30 (15v15).
- **10 min**; vence quem dá o **último golpe** no chefe. Ninguém matou: os dois
  perdem.
- **1 entrada grátis/dia**.
- Recompensa em 3 camadas: resultado (vencedor ganha mais), **baú no chão por
  5 min para todos** após a vitória, e conquistas.
- Sair antes da morte do chefe: perde a recompensa **e não devolve a entrada**.

### 1.5 Path of Fiery Battle (desafio em estágios)

Fórum post/2108 (confirmado):

- Nível **120**, Menu > Raid > Path of Fiery Battle.
- **6 atos × 10 cenas**. Solo ou grupo de **até 5**.
- Todas as cenas da run dentro de um **limite de tempo**.
- **Ticket consumível**; perdido se deslogar ou sair antes de terminar.
- **Wipe preserva o progresso de estágio**; os inimigos resetam. Sem perda de EXP.
- 1ª vitória por correio. Cada conclusão dá a moeda *Trainee's Crescent Jade*,
  trocada no NPC por pedras, tomos de skill, ervas, cristais e caixas de escolha.

### 1.6 Magic Square e Secret Peak (conteúdo "Portal")

Detalhe na seção 2.

### 1.7 Conteúdo de clã

**Clan Challenge** (fórum post/290, confirmado):
- **Domingo** 00–24h. O líder ou quem tem autoridade de guerra escolhe **1
  estágio por semana** e paga com o **Fundo do Clã** (2.500–22.500).
- **Até 50** do clã. **5 chefes sequenciais**, o próximo aparece logo depois do
  anterior. **30 min** para todos; estourou, falha.
- Baú de desafio **por chefe**, bônus no 5º. Clan EXP e Clan Energy.
- Anti-abuso: depois de limpar 1 chefe **não participa de outro desafio**,
  nem trocando de clã.
- Estágios foram de 1–9 para **8** (Challenge) e **10** (Expedition) em dez/2023
  (**conflito** de números entre o fórum e o press release; o press release é
  mais novo).

**Clan Expedition** (post/290):
- **Sábado** 00–24h. **12 chefes** em escada de dificuldade. Invocar custa
  Fundo (2.500–27.500) + Darksteel e Energy do clã (125k–1,375M). Invocação
  extra: 1.000–3.000 de Clan Gold.
- **2 participações por semana por pessoa**. Recompensa **por contribuição**,
  bônus na 1ª conclusão. Trocar de clã depois de receber = sem recompensa.

**Não existe "dungeon de clã" de andares**: o conteúdo PvE de clã é essa
fila de chefes invocados com recurso coletivo.

### 1.8 Expedition (invasão entre servidores)

Fórum post/748 (confirmado):
- Diária, **21:00–01:00**. Nível 70, no Castelo de Bicheon, clã **top 20**.
  **300 por servidor**. Só servidores **da mesma região**.
- **5 vidas**; zerou, só pode ficar no posto avançado e na caverna de treino.
  Caiu a conexão, volta ao servidor de origem.
- Tokens: **máx. 20/dia**. Ticket com gold, **máx. 4/semana**.
- Modo Assassino com codinome no lugar do nome (ticket semanal).

### 1.9 Eventos em mapa aberto

- **Attack of the Living Wraiths**: quinta 22–23h, **4º andar** dos vales de
  Bicheon, Snake e Redmoon. Nomeado → invoca semi-chefe → **chance** de chefe.
  Dá material de dragão Épico, pedra Mystic, estátua do Dragão Azul.
- **Hidden Valley** (vales): mapas abertos com **1F–4F** e *Secret Passages*
  (PvP). Entrada por Conquest ou nível (Redmoon: nível 75 + missão). O monólito
  da captura de clã fica no 4F.
- **Heist**: sexta, clãs atacam o cofre do Castelo de Bicheon. Portão de ferro →
  chefe *Cheol Mujin* → **baús de graus diferentes** que **precisam ser
  abertos** (senão perde). Teto de darksteel **igual em todos os servidores**.
- **Hydra's Depths**: 5 andares; o 5F é área de Expedição com o chefe *Ragnos*
  (nível 200). Exige Conquest *Sanctuary of Hydra* estágio 13.

---

## 2. Magic Square e Secret Peak em detalhe

### 2.1 Regras comuns

| regra | valor | fonte |
|---|---|---|
| Desbloqueio | concluir as missões do **Labirinto 1F** (Magic Square) | guia (Touch Tap Play, Level Winner) |
| Entradas grátis | **2/dia**; o máximo cresce com o prédio **Conquest > Portal** | confirmado |
| Recarga | gold **150 (1ª) e 200 (2ª)**, 2/dia; ou ticket. Saldo acumula até **99** | confirmado (post/43) |
| Duração | **30 min** por entrada, botão **[+]** estende com ticket/gold; **Auto-Extend até 20 vezes seguidas** | confirmado |
| Auto-Extend para | se o personagem **morre ou sai** | confirmado |
| Morte | renasce na **Primordial Chamber**, zona segura sem combate, **dentro** do conteúdo | confirmado |
| Saída | porta abaixo do timer; o tempo restante se perde | guia |
| Andar | cada andar abre pelo **Entry Power Score**; a tela mostra poder recomendado, nível dos mobs e prévia de loot antes de escolher | confirmado + guia |
| PvP | ligado na maioria das câmaras | guia |
| Reset | 16:00 UTC (ASIA 00:00, EU 18:00, SA 13:00, NA 12:00) | confirmado |

### 2.2 Diferença entre os dois

| | Magic Square | Secret Peak |
|---|---|---|
| Forma | **26 câmaras** isoladas, acesso por **teleporte aleatório** (cada câmara ~2,2–4,4% de chance no warp) | **um mapa inteiro** contínuo, anda livre |
| Chefes | poucos, em câmaras próprias (*Leader's Chamber I–III*, *Demon's Chamber*) | espalhados; fixos por horário + verdes/amarelos por respawn |
| Foco percebido | escolher a câmara do recurso (EXP, gold, prata branca, pedra mágica, darksteel) | EXP e coleta em área aberta; "dá mais EXP" |
| Status | guia (GameWith, pinoygamer via busca) | idem |

Câmaras da Magic Square (GameWith): Principal (loja de poção, troca de
material), **Training** (EXP, **PvP desligado**), recursos (Gold, White Silver,
Magic Stone), chefes (Leader I–III, Demon), especiais (**Dark Steel**, Treasure,
Protection, Sealing, Cooperation). A câmara de Dark Steel tem **nós Épicos e
Lendários**, raros, e **PvP ligado** "para disputar o direito de minerar".

### 2.3 Andares, poder e nível dos mobs

**Secret Peak** (GameWith, guia):

| andar | poder | mobs |
|---|---:|---|
| 1F | 7.900 | 25–40 |
| 2F | 27.000 | 45–55 |
| 3F | 40.000 | 60–70 |
| 4F | 48.000 | 75–85 |
| 5F | 53.000 | 90–100 |
| 6F | 67.000 | 105–115 |
| 7F–10F | (não publicado) | (não publicado) |
| 11F Fissurado | **275.000** (confirmado) | até **200** |

**Magic Square**:

| andar | poder | mobs | status |
|---|---:|---|---|
| 1F | 9.600 | 20–35 | **incerto** (snippet; outro guia diz "até 35") |
| 2F–9F | — | — | não encontrado |
| 10F | 208.000 | 160–175 | incerto (já citado no doc de desenho) |
| 11F Fissurado | **270.000** | até 200 | confirmado |

Histórico: o **9F** entrou em fev/2023 (Antara), o **11F Fissurado** em
14/mai/2024 (Inven). Andares novos entram **no topo** a cada ano, conforme o
teto de poder da base sobe.

### 2.4 Chefes e horários

| chefe | respawn |
|---|---|
| MS Leader's Chamber I | 30 min após morrer |
| MS Leader's Chamber II | 45 min |
| MS Leader's Chamber III | **fixo a cada 3 h** (00, 03, 06… UTC+8) |
| SP chefe fixo | **fixo a cada 3 h** (13, 16, 19, 22, 01, 04, 07, 10) |
| SP chefe verde / amarelo | 30 min / 1 h após morrer (**incerto**: comentário de usuário) |

Chefes dão **materiais trocáveis por baú de tomo de skill** (GameWith).

### 2.5 Recompensas e "pontos"

- MS/SP dão EXP, cobre, **darksteel**, **Magic Stone**, gold, prata branca,
  baús, *Ancient Coins* (guias).
- Andar mais alto = **mais quantidade e grau maior** do mesmo tipo de recurso
  (GameWith).
- 9F: pedra *Legendary Darkened Enhancement* e *Epic Snow Panax* (Antara).
- 11F: *Spacetime Powder* para craftar o 3º equipamento de transferência
  (Inven).
- **Energy** e **material de dragão** como drop da MS/SP: **não encontrado**
  nas fontes. Material de dragão Épico aparece em raid (1ª vitória) e nos
  Wraiths.
- **"Pontos" diferentes entre MS e SP: não encontrado.** Não há sistema de
  pontuação por andar; o que muda é o poder exigido e a tabela de drop.

### 2.6 Ticket da versão Fissurada

Três fontes de ticket (post/43):
1. **Craft** com darksteel e pedras (a partir de tickets comuns, segundo a Inven);
2. **Compra** com Mileage ou Clan Coin, **2–3×/semana** (as fontes divergem:
   "até 3 vezes por semana, reset segunda 00:00 UTC+8" num snippet do mesmo
   post), **sem dinheiro real direto**;
3. **Drop** de monstros dos andares 8F–10F.

---

## 3. Anti-farm, anti-bot e economia

| mecanismo | como o MIR4 faz | status |
|---|---|---|
| Entrada diária pequena | Raid 2, Boss Raid 1, Competitive 1, MS/SP 2 | confirmado |
| Recarga paga e com teto | gold, 2/dia, preço crescente (150 → 200 na MS/SP) | confirmado |
| Topo sem entrada grátis | Hell Raid e Fissurado só por ticket (tarefa diária, craft, loja semanal) | confirmado |
| Progressão aumenta entrada | prédios de Conquest (*Portal*, *Tower of Victory*) | confirmado |
| Tempo como moeda | MS/SP é **tempo**, não conclusão: 30 min por entrada | confirmado |
| Teto de token por dia | Expedition: 20 tokens/dia; 4 tickets/semana | confirmado |
| 1ª vitória uma vez | recompensa grande, por correio | confirmado |
| Abandono não devolve | Competitive Raid e Path of Fiery Battle queimam a entrada | confirmado |
| Anti-troca de clã | Clan Challenge/Expedition não pagam de novo quem troca de clã | confirmado |
| Item vinculado | há itens *Bound* intransferíveis; equipamento Épico comum é negociável no Marketplace (em darksteel) | guia (MMOM), **incerto** quanto a quais itens exatamente |
| Teto de conversão | smelting de darksteel → DRACO com **limite diário por personagem** (100 DRACO/dia nas fontes de 2021–22) | guia, **datado** |
| Porta de tempo no bot | após a onda de bots nas minas, a Wemade **aumentou os requisitos** de acesso às áreas de darksteel para que preparar conta nova leve semanas | guia (Hive), **incerto** |
| Banimento em massa | "mais de 1,1 milhão de contas" banidas por anti-cheat | **incerto** (fonte P2E, não oficial) |
| Multi-cliente | **permitido**: 1 por celular + 2 pelo launcher do PC + 1 pelo Steam | guia (MIR4 Wiki FAQ via busca) |

Leitura: o MIR4 **não** confia em detecção de bot. Ele limita **por
personagem e por dia**, e deixa multi-cliente legal. O resultado conhecido é
que as zonas de darksteel encheram de bots ("~90% em áreas de mineração",
reclamação da comunidade Steam, **incerto**). A lição para nós é que
**vinculado por padrão** e **teto diário** fazem o trabalho que a detecção não
faz; e que a economia **negociável** (darksteel ↔ DRACO) foi o que atraiu os
bots.

---

## 4. Grupo e matchmaking

| pergunta | resposta | status |
|---|---|---|
| Existe auto-match cego? | **Não encontrado.** Só **lista de recrutamento**: *Search Raids* (ver grupos que recrutam, pedir para entrar) e *Create Raid* (criar grupo) | confirmado (fórum post/42) |
| Auto-start | "o grupo começa sozinho quando 5 entram" | **incerto** (snippet) |
| Entrar sozinho | Raid pode ser tentada solo; Path of Fiery Battle solo ou até 5 | guia / confirmado |
| Entre servidores | grupo com "Dragonians **da sua região**"; *Raid Party Chat* mostra **servidor de origem + nome** | confirmado |
| Competitive Raid | times **sorteados** no início; o fórum fala em "busca automática" | confirmado |
| MS/SP | **sem grupo formal**: mapa compartilhado do servidor, grupo/clã do mundo aberto conta para PvP | guia |
| Conteúdo de clã | só membros do clã; quem inicia é líder/autoridade de guerra | confirmado |
| Expedição | por clã e ranking; só mesma região | confirmado |

---

## 5. UI/UX

| momento | o que o MIR4 mostra | status |
|---|---|---|
| **Entrada** | Menu principal > aba **Raid** com sub-abas (Raid, Boss Raid, Hell Raid, Competitive Raid, Path of Fiery Battle). Lista à esquerda com cada conteúdo, **poder exigido** e **máximo de jogadores**; botões *Search Raids* / *Create Raid* no canto inferior direito | confirmado + guia |
| **Portal** | ícone de Portal abre MS/SP; escolhe o **andar** vendo poder recomendado, nível dos mobs e loot possível; botão **Recharge** mostra entradas e custo | confirmado + guia |
| **Durante** | **timer** no alto; ícone de **porta** abaixo do timer para sair; em MS/SP, botão **[+]** para estender e opção Auto-Extend | guia + confirmado |
| **Morte** | Raid: cooldown de ressurreição (Hell Raid 10 s +10 s/morte), sem perda de EXP. MS/SP: renasce na câmara segura dentro do conteúdo | confirmado |
| **Falha** | tempo esgotou = derrota (Competitive: os dois times perdem; Clan Challenge: timeout aos 30 min). Path of Fiery Battle guarda o estágio no wipe | confirmado |
| **Recompensa** | baús no chão que **precisam ser abertos** (Heist, Competitive: 5 min); 1ª vitória **por correio**; bolsa cheia → correio | confirmado + guia |
| **Mobs restantes / andar** | contador de mobs restantes na raid: **não encontrado** nas fontes textuais | — |

---

## 6. Mobile vs PC

| ponto | MIR4 | relevância |
|---|---|---|
| Servidores | **os mesmos**; cross-play total | um só balanceamento |
| UI | a mesma nos dois (mobile-first); PC muda só qualidade gráfica | confirma HUD por toque |
| Multi-cliente | até 4 clientes simultâneos por pessoa (celular + PC) | abre farm paralelo; entradas **por personagem** viram multiplicáveis |
| Auto combate | existe nos dois; raids e MS/SP são jogadas em auto por muita gente (comunidade) | **incerto** quanto a diferenças; não achei nada exclusivo de uma plataforma |
| Diferença de conteúdo | **nenhuma encontrada** | — |

---

## 7. O que isso diz sobre o `DUNGEONS_E_RAIDS.md`

### Confirma

- **Grupo de 5** para a dungeon de grupo, e raid com mais gente.
- **Entradas diárias pequenas + recarga com gold, 2/dia, preço crescente**
  (MIR4: 150 → 200 gold; nosso ×1 → ×3 é mais agressivo, o que é aceitável).
- **Portão por poder** mostrado na lista, antes de entrar.
- **1ª vitória paga muito mais**, e no MIR4 ela chega **por correio**: casa
  exatamente com a "carta de recompensa" do nosso desenho.
- **Topo sem entrada grátis**, com ticket vindo de tarefa diária (Hell Raid):
  é o nosso Selo da Tempestade.
- **Grupo entre servidores só na região**, com o **servidor de origem ao lado
  do nome** (`Nome@SA01`).
- **Nomeado → semi-chefe → chefe** (Wraiths) para o chefe de campo.
- **Moeda de conclusão trocada em NPC** (Crescent Jade ≈ Marcas da Tempestade).
- **Entrada queimada em abandono** (Competitive, Path of Fiery Battle).
- **Wipe não zera o progresso** (Path of Fiery Battle guarda o estágio): apoia
  o baú por andar da Gruta.

### Contradiz ou corrige

- **Tabela da seção 1**: acrescentar **Hell Raid = 15 jogadores, 15 min, chefes
  nível 100/120/140, bônus por tempo**; **Competitive Raid** e **Path of Fiery
  Battle** não estão listados; **Boss Raid "até 15" e "nível 30" continuam sem
  fonte oficial**.
- **"Grupo cross-server com auto-match"**: o MIR4 **não tem auto-match
  cego** documentado, só lista + começar quando lota. O nosso auto-match é
  **invenção nossa**, e está certo como decisão (fila vazia em realm novo), mas
  a seção 1 não deve dizer que veio do MIR4.
- **Dificuldade**: o MIR4 usa **estágios numerados por raid** (Raid até o 14º,
  Boss Raid até o 10º), cada um com nível de mob maior, e não
  Normal/Difícil/Pesadelo. As duas coisas funcionam; a escada numerada escala
  melhor em conteúdo que dura anos.
- **Morte em instância**: o MIR4 **não pune** morte em raid (sem perda de EXP,
  só cooldown crescente de ressurreição). O custo de **reparo por morte**
  proposto na seção 8 vai contra isso; o **cooldown de ressurreição crescente**
  é o equivalente que o MIR4 usa.
- **"Aventuras (janela, tecla livre a definir)"** e **"Diário (J)"** na seção 9
  contrariam a regra do projeto (**nada abre por tecla**). O MIR4 também entra
  por **Menu > Raid**.
- **Chave de craft na raid "o dobro" (`MULT_RAID`)**: a regra passada é
  **15/10/6/3/1% para chefe de dungeon e de raid**. O doc dobra na raid; é
  preciso decidir qual vale. O MIR4 não publica taxa de drop para comparar.

### Lacunas (o MIR4 tem, nós não)

- **Bolsa cheia → correio**: o coletor de cartas precisa de um destino quando
  a bolsa não cabe.
- **Cooldown de ressurreição crescente** (10 s +10 s) dentro da instância.
- **Bônus por tempo** (Hell Raid: matar no nível 140 em 15 min).
- **Baú no chão que precisa ser aberto** (Heist, Competitive): é a cena de
  recompensa, e mobile gosta de toque.
- **Entradas crescem com a progressão** (Tower of Victory, Portal): no Tempest
  isso pode vir da colônia offline ou do nível de conta, sem TP.
- **Conteúdo por tempo com extensão** (MS/SP: 30 min + Auto-Extend) para a
  fase 7 "Mar Revolto": o tempo é o limite, e o Auto-Extend tem teto (20).
- **Chefe fixo a cada 3 h** + chefes por respawn curto (30/45 min) no mesmo
  mapa.
- **Ticket de topo por craft** (Fissurado): ralo de material, e não de dinheiro.
- **Conteúdo de clã invocado com recurso coletivo** (Clan Challenge: 5 chefes
  em sequência, 30 min, 1×/semana), para quando existir clã.
- **Raid competitiva** (dois times, último golpe vence): PvE com rivalidade,
  sem dano entre jogadores. Idea barata para depois.
- **Travas anti-abuso de troca de grupo** ("depois de limpar 1 chefe, não
  entra em outro desafio"): lockout por semana também ao trocar de grupo.

### Específico do nosso jogo

- **Auto combate nunca desvia**: no MIR4 o auto roda a raid inteira, e os
  golpes são de "muito HP". Como o nosso desvio é na mão e o foco é iOS, o
  **telegráfico precisa ser legível em tela pequena** e a janela mínima de
  1,2 s do doc deve ser **medida no iPhone**, com toque, e não no PC.
- **Multi-cliente**: o MIR4 permite 4 e sofreu com bot. As entradas do doc são
  **por personagem**; com tudo vinculado, o ganho do alt é pequeno, mas o teto
  de **Épico por Marcas** e o lockout de 1ª vitória semanal deveriam ser **por
  conta**, e não por personagem.

---

## Fontes

Oficiais (fórum/patch notes da Wemade):
- Raid / Boss Raid / Hell Raid, recrutamento e chat entre servidores: https://forum.mir4global.com/post/42
- Magic Square / Secret Peak, recarga, Auto-Extend, Fissurado, reset por região: https://forum.mir4global.com/post/43
- Hell Raid (patch 04/out/2022: 15 jogadores, 15 min, ressurreição, tickets): https://cs.mir4global.com/post/1046?sorttype=1
- Competitive Raid: https://forum.mir4global.com/post/2401
- Path of Fiery Battle: https://forum.mir4global.com/post/2108
- Clan Expedition / Clan Challenge: https://forum.mir4global.com/post/290
- Expedition: https://forum.mir4global.com/post/748
- Tower of Victory / Conquest (desbloqueio, entradas): https://forum.mir4global.com/post/1887
- Sanctuary of Hydra / Hydra's Depths: https://forum.mir4global.com/post/1857?lang=en
- Boss Raid do MIRAGE (evento): https://forum.mir4global.com/post/838?sorttype=1
- Heist: https://forum.mir4global.com/post/888

Imprensa:
- PR Newswire, Vipergeist Prison (14º estágio) e Claydoh GEN (10º estágio): https://www.prnewswire.com/news-releases/challenge-new-enemies-in-mir4-new-raid-and-boss-raid-revealed-301688395.html
- Pocket Gamer, Hell Raid: https://www.pocketgamer.com/mir4/hell-raid/
- Pocket Gamer, Wraiths + Wailing Dead Mine / Nefariox King: https://www.pocketgamer.com/mir4/mir4-introduces-attack-of-the-living-wraiths-and-new-raid-dungeons-in-latest-upd/
- Inven Global, Fissured MS / SP 11F: https://www.invenglobal.com/articles/18824/wemade-presents-mir4-fissured-magic-square-and-secret-peak-11f-update
- ANTARA, 9º andar da MS/SP: https://en.antaranews.com/news/273462/wemade-updates-the-highest-floors-of-magic-square-and-secret-peak-in-mir4
- Business Wire, Clan Expedition/Challenge estágio máximo: https://www.businesswire.com/news/home/20231212201524/en/Wemade-MIR4-Presents-the-Clan-Expedition-and-Clan-Challenge-Highest-Stage-Update

Guias:
- GameWith, Magic Square (câmaras, respawn, Dark Steel Chamber): https://gamewith.net/mir4/article/show/31619
- GameWith, Secret Peak (poder e mobs por andar): https://gamewith.net/mir4/article/show/31581
- Touch, Tap, Play, Magic Square: https://www.touchtapplay.com/mir4-magic-square-guide/
- Touch, Tap, Play, Raids: https://www.touchtapplay.com/how-to-join-raids-in-mir4-raids-guide/
- Level Winner, áreas, morte, Magic Square: https://www.levelwinner.com/mir4-advanced-guide-pvp-mode-death-penalties-area-types-codex-and-magic-square-explained/
- gameplay.tips, FAQ (porta de saída, sem perda de EXP, correio): https://gameplay.tips/guides/mir4-ultimate-gameplay-faq.html
- gameplay.tips, Clã: https://gameplay.tips/guides/mir4-ultimate-clan-guide.html
- MMOM, troca e itens vinculados: https://www.mmom.com/news/detail_195.html
- Touch, Tap, Play, darksteel → DRACO: https://www.touchtapplay.com/how-to-convert-darksteel-to-draco-in-mir4/
- Hive, bots e requisitos de mineração: https://hive.blog/hive-140217/@quixoticflux/mir4-vs-the-bots-can-wemade-stop-botting-from-ruining-its-crypto-ecosystem
- Steam, loja (cross-play PC/mobile): https://store.steampowered.com/app/1623660/MIR4/
- MIR4 Wiki, Raid / Magic Square / Secret Peak (HTTP 500 na consulta; dados só via resumo de busca): https://www.mir4.wiki/wiki/Raid · https://www.mir4.wiki/wiki/Magic_Square · https://www.mir4.wiki/wiki/Secret_Peak
- MIR4 Wiki, Hidden Valley Capture (andares e requisitos dos vales, via busca): https://www.mir4.wiki/wiki/Hidden_Valley_Capture

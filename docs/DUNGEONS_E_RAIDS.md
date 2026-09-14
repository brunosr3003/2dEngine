# Dungeons, raids e chefes: conteúdo de grupo entre servidores

> Documento de **desenho**, não de estado. Nada daqui está implementado além do
> que a seção "O que já existe" lista. Pesquisa do MIR4 no começo, desenho do
> Tempest no meio, plano de fases no fim. Números marcados com ⚠️ são pontos de
> partida, não medições: afinam no playtest.

## 1. O que o MIR4 faz (pesquisado)

Fontes no fim do documento. Onde as fontes divergem ou não dizem, está marcado
**(incerto)**.

| conteúdo | formato | grupo | entrada | requisito | o que dá |
|---|---|---|---|---|---|
| **Raid** | instância com vários inimigos | até **5** | **2 grátis/dia**, +2 recargas com gold | poder de entrada por raid; abre no **nível 20** | primeira vitória dá itens grandes (estátua de dragão, pedra de melhoria Épica/Lendária) |
| **Boss Raid** | um chefe forte | até **15** | **1 grátis/dia**, +2 recargas com gold | poder por chefe; abre no **nível 30** (incerto: uma fonte só) | idem, e "mais membros, mais recompensa especial" |
| **Hell Raid** | chefes mais fortes que o Boss Raid | — | **sem entrada grátis**: ticket de NPC ou do baú diário | nível 90 | materiais Épicos |
| **Praça Mágica / Pico Secreto** | mapa aberto com PvP, **30 min/dia** extensíveis por ticket | solo, disputado | 2 entradas grátis/dia, recarga com gold | andar por **poder** (Pico 1F: 7.900 de poder, mobs 25–40; 6F: 67.000, mobs 105–115; Praça 10F: 208.000, mobs 160–175) | darksteel em nós especiais, baús de chefe (materiais de livro de skill), pedras de melhoria; chefe renasce por **horário fixo** (a cada 3h) ou 30–60 min |
| **Versão "Fissurada" (11F)** | andar de topo | solo | **só por ticket** (craft, loja do clã 3×/semana, drop raro) | 270–275 mil de poder | materiais Épicos diários |
| **Chefe de Mundo** | arena por portal, **450 por arena**, excedente em fila | aberto | horário semanal fixo, servidor sorteado | — | (incerto: a regra de contribuição não é pública) |
| **Ataque dos Espectros** | evento semanal de 1h no 4º andar dos vales | aberto | horário | — | nomeado → chance de semi-chefe → chance de chefe; baús com **material de dragão Épico** |
| **Expedição** | **entre servidores**: invadir outro servidor e atacar os chefes de campo e o cofre de darksteel | 300 por servidor | janela de até 4h, 21:00 | clã **top 20** | darksteel em massa; o invasor tem **5 vidas** |
| **Captura do Vale / Cerco de Bicheon / Sabuk** | guerra de clãs; Sabuk junta os reis de cada servidor num **servidor de dominação** | clãs | semanal / a cada 4 semanas | pesquisa de clã | 15% de imposto do darksteel minerado no vale, título |
| **Vale da Vida e da Morte** | campo de batalha entre servidores | 10 times de 5 | 2/dia | nível 100, **todos normalizados** no nível 100 | pontuação, **MVP** leva caixa extra |

O que importa tirar disso:

- **Grupo entre servidores já é da região.** O MIR4 forma o grupo de raid com
  jogadores "da sua região", com chat de grupo entre servidores. É uma **lista
  de recrutamento** (Buscar / Criar raid) com a opção "começar quando lotar".
  Se existe auto-match cego além disso: **(incerto)**.
- **O portão é o poder, não só o nível.** Cada andar e cada chefe mostra o
  "Entry Power Score".
- **Entrada diária pequena, recarga com gold.** O gold da recarga é um ralo.
- **A primeira vitória paga muito mais.** É o que puxa para o conteúdo novo sem
  obrigar a repeti-lo pra sempre.
- **Material Épico vem de conteúdo de topo**, com saída diária fixa
  (Fissurada, Hell Raid, Expedição), e não de mob comum.
- **MVP**: o maior pontuador leva uma recompensa extra, calculada
  individualmente.
- **Normalização de nível** existe onde a competição precisa ser justa.

## 2. O que copiamos, adaptamos e descartamos

| do MIR4 | decisão | por quê |
|---|---|---|
| Raid de 5 e Boss Raid de 15 | **adaptar: 5 e 10** | Com 15, o AOI de 60 entidades enche só de jogador, pet e efeito, e sobra pouco para os adds. 10 mantém o chefe legível e cabe em muitas raids por processo |
| Portão por poder | **copiar** | O `poder` já existe no cliente (`client/bolsa.rs`). Vai para o `shared` (seção 12) |
| Entradas diárias + recarga com gold | **copiar**, com teto | É o ralo de gold mais limpo que existe: o jogador escolhe pagar |
| Primeira vitória vale mais | **copiar** | Premia descobrir o conteúdo, não repetir |
| MVP | **adaptar** | Contribuição **por papel**, e não só dano, senão tanque e suporte nunca são MVP |
| Grupo da região entre servidores | **copiar e ampliar** com auto-match | Um realm começando não tem gente para lotar uma raid de 10 às 3h da manhã. O pool entre realms é o que dá liquidez à fila |
| Normalização de nível | **adaptar: só para baixo** | Quem está acima da faixa desce até o teto dela. Mata o "carregar" e mantém o chefe difícil |
| Praça Mágica / Pico Secreto | **adiar** (fase 7) | O papel "farm de darksteel por tempo" já é da coleta e da colônia offline. Não vamos criar uma segunda torneira antes de medir a primeira |
| Chefe de mundo com 450 | **adaptar: fragmentos de ≤ 150** | Um processo não fecha o tick com 500 (SERVIDORES_E_CANAIS, "Evento em mapa único"). Vira várias instâncias paralelas |
| Espectros (nomeado → semi-chefe → chefe) | **copiar** no chefe de campo | Transforma o respawn fixo em evento, e custa só dado |
| Expedição / Cerco / Sabuk (PvP de clã) | **descartar por ora** | Não existe clã. PvP entre realms é outro documento |
| Hell Raid só por ticket | **adaptar** como "Pesadelo" | Mesma ideia: a dificuldade de topo não tem entrada grátis ilimitada |
| Ticket vendido por dinheiro | **descartar** | TP **nunca** compra entrada nem recompensa. É pay-to-win, e a ECONOMIA decide que TP só vai à loja cosmética e às pedras de refino |
| Peça de equipamento no baú | **adaptar** (ver seção 8) | Hoje "mob e chefe não dão equipamento" (LOOT_DOS_MOBS). A regra continua para **mob**. O **baú de conclusão** vira a única fonte de peça fora do craft |

## 3. Catálogo de conteúdos do Tempest

Cinco tipos. Nome temático primeiro, o genérico entre parênteses.

| tipo | nome no jogo | grupo | duração alvo | limite | entradas | abre em |
|---|---|---|---|---|---|---|
| **Porão** (dungeon solo) | *Porão do Naufrágio* | 1 | 6–8 min | 10 min | ilimitado, recompensa 3/dia | nível 6 |
| **Gruta** (dungeon de grupo por andares) | *Grutas da Maré* (uma por ilha) | 3–5 | 12–18 min | 25 min | **2/dia** (acumula até 4) + 2 compráveis | nível 10 |
| **Raid** (chefe) | *Caçada* | 6–10 | luta de 8–12 min | 30 min (enrage aos 10–12) | **1/dia** (acumula até 3) + 1 comprável | nível 25 |
| **Chefe de campo** | *Maré Sangrenta* | aberto, no canal | 5–10 min | — | sem entrada; baú 1×/dia por chefe | nível da ilha |
| **Chefe de mundo** | *Chamado do Leviatã* | aberto, fragmentos ≤ 150 | 20 min | horário | 1 baú por evento | nível 30 |

Regras que valem para todos:

- **Reset diário às 04:00** do fuso do realm; **reset semanal na quarta às
  04:00**. Mesmo horário para todos os realms da mesma região, senão o pool
  entre realms fica com gente de "dias" diferentes.
- **Entrada é consumida quando a instância começa** (todos entraram e o
  primeiro inimigo foi puxado), não na fila. Cai da fila ou o pronto-check
  falha: nada foi gasto.
- **Quem entra com a entrada zerada vai como Ajudante.** Ganha cobre, XP e
  Marcas da Tempestade (seção 8), mas nem peça nem chave. É o que mantém a fila
  cheia de gente que já fez o dia. Sem ajudante, a fila morre às 23h.
- **Entrada comprável**, com gold e preço crescente no dia: 1ª a 1×, 2ª a 3×
  ⚠️. É ralo, e o teto impede de virar torneira de quem tem gold.

### Porão (solo)

É o `DUNGEON_MODE` que já existe (8 lanes, 4 salas, 10 min), com recompensa e
entrada. Serve de tutorial de "instância": entrar, sala, portão, chefe, baú,
voltar. **Não entra no matchmaking.** Tem uma por faixa até o nível 30; depois
disso o solo diário é a Gruta em Normal com auto-match.

### Gruta (grupo por andares)

- **3 andares + chefe.** Cada andar é uma ilhota ligada por ponte com portão,
  igual às salas da lane atual.
- Andar 1 é de mobs da faixa. Andar 2 tem um **semi-chefe**. Andar 3 tem uma
  mecânica de grupo simples (seção 7). Chefe no fim.
- **Recompensa por andar** (baú pequeno) **e na conclusão** (baú grande). Cair
  no andar 3 não zera o dia inteiro.
- Grupo de 5: **1 tanque, 1 suporte, 3 dano**. Aceita 3–4 em Normal, com vida
  dos inimigos escalada (seção 6).

### Raid (Caçada)

- **Um chefe**, com fases. Adds entre as fases.
- 10 jogadores: **2 tanques, 2–3 suportes, 5–6 dano**. Mínimo 6 em Normal.
- **Baú por contribuição** a cada vitória, mais o **baú de primeira vitória
  semanal** por chefe e dificuldade. Esse segundo é o grande: é nele que moram
  as chances de Épico.

### Chefe de campo (Maré Sangrenta)

- Usa `boss_areas`, que já existe: um chefe por área, com respawn.
- Evento: durante a Maré (2×/dia por ilha ⚠️), os mobs **nomeados** da ilha
  têm chance de chamar um **semi-chefe**, e o semi-chefe chance de chamar o
  **chefe**. É a mecânica dos Espectros do MIR4.
- Fica no **canal**, sem trocar de processo. Contribuição por dano/cura/tanque,
  baú para quem passou do limiar. **Não é entre realms.**

### Chefe de mundo (Leviatã)

- Semanal. Instâncias de até 150, preenchidas pelo matchmaker **misturando
  realms** da região.
- A vida do chefe escala com quem está no fragmento. A contribuição é ranqueada
  **dentro do fragmento**.
- É a primeira coisa entre realms que o jogador vê em massa. Fica para depois
  da raid funcionar entre realms (fase 6).

## 4. Progressão e desbloqueio

### O que abre em cada faixa

O nível máximo é 100 (`CHAR_LEVEL_CAP`). As ilhas existentes cobrem até o 60
(`shared::terreno::ARQUIPELAGO`). **De 60 para cima não há ilha:** os
conteúdos de 60+ ficam amarrados a ilhas que ainda serão feitas (nomes de
trabalho abaixo).

| faixa | ilha | Porão (solo) | Gruta (grupo) | Caçada (raid) | chefe de campo |
|---|---|---|---|---|---|
| 1–10 | Bosque | *Porão do Naufrágio* (6) | — | — | Lobo Grande (existe) |
| 10–20 | Bosque | *Adega do Contrabandista* (14) | **Toca dos Lobos-do-Mar** (10) | — | Urso-Rei (14) |
| 20–30 | Geleira | *Casco Congelado* (22) | **Grutas de Gelo Fundo** (20) | **Mãe-da-Nevasca** (25) | Yeti Ancião (24) |
| 30–40 | Ermo | — | **Tumba das Areias Salgadas** (30) | **Escorpião-Colosso** (35) | Serpente das Dunas (34) |
| 40–50 | Planalto | — | **Mosteiro dos Ventos** (40) | **Grifo Trovejante** (45) | Carneiro de Pedra (44) |
| 50–60 | Planalto | — | **Forja do Titã** (50) | **Colosso de Basalto** (55) | Águia-Relâmpago (54) |
| 60–70 | *Recife da Tempestade* (nova) | — | **Cemitério de Navios** (60) | **Capitão Afogado** (65) | a definir |
| 70–80 | *Recife da Tempestade* | — | **Vórtice Submerso** (70) | **Rainha Kraken** (75) | a definir |
| 80–100 | *Olho da Tempestade* (nova) | — | **Fenda do Trovão** (80) | **Leviatã Desperto** (85) | — |
| 30+ | (evento) | — | — | — | Chefe de mundo **Chamado do Leviatã**, escalado por faixa |

O número entre parênteses é o nível mínimo em Normal.

### Como um conteúdo novo abre

Três portas, **todas** necessárias. Nenhuma sozinha abre.

1. **Nível** mínimo da tabela.
2. **Descoberta por missão.** Uma missão curta da cadeia da ilha, por exemplo
   *"Rumores na Taberna: o Escorpião-Colosso"*, leva o jogador até a
   **entrada física** no mapa (auto missão + viagem do mapa, que já existem).
   Tocar a entrada marca o conteúdo como descoberto. Depois disso, a fila abre
   de qualquer lugar.
3. **Vitória anterior** (só para a raid): vencer a Gruta da mesma ilha em
   Normal pelo menos uma vez.

A dificuldade sobe assim:

| dificuldade | abre com | nível | poder mínimo |
|---|---|---|---|
| **Normal** | as três portas acima | mínimo da tabela | 80% do poder de referência da faixa |
| **Difícil** | 1 vitória em Normal | mínimo + 5 | 100% |
| **Pesadelo** | 1 vitória em Difícil, **só em conteúdo de 60+** | mínimo + 10 | 115%. Sem entrada grátis: exige **Selo da Tempestade** (baú semanal de primeira vitória em Difícil, ou 1 comprável/semana com Marcas) |

**Temporada** ⚠️: a cada ~8 semanas, um chefe do catálogo volta em
"Variante de Temporada" (mecânica extra, modelo recolorido) com um baú
cosmético. Sem poder novo, para não obrigar ninguém.

### Poder de referência

`poder_referencia(nivel)` é uma função do `shared`: o poder de um personagem
**daquele nível** vestindo os 7 slots **no grau esperado da faixa, tier II
+3**. O requisito é uma porcentagem disso, e não um número digitado por
conteúdo. Muda a curva do item, o requisito acompanha. É a regra de "uma
verdade por pergunta" do README: o mesmo `poder()` responde à ficha, à fila e
ao portão.

### Ajuste de nível (sincronia)

- **Para baixo, sempre.** Quem está acima de `nível_máx_da_faixa + 5` tem
  atributos e poder das peças **escalados até esse teto** dentro da instância.
  As skills não mudam (as 3 já abrem no 10).
- **Para cima, nunca.** Faltou nível ou poder, não entra.
- **Recompensa segue a faixa do conteúdo**, não do jogador. Nível 80 fazendo a
  Gruta do Bosque ganha material cinza, e em pouca quantidade (retorno
  decrescente, seção 8).
- **Vida dos inimigos escala com o grupo** abaixo do cheio: a Gruta com 4 tem
  85%, com 3 tem 70% ⚠️; a raid tem −6% por vaga vazia abaixo de 10.

## 5. Global entre servidores

### O problema

Realm é banco próprio e **não existe consulta que atravesse realms**
(SERVIDORES_E_CANAIS). Uma raid com gente de `SA01` e `SA02` não pode ler nem
gravar o banco de nenhum dos dois de dentro da instância. E o personagem
**não pode trocar de banco** para visitar: isso é merge, e merge é operação.

### A solução: o personagem vai de "passe", a recompensa volta por "carta"

Três peças novas, **fora** dos realms, na mesma camada da conta global e do
livro-caixa de TP que a ECONOMIA já exige:

| peça | o que é | onde mora |
|---|---|---|
| **`mesa`** (binário novo) | grupos, filas, matchmaking, pronto-check, alocação de instância, penalidades | processo único por região; estado quente em memória, frio no banco central |
| **processo de instância** | o **mesmo binário `server`** com `MMO_INSTANCIA=gruta\|raid\|leviata` e **sem `DATABASE_URL` de realm** | subido pelo `supervisor` de instâncias, como canal |
| **banco central** (`tempest_central`) | contas (já planejado para TP), instâncias, membros, **cartas de recompensa**, histórico de fila | fora de todos os realms |

```mermaid
flowchart LR
  subgraph SA01["Realm SA01 (banco próprio)"]
    C1[canal campo] --- DB1[(tempest_sa01)]
    COL1[coletor de cartas] --- DB1
  end
  subgraph SA02["Realm SA02 (banco próprio)"]
    C2[canal campo] --- DB2[(tempest_sa02)]
    COL2[coletor de cartas] --- DB2
  end
  subgraph CENTRAL["Região SA — central"]
    MESA[mesa<br/>filas · grupos · match]
    SUP[supervisor de instâncias]
    I1[instância gruta #1<br/>8 lanes]
    I2[instância raid #1<br/>4 lanes]
    DBC[(tempest_central<br/>contas · instâncias · cartas)]
  end
  C1 -- "entrar na fila + passe assinado" --> MESA
  C2 -- "entrar na fila + passe assinado" --> MESA
  MESA -- "alocar lane" --> SUP
  SUP --> I1 & I2
  MESA --- DBC
  I1 -- "carta de recompensa (outbox)" --> DBC
  I2 -- "carta de recompensa (outbox)" --> DBC
  COL1 -- "puxa cartas de SA01, aplica, confirma" --> DBC
  COL2 -- "puxa cartas de SA02, aplica, confirma" --> DBC
```

### Fluxo de ponta a ponta

```mermaid
sequenceDiagram
  participant J as Cliente
  participant R as Canal do realm (SA01)
  participant M as mesa
  participant I as Instância
  participant C as tempest_central
  participant K as Coletor SA01
  J->>R: FilaEntrar{conteudo, dificuldade, papeis}
  R->>R: valida nível, poder, descoberta, entradas, penalidade
  R->>R: grava reserva de entrada (não consome)
  R->>M: ticket de fila + Passe assinado (snapshot do personagem)
  M-->>J: FilaEstado (via R)
  M->>M: forma o grupo (seção 6)
  M-->>J: ProntoCheck (20 s)
  J->>R: ProntoResponder{aceito}
  M->>I: aloca lane, entrega os passes
  M-->>R: EntrarInstancia{host, ticket}
  R->>R: grava posição de retorno + em_instancia = id
  J->>I: conecta com o ticket
  I->>I: confere assinatura, recalcula poder do snapshot
  I->>C: instância começou → entradas consumidas
  Note over I: combate autoritativo, igual a um canal
  I->>C: carta por jogador a cada chefe morto (id único, assinada)
  I-->>J: ResultadoInstancia + TrocarZona de volta
  K->>C: busca cartas pendentes de SA01
  K->>K: uma transação: INSERT recompensas_aplicadas ON CONFLICT DO NOTHING + itens + entrada + lockout
  K->>C: confirma aplicação
```

### O passe (snapshot somente-leitura)

O canal do realm monta e **assina** (ed25519, chave por realm, pública na
central):

```
Passe {
  passe_id, realm, conta_id, char_id, nome, nivel,
  conjunto_equipado, equipamento: [ItemInstance...],   // grau, tier, refino, encanto
  skills_liberadas, poder_calculado,
  consumiveis: [(item_id, qtd)],                        // poções que pode usar lá dentro
  reserva_entrada_id, dificuldades_liberadas,
  emitido_em, expira_em (30 min)
}
```

- A instância **recalcula** os atributos a partir do equipamento, com o mesmo
  código do `shared`. Poder calculado diferente do declarado = passe recusado.
- **O equipamento fica travado** enquanto `em_instancia` estiver setado no
  realm: não dá para vender, trocar nem refinar. É o que torna o snapshot
  verdade, e fecha o "entrar forte, trocar a peça por fora".
- **Consumível é saldo, não item.** A instância devolve na carta quantas poções
  foram usadas, e o coletor debita. Se já não houver, debita até zero. Poção
  não vale a complexidade de custódia que a TP exige.

### A carta de recompensa (escrita de volta, transacional e idempotente)

- **Quem decide a recompensa é a instância.** Ela rola a tabela e grava na
  central uma carta por jogador, **no momento** em que o chefe (ou o andar)
  cai. Não espera o fim: se a instância morrer depois, o que já foi ganho está
  salvo.
- A carta **descreve**, não cria: `(item_id, grau, tier, qtd)`, cobre, XP,
  Marcas, `lockout`, `entrada_consumida`. **Instância de item só nasce no
  banco do realm**, com id do realm.
- `carta_id` é UUID gerado pela instância. No realm:

```sql
BEGIN;
INSERT INTO recompensas_aplicadas (carta_id, char_id, aplicada_em)
VALUES ($1, $2, now()) ON CONFLICT DO NOTHING;
-- 0 linhas inseridas → já aplicada: COMMIT e só confirma na central
-- 1 linha → aplica itens, cobre, XP, marcas, entrada, lockout, progresso
COMMIT;
```

  Depois disso, `UPDATE cartas SET aplicada_em = now()` na central. Se cair
  entre os dois, o coletor tenta de novo, o `ON CONFLICT` absorve e a carta
  não duplica. É o mesmo padrão de custódia da troca TP↔gold: dois bancos
  **não têm transação comum**, então a idempotência faz esse papel.
- **O coletor roda dentro do processo do realm** (uma task no canal 1 ou no
  `supervisor`), puxando a cada 5 s ⚠️ e também quando o jogador volta da
  instância. Assim o baú aparece na bolsa antes da tela de carregamento acabar.
- **O realm não confia cegamente.** Ele confere a assinatura da instância
  (chave do supervisor de instâncias), `qtd ≤ máximo da tabela do conteúdo` e
  `cartas_da_instância ≤ chefes do conteúdo`. Carta fora disso vai para
  `cartas_rejeitadas` e dispara alerta no panóptico.

### Anti-trapaça e validação

O combate já é autoritativo (NETWORKING): o cliente só mente sobre qual botão
apertou. Sobra proteger as costuras:

| costura | proteção |
|---|---|
| passe forjado ou inflado | assinatura do realm + recálculo do poder na instância |
| usar o mesmo passe duas vezes | `passe_id` único na central; segundo uso é recusado |
| duplicar item trocando equipamento durante a raid | `em_instancia` trava o equipamento no realm |
| carta forjada | assinatura da instância + teto por tabela no realm |
| farmar com alt ajudante | Ajudante não ganha peça nem chave; recompensa **vinculada** (ECONOMIA: vinculado é o padrão) |
| AFK sugando baú | limiar de contribuição (seção 7); abaixo dele não há baú |
| bot em fila | penalidade de abandono + voto de expulsão; o panóptico ganha a aba "instâncias" |

### Latência e região

- Pool de matchmaking **por região** (`SA`, `NA`…), como o MIR4. Hoje só
  existe uma região e uma VPS, então a "central" roda na mesma máquina. O
  desenho não muda quando ela sair.
- Sem predição no cliente, o **telegráfico precisa de janela maior que o RTT**:
  mínimo de 1,2 s ⚠️ entre o aviso e o dano. Com mais de 180 ms de RTT medido
  no handshake, a fila sugere outra região, sem bloquear.

### Desconexão, reconexão e falhas

| evento | o que acontece |
|---|---|
| **cliente cai** | O personagem fica parado e **imune a dano por 10 s**, depois vulnerável. A vaga fica reservada por **3 min**. Logar de novo no realm vê `em_instancia` e manda `EntrarInstancia` com o mesmo ticket (válido até a instância fechar) |
| **não voltou em 3 min** | Sai do grupo sem contar como abandono na 1ª vez do dia. A vaga abre para **backfill** |
| **grupo inteiro caiu por 3 min** | Instância fecha. Cartas já gravadas valem. Entrada consumida **não volta**: o problema foi do lado deles |
| **instância (processo) cai** | O supervisor vê o heartbeat sumir e a `mesa` marca `falhou`. Entrada **devolvida** (carta de estorno) para quem não tinha fechado o conteúdo. Cartas de chefes já mortos valem. O grupo vai **ao topo da fila**, com o mesmo grupo e pronto-check automático |
| **`mesa` cai** | Instâncias em curso continuam: não dependem dela depois de alocadas. Filas se perdem, e o cliente mostra "procurando de novo" e re-enfileira sozinho. Estado frio (penalidades, histórico) está na central |
| **central cai** | Instância segura as cartas em fila local (outbox em disco) e reenvia. Filas novas param. O jogo aberto nos realms não é afetado |
| **realm cai com o jogador na instância** | Nada muda na instância. As cartas esperam na central até o coletor voltar |

## 6. Matchmaking e grupos

### Grupo

O grupo sai do processo e vai para a `mesa`. Hoje o `party_id` vive na memória
de um canal (`world.rs`), e **some na troca de zona**. Como jogadores do mesmo
realm estão em canais diferentes, e de realms diferentes em máquinas
diferentes, o grupo precisa morar acima dos dois.

- **Grupo manual**: convite por nome (`Nome@SA01` entre realms, só `Nome` no
  mesmo), líder, promover, sair, expulsar (líder expulsa **fora** da
  instância). As mensagens `PartyInvite/Accept/Decline/Leave` que já existem
  viram fachada da `mesa`.
- **Grupo de clã**: reservado. Quando existir clã, "grupo do clã" é só um
  filtro na busca.
- **Lista de recrutamento** (o que o MIR4 tem): o líder publica o grupo com
  conteúdo, dificuldade, poder mínimo e vagas por papel. Quem busca vê e
  "pede para entrar". Opção **começar quando lotar**.
- **Auto-match**: a fila cega descrita abaixo. Grupo parcial também entra e é
  completado.

### Papéis, amarrados ao conjunto de arma

| papel | conjunto | por quê |
|---|---|---|
| **Tanque** | espada e escudo | Muralha (−50% dano), Investida; **multiplicador de ameaça 3×** do conjunto ⚠️ |
| **Suporte** | anel mágico | Aura é a única cura em grupo |
| **Dano** | katana, duas pistolas | Saque/Dança/Vento; Tiro/Rajada/Barril |

- O jogador declara **até 2 papéis** na fila. Só pode declarar um papel se
  **possui** o conjunto dele em grau ≥ ao da faixa. O servidor confere na
  bolsa do passe.
- No pronto-check a `mesa` diz o papel sorteado, e é preciso **estar com o
  conjunto equipado ao entrar**, senão a entrada é recusada com o motivo.
  Trocar de arma dentro da instância é livre (a regra do combate continua).
- **Chamado às armas**: quando um papel falta na região, quem entra na fila
  com ele ganha +50% de Marcas ⚠️. O MIR4 não tem isso; é a correção padrão de
  fila com escassez de tanque e suporte.

### Composição e critérios

| conteúdo | ideal | mínimo aceito após relaxar |
|---|---|---|
| Gruta (5) | 1T · 1S · 3D | Normal: 3 jogadores, qualquer papel |
| Raid (10) | 2T · 3S · 5D | Normal: 6 com ≥1T e ≥1S. Difícil/Pesadelo: 8 com 2T e 2S |

Critérios, em ordem de peso:

1. **Chave da fila**: região + conteúdo + dificuldade (duro, nunca relaxa).
2. **Papel** (composição acima).
3. **Poder** relativo ao de referência: começa em ±25% da mediana do grupo.
4. **Nível** dentro da faixa do conteúdo (duro).
5. **Histórico**: não junta quem votou para expulsar quem nas últimas 24 h, e
   evita repetir a mesma pessoa que abandonou você.
6. **Idioma/realm**: preferência leve por mesmo realm (chat e amizade), nunca
   bloqueio.

### Relaxamento por tempo

| em fila há | relaxa |
|---|---|
| 0–60 s | nada |
| 60 s | poder para ±50% |
| 120 s | aceita 2S no lugar de 1S+1D (Gruta) e 4 suportes/6 dano (raid) |
| 180 s | aceita 0 tanque em Normal **se** o poder médio ≥ 120% da referência |
| 300 s | oferece **entrar com menos** (mínimos da tabela), com vida escalada |
| 600 s | teto da fila: avisa e sugere Normal ou outro conteúdo; continua na fila se quiser |

Pré-grupo nunca é quebrado. Ele entra como bloco e só recebe o que falta.

### Pronto-check, backfill, expulsão e abandono

- **Pronto-check**: 20 s. Todos aceitam, aloca. Alguém recusa ou deixa expirar:
  **quem aceitou volta ao topo da fila**, e quem recusou sai. 2 recusas numa
  hora = 5 min sem fila.
- **Backfill**: vaga aberta antes do **último chefe** chama a `mesa` por até
  2 min. Quem entra de backfill **não gasta entrada** se o progresso já passou
  de 50%, e recebe carta só do que ainda cair.
- **Voto de expulsão**: maioria simples dos outros (3 de 4 na Gruta, 6 de 9 na
  raid). **Não durante a luta com chefe.** 1 voto iniciado por pessoa por
  instância, 60 s entre votos, motivo obrigatório (AFK, ofensa, sabotagem). O
  expulso recebe as cartas até ali, sem penalidade de abandono. Pré-grupo não
  expulsa aleatório sozinho: o voto dele conta 1 por pessoa.
- **Abandono** (sair da instância antes do fim, ou não voltar da queda depois
  da 1ª do dia): sem fila por 10 min, depois 30 min, depois 2 h, numa janela de
  24 h, **por conta**, na central. Um realm não serve de refúgio para outro.

## 7. Mecânicas de chefe

O combate é **por alvo** (COMBATE_POR_ALVO, SKILLS): o jogador não mira no
chão. Então a mecânica de chefe é **movimento e posição**, não mira, e a
leitura é o telegráfico que o servidor manda.

### Vocabulário (um conjunto só para todos os chefes)

| forma | leitura | resposta esperada |
|---|---|---|
| **círculo** | pintado no chão, enche até disparar | sair |
| **cone / linha** | a partir do chefe, na direção de um jogador | sair do eixo |
| **anel** | seguro no centro, dano fora | entrar |
| **marca** em jogador | círculo que segue alguém por 4 s, dano em volta dele | o marcado se afasta do grupo |
| **dividir** | círculo grande em alguém: dano ÷ quem está dentro | ≥ 3 entram juntos, senão morre |
| **escudo** | chefe ganha escudo de X de vida por 8 s | burst: quebrou, chefe atordoado; não quebrou, dano no grupo inteiro |
| **adds** | onda de mobs com alvo no suporte | tanque puxa, dano limpa |
| **ponto de âncora** | 2–3 totens que curam o chefe | dividir o dano entre eles |

- **Telegráfico = mensagem**, não entidade: `BossTelegrafo { forma, centro,
  dir, raio, angulo, dispara_em_ms }`. Não gasta a cota do AOI e o cliente
  desenha no mundo com o mesmo pipeline dos efeitos de skill.
- **Janela mínima de 1,2 s** entre o aviso e o dano, e 1,6 s em Normal ⚠️.
- **Dano resolvido no servidor** no instante do disparo, com a posição
  autoritativa. É o que o jogo já faz com as skills.

### Fases e enrage

- **Fases por vida**: 100–70%, 70–40%, 40–0%. Cada fase adiciona uma forma do
  vocabulário. Transição = 3 s de chefe imune e adds.
- **Enrage duro**: aos 10 min (Gruta: chefe aos 5) o chefe ganha +50% de dano a
  cada 10 s. Não é "timer de wipe" instantâneo: dá para ver chegando.
- **Limite da instância**: 25/30 min. Estourou, a instância fecha e as cartas
  já gravadas valem.

### O que exige grupo de verdade

- **Dividir** exige 3 pessoas juntas.
- **Tanque trocando**: o chefe de raid aplica "Ferida" empilhável no alvo. Em 4
  pilhas o tanque precisa passar a ameaça (Investida no chefe dá pico de
  ameaça ⚠️). Por isso a raid tem 2 tanques.
- **Escudo** exige dano somado numa janela.
- **Âncoras** exigem dividir o grupo.

### Contribuição

Placar por jogador, **normalizado pelo papel**:

```
dano      = dano causado ao chefe e aos adds
cura      = cura efetiva (sem excesso)
tanque    = dano do chefe recebido enquanto era o alvo + dano mitigado pela Muralha
mecanica  = +pontos por escudo quebrado, âncora destruída, "dividir" absorvido
pontos    = (valor do papel ÷ mediana do mesmo papel no grupo) × 100 + mecanica
```

- **Limiar do baú**: ≥ 25 pontos **e** ≥ 60 s vivo e em combate na luta ⚠️.
  Abaixo disso, só cobre e XP.
- **Faixas**: ≥ 25 → Baú de Bronze · ≥ 80 → Prata · top 1 por papel → **Ouro
  (MVP do papel)**, com rolagem extra na tabela de peça.
- Com a normalização, o melhor tanque e o melhor suporte **também** são MVP.
  "MVP" puro de dano, como no MIR4, empurraria todo mundo para katana.

## 8. Recompensas

### A regra do usuário, em tabela

**Sem Épico até o nível 50. Épico raro a partir do 60, mais comum no 70+ e em
Pesadelo. Lendário só no endgame (80+), e mesmo lá raríssimo.**

### De onde sai equipamento (mudança de regra)

- **Mob e chefe de campo continuam sem dropar equipamento** (LOOT_DOS_MOBS).
- **O baú de conclusão** (Gruta, Raid e o baú de primeira vitória semanal) é a
  **única fonte de peça fora do craft**. A peça sai **vinculada**, sempre **tier
  I** (tier II em Pesadelo), com refino +0.
- Isso não quebra o craft: o que sai no baú é o **piso** da faixa, e o craft
  (com chave) continua sendo o caminho para escolher peça e cor. A peça do baú
  alimenta a combinação 2×tier (ITENS) e a troca "tier IV +8 substitui chave"
  (ECONOMIA_DE_CRAFT).

### Chance de peça e grau, por faixa e dificuldade

"Peça" = chance de o baú trazer 1 peça. Depois disso, o grau é rolado na
distribuição da linha. Valores ⚠️ iniciais.

| faixa | dificuldade | peça (Gruta) | peça (Raid, Prata) | Comum | Fino | Raro | Épico | Lendário |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 1–10 | Porão | 20% | — | 100 | — | — | — | — |
| 10–20 | Normal | 25% | — | 75 | 25 | — | — | — |
| 10–20 | Difícil | 30% | — | 50 | 48 | 2 | — | — |
| 20–30 | Normal | 25% | 40% | 55 | 42 | 3 | — | — |
| 20–30 | Difícil | 30% | 50% | 15 | 75 | 10 | — | — |
| 30–40 | Normal | 25% | 40% | — | 80 | 20 | — | — |
| 30–40 | Difícil | 30% | 50% | — | 55 | 45 | — | — |
| 40–50 | Normal | 25% | 40% | — | 60 | 40 | — | — |
| 40–50 | Difícil | 30% | 50% | — | 20 | 80 | — | — |
| 50–60 | Normal | 25% | 40% | — | 10 | 90 | **0** | — |
| 50–60 | Difícil | 30% | 50% | — | — | 100 | **0** | — |
| 60–70 | Normal | 25% | 40% | — | — | 98 | **2** | — |
| 60–70 | Difícil | 30% | 50% | — | — | 92 | **8** | — |
| 60–70 | Pesadelo | 35% | 60% | — | — | 80 | **20** | — |
| 70–80 | Normal | 25% | 40% | — | — | 85 | **15** | — |
| 70–80 | Difícil | 30% | 50% | — | — | 65 | **35** | — |
| 70–80 | Pesadelo | 35% | 60% | — | — | 40 | **60** | — |
| 80–100 | Difícil | 30% | 50% | — | — | 50 | 50 | — |
| 80–100 | Pesadelo | 35% | 60% | — | — | — | 99,5 | **0,5** |

Modificadores:

- **Baú de Bronze** tem metade da chance de peça; **Ouro (MVP)** rola duas
  vezes e fica com o melhor.
- **Primeira vitória semanal** (por chefe e dificuldade): **peça garantida**,
  rolada uma linha acima na coluna de grau. 50–60 Difícil continua sem Épico:
  o teto da faixa vale sempre.
- **Primeira vitória de todas** (uma vez na vida, por conteúdo e dificuldade):
  peça garantida + 1 chave da cor da faixa.

Conta de sanidade (60–70, Difícil, jogador de 2 Grutas/dia + 1 raid/dia + 2
primeiras semanais): ~14 × 30% × 8% + ~7 × 50% × 8% + 2 × ~20% ≈ **1 Épico por
semana**. Para um jogador de 60–70 vestir 7 slots de Épico só do baú, são
~2 meses. O craft continua sendo o atalho de quem escolhe.

### Proteção contra azar: Marcas da Tempestade

- Toda conclusão dá **Marcas** (vinculadas): Gruta 10, Raid 25, Ajudante 60%
  disso, Chamado às armas +50%.
- **Mestre das Marés** (NPC novo, seção 9) troca Marcas por:
  - peça **Rara** à escolha do slot: 150 Marcas (só 30+);
  - peça **Épica** à escolha do slot: 900 Marcas, **só 60+**, 1 por semana;
  - Selo da Tempestade (entrada de Pesadelo): 120, 1 por semana;
  - chave da cor da faixa: 80.
- O teto semanal de Épico por Marcas mantém a regra "Épico é raro" mesmo para
  quem joga 10 h por dia.

### Materiais de craft por faixa (coerente com ECONOMIA_DE_CRAFT)

Cor = grau (cinza Comum, verde Fino, azul Raro, roxo Épico). **Material roxo
continua sem drop em lugar nenhum**: só por síntese (10 azuis + pó). É o que
fecha a economia de craft e não será aberto aqui.

| faixa | cor dos materiais (Aço, Platina e os seis de 100) | Darksteel | Pó Cintilante | chave (Escama/Garra/Couro/Chifre) |
|---|---|---|---|---|
| 1–10 | cinza | 50–100 | — | — |
| 10–20 | cinza 90 · verde 10 | 150–300 | 10% × 1 | Gruta 5% · cinza |
| 20–30 | cinza 60 · verde 40 | 400–800 | 25% × 1 | Gruta 10% · Raid **1 garantida** · cor sorteada |
| 30–40 | verde 80 · azul 20 | 1–2k | 40% × 1–2 | Gruta 12% · Raid 1 garantida |
| 40–50 | verde 55 · azul 45 | 2,5–5k | 60% × 2 | Gruta 15% · Raid 1 |
| 50–60 | verde 20 · azul 80 | 6–12k | 2–3 | Gruta 15% · Raid 1–2 |
| 60–70 | azul 100 | 15–25k | 3–5 | Gruta 20% · Raid 2 |
| 70–80 | azul 100 (quantidade ×1,5) | 30–50k | 5–8 | Raid 2–3 |
| 80–100 | azul 100 (×2) | 60–100k | 8–12 | Raid 3 |

Pesadelo = ×2 em Pó e Darksteel. Quantidades ⚠️.

**Calibragem**: a coleta ativa já tem rendimento conhecido (COLETA, ~1 coleta a
cada 1,5–3 s num veio). A regra para afinar é: **por hora, o conteúdo de grupo
rende no máximo ~1,5× a coleta ativa da mesma faixa em material comum** e é
**exclusivo** em peça, Marcas e chave garantida. Senão a coleta morre e o jogo
vira fila de dungeon. O panóptico já mede coleta por jogador; a aba de
instâncias passa a medir rendimento por hora de conteúdo, e a comparação é
direta.

### Moedas

- **Cobre**: sempre, na escala da faixa (é o gold dos mobs, LOOT_DOS_MOBS).
- **Ouro**: pouco. É prêmio de missão diária, não do baú.
- **TP: nunca.** TP só entra por dinheiro (ECONOMIA, DECIDIDO).

### Proteção contra farm

| mecanismo | valor ⚠️ |
|---|---|
| entradas | Gruta 2/dia (acumula 4), Raid 1/dia (acumula 3), Porão com recompensa 3/dia |
| compra de entrada | com gold, 2/dia na Gruta e 1/dia na Raid, preço ×1 e ×3 |
| primeira vitória | semanal por chefe e dificuldade |
| Pesadelo | só com Selo (máx. ~2/semana) |
| retorno decrescente | jogador ≥ 10 níveis acima do máximo da faixa: peça −100%, materiais −50%. ≥ 20 acima: só cobre e Marcas de Ajudante |
| Épico por Marcas | 1/semana |
| tudo | **vinculado** (ECONOMIA: vinculado é o padrão; é o que mata multi-conta sem detectar nada) |

### Ralos que o conteúdo cria

- **Gold** das entradas compradas.
- **Cobre e darksteel** do **reparo**: morrer na instância custa durabilidade
  do conjunto ⚠️. É opcional, decidir junto com a economia, porque não existe
  durabilidade hoje.
- **Refino**: peça tier I do baú é combustível para a combinação 2×tier, que
  zera o refino (ITENS). Quem refina a peça errada perde.
- **Marcas** não acumulam além de 3.000. O excesso vira cobre na troca 1:50 ⚠️.

## 9. Integração com o que já existe

| sistema | o que muda |
|---|---|
| **Missões** (MISSOES) | Objetivo novo `CONTEUDO_CONCLUIR { conteudo_id, dificuldade_min }` (0 = qualquer). **Contratos do dia** no Mestre de Missões: "conclua 1 Gruta", "vença 1 Caçada", "ajude 1 grupo como Ajudante" → Marcas + ouro. Missões de **descoberta** por conteúdo na cadeia de cada ilha |
| **Auto missão / auto path** | `QuestDestino` de CONTEUDO_CONCLUIR responde a **entrada física** se ainda não foi descoberta, ou o próprio botão da fila (tipo `FILA`) se já foi. Viagem do mapa leva até a entrada e abre a janela de fila |
| **Mapa** | Marcador por entrada: ícone de portal de maré (Gruta), caveira (Caçada), cinza = não descoberto, com cadeado e motivo = sem nível/poder. Chefe de campo vivo aparece durante a Maré Sangrenta |
| **Vila / NPC** | Papel novo `Mares` (**Mestre das Marés**) no anel de ofícios: loja de Marcas, lista de conteúdos, entrar na fila. É NPC de porta, como os outros (VILA_E_PORTO) |
| **Entrada física** | Estrutura de `shared::construcao` (arco de pedra + tocha) posta pelo gerador da ilha, **da semente**, como a cidade e o porto. Nada viaja no fio |
| **Chefe de campo** | `boss_areas` + `KIND_CHEFE` ganham nome, nível e tabela de baú por área. A Maré Sangrenta é um estado da área |
| **Arena da instância** | A lane atual é **tile map legado** (`map.set`, `DUNGEON_ORIGIN = (11000, 200)`), e o mundo é voxel (`shared::terreno`). A arena nova sai de um **gerador de ilhotas-arena** no `shared` (semente por conteúdo), com as mesmas regras de colisão. O terreno continua sem trafegar |
| **Economia / loot** | Tabelas `conteudo_loot` no banco, editáveis pelo admin como `loot_mobs`, com teto duro por faixa **no código** (Épico < 60 é recusado no seed e no admin) |
| **Panóptico** | Aba **Instâncias**: filas por papel, tempo de fila p50/p90, instâncias vivas, wipe rate por chefe, cartas pendentes e rejeitadas, rendimento/hora |

### Persistência

**No banco do realm:**

```sql
conteudo_descoberto  (char_id, conteudo_id, descoberto_em)
conteudo_progresso   (char_id, conteudo_id, dificuldade, vitorias, primeira_vitoria_em, melhor_tempo_s)
conteudo_entradas    (char_id, tipo, dia, saldo, compradas_hoje)          -- tipo: gruta|raid|porao
conteudo_reserva     (reserva_id PK, char_id, tipo, criada_em, estado)     -- reservada|consumida|devolvida
conteudo_lockout     (char_id, conteudo_id, dificuldade, semana)           -- primeira vitória semanal
recompensas_aplicadas(carta_id PK, char_id, aplicada_em)
characters           + em_instancia UUID NULL, + retorno_pos
cartas_rejeitadas    (carta_id, motivo, payload, em)
```

Marcas da Tempestade = **item** vinculado (id novo), como o Cobre. Não precisa
de tabela própria.

**No banco central (`tempest_central`):**

```sql
contas               (conta_id, ...)                         -- já exigida pela TP
instancias           (instancia_id, conteudo_id, dificuldade, host, lane, estado, criada_em, encerrada_em)
instancia_membros    (instancia_id, realm, char_id, conta_id, passe_id UNIQUE, papel, estado, pontos)
cartas               (carta_id PK, realm, char_id, instancia_id, payload JSONB, assinatura, criada_em, aplicada_em)
penalidades_fila     (conta_id, ate, nivel, janela_inicio)
historico_fila       (conta_id, outro_conta_id, motivo, em)  -- votos/abandonos, pro critério 5
```

A migração central usa o mesmo advisory lock (`pg_advisory_lock`) dos canais.
A pegadinha das migrations concorrentes vale para as instâncias também.

### Protocolo (sobe `PROTOCOL_VERSION`)

Cliente → servidor (canal do realm, que repassa à `mesa`):

```rust
FilaEntrar     { conteudo_id: u16, dificuldade: u8, papeis: u8 /*bitmask T|S|D*/ }
FilaSair
ProntoResponder{ match_id: u32, aceito: bool }
GrupoPublicar  { conteudo_id: u16, dificuldade: u8, poder_min: u32, auto_iniciar: bool }
GrupoBuscar    { conteudo_id: u16, dificuldade: u8 }
GrupoPedirEntrada { grupo_id: u32 }
GrupoPromover  { nome: String }
VotoExpulsao   { alvo: String, motivo: u8 }  /  VotoResponder { voto_id: u32, sim: bool }
InstanciaSair
MaresComprar   { oferta_id: u16, slot: u8 }
```

Servidor → cliente:

```rust
ConteudosDisponiveis { lista: Vec<ConteudoEstado> }  // descoberto, dificuldades, entradas, lockout, motivo do cadeado
FilaEstado     { conteudo_id, dificuldade, na_fila_s: u32, estimado_s: u32, papeis_faltando: u8 }
ProntoCheck    { match_id, expira_s: u8, membros: Vec<(String, u8 /*papel*/)> , seu_papel: u8 }
EntrarInstancia{ host: String, ticket: [u8; 32], conteudo_id, lane: u8 }
GrupoLista     { grupos: Vec<GrupoResumo> }
InstanciaEstado{ andar: u8, total: u8, fase: u8, enrage_em_s: u16, limite_em_s: u16 }
ChefeVida      { eid, frac: u16 /*0..=65535 = 0..100%*/ }    // ver risco "vida em u16"
BossTelegrafo  { forma: u8, centro: [f32;2], dir: f32, raio: f32, angulo: f32, dispara_em_ms: u16 }
Contribuicao   { linhas: Vec<(String, u8 /*papel*/, u16 /*pontos*/)> }
VotoExpulsaoAberto { voto_id, alvo: String, motivo: u8, expira_s: u8 }
ResultadoInstancia { baus: Vec<BauResultado>, marcas: u32, primeira_vitoria: bool }
PenalidadeFila { ate_unix: u64 }
```

`TrocarZona` já cobre a volta para o realm, e a posição de retorno é gravada
antes do handoff (como na troca de zona).

### Telas no cliente

1. **Aventuras** (janela, tecla livre a definir): abas Porão / Gruta / Caçada /
   Chefes. Lista por faixa com cadeado e o **motivo** ("nível 25", "poder
   12.400/15.000", "descubra a entrada"), entradas do dia, lockout semanal,
   prévia de recompensa (grau máximo da faixa, em cor). Botões **Entrar na
   fila** (com papéis), **Buscar grupos**, **Criar grupo**.
2. **Faixa de fila** no HUD, embaixo do rastreador: "PROCURANDO GRUPO · Tumba
   das Areias · 01:23 · T✓ S… D 2/3". O número importa: fila sem número parece
   travada (a mesma lição da fila de entrada).
3. **Pronto-check**: modal central com contador, membros e papel. Aceitar /
   Recusar.
4. **Carregamento** (a mesma da troca de zona).
5. **HUD de instância**: andar x/3, barra grande do chefe com fase, relógio do
   enrage, lista do grupo com papel.
6. **Telegráficos** no mundo, no pipeline de efeitos das skills.
7. **Resultado**: placar de contribuição, baús abrindo em ordem
   (Bronze/Prata/Ouro), "+Marcas", "Primeira vitória!".
8. **Mestre das Marés**: loja de Marcas.
9. **Diário (J)**: aba "Semana" com primeiras vitórias e entradas.

## 10. O que já existe (estado do código, só leitura)

- **`DUNGEON_MODE`** (`server/world.rs`): processo dedicado com **8 lanes
  solo** isoladas pelo AOI (`DUNGEON_LANE_COUNT = 8`, espaçamento 80), 4 salas
  (3 de ondas até 20 mortes + chefe), portões como `WALL` no tile map, limite
  de 10 min, 10 s de graça para o loot e `DungeonComplete` de volta. Flag
  `SelectDungeonMode { raid }` → run só de chefe. **Reaproveitável**: máquina
  de estados de sala/portão/limite, lane por AOI e alocação de slot. **Não
  reaproveitável**: arena em tile map legado, um ocupante por lane, sem
  entrada, recompensa ou persistência.
- **Party** (`world.rs`): `PartyInvite/Accept/Decline/Leave`, `/party` no chat,
  divisão de XP com +20% entre membros próximos. **Em memória do processo**:
  some na troca de zona.
- **`boss_areas`**: um chefe por área, respawn por timer, `KIND_CHEFE = 7`
  (uma linha de loot só: darksteel, pó, escama, poções). Sem nome, contribuição
  ou baú.
- **`spawn_boss`** admin (`world/boss_teste.rs`): Guardião de Treino nível 20,
  **vida limitada a 60.000 "pelo protocolo"**. A vida viaja em `u16` no
  `EntityState` (13 bytes).
- **`poder()`** existe **só no cliente** (`client/bolsa.rs`).
- **Fila de entrada** (`FilaDeEntrada`, `tick_fila`) e **troca de zona**
  (`TrocarZona`, posição gravada antes): a base do handoff para a instância.
- **Supervisor** de canais: modelo para o supervisor de instâncias.
- **Conta global**: planejada (ECONOMIA/TP), **não existe**. `accounts` ainda
  mora em cada realm (`web/db.rs`). É **pré-requisito** do cross-realm.

## 11. Plano de implementação

Cada fase termina com algo **jogável e testável com bots** (a regra do projeto:
mecânica de jogador só se entrega com bots). Estimativa relativa: P ≈ 1–2
dias, M ≈ 1 semana, G ≈ 2–3 semanas.

| fase | entrega | teste de aceite | tamanho |
|---|---|---|---|
| **F0 · pré-requisitos** | `poder()` e `poder_referencia()` no `shared`. `ChefeVida` em fração (vida de chefe > 65.535). Catálogo `shared::conteudo` (ids, faixas, tamanhos, dificuldades) | teste unitário: cliente e servidor chegam ao mesmo poder; chefe de 2 milhões de vida mostra barra certa | P |
| **F1 · Gruta local com auto-match simples** | `mesa` mínima (1 realm, memória, sem banco): fila por conteúdo, composição 1T/1S/3D com relaxamento, pronto-check. `DUNGEON_MODE` vira lane **de grupo** (até 5), com andares e chefe. Grupo sai do processo e vai para a `mesa` | 5 bots em canais diferentes do mesmo realm entram na fila, formam grupo, entram na mesma lane, matam o chefe e voltam ao canal certo | M |
| **F2 · recompensa, entradas e persistência** | Tabelas do realm (entradas, reserva, progresso, lockout). Baú com a tabela da seção 8, teto duro de grau por faixa, Marcas, Mestre das Marés, telas Aventuras/Fila/Resultado. Missões de contrato e de descoberta | bot faz 2 Grutas, a 3ª é recusada; 80 mil baús simulados (padrão do `audit_mob_loot`) nunca dão Épico < 60; reiniciar não duplica baú | M |
| **F3 · Caçada (raid)** | Lane de 10, vocabulário de telegráficos, fases, enrage, ameaça do tanque, contribuição por papel, voto de expulsão, backfill, penalidade de abandono. Primeiro chefe: **Mãe-da-Nevasca** (25) | 10 bots com papéis; grupo sem suporte dá wipe em Difícil e passa em Normal; o placar dá MVP para tanque e suporte; p99 do tick < 30% com 4 raids no processo | G |
| **F4 · arena voxel** | Gerador de ilhotas-arena no `shared`, entrada física na ilha, marcador no mapa, auto path até a entrada. Aposenta a lane em tile map | a mesma run da F1 numa arena gerada da semente; o cliente não recebe terreno | M |
| **F5 · entre realms** | Conta global + `tempest_central`. Passe assinado, supervisor de instâncias, cartas + coletor idempotente, trava `em_instancia`, reconexão, estorno por queda | 2 realms (SA01/SA02) em bancos separados formam uma raid. Matar a instância no meio: entradas devolvidas, cartas de chefe morto aplicadas **uma vez**. Matar o coletor entre a transação e a confirmação: sem duplicação | G |
| **F6 · Chefe de campo e Chefe de mundo** | Maré Sangrenta nas 4 ilhas (nomeado → semi-chefe → chefe). Leviatã semanal em fragmentos ≤ 150 misturando realms | 300 bots de 2 realms: 2 fragmentos, tick ok, baú por limiar de contribuição | M |
| **F7 · topo e temporada** | Pesadelo (Selo), conteúdo 60+ (depende das ilhas novas), Variante de Temporada; talvez o "Mar Revolto" (a Praça Mágica adaptada), só se a medição de coleta pedir | — | M cada |

### Riscos

| risco | por que importa | mitigação |
|---|---|---|
| **Suporte fraco** | Só a Aura cura o grupo; 3 skills por arma podem não segurar uma raid de 10 | Afinar a Aura antes de F3; se não bastar, 4ª skill do anel com escudo em aliado é decisão de COMBATE, fora deste doc |
| **Tanque sem ameaça** | Não há provocar; o chefe escolhe alvo por proximidade/dano | Ameaça por conjunto (espada e escudo ×3) + pico na Investida. Validar com bots na F3 |
| **Sem predição, desvio de telegráfico** | RTT alto vira morte injusta | Janela ≥ 1,2 s, dano pelo servidor no disparo, sugestão de região |
| **Fila vazia** | Um realm começando não tem 10 pessoas às 3h | Ajudante, entrar com menos, Chamado às armas e, sobretudo, F5 (o pool entre realms é o remédio) |
| **Duplicação na volta da recompensa** | Dois bancos sem transação comum | Carta com id único + `ON CONFLICT` na mesma transação do item; teste de matar o coletor no meio |
| **Vida de chefe em `u16`** | Chefe de raid precisa de milhões | `ChefeVida` em fração, só para chefe (F0) |
| **Conta global inexistente** | Sem ela não há `conta_id` comum, nem penalidade entre realms | É pré-requisito da F5 e já é exigida pela TP; fazer uma vez, para as duas |
| **Ilhas de 60+ não existem** | Épico só faz sentido com conteúdo de 60+ | F1–F3 vão até o 60. O topo espera as ilhas. A tabela de recompensa já nasce com o teto certo |
| **Conteúdo out-earning a coleta** | Mata a disputa por spot, que é o coração da COLETA | Regra dos 1,5× por hora, medida no panóptico antes de abrir cada faixa |
| **Custo de arte** | Cada chefe é um modelo detalhado (orçamento de arte: modelo detalhado é chefe) | Um modelo por ilha na F3, recolorido por dificuldade e temporada |
| **Regra "mob não dá equipamento"** | O baú de peça muda o LOOT_DOS_MOBS | **DECIDIDO pelo usuário:** mob continua sem dar equipamento; o baú de conclusão de dungeon/raid é a única fonte de peça fora do craft |

## Fontes

- MIR4 Official Community — Raid e Boss Raid: https://forum.mir4global.com/post/42
- MIR4 Official Community — Magic Square / Secret Peak e versão Fissurada: https://forum.mir4global.com/post/43
- MIR4 Official Community — World Boss (Nerkan/Turkan/Drakazan, 450 por arena): https://forum.mir4global.com/post/703
- MIR4 Official Community — Valley of Life and Death (MVP, normalização no nível 100): https://forum.mir4global.com/post/1591
- MIR4 Official Community — Bicheon Castle Siege: https://forum.mir4global.com/post/407
- MIR4 Official Community — Expedition / Domination Server: https://forum.mir4global.com/post/748 · https://forum.mir4global.com/post/1394
- MIR4 Wiki — Hidden Valley Capture: https://www.mir4.wiki/wiki/Hidden_Valley_Capture (fora do ar na consulta; resumo via busca)
- MIR4 Wiki — Raid: https://www.mir4.wiki/wiki/Raid (HTTP 500 na consulta; níveis 20/30/90 via resumo de busca, **incerto**)
- Touch, Tap, Play — How to Join Raids in MIR4: https://www.touchtapplay.com/how-to-join-raids-in-mir4-raids-guide/
- GameWith — Secret Peak (poder e nível por andar): https://gamewith.net/mir4/article/show/31581
- GameWith — Magic Square (respawn de chefe, câmaras): https://gamewith.net/mir4/article/show/31619
- Inven Global — Fissured Magic Square e Secret Peak 11F: https://www.invenglobal.com/articles/18824/wemade-presents-mir4-fissured-magic-square-and-secret-peak-11f-update
- ANTARA News — andares mais altos da Praça/Pico: https://en.antaranews.com/news/273462/wemade-updates-the-highest-floors-of-magic-square-and-secret-peak-in-mir4
- ANTARA News — Sabuk Clash: https://en.antaranews.com/news/276291/mir4-updates-sabuk-clash-to-determine-the-most-powerful-clan
- PR Newswire — nova raid e boss raid (recompensas de primeira vitória): https://www.prnewswire.com/news-releases/challenge-new-enemies-in-mir4-new-raid-and-boss-raid-revealed-301688358.html
- Business Wire — Expedition e Attack of the Living Wraiths (out/2021): https://www.businesswire.com/news/home/20211004006051/en
- gameplay.tips — Attack of the Living Wraiths: https://gameplay.tips/guides/mir4-attack-of-the-living-wraiths-bosses-guide.html
- MMOM — Epic Material Crafting Guide (fontes de material Épico): https://www.mmom.com/mir4-news/detail_mir4-ultimate-epic-material-crafting-guide.html

Pontos **incertos** na pesquisa: horário de reset (as fontes dizem 04:00
regional, 00:00 UTC+8 e 16:00 UTC, conforme região e época); se o MIR4 tem
auto-match cego além da lista de recrutamento; a fórmula de contribuição e as
taxas de drop das raids (não são públicas); o nível exato de abertura do Boss
Raid (30 aparece numa fonte só); a tabela completa de andares da Praça Mágica
(só 1F e 10F confirmados).

# História — a linha de missões principal

A história é a missão que nunca acaba. Todo personagem recebe o primeiro passo
ao entrar no mundo (novo ou de antes da história) e o servidor passa sozinho
pro seguinte assim que um termina. Não se aceita, não se abandona. Ela só
trava por **nível** ("Alcance o nível 20") e, na troca de ilha, pela rota
estar no ar.

Código: `crates/shared/src/historia.rs` (passos, falas, epílogo, pontos-chave),
`crates/server/src/quests.rs` (`garantir_historia`, `avancar_historia`,
`checar_trava`, `cumprir_passo`) e `GameWorld::tick_historia` no servidor.

## Sinopse

Há gerações quatro **faróis de pedra-trovão**, um em cada ilha, seguram a
Grande Tempestade longe do arquipélago. Na noite em que o farol do Bosque
apaga, você chega à praia como náufrago. Os bichos enlouquecem — o cobre que
todos carregam é trovão endurecido —, os **Peacemain** guardam portos e faróis
e os **Morganeers**, piratas de Morgan, querem a tempestade para eles. De ilha
em ilha você reacende os faróis, até jurar como Guardião no Planalto, perto do
olho da tempestade. Daí em diante começam as **Crônicas da Tempestade**.

## Capítulos escritos

| Capítulo | Ilha | Ids | Passos | Travas |
|---|---|---|---|---|
| I · O Farol do Bosque | Bosque (1–15) | 700–718 | 19 | nenhuma (ver abaixo) |
| II · O Farol Congelado | Geleira (15–30) | 719–736 | 18 | 20, 25, 30 |
| III · Areias que Gritam | Ermo (28–42) | 737–751 | 15 | 35, 40 |
| IV · O Coração da Tempestade | Planalto (40–60) | 752–768 | 17 | 45, 50, 55, 60 |

Cada passo leva a um ponto-chave:

- **Conversar** com NPC da vila ou do porto (Mestre de Missões, Alquimista,
  Treinador, Ferreiro, Taberneiro, Identificador, Alfaiate, Cartógrafo,
  Capitão do Porto), com falas escritas. Conclui ao terminar o diálogo.
- **Ir a um ponto-chave** (`objective_kind::LUGAR`): praça, saída da cidade,
  porto, ponta do cais, **mirante** (o ponto mais alto da ilha fora da cidade)
  e **costa distante** (a terra firme mais longe da praça). As posições saem
  do relevo e da vila da ilha (`historia::ponto_da_historia`), não de números
  escritos. Conclui ao chegar.
- **Caçar** o bicho da vez (lobo, urso, pistoleiro, tigre, owlbear, arqueiro,
  mago ou qualquer um), **quebrar pedra**, **criar** e **refinar** uma peça —
  os mesmos objetivos das missões do Mestre.
- **Vencer uma dungeon** (`objective_kind::DUNGEON`, alvo = id do conteúdo):
  só a vitória DAQUELA dungeon conta. Tocar no passo abre o painel de Dungeons
  já nela. No capítulo I: Porão do Naufrágio (710) e Adega do Contrabandista (717).
- **Trava de nível** (`objective_kind::NIVEL`): fica no rastreador com a barra
  de XP até o nível e conclui sozinha no level-up.
- **Viagem** (`objective_kind::VIAGEM`): falar com o Capitão do Porto embarca
  pra ilha seguinte (reconecta no canal dela). Sem canal no ar: "Rota
  indisponível no momento", sem avançar. Conclui ao entrar no mundo da ilha.
  O passo LIBERA a ilha: dali em diante o menu **Viajar** do Capitão (em toda
  ilha) leva e traz de graça (`shared::viagem`, ver VILA_E_PORTO.md). O "Ir"
  de um passo que acontece noutra ilha leva ao Capitão desta.

Recompensas seguem a faixa: ouro, XP, poções (maiores a partir da Geleira),
Poção de Experiência nas missões de área (caça e pedra), material na cor da
faixa (aço cinza no Bosque, verde na Geleira e no Ermo, azul no Planalto).
Nada de equipamento.

**Exceção deliberada: o passo 706 entrega uma chave.** "O metal da tempestade"
dá, além do cobre, 1 Couro cinza. A regra acima continua valendo — chave é
material de craft, não equipamento —, mas o registro importa porque é ela que
torna possível o passo seguinte: o 707 manda criar a primeira peça, e **toda**
receita de equipamento começa com uma chave (`receitas.rs`), que na ilha só cai
de chefe, a 5%. Sem essa entrega, o 707 pedia por volta do nível 5 algo que o
jogador não tinha como fazer. Quem for mexer nas recompensas do 706 precisa
saber que está mexendo no destravamento do craft, não num brinde.

## O capítulo I carrega o jogador (decisão de 17/09/2026)

O dono pediu que o início "leve a um ponto aceitável: todos os itens podendo
farmar, fazer dungeon". A primeira versão só reescreveu textos e a sequência
continuou com uma trava "Alcance o nível 5" no passo 704. Com a curva padrão
(nível 5 = 15.000 XP) e lobo dando 32, isso eram ~450 lobos.

Agora, no capítulo I:

- **Não há trava de nível.** As três viraram conteúdo: 704 derrubar árvores
  (apresenta a madeira), 710 Porão do Naufrágio, 717 Adega do Contrabandista.
- **A XP dos passos leva o nível**, sem contar bicho: nível 6 antes do Porão
  (pede 6), 14 antes da Adega (pede 14) e 15 ao embarcar pra Geleira. A XP é
  escrita na curva padrão e o servidor aplica o `xp_multiplier` por cima.
- **Os passos antes do craft entregam a receita inteira** da primeira
  armadura cinza: Berloque ×10 (702), Quintessência ×10 (703), Darksteel ×200
  (704), Aço ×30 (705), Cobre ×300 e Couro (706).

Os dois contratos são testes em `historia.rs`:
`capitulo_um_carrega_o_nivel_das_dungeons` e `o_inicio_entrega_a_primeira_armadura`.
Os capítulos II a IV ainda têm travas e XP antiga.

De onde vem cada material para farmar depois, o jogador aprende na oficina do
Mestre (quests 506-509, ver [MISSOES.md](MISSOES.md)).

## Crônicas da Tempestade (epílogo procedural)

Depois do passo 768, o índice continua para sempre. Cada crônica tem seis
passos, gerados só a partir do índice (`historia::cronica`):

1. derrotar N inimigos de qualquer tipo;
2. quebrar N pedras (paga aço azul);
3. criar uma peça;
4. refinar 1–3 vezes;
5. ir a um ponto-chave (mirante, costa ou cais, alternando);
6. alcançar o nível `60 + 5 × crônica` (65, 70 … 100). Acima do teto de nível
   (100) vira mais uma caça ("Guardião da Tempestade").

Recompensas crescem por crônica. Ids das crônicas começam em 10000 e vão até
65535 (mais de nove mil crônicas); o mesmo id gera sempre o mesmo passo, e o
texto é gerado uma vez por id e guardado.

## Estado e persistência

Só duas linhas por personagem em `character_quests`:

- a linha marcadora `quest_id = 699`, `status = 3`, `progress` = índice do
  passo atual;
- a linha do passo em andamento (id do passo, status e progresso de sempre).

Passo concluído não vira linha: a história infinita não engorda o banco.

## Cliente

- O rastreador mostra o passo da história sempre no topo, dourado com um
  losango; a trava mostra "Alcance o nível N · você: X" e a barra de XP.
- Clicar nele liga a auto missão, que segue passo após passo sozinha: pausa
  nos diálogos (o jogador aperta Próximo/Concluir) e para numa trava de nível
  com aviso.
- O menu de todas as missões abre com a seção **História**: capítulos escritos
  e as crônicas atual e seguinte, com concluídos, o atual e os futuros com
  cadeado e o motivo (nível no caminho, passo anterior, ilha).

## Tutoriais no capítulo I (19/09/2026)

Cinco passos da história ensinam UMA coisa da interface cada, na hora em que
ela começa a fazer falta (`objective_kind::TUTORIAL`, `quests::tutorial`):

| id | passo | depois de | ação |
|---|---|---|---|
| 770 | Poção na hora certa | 701 (o Alquimista dá as poções) | ajustar a % da poção na Barra (Menu › Sistema › Barra) |
| 771 | Luta sem as mãos | antes de 702 (lobos) | ligar o AUTO COMBATE |
| 772 | Golpe no automático | 703 (Treinador) | arrastar uma skill pra CIMA (uso automático) |
| 773 | Coleta sem esforço | antes de 704 (lenha) | ligar o AUTO COLETA |
| 774 | O mapa mostra o caminho | antes de 709 (mirante) | tocar num lugar do mapa |
| 775 | O que o nível te deu | antes de 702 | gastar um ponto de atributo (Menu › Ficha) |
| 776 | A luz nas pedras | antes de 704 | coletar um cristal de Energia |
| 777 | O primeiro despertar | depois de 719 (cap. II) | evoluir uma habilidade de tier |

- O cliente avisa o gesto com `ClientMessage::Tutorial { acao }`, e só com o
  passo ativo. O "Ir" do passo abre onde se faz (a Barra, o mapa) ou mostra a
  dica.
- **Os três novos são contados pelo SERVIDOR** (`passo_de_tutorial`), no ponto
  em que a ação de fato aconteceu: o ponto gasto em `handle_alloc_stat_point`
  (depois de descontar a Energia), o cristal no crédito da pedra tier 5, o tier
  em `handle_evoluir_skill` no `Ok`. Contar no toque do botão fecharia o passo
  numa tentativa que falta Energia pra concluir.

### O foco: tela escura e toque travado (20/09/2026)

Texto no rastreador não ensina onde fica: "toque em COMBATE" não diz **onde**
COMBATE está. `client/foco.rs` escurece a tela inteira menos um buraco, pisca
um anel dourado em volta dele e **trava o toque fora dali**.

- Quem desenha um botão que pode ser alvo **marca** o retângulo
  (`foco::marca(chave, r)`); o passo armado **pede** uma lista de chaves
  (`foco::pede`). Ganha a primeira que estiver na tela, e é assim que o buraco
  anda sozinho MENU → Ficha → "+" sem ninguém saber da ordem de desenho dos
  outros. O alvo marcado num quadro vale no quadro seguinte.
- A trava está num lugar só: `foco::clique()` no lugar de
  `is_mouse_button_pressed(Left)`, nos 61 pontos da interface. Trava pelo
  **ponto do dedo**, não pelo retângulo do widget — é o que faz valer também
  pro mundo, pro joystick e pra quem eu esquecesse de converter.
- Armar é tocar no passo no rastreador. Vale por `FOCO_TUTORIAL_S` (90 s) ou
  até o passo sair de ACTIVE: se o alvo sumir da tela, o escuro se desfaz
  sozinho em vez de deixar o jogador preso atrás dele. Chave pedida que ninguém
  marcou também não acende nada — melhor sem foco que tela preta sem buraco.
- `COLETA_ENERGIA` não tem foco: a ação é no mundo, e escurecer a tela
  esconderia exatamente a pedra que ele precisa achar.
- **Ids fora da sequência de propósito:** a história agora anda pela POSIÇÃO
  na lista (`id_do_passo`, `indice`, `capitulo_escrito`), e não por
  `700 + índice`. Inserir passo não renumera nenhum outro.
- O jogador guarda o ÍNDICE na linha marcadora; quem já tinha passado do ponto
  de inserção tem o marcador realinhado ao passo em andamento no login
  (`quests::garantir_historia`). Esse personagem não vê os tutoriais que ficaram
  para trás.

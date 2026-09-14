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
| I · O Farol do Bosque | Bosque (1–15) | 700–718 | 19 | 5, 10, 15 |
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
- **Trava de nível** (`objective_kind::NIVEL`): fica no rastreador com a barra
  de XP até o nível e conclui sozinha no level-up.
- **Viagem** (`objective_kind::VIAGEM`): falar com o Capitão do Porto embarca
  pra ilha seguinte (reconecta no canal dela). Sem canal no ar: "Rota
  indisponível no momento", sem avançar. Conclui ao entrar no mundo da ilha.

Recompensas seguem a faixa: ouro, XP, poções (maiores a partir da Geleira),
Poção de Experiência nas missões de área (caça e pedra), material na cor da
faixa (aço cinza no Bosque, verde na Geleira e no Ermo, azul no Planalto).
Nada de equipamento.

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

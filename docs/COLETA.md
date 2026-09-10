# Coleta: o lugar é a mecânica

> Substitui o sistema anterior por inteiro. O que havia — machado, foice,
> picareta, vara de pesca, três proficiências de coleta e um portão de nível
> por tier — foi descartado. Não existe mais ferramenta no jogo.

## A regra, em uma frase

**Qualquer um coleta qualquer coisa. O que muda o ganho é onde você está.**

Sem ferramenta, sem nível, sem proficiência, sem clique e sem alvo. O jogador
parado numa encosta de pedra recebe mineral sozinho; parado numa mata fechada
recebe madeira; parado num campo raso não recebe quase nada. Andar até um
lugar melhor é a única alavanca — e é por isso que a disputa é pelo **spot**.

## Como o número sai do terreno

`Ilha::riqueza(pos, raio)` olha 6 unidades (12 blocos) em volta e devolve duas
contagens:

- **madeira** — troncos no raio, pelo índice de estorvos que a colisão já usa.
- **pedra** — colunas de rocha exposta, amostradas de dois em dois blocos.

Rocha exposta não é decoração: o gerador pinta de cinza exatamente onde o
degrau passou do pulo. **A mina é visível de longe e ninguém precisa de mapa** —
o paredão que você não consegue escalar é o mesmo que rende minério.

As duas contagens têm unidade diferente (área amostrada contra contagem de
coisas), então a pedra é convertida em *troncos equivalentes*: um paredão
inteiro vale `COLETA_PEDRA_CHEIA`. Medido na ilha inicial, com 32.214 amostras
em terra:

| lugar | o que tem | densidade |
|---|---|---|
| mata mais fechada da ilha | 9 troncos | 9,0 |
| paredão mais cheio | 113 de 113 colunas de rocha | 8,0 |
| lugar comum | 1–2 troncos | 1–2 |
| 35% da terra | nada | 0 |

O tier do material (T1–T4) vem da **distância do desembarque**, a mesma regra
que faz o mob distante ser mais alto. Quem quer T4 anda até lá; não há portão.

## A reserva: por que a disputa é real

Se a densidade fosse tudo, dois jogadores no mesmo paredão receberiam o dobro
do que um recebe, e "disputar o spot" seria conversa. Então o lugar tem
**reserva**: uma célula de 8 unidades guarda quanto ainda não foi tirado dela,
em `[0, 1]`.

- cada coleta gasta `1/densidade` da reserva — lugar rico aguenta mais coleta
  que lugar pobre, que é a única forma de o teto acompanhar a densidade;
- a reserva volta sozinha a `COLETA_RESERVA_REGEN_POR_S` (célula esgotada fica
  cheia em 40s);
- o intervalo é `COLETA_INTERVALO_BASE_S / (densidade × reserva)`.

Disso sai, sem nenhuma regra escrita à mão pra dividir:

**Regime.** A célula se acomoda em `BASE × REGEN` (0,3) e o intervalo estável
fica em `1 / (densidade × REGEN)` — proporcional à densidade, como tem que
ser, e **independente de quantos jogadores estão em cima dela**. Com os
números de hoje: melhor spot da ilha ≈ 5s por coleta, lugar comum ≈ 20s,
lugar pobre ≈ 40s.

**Divisão.** Dois jogadores na mesma célula gastam a MESMA reserva. O teto não
dobra: cada um leva metade. É essa linha que faz a disputa existir.

**Rajada.** Quem chega primeiro num spot descansado pega a reserva cheia e
coleta a ~1,5s até ela baixar. Chegar antes vale alguma coisa.

## O que não entra aqui

- **Ferramenta** — não existe. Nenhuma, em nenhum tier.
- **Nível de coleta** — não existe. Nem proficiência, nem XP.
- **Clique** — não existe. Não há nó clicável nem alvo.
- **Drop no chão** — o material vai direto pra bolsa. Obrigar a pisar no drop
  devolveria justamente o clique que saiu.

## Onde o bônus vai entrar

`GameWorld::velocidade_de_coleta(sid)` devolve `1.0` e é o **único** ponto de
entrada de bônus de coleta. Já está decidido que o que acelerar a coleta não
será item de coleta nem nível; seja o que for (buff, montaria, construção no
spot, guilda), multiplica ali e em lugar nenhum mais.

## Como observar

A coleta é automática e silenciosa: sem instrumentação só se enxerga o
inventário crescendo, não *por que* ele cresce nesse ritmo. O panóptico mostra,
na ficha de cada jogador:

- **coleta** — segundos por coleta agora e em regime, e o tier do lugar;
- **spot** — densidade, aberta em pedra e madeira;
- **reserva** — quanto da célula ainda não foi esgotado.

## Mapas de arquivo

Tutorial e mapas do editor continuam tendo **nó de coleta posto à mão**. Lá a
densidade é a contagem de nós vivos no raio e cada coleta esgota um nó, que
volta pelo `respawn_seconds` dele. É o mesmo acumulador e o mesmo intervalo —
muda só de onde vem o número.

## Constantes

Todas em `shared/constants.rs`, seção *Coleta*:

| constante | valor | o que decide |
|---|---|---|
| `COLETA_RAIO_SPOT` | 6,0 | o que conta como "estar na mina" |
| `COLETA_INTERVALO_BASE_S` | 12,0 | numerador de `BASE / densidade` |
| `COLETA_INTERVALO_MIN_S` | 1,0 | piso, pra spot denso não virar torneira |
| `COLETA_PEDRA_CHEIA` | 8,0 | quanto vale um paredão em troncos |
| `COLETA_CELULA` | 8,0 | tamanho do spot, pra efeito de esgotamento |
| `COLETA_RESERVA_REGEN_POR_S` | 1/40 | teto sustentável de um lugar |
| `FARM_NODE_RESPAWN_S` | 30,0 | só para nó de mapa de arquivo |

## O que ainda não existe

- **Mina como lugar construído.** Hoje a pedra é o relevo do gerador. Um veio
  desenhado (caverna, pedreira, filão) seria trabalho de geração de terreno, e
  a coleta o leria de graça — a densidade não pergunta de onde a rocha veio.
- **Retorno visual.** O tronco não some quando rende madeira, e a rocha não
  muda. O jogador percebe o esgotamento pelo ritmo, não pelo olho.
- **Pesca.** Continua com boia e lance manual. Perdeu a vara — pescar é de
  graça, como coletar —, mas não virou automática ainda.

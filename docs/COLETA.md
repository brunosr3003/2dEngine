# Coleta: o lugar é a mecânica

> Substitui o sistema anterior por inteiro. O que havia — machado, foice,
> picareta, vara de pesca, três proficiências de coleta e um portão de nível
> por tier — foi descartado. Não existe mais ferramenta no jogo.

## A regra, em uma frase

**Qualquer um coleta qualquer coisa. O que muda o ganho é onde você está.**

Sem ferramenta, sem nível, sem proficiência, sem clique e sem alvo. Quem está
parado num veio recebe minério sozinho, e a frequência sai da **densidade**:
quantas pedras vivas há no raio do spot. Andar até um lugar melhor é a única
alavanca — e é por isso que a disputa é pelo spot.

## A pedra é uma pedra

Não é densidade abstrata de terreno: é um objeto plantado no relevo, gerado
da mesma semente no cliente e no servidor, como a árvore já era. Nenhum byte
de pedra viaja no fio.

**A cor é o tier.** Cinza, verde, azul, roxo — não existe laranja. O cristal é
*emissivo*: não toma luz de face nem oclusão (`Material::emissivo`). Isso não
é enfeite. É o que permite ler o valor de uma pedra do outro lado do vale e
decidir para qual pico subir; um cristal sombreado só se leria de perto, e não
há UI nenhuma para compensar isso. Pedra melhor também tem mais cristal no
modelo — ela *parece* melhor de longe.

| pedra | coletas até acabar | volta em | o que entrega |
|---|---|---|---|
| **cinza** | 14 | 300s | só cinza |
| **verde** | 24 | 420s | 80% cinza · 20% verde |
| **azul** | 64 | 600s | 65% cinza · 25% verde · 10% azul |
| **roxa** | 128 | 900s | 55% cinza · 27% verde · 18% azul · **0% roxo** |

O que a pedra roxa compra é **tempo de coleta**, não um material novo: nove
vezes mais coletas que a cinza, e a maior taxa de azul do jogo. Material roxo
não cai de lugar nenhum — ele só existe subindo a cor do azul, e é isso que
fecha a economia (ver `docs/ECONOMIA_DE_CRAFT.md`).

## Onde a pedra nasce

Acima de **42% do pico do bioma** e dentro de **veio** (ruído de baixa
frequência). As duas condições juntas é o que faz mina ser *lugar*, e não
pedra solta espalhada pelo mapa: se houvesse minério em qualquer encosta,
andar até um lugar não significaria nada.

O tier vem da **altitude**: quanto mais alto o pico, melhor o minério, e a
pedra roxa fica no ponto mais alto da ilha. É o único jeito de o topo da
montanha ser um destino.

Medido na ilha inicial (raio 800, metade da real):

| | pedras |
|---|---|
| cinza | 421 |
| verde | 109 |
| azul | 82 |
| roxo | 21 |

O melhor veio tem **24 pedras** no raio do spot; 33% da terra não tem nada.

## O ritmo

`intervalo = COLETA_INTERVALO_BASE_S / pedras vivas`, com piso de 1s. Um veio
cheio de 8 pedras rende uma coleta a cada 1,5s; sobrando duas, cai para 6s.

Como cada coleta gasta uma das 14–128 da pedra e a pedra **some do mundo**
quando acaba, o veio esvazia enquanto é explorado e volta quando descansa. Um
veio de 24 pedras cinza se acomoda em ~9 vivas sob um jogador: some dois
terços do que estava lá quando ele chegou, e ele vê isso acontecer.

**Dois jogadores no mesmo veio esvaziam as mesmas pedras**, então cada um leva
metade — sem nenhuma regra escrita à mão para dividir. É essa linha que faz a
disputa existir.

## O que não entra aqui

- **Ferramenta** — não existe. Nenhuma, em nenhum tier.
- **Nível de coleta** — não existe. Nem proficiência, nem XP.
- **Clique** — não existe. Não há nó clicável nem alvo.
- **Drop no chão** — o material vai direto para a bolsa. Obrigar a pisar no
  drop devolveria justamente o clique que saiu.

## Onde o bônus vai entrar

`GameWorld::velocidade_de_coleta(sid)` devolve `1.0` e é o **único** ponto de
entrada de bônus de coleta. Já está decidido que o que acelerar a coleta não
será item de coleta nem nível; seja o que for (buff, montaria, construção no
spot, guilda), multiplica ali e em lugar nenhum mais.

## O que viaja no fio

Só a exceção. A pedra os dois lados geram da semente; o servidor manda
`PedrasEsgotadas` no login (quais estão esgotadas agora) e
`PedraEsgotada`/`PedraVoltou` na AOI. O cliente refaz o pedaço (~0,9 ms) para
ela sumir de verdade da tela — sem isso o veio já limpo continuaria brilhando
e o jogador não teria como saber onde já passou.

## Como observar

A coleta é automática e silenciosa: sem instrumentação só se enxerga o
inventário crescendo, não *por que* ele cresce nesse ritmo. O panóptico mostra,
na ficha de cada jogador: pedras vivas por tier, troncos, densidade, segundos
por coleta e quantas coletas ainda cabem no spot.

## Constantes

Todas em `shared/constants.rs`, seção *Coleta*:

| constante | valor | o que decide |
|---|---|---|
| `COLETA_RAIO_SPOT` | 6,0 | o que conta como "estar na mina" |
| `COLETA_INTERVALO_BASE_S` | 12,0 | numerador de `BASE / pedras vivas` |
| `COLETA_INTERVALO_MIN_S` | 1,0 | piso, para veio grande não virar torneira |
| `COLETAS_POR_PEDRA` | 14/24/64/128 | quanto sai de cada pedra |
| `RESPAWN_DA_PEDRA` | 300/420/600/900s | quanto o veio demora a voltar |
| `RENDIMENTO_DA_PEDRA` | tabela acima | o que cada pedra entrega |
| `MINERIO_LIMIAR` | 0,42 | altura mínima, em fração do pico |
| `DENSIDADE_DE_MINERIO` | 0,035 | quão cheio é um veio |

## A árvore

Roda **na mesma máquina**, com números de marcador — 8 coletas, volta em 90s,
sem tier e sem brilho — só para a madeira não sumir do jogo. Ela ainda não
ganhou o desenho que a pedra ganhou: tier por região da mata, contagem
própria, e a decisão de se a floresta comum inteira é coletável ou se há
bosques marcados.

## O que ainda não existe

- **A árvore com desenho próprio** (acima).
- **Mina como lugar construído.** Hoje o veio é ruído sobre o relevo. Uma
  caverna ou pedreira desenhada seria trabalho de geração de terreno; a coleta
  a leria de graça, porque ela só pergunta o que há em volta.
- **Alcançabilidade.** Nada garante que toda pedra esteja num ponto em que o
  jogador consiga pisar. Veio em face de paredão existe e fica inútil.

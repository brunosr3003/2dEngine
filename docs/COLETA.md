# Coleta: o lugar é a mecânica

> Substitui o sistema anterior por inteiro. O que havia — machado, foice,
> picareta, vara de pesca, três proficiências de coleta e um portão de nível
> por tier — foi descartado. Não existe mais ferramenta no jogo.

## Estado atual: coleta POR NÓ

> Substitui a coleta passiva por lugar descrita nas seções abaixo ("A regra",
> "O ritmo", "Auto coleta"), que ficam como histórico. Continua valendo:
> onde a pedra nasce, a cor é o tier, o que cada coleta entrega e o que
> viaja no fio.

**Parado perto não rende.** O jogador escolhe uma pedra (ou tronco), anda até
ficar ao alcance e coleta **ela**, ciclo a ciclo. Cada ciclo rende a tabela
daquele nó (`economy::linhas_da_pedra` / madeira) e conta um da **reserva**
do nó. Andar, cair ou sair do alcance encerra.
Quando a reserva zera o nó **esgota**: some pra todo mundo, **sai da colisão**
(não barra mais a passagem) e volta no respawn, na colisão e na tela.

| número | valor | fonte |
|---|---|---|
| reserva da pedra (coletas) | cinza 14 · verde 24 · azul 64 · roxa 128 | `docs/COLETA.md` "A pedra é uma pedra" (tabela) · `COLETAS_POR_PEDRA` |
| respawn da pedra | 300 · 420 · 600 · 900 s | `docs/COLETA.md` (mesma tabela) · `RESPAWN_DA_PEDRA` |
| rendimento por coleta | Cobre 40–120 sempre; Aço 55%; Darksteel 35%; Platina 30%; … | `docs/ECONOMIA_DE_CRAFT.md` "Onde tudo isso sai" · `linhas_da_pedra` |
| cor do material por pedra | 100/0/0 · 80/20/0 · 65/25/10 · 55/27/18 | `docs/COLETA.md` (tabela) · `RENDIMENTO_DA_PEDRA` |
| reserva e respawn da árvore | 8 coletas · 90 s | já era **provisório** no código (`COLETAS_POR_ARVORE`); o planejamento não define |
| tempo de um ciclo | pedra 2,5 · 2,8 · 3,1 · 3,4 s; tronco 2,0 s | **decisão provisória (não estava no planejamento)** — o doc só tinha o ritmo por densidade |
| alcance | 1,4 u de borda a borda | **decisão provisória (não estava no planejamento)** |
| raio do AUTO COLETA | 20–100 u, padrão 60 | **decisão provisória (não estava no planejamento)** |

Missões e diárias de coleta (`GATHER`) contam **por coleta rendida** (cada
ciclo), não por nó esgotado.

**Manual:** clicar numa pedra/tronco no mundo anda até o ponto de alcance e,
ao chegar, manda `ColetarNo { coluna }`; o servidor valida existência,
esgotamento e alcance e começa os ciclos. Sem tecla.

**AUTO COLETA** (X ou o botão; **botão direito** abre a configuração):
tipos marcados (Madeira, Pedra cinza/verde/azul/roxa) e raio, salvos nas
preferências do personagem. O servidor aponta o nó vivo mais perto e
alcançável andando (`PedirNoDeColeta` → `NoDeColeta`); o personagem vai,
coleta até esgotar e pede o próximo. Sem nó no raio: "Aguardando recursos…",
sem sair da área. Auto missão de coleta usa os tipos **da missão**, não os da
configuração. O "Ir" do mapa numa região pede o nó daquele tipo.

**Na tela:** o personagem golpeia enquanto coleta (gesto `acao::COLETA`, que
todo mundo vê; pedra de cima, tronco de lado) e vira pro nó; a barrinha
"Coletando · Pedra azul · 2,4 s" enche até o próximo ciclo (`ColetaEstado`).
Não há ferramenta desenhada na mão — decisão provisória.

**Mapas de arquivo** (tutorial) mantêm o nó posto à mão com a coleta passiva
antiga.

**Pendências:** bolsa cheia não para a coleta (o material que não cabe se
perde); outros jogadores não viram pro nó (o rumo não viaja no fio).

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

No **topo** das montanhas, num **chão limpo**, e nunca encostada em outra.
Quatro condições, todas funções puras das colunas — o cliente e o servidor
chegam na mesma resposta sem trocar um byte:

1. **altura de montanha** — acima de 42% do pico do bioma;
2. **espaçamento** — só nasce onde o sorteio da coluna é o menor num quadrado
   de 7×7 blocos, então duas pedras ficam a pelo menos 2 unidades;
3. **chão limpo** — o quadrado de 5×5 blocos em volta (2,5 u) inteiro na
   mesma altura. É o que tira a pedra da quina e do degrau, onde metade dela
   ficava no ar ou enfiada no barranco;
4. **cume** — nada mais de um bloco acima dela num raio de 8 u. Com o terraço
   de 4 blocos do relevo, isso quer dizer o patamar mais alto das redondezas,
   e não uma prateleira no meio da encosta.

Não há sorteio de densidade nem "veio" de ruído: **o cume é o lugar**. O
patamar do topo junta as pedras sozinho, e é ele que vira spot.

O tier vem da **altitude**, com as faixas caindo *entre* os patamares do
relevo — nunca no meio de um, senão o mesmo cume teria pedra de duas cores.
Medido na ilha inicial inteira (raio 1600):

| cor | onde | pedras |
|---|---|---|
| cinza | cumes até 21 u | 226 |
| verde | cumes de 22 a 27 u | 105 |
| azul | cumes de 28 a 29 u | 50 |
| roxa | picos de 30 u pra cima | 25 |

Cada cor com metade da anterior, e a roxa só nos pontos mais altos da ilha.
Só 1 das 406 pedras encosta num tronco, então o cume não precisa de regra
própria pra árvore.

A pedra vai **do joelho à cintura**: no máximo 1,05 u de altura e 1,05 u de
meia-largura, dentro do chão limpo de 1,25 u que a regra reserva pra ela. O
teste `a_pedra_cabe_no_chao_limpo` amarra o tamanho do modelo à regra do
lugar — se alguém aumentar a pedra, ela volta a pendurar na quina, e o teste
acusa.

## O ritmo

`intervalo = COLETA_INTERVALO_BASE_S / pedras vivas`, com piso de 1s. Um veio
cheio de 8 pedras rende uma coleta a cada 1,5s; sobrando duas, cai para 6s.

Como cada coleta gasta uma das 14–128 da pedra e a pedra **some do mundo**
quando acaba, o veio esvazia enquanto é explorado e volta quando descansa. Um
cume de 11 pedras cinza — o tamanho dos maiores da ilha — se acomoda em ~4
vivas sob um jogador, a uma coleta a cada ~3s: somem dois terços do que estava
lá quando ele chegou, e ele vê isso acontecer.

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
| `MINERIO_ESPACO` | 3 blocos | distância mínima entre pedras |
| `MINERIO_PLANO` | 2 blocos | meio-lado do chão limpo exigido |
| `MINERIO_CUME` | 16 blocos | raio em que ela tem que ser a mais alta |

## O que cada coleta entrega (estado real)

Uma coleta numa pedra sorteia a **cor do material** pela escada da pedra
(`tier_do_rendimento`) e rola a linha daquela cor em `farm_node_drops`, cada
entrada independente. A tabela mora em **um lugar só**,
`economy::linhas_da_pedra`, que o seed (M25) grava e o teste
`cada_pedra_rende_os_materiais_do_planejamento` simula (60 mil coletas por cor
de pedra):

| material | quantidade | chance | cor |
|---|---|---|---|
| Cobre | 40–120 | sempre | — |
| Aço | 3–6 | 55% | a sorteada |
| Darksteel | 10–25 | 35% | — |
| Platina | 3–6 | 30% | a sorteada |
| Coração Negro, Sombra-da-Lua, Quintessência, Berloque, Fragmento, Ânima | 2–4 | 12% cada | a sorteada |
| Pó Cintilante | 1 | 3% | — |
| Escama, Garra, Chifre, Couro (chaves) | 1 | 1% cada | a sorteada |

A cor sorteada segue a tabela da pedra acima (a verde dá 80% cinza / 20%
verde, a roxa 55/27/18 e **nunca roxo**). O banco já tem essas linhas; a
migração só roda num banco sem Aço cinza na pedra e não sobrescreve ajuste de
admin.

**O jogador vê o que ganhou:** o cliente compara a bolsa antes e depois de cada
`InventoryUpdate` e faz "+57 Cobre" subir do personagem (`client/ganhos.rs`).
Não vai pro chat — a coleta entrega a cada um ou dois segundos e empurraria
pra fora as mensagens de missão.

## A árvore

Roda **na mesma máquina**, com números de marcador — 8 coletas, volta em 90s,
sem tier e sem brilho — só para a madeira não sumir do jogo. Entrega **Madeira
T1** (3–5, sempre) e às vezes ouro. Esgotada, **some da tela** como a pedra
(o cliente pula a coluna esgotada). Ela ainda não ganhou o desenho que a pedra
ganhou: tier por região da mata, contagem própria, e a decisão de se a
floresta comum inteira é coletável ou se há bosques marcados.

## Auto coleta

**X** (ou o botão COLETA, acima do COMBATE) liga e desliga. Como a coleta é o
lugar, o auto não "clica" em nada: ele **escolhe o spot e fica lá**.

- O cliente pede `PedirSpotDeColeta`; o **servidor** responde `SpotDeColeta`
  com o ponto onde há mais pedra/tronco **vivo** no raio do spot, num raio de
  60 u do jogador, ignorando o que está esgotado, já deslocado pra fora dos
  corpos (`ponto_livre_perto`) — e **alcançável**: percorre os seis melhores
  veios (`quests::spots_ordenados`, sem dois no mesmo veio) e fica com o
  primeiro a que o A* chega. Cume cercado de paredão não é mais escolhido.
- O personagem vai até lá pela viagem do mapa e fica parado; a coleta sai
  sozinha, como sempre.
- Parado no spot sem a bolsa crescer por 18 s (o veio acabou), pede outro.
- Nada vivo por perto: avisa e tenta de novo a cada 6 s.
- Desliga com andar no teclado, Esc, Z (auto combate) ou X. Auto coleta e auto
  combate são exclusivos: ligar um desliga o outro.
- A auto missão de coleta liga a auto coleta no spot que o servidor indicou.

## O que ainda não existe

- **A árvore com desenho próprio** (acima).
- **Mina como lugar construído.** Hoje o lugar é o cume que o relevo já tem.
  Uma caverna ou pedreira desenhada seria trabalho de geração de terreno; a
  coleta a leria de graça, porque ela só pergunta o que há em volta.
- **Caminho até o cume.** A pedra está sempre em chão plano, mas nada garante
  que exista rampa até aquele patamar — um cume cercado de paredão acima do
  pulo tem pedra que ninguém alcança. A auto coleta já desvia disso (spot
  alcançável); o mundo em si ainda tem essas pedras.
- **Corpo esgotado ainda barra.** A pedra/árvore esgotada some da tela, mas a
  colisão do servidor (estorvos do `shared`) não sabe de esgotamento: fica um
  obstáculo invisível até ela voltar.
- **Síntese de cor e receitas** — ver `docs/ECONOMIA_DE_CRAFT.md`; não são
  coleta.

## Bolsa cheia, ferramentas e gesto

**Bolsa cheia pausa, nunca perde item.** Antes de concluir um ciclo, o
servidor sorteia o que sai e simula a entrega numa cópia da bolsa
(`coleta::cabe_tudo`: empilha no que já tem, depois ocupa espaço vazio). Se um
item sequer não couber, o ciclo **não conclui**: nada é entregue, a reserva do
nó não anda, a missão não conta. A coleta fica **pausada** no mesmo nó
(`ColetaEstado { pausado: true }`), a animação apaga, o chat avisa "Bolsa cheia
— coleta pausada" e a barra mostra o aviso parada. Ela tenta de novo sozinha
quando a bolsa **muda** (impressão da bolsa guardada na pausa) — tentar a cada
ciclo com a bolsa igual só piscaria a barra. O AUTO COLETA fica no nó com a
faixa "AUTO COLETA · BOLSA CHEIA"; a auto missão de coleta, que usa o mesmo
auto, também. Andar encerra como sempre.

**Ferramenta por tipo.** Durante a coleta a arma some e a mão direita segura a
ferramenta do tipo — `tools/voxrender/armas.py`, pega marcada como as armas:

| Tipo | Ferramenta | Arquivo |
|---|---|---|
| Madeira | machado | `personagem/machado.vox` |
| Pedra cinza/verde/azul/roxa | picareta com a cabeça na cor do veio | `personagem/picareta_1..4.vox` |

Quatro arquivos pequenos (70 voxels cada) em vez de troca de paleta no
carregamento: a regra "tier é cor" do saque vale pro item, e aqui a cor é do
veio, que só existe na coleta.

**O tipo vai no fio.** O byte `acao` leva o gesto `COLETA` (pedra, variante =
cor − 1) ou `COLETA_MADEIRA`; quem está de fora vê a ferramenta e a cor certas.

**Gesto** (rig por código, `rig::aplica_coleta`, um golpe a cada 1,3 s — cabe
um e meio a dois por ciclo):

- **picareta:** sobe as duas mãos acima da cabeça, desce de uma vez com o
  tronco inclinando e o peso caindo, quica no impacto e volta;
- **machado:** arma pra direita, varre em arco horizontal torcendo o tronco,
  recua no impacto e volta ao meio.

Sem partícula de lasca/faísca ainda (pendência).

# O mundo

Quatro ilhas, progressão pro oeste. Cada uma é uma **zona** — processo próprio,
como campo e cidade já eram.

| ilha | bioma | raio | diâmetro | colunas | RAM/canal |
|---|---|---|---|---|---|
| `ilha_inicial` · Bosque | floresta | 800 m | 1,6 km | 10,2 M | 20,5 MB |
| `ilha_gelo` · Geleira | gelo | 400 m | 800 m | 2,6 M | 5,1 MB |
| `ilha_deserto` · Ermo | deserto | 400 m | 800 m | 2,6 M | 5,1 MB |
| `ilha_planalto` · Planalto | montanha | 800 m | 1,6 km | 10,2 M | 20,5 MB |

## Terreno não trafega

Servidor e cliente geram o mesmo relevo da mesma **semente**. É a propriedade
que o Sea of Cubes já tinha e que resolve de graça o problema que o `map.rs`
documentava: o arquipélago real tem ~24M tiles, o que daria ~72 MB de JSON por
login.

Por isso o ruído é Perlin com LCG escrito à mão, não `rand`: o mundo inteiro
depende de as duas pontas sortearem a mesma sequência, e isso não pode
depender da versão de uma dependência.

## O servidor não tem o volume

Ele guarda **dois bytes por coluna** — o índice do bloco de topo — e nada mais.
O volume voxel é do cliente, gerado por chunk em volta do jogador.

```
volume cheio   ~416 MB/km² por canal
só o topo       ~16 MB/km² por canal
```

26× mais barato, e é isso que mantém a posição de rede em 2D e o snapshot em
13 bytes: o cliente deriva o Y chamando `altura()`, não recebe.

A altura é **índice de bloco inteiro**, não float. O mundo é voxel: o topo de
um bloco é plano, e interpolar faria a colisão discordar do que o olho vê.

## Movimento

```
1 bloco de subida    anda (escada)
2 a 3 blocos         só pulando
4 ou mais            parede — não há escalada
```

É uma comparação de inteiros, e é a regra de colisão inteira. Não existe malha
de navegação nem colisão 3D. Descer é livre: cair de um barranco é movimento
válido, ficar preso em cima dele não.

Três blocos são 1,5 unidade, quase a altura do boneco (1,68). É um pulo grande
de propósito: com dois, quase todo barranco de ilha continuava parede e o
relevo lia como corredor.

O pulo **não muda a física, muda o degrau**: não existe gravidade nem
velocidade vertical em lugar nenhum da simulação. Pular é o corpo aceitar um
degrau mais alto por meio segundo. O arco vertical é o cliente contando o
tempo — a altura não viaja no fio, então não há o que forjar.

Além do relevo, **tronco, matação e toco barram passagem**; flor, capim,
arbusto, samambaia e talo não. Colidir com a forração, que cobre o chão
inteiro, transformaria o mundo em labirinto. Onde cada um nasce é decidido no
`shared` e o cliente só desenha o que o servidor decidiu — se cada lado
sorteasse por conta, a divergência viraria árvore atravessável de um lado e
parede invisível do outro.

Caverna e ponte não são andáveis. Boca de caverna vira **portal pra outra
zona**, que é o que o jogo já faz com campo→cidade.

## Como o relevo foi afinado

Um MMO não precisa de relevo bonito, precisa de **chão**: mob, chefe e briga
querem área plana. A primeira versão parecia certa e era inútil:

```
escala 0,35:  13,5 un de desnível em 1,6 km  →  99,6% andável, 0,0% de parede
```

Dois por cento de inclinação de ponta a ponta. Tudo meio-plano, nada
claramente chão nem claramente encosta — e nenhuma borda em lugar nenhum.

Terraço sozinho não resolveu: mesmo com degrau de 8 blocos, `parede` ficava em
0,3%. **Ele não cria contraste que não existe no relevo.**

O que funciona é amplitude **mais** terraço — a amplitude cria o desnível, o
terraço o recolhe em platôs planos separados por escarpa:

```
escala  terraço   pico    plana  andável  parede  sítio chefe
  0,35      6bl  12,5un   81,0%    97,8%    0,3%        11,2%
  0,60      6bl  23,0un   74,1%    93,6%    1,9%         7,9%
  0,90      4bl  34,5un   66,3%    86,5%    5,9%         6,0%   ← escolhido
  1,20      4bl  46,5un   60,6%    81,2%    9,4%         5,3%
```

Repare que o terraço **devolve** área plana em toda amplitude (a 0,90 leva
`plana` de 60,6% para 68,8%) e ao mesmo tempo cria parede. Área plana nunca
foi o recurso escasso; contraste era.

### Como está hoje

```
ilha            terra   plana  andável  parede  sítio mob  sítio chefe
inicial         45,5%   66,3%    86,5%    5,9%      15,7%         6,0%
gelo            43,3%   73,3%    88,1%    5,5%      19,5%         8,9%
deserto         38,4%   80,6%    98,2%    0,6%      25,2%        11,3%
planalto        51,6%   49,6%    74,7%   12,1%       9,1%         3,0%
```

`sítio mob` e `sítio chefe` são a fração do mapa que aceita um disco plano de 4
e de 12 unidades — são esses dois que dizem se o mundo serve. 6% numa ilha de
1,16 km² são ~70 mil m² de arena de chefe.

A progressão de dificuldade sai do próprio terreno: a inicial é generosa, o
deserto é campo aberto, e o planalto é o mais hostil — 74,7% andável e 12,1% de
parede.

O planalto quase deu errado do outro jeito: com o corte da serra em 0,18 e
terraço de 6 blocos com força 0,95, **46% da terra colapsava num nível só** e a
ilha virava uma mesa. Terraço forte demais nivela em vez de terracear.

## Medir de novo

```sh
cargo run --release --bin terreno              # rápido, raio/4
cargo run --release --bin terreno -- --cheio   # tamanho real
cargo run --release --bin terreno -- --varre   # varre escala × terraço
cargo run --release --bin terreno -- --cheio --png /tmp/ilhas
```

Afinar relevo no olho é como afinar servidor no olho: o número que interessa
— quanto do mapa aceita um chefe — não se enxerga na imagem. E a rampa de cor
do desenho acompanha o pico da própria ilha, senão a escala mente e o planalto
sai todo branco.

## Cache em disco

Onze canais da mesma zona sobem juntos; sem cache, cada um gastaria os mesmos
~3,3 s de CPU calculando exatamente o mesmo relevo. O campo vai pra
`data/ilhas/<semente>-<raio>.alt` (20 MB na ilha grande) e os outros só leem.

O arquivo é derivado da semente: pode ser apagado a qualquer momento e não vai
pro git. O cabeçalho carrega semente, raio, bioma e escala — se algum mudar, o
cache é descartado. Arquivo de uma ilha servindo como se fosse de outra é bug
que só aparece em produção.

## O cliente desenha o volume

O cliente reconstrói o volume voxel do mesmo `Gerador` — **nada de terreno vem
pela rede**. Os dois lados passam pelo mesmo `bloco_em`, então o chão que o
jogador vê e o chão em que o servidor o põe são o mesmo por construção, não
por disciplina.

**Por pedaço, não a ilha inteira.** A ilha grande tem 10,2 M colunas (3,3 s de
geração); o que cabe na tela são alguns milhares. O mundo é cortado em pedaços
de 32 colunas (16 unidades), gerados sob demanda em volta do jogador e jogados
fora quando ele se afasta. Raio 4 = 81 pedaços vivos, 128 unidades de alcance.

```
custo de um pedaço (34x34 colunas, com a borda)   420 µs
cabem numa fatia de 16 ms                         38 pedaços
gerados por quadro                                3   (~8% do quadro)
campo inteiro (81 pedaços)                        ~0,45 s, espalhados
```

O orçamento por quadro existe pra o mundo aparecer em duas piscadas em vez de
travar meio segundo.

**Só a casca.** Como não há caverna, o volume é inteiramente determinado pela
altura de cada coluna — então a malha não percorre volume nenhum: desenha o
**topo** de cada coluna e a **parede lateral** onde o vizinho é mais baixo.
Mesma imagem de um voxel completo por uma fração do custo.

**O teto da chamada é o de ÍNDICES, não o de vértices.** A macroquad aceita
10.000 vértices por `draw_mesh` — mas só **5.000 índices**, e cada quad gasta 4
vértices contra 6 índices. O índice estoura primeiro: uma malha de 6.000
vértices pede 9.000 índices e metade dela é descartada.

O sintoma não parece limite nenhum. O terreno ganha **buracos pretos em
tabuleiro**, que passam muito bem por bug de merge guloso ou por distância de
desenho — cheguei a subir o raio de 4 pra 8 atrás disso. O log tinha
**237.491** linhas de `geometry() exceeded max drawcall size, clamping` desde o
começo.

O teto agora é 800 quads por malha (3.200 vértices, 4.800 índices), os dois
abaixo do limite. Zero avisos.

**Merge guloso nos topos.** 66% a 80% das colunas de terra têm os quatro
vizinhos no mesmo nível (medido no gerador) — sem juntar, cada uma viraria um
quad e o pedaço estouraria sozinho o teto de 10.000 vértices por chamada da
macroquad. O leito submerso entra no mesmo merge: coluna abaixo da linha d'água
vira uma superfície só, senão a costa vira milhares de quads. A água em si é
outra malha, com material próprio (ver "O mar", abaixo).

A variação de cor vem da **posição do quad**, não da coluna: por coluna, cada
vizinha teria cor própria, o merge deixaria de juntar, e o pedaço
quadruplicaria pra ganhar um chiado que ninguém vê de cima.

**A câmera sobe com o chão.** Presa em zero, o jogador sumia dentro do morro
assim que o terreno passou a ter 34 unidades. E o clique de mira usa a mesma
câmera do desenho — mirar com uma e desenhar com outra erra por metros na
encosta.

Medido em jogo: **60 fps, 121 pedaços vivos** (raio 5), terreno contínuo até o
horizonte.

## O mar

Antes o mar era o topo das colunas submersas num azul chapado, e além do
último pedaço gerado aparecia o céu no lugar do oceano. Agora
(`crates/client/src/agua.rs`):

- **Leito:** o topo das colunas submersas é areia que escurece com a
  profundidade (`cor_do_leito`). É o que a água rasa deixa ver.
- **Superfície própria** por pedaço, em `ALTURA_DA_AGUA` (nível do mar +
  0,12), acima do leito e abaixo do primeiro bloco de terra. Grade de 1 u
  perto da costa e de 4 u em mar aberto; pedaço todo terra não gera nada.
- **Cor por profundidade**, calculada uma vez por vértice na geração do
  pedaço (profundidade = nível do mar − fundo do `Gerador`): turquesa e
  translúcido no raso, azul no meio, azul escuro quase opaco a partir de 6 u.
- **Horizonte:** um anel de oceano de 84 a 800 u segue o alvo da câmera
  (refeito a cada 32 u andados) e some na cor do céu.

**Animação, só no shader**, com um uniform de tempo e nada de vértice na CPU:

- onda: soma de duas senoides (0,06 + 0,05 u) no vertex shader, com amplitude
  zero até 0,5 u de profundidade e cheia a partir de 3 u, pra água não
  descolar da areia (`onda_de`);
- espuma: faixa clara na linha da costa (`espuma_de`), pulsando e deslizando;
- brilho: manchas claras que andam devagar, só onde há onda.

Pra GPU fraca, `ONDAS = false` desliga a ondulação e mantém o resto. O fragment
shader usa `highp` só quando o driver oferece (GLES2 deixa opcional).

Custo medido (teste `malha_da_agua_cabe_no_desenho`, 11 × 11 pedaços em volta
do porto da ilha inicial): 76 pedaços com água, 41.840 vértices, 62.760
índices em 76 malhas, gerados em ~11 ms; horizonte com 2.016 índices. Toda
malha respeita o teto de 800 quads por chamada.

## A colisão é a regra de degrau

O servidor carrega o mesmo campo de altura e `Ilha::mover_e_deslizar`
substitui o `WorldMap::move_and_slide` de tiles. Mesma forma — um eixo de cada
vez, o que barra num eixo continua no outro — mas quem barra não é geometria,
é a comparação de inteiros: subir mais que um bloco não acontece.

Colisão e desenho saem do **mesmo `Gerador`**, então não há como divergirem.

### Só a borda da frente

A primeira versão testava os quatro lados do corpo. Parece mais seguro e
trava o jogador: encostado num paredão, a amostra do lado da parede reprova
**toda** direção, inclusive a de se afastar dele.

O teste de caminhada pegou:

```
andou 17 unidades em 20.000 passos — travou em algum canto
```

Agora só a borda no sentido do movimento é testada, com três pontos (centro e
dois ombros) — com só o centro, o ombro entra no barranco antes de o passo ser
reprovado. Depois da correção:

```
caminhou 2650 de 2666 unidades possíveis (99% dos passos passaram)
```

Nem passar por tudo (mundo sem parede) nem travar (mundo sem chão).

### O spawn é pousado

As posições herdadas foram escolhidas num mapa de tiles plano de 180x140; a
ilha tem 1,6 km. Soltas nela, muitas caem no mar. `pousar()` procura terra
firme em espiral — melhor mover um pouco do que nascer boiando.

### O que ainda amostra o destino, não o caminho

Um passo maior que um bloco atravessaria uma parede de uma coluna só. A 30Hz e
velocidade de jogador o passo é ~0,13 de unidade contra blocos de 0,5 — quase
quatro vezes de folga. Se um dia houver corrida ou arranco, isso vira
varredura.

## Duas coisas que só apareceram jogando

**O personagem afundava no degrau antes de subir.** O cliente apoiava a
entidade na coluna do **centro**; andando pra cima de um degrau, o centro
ainda está na coluna de baixo enquanto a frente do corpo já passou por cima da
de cima — e o modelo entra no bloco. Agora o apoio é a coluna **mais alta
debaixo do corpo** (nove amostras no círculo), que é o que um pé faz.

E a troca é suavizada: o apoio pula de bloco de uma vez, e trocar o Y
desenhado junto faz o modelo piscar meio metro pra cima. A primeira vez
assenta sem animar — senão a entidade cai do céu ao entrar no AOI.

**Mob empurrava jogador — por dois caminhos diferentes.**

O primeiro era o `separar`, que dava metade da penetração pra cada lado: uma
horda de vinte lobos carregava o personagem pelo mapa.

Os corpos passaram a ter **mobilidade**: 1 cede normalmente, 0 não sai do
lugar, e a penetração é repartida na proporção entre os dois. Jogador entra
com zero — ele empurra os mobs, eles não a ele. Dois corpos com mobilidade 1
continuam dividindo meio a meio, então a mudança generaliza o comportamento
antigo em vez de trocá-lo; dois imóveis não se separam, que é o único caso em
que a sobreposição fica (e é preferível a um deles teleportar).

Os três casos têm teste, pra não voltarem sozinhos numa refatoração.

O segundo caminho continuou empurrando depois disso, e não vinha da física: o
auto-attack do mob tinha `knockback: 0.3`, e knockback **sobrescreve** a
velocidade do jogador (`Session.knockback_vel`). Um lobo sozinho é um cutucão;
uma matilha é o personagem andando pra onde não quer. Zerado no auto-attack —
knockback de **skill** continua valendo, porque aquele é efeito desenhado e
este era incidental.

A lição: "estão me empurrando" tinha duas causas, e consertar a primeira não
mostra sinal nenhum de que a segunda existe.

## Modo imortal

`MMO_IMORTAL=1`: dano em jogador não entra. É ferramenta de construção de
mundo — sem isso não dá pra andar trinta segundos olhando terreno sem morrer
pro mob. Mob continua morrendo normalmente.

O dano segue sendo **calculado** e o número flutuante continua aparecendo; só
o HP não cai. Curto-circuitar antes esconderia justamente o que se quer ver:
se o mob acerta, e quanto. O boot avisa em `WARN` — invencibilidade silenciosa
é o melhor jeito de perder uma hora depois achando o balanceamento estranho.

## Como o mundo virou lugar em vez de borrão

Três mudanças, e a primeira valeu mais que as outras duas juntas.

**Material discreto, não gradiente.** O terreno era uma rampa contínua de um
tom por bioma — daí o borrão verde. Agora é faixa de altura com paleta fixa
(emprestada do Sea of Cubes, que já passou pelo olho): areia molhada, areia,
grama, terra, rocha, arenito, neve, gelo.

**A lateral do bloco não é da cor do topo.** Lateral de grama é a *terra*
debaixo dela; de neve é rocha. Pintar o lado com a cor do topo era o que
fazia o mundo inteiro parecer uma mancha de uma cor só — e é erro que em
voxel não existe.

**Cinza é onde não se sobe.** A regra de material tem um termo de declive:
degrau acima do pulo vira rocha exposta. Não é decoração — é a **regra de
movimento pintada no chão**, e o jogador aprende a ler o mapa sem nenhum texto.

O céu também deixou de ser quase-preto. Fundo escuro aparece em todo vão do
relevo e lê como buraco na malha; foi exatamente o que me fez caçar bug de
geometria por um bom tempo antes de descobrir o teto de índices.

### Vegetação

Como no Sea of Cubes, as árvores são **geometria procedural**, não `.vox`:
copada, bétula, pinheiro e seca, com mistura por bioma — floresta de espécie
única lê como papel de parede, e isso vale mais onde há poucas árvores, porque
lá cada uma se vê inteira.

Densidade por 100 m²: floresta 2,6 · montanha 1,2 · gelo 0,30 · deserto 0,05.
No gelo é o vazio que faz a ilha ler como fim de mundo.

Três decisões que custaram uma iteração cada:

- **Assadas na malha do pedaço**, não desenhadas uma a uma. Pedaço é cache, e
  a macroquad não tem transform por malha — desenhar árvore por árvore seria
  transformar geometria na CPU centenas de vezes por quadro. Assim, vegetação
  estática custa **zero** por frame.
- **A copa são três caixas deslocadas.** De câmera de cima o que se vê é a
  *pegada* da copa, e uma caixa só desenha um quadrado — o mato inteiro lia
  como caixote largado no chão.
- **Porte menor do que parece certo.** Árvore de três vezes o jogador esconde
  o mob que ele veio caçar. Ficou em ~2×: alta o bastante pra ler, baixa o
  bastante pra enxergar através do bosque.

### O bug que nenhum print pegaria

Depois de três iterações ajustando cor e tamanho das flores, resolvi **contar**
em vez de olhar:

```
antes:   Moita 100%
depois:  Moita 29% · Flor 21% · Samambaia 19% · Arbusto 18% · Pedra 9% · Toco 4%
```

**O mesmo hash decidia se a planta nasce e qual espécie ela é.** Só passa no
teste de densidade quem tem valor baixo — e valor baixo tem os bits de cima
zerados, que era justamente de onde eu tirava a espécie. Toda planta virava a
primeira da lista, e as árvores eram todas copada pelo mesmo motivo.

Eu tinha passado três rodadas ajustando a aparência de uma coisa que **nunca
tinha sido plantada**. O censo de espécies virou teste permanente.

### Vegetação é VOLUME DE VOXEL, não caixa

A primeira versão eram três ou quatro caixas por árvore. Compilava, tinha a
densidade certa e ficou feia — porque o que faz vegetação voxel parecer
vegetação não é o volume ocupado, é o **contorno e a sombra**:

- **tronco inclinado.** Árvore perfeitamente reta lê como poste.
- **galhos** saindo do terço de cima, cada um terminando numa bolha.
- **bolha ruidosa**, não esfera: perto da casca falta bloco, e é a falha que
  faz a copa ter folha em vez de superfície.
- **três tons de folha** sorteados por bloco. Copa de um verde só lê como massa.
- **oclusão de canto (AO)** na malha. É ela que separa "cubos empilhados" de
  "volume".

O voxel da vegetação é **metade** do bloco do terreno: detalhe mais fino que o
chão é o que faz a planta ler como planta e não como pedaço de morro.

Os modelos são assados uma vez por espécie e variante (seis cada) e
instanciados copiando vértice com offset — a macroquad não tem transform por
malha, então gerar o volume por planta seria refazê-lo milhares de vezes.

### Três bugs que essa mudança revelou

**Tronco e folha invisíveis.** Eu guardava o discriminante do `Material` no
volume mas lia de volta por uma tabela de dez entradas; `Tronco` é 11, então
`get(10)` devolvia `None` e a face nunca era emitida. Só a rocha caiu por acaso
no índice certo, e o mundo ficou coberto de pedra e nada mais. A correção é
`Material::TODOS` no `shared` — tabela paralela nunca mais.

**Modelo maior que o orçamento passava inteiro.** O `instancia` só sabia
fechar a malha quando o modelo *não cabia no que sobrou*; um modelo maior que o
orçamento inteiro — e uma copada é — passava direto e a malha saía com **10.284
índices**, o dobro do teto. Agora copia em blocos de quad, fechando quantas
malhas precisar.

**Merge guloso rendeu pouco.** Só 15% — AO varia demais numa copa orgânica pra
duas células terem os quatro cantos iguais. Ficou no código porque em tronco e
pedra ele ainda ajuda, mas não foi ele que resolveu o custo.

### O que resolveu: desenhar só o que se vê

```
antes            121 pedaços desenhados     19 fps
depois       ~35 de 143 desenhados          60 fps
```

O raio de geração existe pra o mundo já estar pronto quando o jogador virar;
**desenhar** tudo ele é desperdício. Com a câmera de cima o que cabe na tela é
uma cunha, e um teste de cone por pedaço derruba três quartos das chamadas sem
mudar um pixel. O HUD mostra `desenhados/vivos` — a diferença é o que o corte
está economizando.

### Forração

A camada que faltava, e é ela que separa "campo de golfe com árvore" de mundo:
**5 plantas por 100 m² contra 1,1 árvore** no Sea of Cubes. Planta é pequena e
barata, então a densidade é várias vezes maior.

Sete espécies — moita, flor, arbusto, samambaia, pedra, toco, talo — e **cada
bioma tem vocabulário próprio**, os curtos de propósito:

```
floresta  moita, samambaia, arbusto, flor, pedra, toco      5,0 / 100 m²
gelo      pedra 62%, arbusto 28%, moita 10%                 1,2
deserto   talo 46%, arbusto 32%, pedra 22%                  3,4
montanha  pedra 70%, moita 18%, arbusto 12%                 5,5
```

Gelo sem flor e sem capim se reconhece de longe **pela ausência**. Cordilheira
forrada de flor seria serra com nome bonito.

A pedra é a única que nasce em qualquer chão, inclusive rocha e neve — matacão
em cima de laje é exatamente o que uma cordilheira tem.

### A coluna é estratificada

Era o que fazia "o arranjo entre terra, pedra e grama" ficar péssimo: eu
pintava a **parede inteira de um material só**, então um barranco de dez blocos
saía todo marrom. No Sea of Cubes a coluna tem camadas:

```
prof 0      material da superfície (grama, areia, neve…)
prof 1-2    TERRA   (areia na praia, gelo no gelo — o subsolo muda com o clima)
prof 3+     ROCHA
```

É isso que faz barranco ler como barranco: grama em cima, faixa marrom, pedra
embaixo. A parede desce em **faixas do mesmo material** em vez de um quad só,
então a estratificação aparece sem virar um quad por bloco.

*"Areia com terra marrom logo abaixo é praia, não deserto; um bloco de
profundidade já denuncia."*

### Grão do voxel, não do quad

A variação de cor era multiplicativa **por quad fundido** — e com merge guloso
um platô inteiro é um quad, então saía de um tom só. Agora é o grão do Sea of
Cubes: `((x*7 + z*13 + y*5) & 7) - 3`, aditivo, ±3 por **bloco**, inclusive na
vertical.

### Mancha de terreno

O chão tinha um verde só de horizonte a horizonte. Agora um ruído de **baixa
frequência** (regiões de dezenas de metros) escolhe entre quatro tons — grama
clara, grama, grama escura e terra batida nas pontas. Não custa geometria
nenhuma: é a cor do quad que já ia ser desenhado.

Medido depois: **60 fps, 121 pedaços, zero avisos de drawcall.**

O orçamento de índice tem teste agora, com pedaços reais das quatro ilhas:

```
ilha_inicial   pior malha: 3.200 vértices, 4.800 índices
ilha_planalto  pior malha: 3.200 vértices, 4.800 índices
```

Ele existe porque o teto de índice já quebrou o mundo uma vez em silêncio, e
vegetação só faz o orçamento apertar. O custo é o pedaço virar ~1,8 malha em
vez de 1 — uns 220 drawcalls no campo inteiro, número a vigiar quando isso for
pro celular.

## Câmera

Gira em torno do alvo (Q/E ou arrastar com o botão do meio), aproxima e afasta
(roda), e a **inclinação é fixa**.

Inclinação fixa não é limitação, é decisão: com pitch livre o jogador aponta a
câmera pro horizonte, vê o mundo inteiro carregando e o orçamento de pedaço
deixa de fechar — além de perder a leitura de cima, que é o que faz combate por
alvo funcionar.

**A câmera sobe pra não perder o jogador.** Com ela girando, qualquer morro
entre ela e o boneco tapa a vista, e perder o personagem atrás do relevo é pior
que qualquer outra falha de câmera. Em vez de raycast contra a malha, ela
amostra a **altura** ao longo da linha — o campo de altura já está ali e
responde em O(1).

O botão direito continua sendo do jogo e o esquerdo mira; câmera em botão de
combate é briga garantida com o alvo.

## Movimento

**"Pra frente" é longe da câmera, não o norte do mundo.** A rotação do input
acontece no cliente e o que sai no fio continua sendo direção em espaço de
mundo — girar a câmera **não concede confiança nenhuma nova** ao cliente.

Medido: com um yaw, segurar W levou de (86,68) a (90,56); depois de girar, o
mesmo W levou de (90,56) a (102,56). A direção acompanha a câmera.

### Toque no chão: A\* no servidor

O cliente manda um **destino**, nunca um caminho — e é um destino que ele já
poderia alcançar andando, então não concede nada que o joystick não conceda.
Quem decide por onde dá pra passar continua sendo quem tem o relevo.

O A\* roda numa **grade grossa** de 8 blocos (4 unidades): a ilha grande tem
10,2 M colunas e buscar nelas seria um mundo inteiro por clique. A rota só diz
*por onde* ir; o desvio fino continua com o `mover_e_deslizar` de sempre.

Cada aresta da grade grossa verifica as colunas **finas** no meio do caminho —
um passo de oito blocos esconderia um paredão de oito blocos, e a rota mandaria
o jogador andar contra a parede pra sempre.

Destino inalcançável devolve **caminho parcial**: andar na direção certa e
parar no meio é melhor que ficar plantado.

### Por que isso não abre porta pra cheat

O servidor já era autoritativo e continua:

| porta | como está fechada |
|---|---|
| teleporte | o cliente manda **direção**, nunca posição |
| vetor longo | `length_squared() > 1.0 → normalize()` |
| input a 300Hz | `pending_input = Some(frame)` **sobrescreve** — um por tick |
| atravessar parede | colisão contra o campo de altura roda no servidor |
| rota forjada | o cliente manda um ponto; o A\* é do servidor |
| A\* como DoS | alcance de 220 un, um pedido por 0,2 s por sessão, teto de nós |

## O que falta

## Onde os mobs nascem

As zonas do mapfile foram descartadas nas ilhas: coordenadas de um mapa de
tiles de 180x140, soltas numa ilha de 1,6 km, põem a horda inteira empilhada
num canto — e em declive, onde mob escorrega pro pé da ladeira.

Agora a ilha é varrida numa grade de 12 blocos e **cada posição de mob é um
sítio plano validado** por `sitio_plano` (disco sem degrau maior que um bloco e
com o pé seco). Os sítios são embaralhados com a semente da ilha antes da
escolha — sem isso a varredura de cima pra baixo agruparia tudo no norte.

```
ilha_inicial: 13.420 sítios planos, 59 zonas, 1.022 mobs nível 1..15
```

**O bicho não aparece nesse código.** O que a distribuição decide é o **nível**,
pela distância do desembarque: perto é o mínimo da ilha, a ponta mais longe é o
máximo. Quem escolhe a criatura é a tabela de nível — mob novo entra como dado,
sem tocar em código de mundo. A faixa de cada ilha está em `DefIlha::nivel`
(Bosque 1-15, Geleira 15-30, Ermo 28-42, Planalto 40-60).

O `lazy_spawn` continua valendo: das 59 zonas, só as perto do jogador custam
alguma coisa.

### Fortes

Um centro a cada `FORTE_A_CADA` (7) vira **forte**: a mesma faixa de nível da
vizinhança, mas o raio cai de 45 para 24 unidades, o espaçamento entre mobs cai
de 7 para 4 e o teto sobe de 18 para 34. Dá **mais que o dobro de inimigos por
unidade quadrada** — é o que `o_forte_e_densidade_e_nao_nivel` mede, na ilha de
verdade.

O nível **não** sobe junto. Densidade e nível somados fariam do forte "a zona
que você ainda não pode visitar" em vez de uma escolha: entrar, contornar, ou
voltar com o grupo.

Os centros já saem embaralhados pela semente da ilha, então pegar de 7 em 7
espalha os fortes sem uma segunda passada de espaçamento — e dá o mesmo mapa em
toda subida do servidor. O índice 0 nunca é forte: é o centro mais perto do
desembarque.

O mapa marca: mancha mais cheia, anel duplo, uma torre com ameias por cima e
"FORTE" na dica, com a linha "muito mais inimigos no mesmo espaço". O minimapa
mostra o anel grosso e a torre. Sem isso o forte seria uma emboscada, não um
lugar.

**A missão de matar bicho manda pro forte** a partir de `FORTE_NA_MISSAO_NIVEL`
(5): matar N bichos num lugar com o dobro da densidade acaba em metade do
tempo, e é isso que dá ao forte uma razão de existir além de estar marcado no
mapa. Abaixo desse nível, não: com o forte valendo desde o nível 3 a primeira
caçada (702) dá **quatro bichos em cima, vida a zero e morte** — medido por
`metas_do_inicio`. O 4 já passa; o 5 é ele com um nível de folga, porque a
simulação é um modelo e o jogador de verdade erra mais. A preferência não é
incondicional: `zona_do_bicho` escolhe a zona mais perto que serve, então um
forte do outro lado da ilha perde pra zona comum ali do lado.


- **Gate de tutorial herdado**: `SKIP_TUTORIAL_GATE=1` pra contornar. É lixo
  do projeto antigo e tem que sair.
- **Colocar as zonas de spawn** nos sítios planos que `sitio_plano` acha.
- **Editor** pra encaixar as peças `.vox` feitas por fora.

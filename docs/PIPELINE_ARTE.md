# Pipeline de arte: voxel → sprite

A fonte da arte é o modelo `.vox`; o sprite PNG é **saída de build**. É o
pipeline do Dead Cells, e existe por dois motivos concretos:

- **Rejeição 4.3 da Apple** ("parece com outros apps"). O jogo usava Mana Seed,
  um pack pago e popular. A Apple nunca diz a causa, mas asset pack comum é
  gatilho conhecido — e trocar por *outro* pack cairia no mesmo lugar. Modelo
  próprio é arte que só existe neste repositório.
- **As 8 direções saem de graça.** Girar o modelo substitui o trabalho de
  desenhar cada direção à mão, que é a parte cara de manter um paper doll.

De quebra, resolve peso: os `.vox` são pequenos e vão pro git; os PNGs são
gerados e ficam fora dele.

## A ferramenta

`tools/voxrender/voxrender.py` — Python puro, só depende de PIL.

```sh
# um modelo, 8 direções
python3 tools/voxrender/voxrender.py modelo.vox -o saida.png

# lote de uma pasta inteira
python3 tools/voxrender/voxrender.py ~/zone14/ -o assets/mobs/ -d 8 -s 1
```

| flag | o que faz |
|---|---|
| `-d N` | número de direções (default 8) |
| `-s N` | pixels por voxel (default 4; `1` dá sprite de ~100px) |
| `-e G` | elevação da câmera em graus (default 35; 90 = topo puro) |
| `--angle G` | ângulo inicial |
| `--keep-gaps` | não juntar peças separadas (ver abaixo) |

Saída: uma folha com as direções lado a lado, células do mesmo tamanho, todas
com **o pé encostado na base** — o cliente ancora o sprite pelo pé, então o
chão precisa ser o mesmo em todos os quadros.

O `.vox` é RIFF-like: `SIZE` (dimensões), `XYZI` (voxels) e `RGBA` (paleta de
256 cores). A renderização é ortográfica, painter's algorithm, com descarte de
faces internas — sem isso o `lobo.vox` geraria 71 mil voxels de polígono e só a
casca importa. O sombreamento é por face (topo com luz cheia, laterais mais
escuras), sem luz de verdade.

Custo: **~9 segundos pros 71 modelos** do zone14 numa direção.

## Modelos inteiros × modelos em peças

Os modelos do zone14 se dividem em dois grupos, e isso muda como usar cada um.

**Saem prontos** — barcos (`sot_sloop`, `chalupa`, `barco_chalupa`,
`sloop_hull`), nuvens, golens (`colosso_de_pedra`, `golem_de_pedra`,
`golem_de_terra`), árvores (`ent`, `treebeard`, `barbarvore`), esqueletos
(`esquelord`, `rei_esqueleto`), aves (`aguia`, `coruja`, `gaivota`, `flamingo`,
`arara`, `pavao`, `tucano`), porcos e tubarões.

**Vêm em peças soltas** — quadrúpedes e alados: `lobo`, `urso`, `tigre`,
`cervo`, `owlbear`, `hipogrifo`, `dragao_vermelho`. As patas ficam separadas do
corpo no eixo Z (o `lobo.vox` tem as patas em z 0..12 e o corpo só a partir de
z 31, com 18 camadas vazias no meio), porque foram autorados pra ser riggados
em runtime no zone14.

`close_z_gaps` (ligado por default) encosta as peças e resolve o caso estático.
Mas a separação **não é defeito, é o rig**: é ela que vai dar o ciclo de
caminhada de graça — basta mover o grupo das patas por quadro em vez de
redesenhar o bicho inteiro.

## Orçamento de arte: mob comum é barato, boss é caro

Regra do projeto, e ela decide como cada modelo é feito:

```
player  (pirate_captain.vox)   1.344 voxels →    356 triângulos
mob     (lobo_pequeno.vox)     1.743 voxels →  1.324 triângulos
boss    (lobo.vox)            71.761 voxels → 14.112 triângulos
```

O `lobo.vox` do zone14 é **modelo de boss**. Dele há um na tela; dele dá pra
gastar 14 mil triângulos. Mob comum aparece às dezenas — precisa de uma fração
disso, e menor de tamanho também.

`tools/voxrender/voxsimplify.py` faz a redução a partir do modelo detalhado:

```sh
voxsimplify.py lobo.vox -o lobo_pequeno.vox -f 3 --escala 0.75
```

Cada cubo `-f`³ vira um voxel só, com a cor que mais aparece dentro dele. Isso
preserva silhueta e paleta e derruba a área de superfície, que é o que vira
triângulo. O `--escala` encolhe depois, porque mob comum também tem que ser
menor que o boss na tela.

Resultado: 71.761 → 1.743 voxels (2%), 14.112 → 1.324 triângulos (**10,7× mais
leve**).

O cliente escolhe pelo flag `BOSS` do estado: `T::Enemy if boss => "lobo"`,
senão `"lobo_pequeno"`.

## O que ainda falta

- **Animação.** Hoje só o quadro parado. O próximo passo é uma spec por modelo
  dizendo quais grupos de voxel movem e como (`patas: bob 2px, 6 quadros`),
  gerando as linhas de animação da folha.
- **Ligar no cliente.** O `render.rs` desenha inimigo e loot como círculo. Falta
  o mapa de `enemy_kind` → sprite.
- **Terreno.** Tile ainda é cor chapada. Voxel também serve pra tile, e aí o
  mundo inteiro fica no mesmo estilo.

## A frente do modelo

**Todo modelo olha pro `+Y` do voxel.** A malha manda esse eixo pro `+Z` do
mundo, e é pra lá que o código de rotação assume que a frente aponta.

Isso é o contrário do que a câmera padrão do MagicaVoxel mostra: modelando de
frente pra ela, você está modelando o `-Y`. O `player.vox` chegou assim, e o
sintoma no jogo foi o boneco **andando de costas** — virando o rosto pra
câmera justamente quando corria pra longe dela.

Para consertar um modelo virado, gire o ARQUIVO:

```bash
tools/voxrender/voxgira.py assets/vox/modelo.vox
```

Ele reescreve só as coordenadas dentro dos chunks `XYZI`, sem tocar em paleta,
materiais ou grafo de cena, e nega os dois eixos horizontais — rotação, não
espelho. Espelhar trocaria a mão do modelo e a orientação dos triângulos.

Não corrija no cliente. Uma lista de exceções por modelo é o tipo de coisa que
ninguém lembra de atualizar, e um modelo virado não quebra nada: ele
simplesmente anda de costas, e isso passa despercebido por semanas.

O teste `todo_modelo_olha_pra_frente` (em `crates/client/src/vox.rs`) lê os
arquivos que vão pro jogo e confere, pelo detalhe do rosto — o olho azul do
jogador, os dentes brancos do lobo —, que ele está na metade dianteira da
cabeça. Modelo novo com rosto de outra cor precisa entrar na lista dele.

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

**E a separação é também o ESTILO.** O gerador do zone14 desenha esses
quadrúpedes SEM PERNAS, com as patas flutuando de propósito — "SOMENTE AS
PATAS FLUTUANTES DE LOBO, SEM PERNAS", com um vão de ar entre pata e tronco
(`generate_wolf.py`); tigre e urso seguem o mesmo desenho. No lobo as patas
ficam até do LADO do tronco, não embaixo. Por isso os modelos de mob entram
no jogo **sem encostar nada** (`tools/voxrender/mobs.py`): uma primeira versão
"consertava" isso e mudava o desenho dos bichos.

`close_z_gaps` (ligado por default no render de sprite) encosta as peças pro
caso estático — é escolha do render, não do modelo que vai pro jogo.
Mas a separação **não é defeito, é o rig**: é ela que dá o ciclo de
caminhada de graça — basta mover o grupo das patas por quadro em vez de
redesenhar o bicho inteiro.

**E já anda assim.** `tools/voxrender/bichos.py` pega as fatias do zone14
(`models/<bicho>_<peca>.vox`: tronco, cabeça, pescoço, cauda, quatro patas),
reduz TODAS pelo mesmo fator no mesmo grid — senão a pata não casa com o
tronco — e grava um `.vox` com as peças nomeadas em `assets/vox/bichos/`. O
cliente (`crates/client/src/bicho.rs`) anima com a marcha do zone14: pata
solta anda por TRANSLAÇÃO, o pé faz um D (reto pra trás plantado, volta pela
frente levantando), passeio em quatro tempos que vira trote quando apressa,
e o ciclo casa com a distância andada pra o pé não patinar. O golpe é a
patada da direita. `mobs.py` continua gerando o modelo inteiro, que é o
desenho de reserva quando o arquivo em peças falta.

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

- **Animação e rig.** O plano inteiro — corpo em seis peças, encaixes, poses,
  criaturas procedurais, formato dos arquivos — está em `docs/PERSONAGEM.md`.
  O leitor de `.vox` do cliente ainda só entende uma peça por arquivo; ler o
  grafo de cena do MagicaVoxel é a primeira etapa de lá.
- **Mob por tipo.** O cliente desenha os 8 tipos de mob com o mesmo lobo
  pequeno. O mapa `enemy_kind` → modelo depende da decisão de quais tipos são
  criatura e quais são humanoide.

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

## Ícones dos itens

Os ícones da bolsa, loja, barra, craft e forja saem de um **atlas ilustrado
gerado por código**, um ícone único por item (85 hoje), com a mesma técnica
dos ícones do HUD e das skills:

```sh
python3 tools/icones/gerar_icones.py
```

- Cada item é um **SVG escrito em Python** (viewBox 64); `rsvg-convert`
  rasteriza a 3× a célula e o Pillow reduz com LANCZOS em alfa
  pré-multiplicado. Determinístico: rodar de novo gera os mesmos bytes.
  Precisa de `rsvg-convert` (librsvg) e Pillow.
- Saídas: `assets/icones/itens.png` (células de **96×96**, 10 por linha) e
  `crates/client/src/icones_indice.rs` (item_id → célula, ordenado). Os dois
  vão pro git; o PNG entra no binário por `include_bytes!` e o cliente desenha
  reduzido com filtro **linear**.
- Estilo, igual ao HUD novo: silhueta clara por categoria, contorno escuro
  uniforme tirado da própria cor, gradiente de 3 tons com luz de
  cima-esquerda, brilho especular pequeno e sombra curta. Equipamento tem
  silhueta própria por peça; poção tem frasco por tipo com o líquido na cor;
  a cor do tier (`tier_color_hex`) entra nos materiais, madeira e couro, com
  aura nos tiers azul e roxo.
- A moldura de raridade **não** é assada: raridade é da instância. O cliente
  desenha com `icones::icone(id, rect, Some(raridade), qtd)`.
- Item novo: acrescente o desenho em `catalogo()` e rode o script. Item sem
  ícone cai no desenho antigo por categoria (`bolsa::icone`). O teste
  `todo_item_conhecido_tem_icone_dentro_do_atlas` (em `icones.rs`) lista os
  itens do seed — item novo entra lá também; `icones_nao_vazios_e_todos_diferentes`
  garante que nenhum ícone saiu vazio ou igual a outro.

## Ícones do HUD, das skills e do mapa

Arte vetorial própria, gerada por script e rasterizada em três atlas:

```sh
python3 tools/icones/gerar_icones_ui.py
```

- Cada ícone é um SVG 64×64 descrito em Python (`HUD`, `MAPA`, `SKILLS` no
  script). `rsvg-convert` rasteriza a 3× o tamanho da célula e o Pillow
  reduz com LANCZOS em alfa pré-multiplicado (sem franja escura). Mesma
  entrada → mesmo PNG, byte a byte.
- Saídas: `assets/icones/hud.png` (64 px, 37 ícones), `mapa.png` (48 px, 16),
  `skills.png` (96 px, 12) e `crates/client/src/icones_ui_indice.rs`.
- Estilo HUD/menu/mapa: silhueta **branca** com gradiente vertical suave,
  contorno escuro translúcido e detalhes escuros; cantos e pontas redondos. O
  cliente **tinge** pela cor do estado (normal, sobre, ativo, travado) — o
  branco vira a cor, o contorno continua escuro e legível em qualquer fundo.
- Estilo skills: disco com gradiente radial na cor da arma (espada dourado,
  katana ciano, pistolas laranja, anel verde, Julgamento roxo), aro claro,
  brilho no topo e o glifo claro por cima. Só o alfa muda (0,3–0,4 =
  indisponível).
- Cliente: `icones_ui::ui(nome, centro, lado, cor)`, `icones_ui::mapa(nome,
  centro, lado, cor, rotação)` e `icones_ui::skill(id, centro, lado, alfa)`,
  com filtro linear. Se o nome não existir, cada ponto de desenho cai no vetor
  antigo.
- Ícone novo: acrescente no dicionário do script, rode, e ponha o nome em
  `USADOS_UI`/`USADOS_MAPA` (`icones_ui.rs`). Os testes conferem que todo nome
  usado existe, que as células cabem no atlas e que nenhum ícone saiu vazio ou
  igual a outro.

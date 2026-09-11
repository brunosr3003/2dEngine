# O personagem: modelos e animação

> Documento de decisão. **DECIDIDO** foi escolhido; **PROPOSTA** é o plano de
> execução; **ABERTO** ainda depende de uma resposta. Substitui a versão
> anterior, que era de antes do redesenho do combate (8 árvores de arma, 64
> skills, passivas, ferramentas — tudo isso morreu; ver `docs/COMBATE.md`).

## O que já está decidido

**DECIDIDO — proporção estilizada 1:5.** Cabeça em ~1/5 da altura, mãos e arma
um pouco maiores que o real. A câmera olha de 27° a 75° e o alvo é celular: no
realista 1:7 a cabeça e a arma somem de cima, e no chibi 1:3 o corpo fica
pequeno demais pra armadura aparecer. O 1:5 é o meio que lê de cima e ainda
tem corpo.

**DECIDIDO — a aparência é SKIN comprada, não o item equipado.** O que o
jogador veste por número (arma, armadura, tier, peso) não muda o visual; o
visual vem de **skins compradas com TP (Tempest Points)**, a moeda premium —
de armadura (a roupa inteira) e de arma (uma por conjunto). **A skin fica no
PERSONAGEM**, não na conta: comprar pra um não libera pros outros. O peso da armadura continua POR CONJUNTO, mas como regra
de jogo (`COMBATE.md`), não como silhueta.

**O que a skin não pode esconder: o CONJUNTO de arma.** A animação sai do
conjunto — quem usa pistola saca pistola. Então a skin de arma é sempre de UM
conjunto: espada e escudo com outra cara, nunca uma katana que atira.

**DECIDIDO — um corpo só; a customização é a cabeça.** Rosto, cabelo e tom de
pele. A armadura cobre o resto. Um rig, uma tabela de poses, e toda armadura
serve em todo mundo.

**DECIDIDO — mob é criatura E humanoide.** Criaturas no mundo aberto;
humanoides em áreas específicas (não necessariamente chefe).

**JÁ É — o corpo tem 1,68 u de altura e 0,35 de raio.** O `player.vox` atual
tem 42 voxels de altura: **1 voxel ≈ 4 cm**. O corpo novo mantém essa escala.

**JÁ É — 12 skills, 5 formas.** Círculo ×4, linha ×2, cone ×2, projétil ×2, em
si ×2. Nenhuma passiva: tudo que existe tem gesto.

**JÁ É — hoje o personagem é UMA peça em pose T.** Braços esticados, nada se
move. E os 8 tipos de mob são o mesmo lobo pequeno com escala e tinta
diferentes.

## O corpo

### Proporção, em voxels

```
            ┌─────┐
            │     │   cabeça      8   (1/5)
            └──┬──┘   pescoço     1
          ┌────┴────┐
          │         │  torso      13   (com a bacia)
      ┌─┐ │         │ ┌─┐
      │ │ └──┬───┬──┘ │ │  braços 17  (ombro → ponta da mão)
      │ │    │   │    │ │
      └─┘    │   │    └─┘
             │   │        pernas  20
             │   │
             └┘ └┘        ────────────
                          total   42 voxels = 1,68 u
   ombros: 18 voxels — o diâmetro de colisão (0,70 u)
```

**Pose de repouso: braços retos pra baixo, colados ao torso** — não a pose T
de hoje. Numa peça rígida a pose de repouso é a pose parada: com os braços
para baixo, o jogo parado não precisa girar nada, e o corte entre braço e
torso fica limpo.

### DECIDIDO — dez peças rígidas: o membro é dividido

```
cabeça · torso · braço-D · antebraço-D · braço-E · antebraço-E
                 coxa-D  · canela-D    · coxa-E  · canela-E
```

Voxel não se deforma: anima girando peça inteira em volta de um pivô
(pescoço, ombros, cotovelos, quadris, joelhos). A mão faz parte do antebraço;
o pé, da canela.

**Por que dividido e não inteiro:** o que o código consegue animar sozinho.
Um bloco do terreno tem 0,5 u — **~12 voxels do personagem, uns 60% da
perna** — e o movimento sobe degrau de um bloco o tempo todo. Com perna
inteira o pé do degrau de cima entra no bloco ou flutua; com joelho, o código
dobra a perna e planta cada pé no seu degrau (IK de dois ossos, conta pura,
nenhum animador). O mesmo joelho dá o agachamento do pouso e a perna dobrada
de quem está montado; o cotovelo deixa o código apontar as pistolas pro alvo
em qualquer direção, sem uma pose por ângulo.

**O custo, declarado:** dez transformações por corpo em vez de seis
(desprezível pra ~400 triângulos), cada skin de armadura cortada em dez
peças em vez de seis, e a **tampa** das juntas — ver `docs/ARTE_DO_PERSONAGEM.md`.

**As medidas exatas, os nomes e o molde pra esculpir** estão em
`docs/ARTE_DO_PERSONAGEM.md` e `tools/moldes/corpo_molde.vox`.

### PROPOSTA — os encaixes

Não são peças do corpo: são pontos onde outra coisa se prende e segue a peça.

| encaixe | preso em | quem usa |
|---|---|---|
| `mão-D` | antebraço-D | espada, katana, pistola, anel |
| `mão-E` | antebraço-E | escudo, pistola |
| `cintura` | torso | bainha, coldre |
| `costas` | torso | manto do guerreiro, manto do mago |

O que isso dá por conjunto:

| conjunto | mão-D | mão-E | cintura | costas |
|---|---|---|---|---|
| **espada e escudo** | espada | escudo | — | manto do guerreiro |
| **katana** | katana | — | bainha | — |
| **duas pistolas** | pistola | pistola | coldre | — |
| **anel mágico** | anel | — | — | manto do mago |

O manto é uma peça rígida que balança com a velocidade (procedural, custo zero)
— capa de tecido de verdade não existe em voxel sem física, e não precisa.

### Skins de armadura

Uma skin de armadura é **a roupa inteira**, no formato do corpo: os mesmos dez
objetos, cada um SUBSTITUINDO a peça do corpo — não é casca por cima, então
não há geometria escondida desenhada à toa nem face de corpo brigando com face
de armadura. O piratinha (`corpo.vox`) é a roupa **padrão**: quem não comprou
nada anda de pirata.

Isso muda o tamanho do trabalho de arte a favor do playtest: **nenhuma skin é
pré-requisito de jogo**. Antes eram 30 peças (três pesos × dez) que tinham que
existir pra armadura aparecer; agora o jogo inteiro roda com o pirata, e cada
skin nova é conteúdo de loja, feita quando for vendida.

### Skins de arma

Uma por **conjunto**: espada e escudo, katana, pistolas, anel. A skin troca a
CARA, não o conjunto. Cada conjunto tem uma skin padrão — é ela que entra no
playtest.

### DECIDIDO — o que a skin não esconde

Aparência comprada tem um custo pro combate: de longe, ninguém mais lê o que o
outro está usando. Três coisas continuam legíveis, e nenhuma custa modelo:

| o quê | como aparece | por quê |
|---|---|---|
| **o conjunto de arma** | pela própria skin (sempre do conjunto certo) e pelo gesto | diz o que ele vai fazer |
| **o tier do item** | a cor dos frisos (faixa 241–244) sai do item EQUIPADO, não da skin | diz o quanto ele é forte |
| **o peso da armadura** | ícone na placa de nome | diz o quanto ele aguenta |

O friso separa FORMA de PODER: a skin decide o desenho, o item equipado
decide a cor. A mesma skin numa arma cinza brilha cinza; numa roxa, brilha
roxo. As quatro cores são as do cristal da pedra (`cristal_do_tier`) — quem
aprendeu a ler a pedra lê a arma do mesmo jeito — e custam quatro cópias da
malha em memória, geradas no carregamento: **zero modelo a mais**.

| índices da paleta | o que são | quem decide |
|---|---|---|
| **241–244** | frisos, gemas, detalhes | o tier do item equipado |
| **245–248** | cabelo | a cor escolhida |
| **249–252** | pele | o tom escolhido |
| resto | material | a skin |

### A cabeça

| | variações no playtest | como |
|---|---|---|
| rosto | 4 | modelo |
| cabelo | 6 | modelo; o elmo pesado o esconde |
| pele | 6 tons | paleta |

**ABERTO — os números acima são chute.** Eles decidem quanto se modela, então
mudar é barato agora e caro depois.

### Acessórios

Brinco, amuleto, bracelete e cinto **não aparecem no corpo** no playtest. De
27°–75° de altura e numa tela de celular, um brinco é um pixel. Eles existem
como item e como número, não como modelo.

## A animação

Três fontes, em ordem de custo:

### 1. Procedural — código, zero arte

Sai da velocidade e das bandeiras que o servidor já manda. Nunca dessincroniza
do movimento — que é o defeito clássico de clipe gravado — e vale pra todo
modelo com as dez peças, inclusive humanoide de mob.

| estado | de onde vem | o que mexe |
|---|---|---|
| parado | velocidade ~0 | torso sobe e desce 1 voxel (respiração) |
| andar | velocidade | pernas e braços em seno, fase pela distância andada |
| pulo | `PULANDO` + arco que o cliente já desenha | encolhe na subida, abre os braços na queda |
| pouso | fim do arco | agacha dobrando os joelhos, 0,1 s |
| degrau | altura do chão sob cada pé | cada pé plantado no seu bloco, joelho dobrado (IK de dois ossos) |
| tomar dano | vida caiu | tranco do torso pra trás, 0,12 s |
| caído | `DOWNED` | corpo inteiro deitado |
| morte | vida 0 | cai pra trás e fica |
| manto | velocidade | ângulo da capa acompanha |
| montado | `MONTADO` (novo) | pose sentada; tronco acompanha o trote |

### 2. Pose-chave — tabela, não quadro

O que o corpo faz por INTENÇÃO: ataque e skill. Uma pose é **uma rotação por
peça** (dez rotações); o cliente interpola entre elas. Não se
desenha quadro nenhum.

**Ataque básico:** um combo de três golpes por conjunto, três poses por golpe
(preparo, impacto, volta).

**Skills:** as 12, cada uma com seu gesto. O plano anterior juntava por FORMA,
mas os quatro conjuntos são temas diferentes demais pra isso: o "círculo" da
katana é um giro, o das pistolas é arremessar um barril e o do anel é abrir as
mãos. Com só doze skills, gesto próprio custa pouco.

| conjunto | skill | forma | gesto |
|---|---|---|---|
| espada e escudo | Investida | linha | avança com o escudo à frente |
| | Golpe Largo | cone | corte horizontal de ombro a ombro |
| | Muralha | em si | escudo erguido, pé no chão |
| katana | Saque | linha | tira da bainha e corta no mesmo movimento |
| | Dança | círculo | giro completo com a lâmina aberta |
| | Vento Cortante | projétil | corte de cima pra baixo que solta a onda |
| duas pistolas | Tiro Certeiro | projétil | as duas mãos juntas, mira |
| | Rajada | cone | braços abrindo em leque, disparando |
| | Barril | círculo | arremesso por cima do ombro |
| anel mágico | Bênção | em si | mão do anel erguida |
| | Aura | círculo | as duas mãos abertas pra fora |
| | Julgamento | círculo | mão erguida e descida de uma vez |

| | poses |
|---|---|
| combos: 4 conjuntos × 3 golpes × 3 | 36 |
| skills: 12 × 3 | 36 |
| coleta (agachado tocando a pedra, em laço) | 2 |
| **total** | **74** |

O tempo de cada pose sai do servidor: `espera_s` e `conjuracao_s` da skill, a
cadência do ataque. A animação nunca decide nada — ela só mostra o que o
servidor decidiu.

### 3. Criaturas — procedural também

Os quadrúpedes do zone14 **já vêm em peças**: corpo, pescoço, cabeça, quatro
patas, cauda, cada peça num arquivo, todas no mesmo quadro de 128³. É o rig
pronto. A marcha de quatro patas é o mesmo seno das duas pernas com quatro
fases (pares diagonais); cabeça e cauda balançam com a velocidade; o ataque é
um bote do corpo pra frente. **Custo de arte zero de novo.**

Golem e ent são bípedes em peças: usam o procedural do humanoide.

## Os mobs

| | de onde vem | anima com |
|---|---|---|
| **criatura** | estoque do zone14, simplificado (`voxsimplify.py`) | procedural de criatura |
| **humanoide** | o corpo do jogador + roupa de mob + arma | procedural + o combo do conjunto da arma |

Humanoide de mob não tem skill: o mob só tem ataque corpo a corpo ou à
distância. Então ele reaproveita o **golpe 1 do combo** de um conjunto — de
espada se for corpo a corpo, de pistola se for à distância — e não precisa de
uma pose sequer.

O estoque do zone14 em peças: lobo, urso, tigre, cervo, porco, dragão,
hipogrifo, hidra, owlbear, golem de pedra, golem de terra, ent, escaravelho,
morsa, e um punhado de aves. Sem peças: esqueleto-lorde, colosso, árvores.

**ABERTO — o mapa dos 8 tipos.** Grunt, Tank, Ranger, Ninja, Mago, Berserker,
Arqueiro e Chefe são nomes herdados. Qual vira criatura, qual vira humanoide,
e em que áreas os humanoides aparecem.

## Montaria e pet

**DECIDIDO — os dois são itens da loja de cash**, comprados com TP, cada um com
livro de habilidade próprio (também TP).

**DEPOIS — o modelo.** De onde vêm e como são fica pra quando essa parte
começar. O que já está pronto e não depende disso: o encanamento (`Mounted`
aponta pra uma entidade, e o barco já usa) e a pose do personagem montado, que
é procedural — ver a tabela da animação.

## O que o fio precisa

Os códigos de ataque de hoje (`SLASH`, `SHOOT`, `PARRY_FLASH`, `TOOL_SWING`…)
apontam pra páginas de sprite do Mana Seed. Morrem junto com o cliente 2D.

**PROPOSTA — um byte `acao` no `EntityState`:** 4 bits de estado (atacando,
conjurando, coletando, montado…) e 4 de variante (passo do combo 0–2, qual das
três skills do conjunto). O conjunto não precisa viajar a cada tick: ele já
está na aparência.

**PROPOSTA — aparência na META, não no tick.** Skin de armadura, skin de arma
do conjunto atual, conjunto, tier da arma e da armadura (pros frisos), peso
(pro ícone), rosto, cabelo, pele: ~10 bytes, vai no nascimento da entidade e
quando algo muda. Substitui a
`VisualConfig`, que descrevia o paper doll 2D e morreu com ele.

## O formato da arte

**JÁ É — o leitor de `.vox` só entende uma peça por arquivo**, e fica com a
maior. O rig precisa de mais que isso.

**JÁ É — e ele ESPELHA o modelo.** A malha manda o voxel `(x, y, z)` pro mundo
`(x, z, y)`: trocar dois eixos é reflexo, não rotação. Tudo que foi modelado
na mão direita aparece na esquerda. O importador de peças usa o mapa certo
(`(−x, z, y)`, que é rotação) — assim a regra pra quem modela fica a natural:
a direita do personagem é o lado de X maior. E ele também **não recentraliza
cada peça**: a malha de hoje centra cada modelo na própria caixa, o que
desmontaria o corpo.

**PROPOSTA — um arquivo por conjunto de peças, com os objetos NOMEADOS no
MagicaVoxel.** O artista vê o personagem inteiro e move as peças juntas; o
cliente lê o grafo de cena (`nTRN`/`nSHP`) e separa por nome:

| arquivo | objetos dentro |
|---|---|
| `corpo.vox` | `cabeca`, `torso`, `braco_d`, `braco_e`, `perna_d`, `perna_e` |
| `skins/armadura/<nome>.vox` | os mesmos dez objetos do corpo, cada um substituindo a peça |
| `rosto_01.vox` … · `cabelo_01.vox` … | um objeto cada |
| `skins/arma/<conjunto>/<nome>.vox` | um objeto; o marcador magenta é o ponto de pega |

Tudo modelado **no mesmo quadro do corpo**, pra encaixar sem ajuste. Os pivôs
(pescoço, ombros, quadris) ficam numa tabela no código — só existe um corpo,
então só existe uma tabela.

A frente continua sendo `+Y` do voxel, e o teste `todo_modelo_olha_pra_frente`
continua valendo.

## A ordem

Cada etapa é jogável sozinha, e toda etapa que mexe no jogador sobe com os
bots na ilha.

| etapa | entrega | arte nova? |
|---|---|---|
| **1. rig** | leitor de cena do `.vox` (sem espelhar, sem recentralizar); corpo nas dez peças; desenho por peça; locomoção procedural; pé no degrau | não — o molde já está no formato final |
| **2. fio + primeiro ataque** | byte `acao`; aparência na meta; combo da espada; **visualizador de pose** que recarrega a tabela a quente | não |
| **3. os quatro conjuntos** | as 8 armas; os 4 combos; as 12 skills | armas |
| **4. skins e cabeça** | troca de skin; frisos pelo tier do item; rosto, cabelo, pele; criação de personagem | cabeça — skin é loja, não pré-requisito |
| **5. mobs** | criaturas simplificadas com marcha procedural; humanoides nas áreas | criaturas vêm do estoque |
| **6. montaria e pet** | depois — o modelo ainda não foi decidido | — |

O **molde** (`tools/moldes/corpo_molde.vox`, gerado por
`tools/voxrender/molde_corpo.py`) é o que destrava as etapas 1–3 sem esperar
arte: o corpo em caixas, já cortado, nomeado e com as tampas das juntas. A
arte de verdade entra depois, **substituindo o arquivo** — nenhuma linha de
código muda quando ela chega.

## O que ainda depende de você

1. **Arma fora de combate** — sempre na mão, ou guardada (a katana na bainha é
   a identidade do conjunto)?
2. **Correr existe?** O protocolo tem o bit `SPRINT` e o servidor tem
   `SPRINT_SPEED_MULT`, mas nada usa. Se existir, é mais um ciclo procedural.
3. **Quem faz a arte final** — você no MagicaVoxel, um artista, outra fonte?
   Muda o quanto vale investir no visualizador de pose.
4. **O mapa dos 8 tipos de mob** e as áreas dos humanoides.
5. **O visual do anel** — ele é um voxel de cima; brilho ou runa na mão?
6. **Quantas variações de cabeça**.

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

**DECIDIDO — a armadura tem peso POR CONJUNTO.** Leve, média e pesada são três
silhuetas de corpo inteiro, não quinze peças misturáveis. Num jogo de câmera
alta dá pra saber o que o sujeito é olhando de longe — e isso fecha a questão
que o `COMBATE.md` deixou em aberto. O tier é a COR, não o modelo.

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
          │         │  torso      12   (com a bacia)
      ┌─┐ │         │ ┌─┐
      │ │ └──┬───┬──┘ │ │  braços 17  (ombro → ponta da mão)
      │ │    │   │    │ │
      └─┘    │   │    └─┘
             │   │        pernas  21
             │   │
             └┘ └┘        ────────────
                          total   42 voxels = 1,68 u
   ombros: 16 voxels (dois palmos de cabeça)
```

**Pose de repouso: braços caídos, 10° afastados do corpo** — não a pose T de
hoje. Numa peça rígida a pose de repouso é a pose parada: com os braços para
baixo, o jogo parado não precisa girar nada, e o corte entre braço e torso
fica limpo.

### PROPOSTA — seis peças rígidas

```
cabeça · torso · braço-D · braço-E · perna-D · perna-E
```

Voxel não se deforma: anima girando peça inteira em volta de um pivô
(pescoço, ombros, quadris). A mão faz parte do braço.

**ABERTO — cotovelo e joelho.** Membro inteiro (seis peças, estilo Minecraft) é
o que cabe no playtest. Dividir em braço e antebraço (dez peças) dá golpe e
mira muito mais expressivos — mas o artista corta cada peça de novo, então a
decisão tem que vir antes da arte final, não depois.

### PROPOSTA — os encaixes

Não são peças do corpo: são pontos onde outra coisa se prende e segue a peça.

| encaixe | preso em | quem usa |
|---|---|---|
| `mão-D` | braço-D | espada, katana, pistola, anel |
| `mão-E` | braço-E | escudo, pistola |
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

### A armadura

Como o corpo anima por peça, **a armadura tem que vir cortada nas mesmas seis
peças**: uma casca que se encaixa sobre cada uma e gira junto.

| peso | cabeça | torso | braços | pernas | silhueta |
|---|---|---|---|---|---|
| **leve** | nada (cabelo à mostra) | couro justo | braçadeira | calça | fina, cabelo visível |
| **média** | capuz ou tiara | gibão | ombreira pequena | caneleira | média |
| **pesada** | elmo (esconde o cabelo) | placa | ombreira larga | grevas | larga, quadrada |

Três pesos × seis peças = **18 peças de armadura**, no máximo. Tier não é peça:
é **troca de paleta**.

### O tier é uma cor, não um modelo

A mesma convenção de paleta pra tudo — armadura, arma, secundária:

| índices da paleta | o que são | como o cliente trata |
|---|---|---|
| **cor do tier** | frisos, gemas, detalhes | trocados por cinza, verde, azul ou roxo |
| **pele** | rosto, mãos | trocados pelo tom escolhido |
| resto | material | fica como está |

As quatro cores do tier são as mesmas do cristal da pedra (`cristal_do_tier`):
quem aprendeu a ler a pedra lê a armadura do mesmo jeito. Quatro tiers custam
quatro cópias da malha em memória, geradas no carregamento — **zero modelo a
mais**.

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
modelo com as seis peças, inclusive humanoide de mob.

| estado | de onde vem | o que mexe |
|---|---|---|
| parado | velocidade ~0 | torso sobe e desce 1 voxel (respiração) |
| andar | velocidade | pernas e braços em seno, fase pela distância andada |
| pulo | `PULANDO` + arco que o cliente já desenha | encolhe na subida, abre os braços na queda |
| pouso / degrau | fim do arco / subida | agacha 0,1 s |
| tomar dano | vida caiu | tranco do torso pra trás, 0,12 s |
| caído | `DOWNED` | corpo inteiro deitado |
| morte | vida 0 | cai pra trás e fica |
| manto | velocidade | ângulo da capa acompanha |
| montado | `MONTADO` (novo) | pose sentada; tronco acompanha o trote |

### 2. Pose-chave — tabela, não quadro

O que o corpo faz por INTENÇÃO: ataque e skill. Uma pose é **uma rotação por
peça** (seis números de três eixos); o cliente interpola entre elas. Não se
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

## A montaria

Mesmo encanamento do barco: `Mounted` aponta pra uma entidade. A montaria é um
quadrúpede do estoque com um encaixe `sela` no corpo; o jogador senta nele e
fica na pose montado.

Medido: o cervo do zone14 reduzido no fator 3 fica com **43 voxels de altura**
— o tamanho certo pra montar ao lado de um corpo de 42 — e 2,8 mil voxels, o
dobro do jogador. O tigre no mesmo fator fica com 4 mil. As "variações" de
montaria são bicho diferente e paleta diferente, não rig novo.

**ABERTO — quais bichos entram primeiro.** Cervo e tigre já estão medidos.

## O que o fio precisa

Os códigos de ataque de hoje (`SLASH`, `SHOOT`, `PARRY_FLASH`, `TOOL_SWING`…)
apontam pra páginas de sprite do Mana Seed. Morrem junto com o cliente 2D.

**PROPOSTA — um byte `acao` no `EntityState`:** 4 bits de estado (atacando,
conjurando, coletando, montado…) e 4 de variante (passo do combo 0–2, qual das
três skills do conjunto). O conjunto não precisa viajar a cada tick: ele já
está na aparência.

**PROPOSTA — aparência na META, não no tick.** Conjunto de arma, peso da
armadura, tier da arma, tier da armadura, rosto, cabelo, pele: cabe em 7
bytes, vai no nascimento da entidade e quando algo muda. Substitui a
`VisualConfig`, que descrevia o paper doll 2D e morreu com ele.

## O formato da arte

**JÁ É — o leitor de `.vox` só entende uma peça por arquivo**, e fica com a
maior. O rig precisa de mais que isso.

**PROPOSTA — um arquivo por conjunto de peças, com os objetos NOMEADOS no
MagicaVoxel.** O artista vê o personagem inteiro e move as peças juntas; o
cliente lê o grafo de cena (`nTRN`/`nSHP`) e separa por nome:

| arquivo | objetos dentro |
|---|---|
| `corpo.vox` | `cabeca`, `torso`, `braco_d`, `braco_e`, `perna_d`, `perna_e` |
| `armadura_leve.vox` · `_media` · `_pesada` | as mesmas seis, só a casca |
| `rosto_01.vox` … · `cabelo_01.vox` … | um objeto cada |
| `espada.vox`, `escudo.vox`, … | um objeto; a origem é o ponto de pega |

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
| **1. rig** | leitor de cena do `.vox`; corpo de rascunho nas seis peças; desenho por peça; locomoção procedural | não — rascunho gerado por script, já no formato final |
| **2. fio + primeiro ataque** | byte `acao`; aparência na meta; combo da espada; **visualizador de pose** que recarrega a tabela a quente | não |
| **3. os quatro conjuntos** | as 8 armas; os 4 combos; as 12 skills | armas |
| **4. armadura e cabeça** | 3 pesos × 6 peças; tier por paleta; rosto, cabelo, pele; criação de personagem | armadura, cabeça |
| **5. mobs** | criaturas simplificadas com marcha procedural; humanoides nas áreas | criaturas vêm do estoque |
| **6. montaria** | cervo/tigre com sela; pose montado | vem do estoque |

O **rascunho gerado por script** é o que destrava as etapas 1–3 sem esperar
arte: um corpo de caixas em 1:5, já cortado e nomeado no formato final. A arte
de verdade entra depois, **substituindo o arquivo** — nenhuma linha de código
muda quando ela chega.

## O que ainda depende de você

1. **Cotovelo e joelho** — membro inteiro (6 peças) ou dividido (10)? Decide
   como o artista corta, então vem antes da arte final.
2. **Arma fora de combate** — sempre na mão, ou guardada (a katana na bainha é
   a identidade do conjunto)?
3. **Correr existe?** O protocolo tem o bit `SPRINT` e o servidor tem
   `SPRINT_SPEED_MULT`, mas nada usa. Se existir, é mais um ciclo procedural.
4. **Quem faz a arte final** — você no MagicaVoxel, um artista, outra fonte?
   Muda o quanto vale investir no visualizador de pose.
5. **O mapa dos 8 tipos de mob** e as áreas dos humanoides.
6. **Quantas variações de cabeça** e **quais montarias primeiro**.

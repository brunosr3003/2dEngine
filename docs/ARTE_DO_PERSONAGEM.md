# Arte do personagem: o que modelar, e em que medida

> A ficha de quem modela. As decisões e o porquê estão em `docs/PERSONAGEM.md`;
> aqui estão só as medidas, os nomes e as regras de arquivo.

**Comece pelo molde:** `tools/moldes/corpo_molde.vox`. É o corpo em caixas,
já nas medidas certas, com as dez peças separadas e nomeadas e cada corte numa
cor diferente. Abra no MagicaVoxel e esculpa por cima — nome, tela e posição
das juntas já estão certos, e são justamente as três coisas que o jogo não
consegue adivinhar. Ele é gerado por `tools/voxrender/molde_corpo.py`; as
medidas moram lá e aqui.

## As regras que valem pra todo arquivo

1. **1 voxel = 4 cm** (0,04 u). O corpo tem **42 voxels** de altura = 1,68 u.
2. **Frente = +Y. Cima = +Z. Chão em z = 0.**
3. **A direita do personagem é o lado de X MAIOR.** Olhando o rosto dele de
   frente, a mão direita dele fica à SUA esquerda — como com uma pessoa.
4. **Tudo que se VESTE usa a mesma tela: 32 × 24 × 48** — corpo, armadura,
   rosto, cabelo, manto, coldre, bainha. Todos os objetos do arquivo com o
   mesmo tamanho e **na mesma posição** do World. Não mova objeto: mexa só
   dentro dele. É isso que faz uma peça encaixar na outra sem ajuste.
5. **O nome do objeto é contrato** (as listas abaixo). Objeto com nome errado
   é ignorado, e o jogo avisa qual.
6. **Cada voxel no objeto da peça a que ele pertence.** Um voxel de braço
   dentro do objeto `torso` não gira com o braço.
7. **Faixas de paleta reservadas** — use só pra isso:

| índices | pra quê | o jogo faz |
|---|---|---|
| **241–244** | detalhes que mudam com o TIER (frisos, gemas, bordas), do claro ao escuro | troca por cinza, verde, azul ou roxo |
| **245–248** | cabelo, do claro ao escuro | troca de cor do cabelo sem modelo novo |
| **249–252** | pele, do claro ao escuro | troca pelo tom escolhido |
| **255** | marcador (magenta) | lê a posição e **apaga** o voxel |
| resto | livre | fica como está |

A cor que você põe nas faixas 241–252 é só de rascunho: o jogo substitui.

## O corpo — `corpo.vox`, dez objetos

Coordenadas em voxel: o voxel 21 ocupa de 21,0 a 22,0. As caixas são as do
molde; o pivô é o ponto em volta do qual a peça gira.

| objeto | o que é | x | y | z | tamanho | gira em volta de |
|---|---|---|---|---|---|---|
| `cabeca` | pescoço + cabeça | 12–19 | 8–15 | 33–41 | 8×8×8 (+ pescoço 4×4×1) | pescoço (16, 12, 33) |
| `torso` | peito + bacia | 11–20 | 9–14 | 20–32 | 10×6×13 | — (é a raiz) |
| `braco_d` | braço direito, de cima | 21–24 | 10–13 | 25–32 | 4×4×8 | ombro (23, 12, 31) |
| `antebraco_d` | antebraço + mão direita | 21–24 | 10–13 | 16–24 | 4×4×9, mão em z 16–19 | cotovelo (23, 12, 25) |
| `braco_e` | braço esquerdo, de cima | 7–10 | 10–13 | 25–32 | 4×4×8 | ombro (9, 12, 31) |
| `antebraco_e` | antebraço + mão esquerda | 7–10 | 10–13 | 16–24 | 4×4×9 | cotovelo (9, 12, 25) |
| `coxa_d` | coxa direita | 16–19 | 10–13 | 10–19 | 4×4×10 | quadril (18, 12, 19) |
| `canela_d` | canela + pé direito | 16–19 | 10–15 | 0–9 | 4×4×8, pé 4×6×2 | joelho (18, 12, 10) |
| `coxa_e` | coxa esquerda | 12–15 | 10–13 | 10–19 | 4×4×10 | quadril (14, 12, 19) |
| `canela_e` | canela + pé esquerdo | 12–15 | 10–15 | 0–9 | 4×4×8, pé 4×6×2 | joelho (14, 12, 10) |

```
   proporção 1:5                        de frente, em voxels
   cabeça     8   ─┐ 1/5                ombros: 18 de largura
   pescoço    1    │                    (= diâmetro de colisão, 0,70 u)
   torso     13    │                    mão: no meio da coxa
   pernas    20   ─┘ ~metade            pé: 2 à frente da canela
   total     42 = 1,68 u
```

**Pose de repouso: braços RETOS pra baixo, colados ao torso.** Não é a pose T
do modelo de hoje. A pose de repouso é a pose parada — com ela, o jogo parado
não gira nada.

### O que pode e o que não pode mudar

- **Pode:** arredondar, afinar, dar volume, detalhe de roupa — até **2 voxels**
  pra fora da caixa de cada peça.
- **Não pode:** mover as juntas, mudar a altura total ou o comprimento dos
  membros. O pé plantado no degrau e as 74 poses contam com esses números: um
  joelho um voxel mais alto e o pé passa a entrar no degrau.

### A tampa das juntas

No cotovelo e no joelho, a peça de BAIXO sobe dois voxels, recuada um de cada
lado, pra dentro da peça de cima (no molde: um 2×2×2 em z 25–26 no antebraço e
em z 10–11 na canela). Parado ela fica escondida lá dentro; dobrando, é ela que
tapa o buraco que abriria na junta. **Mantenha a tampa** ao esculpir — ela é a
única coisa no arquivo que não aparece e ainda assim é necessária.

## A cabeça

| arquivo | objeto | onde | regras |
|---|---|---|---|
| `rosto_01.vox`… | `cabeca` | a mesma caixa da cabeça do corpo | **substitui** a cabeça do corpo; olhos na face +Y; pele em 249–252 |
| `cabelo_01.vox`… | `cabelo` | até 2 voxels além da cabeça (x 10–21, y 6–17, z 32–43); rabo ou trança pode descer até z 28 nas costas. **Chapéu** pode ir até 3 além da cabeça de cada lado e até o topo da tela (x 9–22, y 5–18, z ≤ 47) | fica **por cima** da cabeça; cor em 245–248. A faixa do chapéu tem que ser **um voxel mais larga que a cabeça** em volta toda, pra as faces dele nunca caírem em cima das da cabeça |

Proposta pro playtest: **4 rostos, 6 cabelos.**

## A armadura — `armadura_leve.vox`, `armadura_media.vox`, `armadura_pesada.vox`

- **Os mesmos dez objetos, com os mesmos nomes.** Cada objeto **substitui a
  peça inteira do corpo** — não é casca por cima. A mão continua no antebraço,
  em pele (249–252).
- **Como fazer:** duplique o `corpo.vox` e esculpa a armadura em cima, peça
  por peça. As juntas e as tampas vêm junto.
- **Quanto pode crescer:** leve quase o corpo; média até +1 voxel; pesada até
  +2, ombreira até +3.
- **Frisos que mudam com o tier:** 241–244. Uma armadura serve os quatro tiers.
- **Cabeça:** a leve não tem. A média tem um objeto `capuz` (substitui o
  cabelo; o rosto aparece). A pesada tem um objeto `elmo` (substitui o cabelo
  e cobre o rosto).

## O que se veste — mesma tela, um objeto cada

| arquivo | objeto | onde fica | preso em |
|---|---|---|---|
| `manto_guerreiro.vox` | `manto` | atrás do torso (y 7–8), da linha dos ombros (z 32) até o joelho (z 10) | torso; gira em (16, 8, 32) — balança sozinho |
| `manto_mago.vox` | `manto` | idem, até o tornozelo (z 2); pode ter capuz caído | torso |
| `coldre.vox` | `coldre` | cinto na bacia (z 19–23) com um coldre de cada lado, atrás do braço (y 8–9); **não descer abaixo de z 19**, senão a coxa atravessa andando | torso |
| `bainha.vox` | `bainha` | quadril esquerdo, inclinada: boca à frente e acima (y 15, z 22), ponta atrás e abaixo (y 4, z 16), por fora da coxa (x 8–9) | torso |

## As armas de mão — tela livre, um objeto

**Orientação: como ela fica na mão com o braço caído.** Cabo ao longo do Y,
lâmina ou cano apontando pra **+Y** (frente).

**Um voxel magenta (255) no centro da pega** — onde a mão fecha. O jogo lê esse
ponto como o encaixe na mão e apaga o voxel.

| arquivo | tamanho (Y × Z × X) | detalhes |
|---|---|---|
| `espada.vox` | 26 × 7 × 3 — cabo 6, guarda 1 (7 de largura), lâmina 19 | marcador no meio do cabo |
| `escudo.vox` | 14 × 14 × 3 | face pra fora (−X, vai na mão esquerda); marcador na alça de trás |
| `katana.vox` | 30 × 3 × 2 — cabo 8, lâmina 22 | marcador no meio do cabo |
| `pistola.vox` | 11 × 6 × 2 — pega vertical, cano pra +Y | marcador no meio da pega; **o mesmo modelo serve nas duas mãos** |

As armas são **1,25× o tamanho real** de propósito: de cima e numa tela de
celular, arma em tamanho real vira um risco. Frisos de tier em 241–244.

**O anel não se modela** — de 27°–75° de altura ele é um voxel. A proposta é
um brilho ou runa sobre a mão feito em código (ABERTO em `PERSONAGEM.md`).

## O que NÃO modelar

- **Acessórios** (brinco, amuleto, bracelete, cinto): não aparecem no corpo.
- **Criaturas e montarias**: vêm do estoque do zone14, já em peças.

## As entregas, em ordem

| # | arquivos | destrava |
|---|---|---|
| 1 | `corpo.vox` — **feito: o piratinha** | o rig com o corpo de verdade |
| 2 | `rosto_01`, `cabelo_01` — **`cabelo_01` feito: o tricórnio**; o rosto do pirata mora no `corpo.vox` | a cabeça |
| 3 | `espada`, `escudo`, `manto_guerreiro` | o primeiro conjunto e o primeiro ataque |
| 4 | `katana`, `bainha`, `pistola`, `coldre`, `manto_mago` | os outros três conjuntos |
| 5 | `armadura_leve`, `_media`, `_pesada` | o peso da armadura |
| 6 | `rosto_02–04`, `cabelo_02–06` | a criação de personagem |

**22 arquivos no total.** Onde salvar: `assets/vox/personagem/`.

## O primeiro personagem: o piratinha

`assets/vox/personagem/corpo.vox` + `cabelo_01.vox`, gerados por
`tools/voxrender/pirata.py`. O `pirate_captain.vox` de antes era uma peça só
em pose T, mais fino e de cabeça menor que esta ficha (tronco 8×4, pernas 3×3,
cabeça 1:6) — copiar voxel a voxel quebraria as juntas. Então ele foi
**refeito nestas medidas, mantendo o que faz ele ser o pirata**: a mesma
paleta, camisa branca de costas cinza, faixa vermelha, cinto de fivela
dourada, calça preta, bota marrom, tapa-olho, um olho azul, barba emoldurando
o rosto, rabicho com fita vermelha — e o tricórnio preto de aba dourada com a
pena rosa.

Duas mudanças em relação ao original, as duas pela câmera de cima:

- **O tricórnio é um triângulo visto de cima** (ponta na frente, dois cantos
  atrás), com a aba virada pra cima nos três lados e não nos cantos. Aba
  quadrada virada em volta toda lia como moldura de quadro.
- **Cabelo e barba num cinza mais claro** — com o (60,60,60) original debaixo
  do chapéu preto, a cabeça virava um bloco escuro com uma janelinha de rosto.
  Fica na faixa do cabelo, então trocar custa zero.

## Como conferir

Até o importador de peças existir (etapa 1 de `PERSONAGEM.md`), o jeito de ver
é o render de linha de comando:

```sh
python3 tools/voxrender/voxrender.py assets/vox/personagem/corpo.vox -o corpo.png -d 4 -s 6
```

Quando o importador entrar, ele confere sozinho o que dá pra conferir: nome de
objeto desconhecido, tela de tamanho diferente, voxel fora da faixa da peça,
marcador faltando na arma — e diz qual arquivo e qual objeto.

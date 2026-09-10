# O personagem: base, animação e o que define as duas

> Documento de decisão. O que está marcado **PROPOSTA** ainda não foi
> implementado; o que está marcado **JÁ É** está no código e restringe o resto.

Animar antes de decidir isto é construir em cima do que ainda vai mudar. E o
código já decidiu mais coisa do que parece.

## O que já está decidido (e não é opinião)

**JÁ É — classe é a ARMA.** Não existe classe separada. O banco tem 64 skills
em **8 árvores de proficiência**, uma por arma: Espada, Machado, Lança, Adaga,
Arco, Cajado, Varinha, Desarmado — 8 cada. A skill é liberada por
`usable_with`, uma lista de armas. Trocar de arma é trocar de classe, e é isso
que o MIR4 faz.

**JÁ É — 13 slots de equipamento** (arma, offhand, elmo, peito, pernas, botas,
luvas, cinto, capa, colar, anel…) mais 4 dedicados a ferramenta (machado,
foice, picareta, vara), todas equipáveis ao mesmo tempo.

**JÁ É — 20 das 64 skills são passivas.** Elas nunca animam. Das 44 ativas, a
forma se repete: 16 círculo, 9 cone, 7 projétil, 6 linha, 6 em si mesmo.

**JÁ É — o corpo tem 0,35 de raio e 1,68 de altura**, a câmera olha de 27° a
75°, e o alvo é celular. Isso é o orçamento: o `player.vox` tem 356 triângulos,
o `lobo.vox` tem 14.112 — o lobo é modelo de CHEFE e mob comum precisa passar
pelo `voxsimplify.py`.

**JÁ É — montaria hoje é barco.** `Mounted` aponta pra uma entidade de barco,
com estações (leme, canhão) e deck andável. Montaria terrestre não existe.

**MORREU — `VisualConfig`.** Ela descreve um paper-doll 2D: `skin_race`,
`outfit`, `hair`, `hat`, tudo em nome de folha de sprite do cliente Unity. Não
serve pro voxel e precisa ser substituída, não adaptada.

## A decisão que trava todas as outras: o corpo é feito de PEÇAS

Voxel não anima por deformação de malha — anima por **peça rígida**. Então o
personagem precisa nascer partido, e essa é a mudança de base:

```
cabeça · torso · braço-E · braço-D · perna-E · perna-D
        + mão-D (arma)  + mão-E (offhand/escudo)
```

Oito peças, cada uma com seu pivô. Hoje o `player.vox` é uma peça só.

O custo é conhecido: a transformação já existe (`draw_mesh_at` gira uma malha
inteira na CPU por entidade). Passar de 1 para 8 matrizes por corpo não muda o
número de vértices — muda o número de multiplicações, e 356 triângulos por
personagem cabem. **O que não cabe é mob de 14 mil triângulos animado.**

## PROPOSTA — locomoção é procedural, ataque é autoral

Fazer 38 clipes à mão para voxel é trabalho que não termina. E não precisa:

**Procedural (custo de arte zero):** parado, andar, correr, pulo (subida, topo,
queda, pouso), subir degrau, cair derrubado. São funções de seno nas pernas e
braços, com a fase saindo da velocidade que o servidor já manda. Nunca
dessincroniza da velocidade — que é o defeito clássico de clipe gravado — e
sai de graça para todo modelo que tenha as oito peças.

**Autoral (poses-chave, não quadros):** os ataques. Três poses por gesto,
interpoladas. E o gesto não é por skill, é por **forma** — que o banco já tem:

| forma | quantas | gesto |
|---|---|---|
| `aoe_circle` | 16 | giro em volta / batida no chão |
| `cone` | 9 | golpe largo à frente |
| `projectile` | 7 | arremesso / disparo |
| `line` | 6 | estocada / investida |
| `self` | 6 | levantar a arma (buff) |

Cinco gestos, não sessenta e quatro. A diferença entre um machado e uma espada
no mesmo gesto é a **velocidade** e o modelo na mão, não uma animação nova.

Mais o **combo básico de três passos** por família de arma, que é o que o
jogador vê o tempo todo. Seis famílias:

| família | armas | leitura |
|---|---|---|
| lâmina | Espada, Adaga | rápido, três golpes encadeados |
| pesada | Machado, Montante | lento, wind-up visível, dá pra reagir |
| haste | Lança | estocada, alcance maior |
| arco | Arco | sacar, mirar, soltar |
| foco | Cajado, Varinha | sem contato, gesto de conjuração |
| desarmado | — | soco, soco, chute |

**Total autoral: 5 gestos + 6 combos × 3 poses = 23 poses.** Contra 38 clipes
completos.

## PROPOSTA — o que o protocolo precisa

Hoje `EntityState` leva 13 bytes e um byte de bandeiras com **3 bits livres**
(`SELF`, `DOWNED`, `CASTING`, `BOSS`, `PULANDO` ocupam cinco). Animação precisa
de mais estado que isso: atacando + qual passo do combo, guardando, coletando,
montado, qual gesto de skill.

A proposta é um byte novo, `acao`: 4 bits de estado e 4 de variante (passo do
combo, gesto da skill). Custo: 14 bytes em vez de 13, e só de quem muda — no
pior caso 1,8 KB/s por jogador sobre os 12,8 KB/s medidos hoje.

**Nada disso vira decisão de jogo no cliente.** O servidor já sabe quando
alguém ataca, conjura e defende; o byte só transporta o que ele decidiu.

## DECIDIDO — a arma tem duas identidades

Montante e Espada+Escudo são **variações**, não árvores. Isso já é o que o
código faz: `Proficiency::from_item` manda `GREAT_SWORD` para a árvore da
Espada, e "espada e escudo" nunca foi uma arma — é espada com um offhand.

Mas a variação muda a **animação**, e não a skill. Então a arma passa a ter
dois campos, e eles não são o mesmo:

| | o que decide | exemplo |
|---|---|---|
| **árvore** | quais skills você pode usar | montante → Espada |
| **família** | como o corpo se move | montante → pesada |

Uma espada de uma mão e um montante compartilham as oito skills e não
compartilham um único quadro de animação. Hoje só a árvore existe em código
(`from_item`, por faixa de id); a família precisa do mesmo tipo de mapa.

## DECIDIDO — todo mundo tem arma E secundária

Nunca uma mão vazia. O que muda de arma pra arma é **onde** a secundária fica,
e isso é rig e não regra:

| família | primária | secundária | onde |
|---|---|---|---|
| lâmina | espada, adaga | escudo, adaga | mão esquerda |
| pesada | machado, montante | bainha, contrapeso | **costas** |
| haste | lança | bainha | **costas** |
| arco | arco | aljava | **costas** |
| foco | varinha | grimório, orbe | mão esquerda |
| foco 2M | cajado | bainha de pergaminho | **costas** |
| desarmado | — | manopla | mão esquerda |

O rig ganha um ponto de encaixe a mais: além de `mão-D` e `mão-E`, um
**`costas`**. Arma de duas mãos ocupa as duas mãos e joga a secundária pras
costas; arma de uma mão deixa a esquerda livre pra secundária de mão.

**O risco que isso abre.** Se o escudo for a única secundária que faz alguma
coisa, todo mundo carrega escudo e o slot vira escolha falsa. Hoje o offhand
só tem número de escudo (defesa e vida). Cada secundária precisa de um motivo
próprio pra existir — bainha dá velocidade de saque, aljava dá alcance ou
munição, grimório dá mana ou tempo de conjuração — senão a resposta é sempre a
mesma e a decisão não é decisão.

## DECIDIDO — montaria terrestre vai existir

O encanamento já serve: `Mounted` aponta pra uma ENTIDADE, e barco é só um
tipo dela. Montaria terrestre entra pelo mesmo caminho.

O que ela acrescenta em animação:

* **no personagem**, uma pose só — sentado, pernas paradas, tronco
  acompanhando o trote. Não é um conjunto novo;
* **na montaria**, um ciclo de locomoção próprio. Se ela for de quatro patas,
  é o mesmo procedural das duas pernas com quatro fases em vez de duas —
  custo de arte zero de novo;
* **montar e desmontar**: duas poses de transição, ou nenhuma se o corte for
  seco (aceitável no começo).

## O que ainda precisa da sua decisão

1. **Aparência do personagem** — em discussão separada, a seu pedido. A
   `VisualConfig` morreu com o cliente 2D e precisa ser substituída; o que
   entra no lugar depende de quanto do equipamento aparece no corpo.
2. **Correr é um estado?** Existe `SPRINT_SPEED_MULT` e o bit `SPRINT` no
   protocolo, mas nada no cliente. Se corrida existe, é mais um ciclo de
   locomoção — que sai de graça no procedural.
3. **Cada secundária faz o quê?** Ver o risco acima. Não trava a animação,
   trava o balanceamento.

# Combate: o desenho do playtest

> Substitui o sistema anterior por inteiro. O que havia — 8 árvores de
> proficiência, 64 skills, offhand livre, arma de duas mãos, build procedural
> de mob — foi descartado. Ver `docs/PERSONAGEM.md` para o rig e a animação.

## A arma é um CONJUNTO, não uma peça

Não existe mais offhand livre nem "duas mãos". A arma é o par inteiro, e a
secundária vem amarrada a ela:

| arma (principal) | secundária | leitura |
|---|---|---|
| **espada e escudo** | manto do guerreiro | linha de frente |
| **katana** | bainha | corte rápido, saque |
| **duas pistolas** | coldre | à distância, pirata |
| **anel mágico** | manto do mago | cura e magia |

Quatro no playtest; outras entram depois.

**Isso resolve sozinho um risco que o offhand livre criava.** Com o offhand
solto, se o escudo fosse a única secundária com número, todo mundo carregaria
escudo e o slot viraria escolha falsa. Amarrando a secundária à arma, a escolha
sai do slot e vai pro conjunto — que é onde ela tem consequência de verdade.

O **anel mágico** no lugar da varinha é decisão de tema: num mundo de
navegação e ilhas, quem cura não anda com um graveto na mão.

## Armadura tem PESO, e o peso é a escolha

Três pesos, e cada um é uma troca declarada:

| peso | dá | cobra |
|---|---|---|
| **leve** | bônus de dano | pouca resistência |
| **média** | equilíbrio | equilíbrio |
| **pesada** | resistência | menos dano |

A armadura deixa de ser "número maior é melhor" e passa a ser uma posição no
eixo dano↔resistência. Combinada com a arma, é ela que faz duas pessoas com a
mesma espada jogarem diferente.

A secundária **é item de verdade**: tem tier e atributo próprios, e melhora
separada da principal. Não é enfeite da arma.

## Acessórios

Quatro, iguais pra todo mundo: **brinco, amuleto, bracelete, cinto**. Não há
variação por classe — o que muda entre dois jogadores é o tier e o que o item
rolou, não a lista de slots.

## Skills

**Três por conjunto de arma, todas ATIVAS.** Doze no playtest.

Passiva não existe mais. Das 64 antigas, 20 eram passivas — número que sobe sem
nada acontecer na tela. Isso importa duplamente aqui: passiva não tem
animação, não tem leitura, e num jogo de vista alta o que o outro jogador vê
você fazer é metade do combate.

Doze ativas é o que dimensiona a arte: cada uma precisa de um gesto, e o
documento do personagem prevê pose-chave por FORMA (cone, círculo, projétil,
linha, em si mesmo). Com doze, o pior caso é doze gestos; o provável é bem
menos, porque formas se repetem entre conjuntos.

## O que isso apaga

* as 8 proficiências de arma (Espada, Machado, Lança, Adaga, Arco, Cajado,
  Varinha, Desarmado) — viram as do conjunto novo;
* `usable_with` por nome de arma nas skills;
* o slot `offhand` como escolha livre;
* arma de duas mãos como categoria;
* o build procedural de mob — **já foi**, ver commit anterior.

## Proficiência

Uma árvore por conjunto de arma — quatro. Trocar de conjunto continua sendo
trocar de classe.

## O que ainda falta decidir

1. **O peso da armadura é por PEÇA ou por conjunto?** Misturar peito pesado com
   botas leves dá mais escolha; travar o conjunto dá leitura visual imediata —
   num jogo de câmera alta, dá pra saber o que o sujeito é olhando de longe.
2. **Os números.** Quanto a armadura leve dá de dano e tira de resistência, o
   que cada skill custa e cobra de espera. Isso é balanceamento e não trava a
   estrutura — mas trava o playtest.

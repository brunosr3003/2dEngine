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

## Acessórios

Quatro, únicos, com tiers.

## O que isso apaga

* as 8 proficiências de arma (Espada, Machado, Lança, Adaga, Arco, Cajado,
  Varinha, Desarmado) — viram as do conjunto novo;
* `usable_with` por nome de arma nas skills;
* o slot `offhand` como escolha livre;
* arma de duas mãos como categoria;
* o build procedural de mob — **já foi**, ver commit anterior.

## O que ainda falta decidir

1. **Quantas árvores de proficiência?** Uma por conjunto de arma (quatro), ou
   proficiência deixa de existir também?
2. **A secundária tem número próprio?** Ela é item com tier e atributo (a
   bainha melhora, o coldre melhora), ou é a identidade visual da arma e o
   número todo está na principal?
3. **O peso da armadura é por PEÇA ou por conjunto?** Misturar peito pesado com
   botas leves dá mais escolha; travar o conjunto dá leitura visual imediata —
   dá pra saber o que o sujeito é olhando de longe.
4. **Quais são os quatro acessórios**, e "único" quer dizer um por slot ou um
   no mundo inteiro?
5. **As skills.** Cada conjunto tem as suas? Quantas? É isso que decide o
   tamanho da animação — o documento do personagem previu 5 gestos por forma,
   e a forma vinha do banco antigo.

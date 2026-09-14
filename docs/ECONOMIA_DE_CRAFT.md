# Economia de craft: poucos recursos girando muito

> Herdado do MIR4 e adaptado. O ponto do desenho é que a lista de recursos é
> **curta de propósito**: arma e sub-arma gastam o mesmo tipo de material,
> armaduras diferentes gastam o mesmo entre si, e acessórios idem. Poucos
> nomes girando muito é o que deixa um item ter preço; uma lista longa vira
> ruído que ninguém consegue precificar.

## As três receitas

Cada craft pede **a chave** (1 unidade, e é ela que decide quantos itens saem
do mundo), **três materiais na cor do item desejado**, e mais darksteel e
cobre em quantidade que varia com o **nível** do item.

| | chave (1) | 300 | 100 | 100 |
|---|---|---|---|---|
| **arma** | Escama | Aço | Pedra do Coração Negro | Pedra Sombra-da-Lua |
| **sub-arma** | Garra | Aço | Pedra do Coração Negro | Pedra Sombra-da-Lua |
| **armadura** | Couro | Aço | Quintessência | Berloque de Exorcismo |
| **acessório** | Chifre | Platina | Fragmento Iluminante | Pedra de Ânima |

Tudo **na cor do item que se quer**: arma verde pede material verde inteiro.

Isso deixa só **onze materiais** na economia — oito coloridos, mais cobre,
darksteel e pó cintilante — e uma chave por família. Arma e sub-arma
compartilham a linha de baixo inteira; só a chave muda.

## Onde tudo isso sai

A **pedra** é uma fonte de materiais; os mobs também fornecem cobre,
materiais e poucas poções conforme [Loot dos mobs](LOOT_DOS_MOBS.md).
Mobs e chefes não dropam equipamentos. Ver `docs/COLETA.md` para como a pedra funciona; aqui importa
só a proporção, e ela segue o custo — o que a receita pede em 300 tem que
cair mais que o que ela pede em 100, senão o gargalo muda de lugar sozinho:

| material | por coleta | chance |
|---|---|---|
| Cobre | 40–120 | sempre |
| Aço (serve arma **e** armadura) | 3–6 | 55% |
| Darksteel | 10–25 | 35% |
| Platina | 3–6 | 30% |
| os seis de 100 | 2–4 | 12% cada |
| Pó Cintilante | 1 | 3% |
| Escama / Garra / Chifre / Couro | 1 | 1% cada |

A chave a 1% é o regulador real: material sobra, chave falta, e é ela que
decide quantos itens o mundo produz por hora.

## A cor do material: sobe por síntese, não por drop

**A pedra roxa não dá material roxo.** Ela dá muito cinza, um pouco de verde
e um pouco mais de azul que a pedra azul — o que ela compra é *tempo de
coleta* (128 coletas contra 14 da cinza), não uma cor nova.

Então de onde vem o roxo? De **subir a cor**, que é para isso que existe o Pó
Cintilante. No MIR4 a conta é **10 para 1** e ela está medida (constantes
lidas do `EpicMaterialCalculator`, que reproduz o jogo):

| passo | material | cobre | darksteel | pó cintilante |
|---|---|---|---|---|
| cinza → verde | 10 | 2.000 | 1.000 | 2 |
| verde → azul | 10 | 20.000 | 5.000 | 25 |
| azul → roxo | 10 | *(extrapolar: ~200.000)* | *(~25.000)* | *(~300)* |

O MIR4 público só documenta os dois primeiros degraus (uncommon→rare,
rare→epic). O terceiro segue o mesmo formato — cobre ×10, darksteel ×5, pó
×12 por degrau — mas o número é **nosso**, não medido.

Dez para um em cada degrau significa **100 cinzas por azul** e **1.000 por
roxo**, antes do cobre. É essa curva que faz a pedra roxa valer a subida: ela
não dá roxo, dá o azul em volume suficiente pra a síntese ser viável.

## A troca por equipamento antigo

> Correção do que se supunha antes: **não** é "dois itens tier IV viram um
> upgrade de cor".

A regra real do MIR4 é outra, e é sobre **equipamento**, não sobre material:

> *"When crafting a Rare or higher equipment, tier 4 & enhancement level 8+
> equipment of the previous grade can replace key materials."*

Ou seja: para craftar um item de cor superior, um item **tier 4 e +8** da cor
anterior **substitui a chave** (a Escama, a Garra, o Couro, o Chifre). É o que
dá saída ao equipamento velho: em vez de virar lixo quando a cor sobe, ele
entra como ingrediente do próximo — desde que tenha sido levado até o topo do
refino.

É por aí que o `+1/+2/+3…` entra na economia: o refino deixa de ser só poder
e vira **matéria-prima de progressão**.

## O que o código faz hoje

- **As 60 receitas existem** (`shared::receitas`): 15 peças (ids 400–414) × 4
  cores, ids de receita `1000 + faixa×100 + peça`. Cada uma pede os seis
  ingredientes da tabela acima — a quantidade é **nossa, de play test**, não a
  do MIR4:

  | cor | grau que sai | nível mín. | chave | principal | cada secundário | darksteel | cobre |
  |---|---|---|---|---|---|---|---|
  | cinza | Comum | 1 | 1 | 30 | 10 | 200 | 300 |
  | verde | Fino | 15 | 1 | 90 | 30 | 1.500 | 2.000 |
  | azul | Raro | 30 | 1 | 300 | 100 | 8.000 | 10.000 |
  | roxo | Épico | 60 | 1 | 300 | 100 | 60.000 | 50.000 |

  O grau sai da cor (o nível da instância rolada cai no tier certo) e o nível
  mínimo é validado no servidor: **nível 20 não cria Épico** — só a partir do
  60 (docs/DUNGEONS_E_RAIDS.md).
- **Banco:** o `recipes` semeia as receitas em `craft_recipes` (coluna nova
  `nivel_min`) com `ON CONFLICT DO NOTHING` — ajuste manual fica. A M27 não
  apaga mais ids 1000+.
- **Servidor:** `craft::conferir` (nível, cada ingrediente, espaço) e
  `craft::aplicar` (consome e cria com instância rolada), e a resposta
  `CraftResultado` diz o motivo da recusa ("faltam: Aço 12/30").
- **Cliente:** painel de Craft (abas Arma/Secundária/Armadura/Acessório/Barco,
  ingredientes com tem/precisa, botão Criar), aberto pelo HUD — sem tecla.
- **Material roxo continua sem fonte** até a síntese de cor, então a receita
  Épica existe mas ainda não se cumpre.

## O que ainda não existe no código

- **A síntese de cor** (10 → 1 com pó cintilante) não está implementada.
- **A troca por equipamento +8** depende do refino, que existe em
  `shared/forja.rs` mas ainda não conversa com o craft.
- **Material roxo não tem fonte nenhuma hoje** — é consequência correta do
  desenho (a pedra roxa não dá roxo), mas só deixa de ser um beco quando a
  síntese de cor entrar.

Fontes da pesquisa: [guia de craft do MIR4](https://gameplay.tips/guides/mir4-definitive-craft-system-guide.html),
[EpicMaterialCalculator](https://github.com/cdrakke/EpicMaterialCalculator),
[MIR4 Wiki — Glittering Powder](https://www.mir4.wiki/wiki/Glittering_Powder).

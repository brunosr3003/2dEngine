# Economia

Estrutura do MIR4 ponta a ponta, **sem escalar os valores**. O que resolve o
tamanho não é baratear o jogo — é a **colônia offline**, que troca "106 horas
de mão" por "18 dias de calendário" sem tocar em nenhuma curva.

Números de refino, craft e mineração: `docs/ITENS.md`.

## As duas torneiras e os três ralos

```
torneiras                        ralos
  mineração (ativa)                darksteel consumido no refino
  colônia offline (passiva)        item destruído acima do +5
                                   gold queimado na taxa do mercado
```

Três ralos contra duas torneiras. A maioria dos MMOs quebra pelo contrário —
torneira demais e ralo de menos — e a inflação come o jogo em seis meses.

## Colônia offline

O jogador coloniza uma ilhota, ou cria uma no mar. Ela coleta sozinha enquanto
ele não está.

| parâmetro | valor | por quê |
|---|---|---|
| taxa | **25% da mineração ativa** | 85.200 DS/dia = 6h de mineração ativa de graça |
| teto de estoque | **12 horas** | é o botão de retenção — ver abaixo |
| o que rende | **só o recurso comum** (darksteel) | insumo raro só de chefe, raid e nó alto |

Com 25%, a curva do MIR4 fica intacta e o calendário fica humano:

```
             MIR4 na mão      com colônia (dias de calendário)
Raro +7           106h                 18d
Épico +7        1.063h                177d
Raro +9         7.154h              1.192d
```

E mineração ativa continua rendendo **quatro vezes mais por hora** — quem joga
anda mais rápido, ela só deixa de ser obrigatória.

### O teto é o botão de retenção

```
teto     1 visita/dia   2 visitas/dia   1 visita/semana
 12h        1.278k          2.556k            183k
 24h        2.556k          2.556k            366k
```

Com 24h, **checar duas vezes por dia não rende nada a mais** — a ilha enche e
para. Com 12h, quem abre de manhã e à noite ganha o dobro, e quem some uma
semana ainda leva 183k em vez de zero. 8h exigiria três visitas diárias, o que
vira cobrança.

### Duas regras de implementação que não são negociáveis

**Coleta offline NUNCA é simulada.** É conta na hora do login:

```
recurso = taxa × min(agora − última_visita, teto)
```

A versão ingênua — cada ilha tiquetaqueando — seria um segundo mundo pra
simular, **maior que o primeiro**, e incluiria as ilhas de todo mundo que
largou o jogo. Custo tem que ser zero enquanto ninguém está lá.

**A ilha sai do gerador que já existe.** `semente = hash(id_do_personagem)` no
`Ilha::gerar`: cada jogador tem a sua, determinística, zero de armazenamento,
materializada só quando ele visita. "Colonizar uma ilhota" e "criar uma no
mar" viram a mesma coisa — escolher uma semente e uma posição. A
funcionalidade custa quase nada porque o terreno já faz o trabalho pesado.

### O que ela conserta de quebra

O mar entre as ilhas hoje é cenário. Com colônia ele vira destino: navegar até
a sua, e mais tarde ver a dos outros. Barco deixa de ser transporte e vira
conteúdo.

## Mercado

Modelo do MIR4: **quase nada é comerciável**, só existe **venda** (não troca),
e a venda paga **taxa em gold**.

### Vinculado é o padrão

Todo recurso nasce vinculado à conta. Vendável é exceção, marcada peça a peça.

O modo de falhar é assimétrico e é por isso que o padrão importa: um item que
virou vendável por acidente **não tem volta** depois que os jogadores acharam;
um vendável que ficou preso por engano é uma linha de correção.

### É isto que mata multi-conta

Multi-conta só rende se o recurso **atravessa** para a principal. Com vínculo,
vinte alts produzem vinte pilhas que não se juntam — e não é preciso detectar
nada, provar nada nem banir ninguém. **Sistema anti-trapaça que não existe é o
único sem falso positivo.**

O auto-negócio também não fecha: o alt vende, ganha gold, e o gold fica no
alt. Pra tirar de lá, a principal teria que comprar a listagem com o gold
dela — o valor anda pro alt, não pra principal, e a taxa come uma fatia no
caminho. Negociar consigo mesmo dá prejuízo.

### A taxa sai do valor da venda, não do bolso

Cobrada adiantado, o jogador novo sem gold **nunca começa a vender**, e o
mercado vira recurso de quem já pagou. Descontando do que ele recebe, o mesmo
gold é queimado e a porta fica aberta.

### Vender o que é irregular, vincular o que é constante

É o critério pra escolher os ~10% comerciáveis. Darksteel todo mundo consegue
no mesmo ritmo — no mercado viraria só um índice de preço. Merece mercado o
que **sai torto**: o material raro que um jogador tirou três e o outro nenhum
na mesma semana.

**Mercado bom conserta distribuição desigual; não converte tempo em dinheiro.**

## TC — Tempest Coin, a moeda premium

**DECIDIDO:** a moeda premium do jogo se chama **TC (Tempest Coin)**. É com ela
que se compram as **skins** — de armadura e de arma (ver `docs/PERSONAGEM.md`)
— e a skin comprada fica **presa ao personagem**: comprar pra um não libera
pros outros.

É o "vinculado é o padrão" levado um passo além: a skin nem chega a ser da
conta.

### Por que ela fica fora das torneiras e dos ralos

TC não aparece no diagrama lá de cima porque não toca o gold. Duas coisas
garantem isso, e as duas já estão decididas:

- **skin não é vendável** — presa ao personagem, ela não entra no mercado;
- **skin não dá número** — a aparência não muda o combate. O tier continua
  visível pela cor dos frisos, que sai do item EQUIPADO, não da skin.

Enquanto as duas forem verdade, dinheiro de verdade não vira gold nem vira
poder, e a economia de cima não precisa ser rebalanceada por causa da loja.
**Qualquer coisa nova que a TC venha a comprar tem que passar pelo mesmo
teste** — não vira gold, não vira número —, senão ela vira a terceira
torneira e nenhum ralo dá conta.

### O que ainda falta decidir sobre ela

- **O saldo de TC fica na conta ou no personagem?** Proposta: na conta — é a
  conta que paga —, e a skin se prende ao personagem na hora da compra.
- **TC pode ser presenteada ou negociada?** Proposta: não. TC negociável é a
  porta pra vender gold por dinheiro por fora do jogo.
- **TC compra mais alguma coisa além de skin?** Cada item novo passa pelo
  teste acima.

## O que falta decidir

- Quais peças exatamente entram nos ~10% vendáveis.
- Nível mínimo (ou progresso de main quest) pra abrir a colônia — é a defesa
  secundária contra alt descartável, caso o vínculo não baste.
- Se a colônia tem melhorias (mais taxa, mais teto) e o que elas custam.

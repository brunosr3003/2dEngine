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
                                   taxa da troca TC↔gold
  dinheiro de verdade → TC         (não cria gold: a troca só MOVE gold)
```

A maioria dos MMOs quebra por torneira demais e ralo de menos, e a inflação
come o jogo em seis meses. A TC entra por dinheiro de verdade, mas **não cria
gold nenhum**: ela só compra gold que outro jogador já tinha farmado.

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
e a venda é em **gold**, com **taxa em gold**. A exceção é a TC, que se vende
por gold — ver a seção dela.

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

O auto-negócio também não fecha: o alt vende e o gold fica no alt. O único
caminho pra levar esse gold pra principal é o próprio mercado — a principal
anuncia qualquer coisa e o alt compra caro —, e a taxa come uma fatia a cada
passagem. **A taxa é a alavanca contra multi-conta:** quanto maior, mais caro
lavar gold de um personagem pro outro.

### A taxa sai do valor da venda, não do bolso

Cobrada adiantado, o jogador novo sem gold **nunca começa a vender**, e o
mercado vira recurso de quem já tem. Descontando do que ele recebe, o mesmo
gold é queimado e a porta fica aberta.

### Vender o que é irregular, vincular o que é constante

É o critério pra escolher os ~10% comerciáveis. Darksteel todo mundo consegue
no mesmo ritmo — no mercado viraria só um índice de preço. Merece mercado o
que **sai torto**: o material raro que um jogador tirou três e o outro nenhum
na mesma semana.

**Mercado bom conserta distribuição desigual.** O único lugar onde tempo e
dinheiro se encontram é a troca TC↔gold — e ela não cria nada, só move.

## TC — Tempest Coin

**DECIDIDO:**

- **TC é da CONTA e vale em todos os servidores.**
- **Por enquanto, TC só entra comprando com dinheiro.**
- **TC compra na loja de cash** — as skins de armadura e de arma, que ficam
  presas ao personagem (ver `docs/PERSONAGEM.md`).
- **TC se vende no mercado por gold.** É assim que quem paga consegue gold, e
  quem não paga consegue TC. **Todo o resto do mercado é em gold: o gold é a
  moeda principal do jogo.**

É o modelo do WoW Token, das gemas do Guild Wars 2 e do PLEX do EVE.

### O ciclo

```
quem paga:   dinheiro ─► TC ─► vende a TC por gold ─► compra item em gold
quem farma:  farma ─► vende item por gold ─► compra TC com gold ─► loja de cash
```

### Por que assim, e não TC como moeda do mercado

A versão anterior punha a TC como moeda do mercado inteiro. Esta é melhor em
quatro pontos:

- **O gold tem ralo e tem demanda.** A taxa do mercado volta a queimar gold, e
  comprar TC dá a ele um uso que não acaba.
- **A troca não cria gold.** Dinheiro de verdade só compra gold que alguém
  farmou. Nenhuma torneira nova.
- **A TC não volta a virar dinheiro dentro do jogo.** Farmar rende skin, não
  saque. Bot ainda ganha alguma coisa (gold → TC → skin), mas muito menos do
  que ganharia vendendo item direto por TC.
- **O preço da TC em gold é o termômetro da economia.** Subindo, ou o gold
  está inflacionando ou a TC está escassa. Vai pro panóptico, na aba de
  economia.

### Como funciona por baixo

A TC mora na CONTA (global) e o gold no PERSONAGEM (banco do realm). Por isso:

- **A troca TC↔gold é por realm** — o gold de um realm só existe no banco dele.
- **Cadastro de conta único e livro-caixa central**, fora dos `DATABASE_URL`
  dos realms (hoje a tabela `accounts` mora dentro de cada realm; isso muda).
  Como é dinheiro de verdade, cada movimento de TC é uma linha de razão (quem,
  quanto, por quê, saldo depois) — nunca um saldo sobrescrito.
- **Custódia, nunca "debita dos dois e torce".** Ao anunciar TC, ela sai do
  livro central na hora e fica guardada; quem compra paga o gold no realm;
  só então a TC é creditada a ele. Dois bancos diferentes não têm transação
  comum — a custódia é o que garante que TC nunca some nem duplica no meio.

### O que ainda falta decidir sobre ela

- **A taxa da troca TC↔gold** — cobrada em gold (mais um ralo de gold) ou em TC
  (queima TC)? E quanto.
- **A loja de cash vende gold direto?** Recomendo que não: gold criado pela
  loja é torneira comprada — inflação e pay-to-win ao mesmo tempo. Gold só
  pela troca entre jogadores.
- **A taxa do mercado de itens.**
- **Portão pra vender** (nível, progresso de história) contra conta
  descartável.
- A TC um dia entra por outro caminho além da compra (evento, conquista)?

## O que falta decidir

- Quais peças exatamente entram nos ~10% vendáveis.
- Nível mínimo (ou progresso de main quest) pra abrir a colônia — é a defesa
  secundária contra alt descartável, caso o vínculo não baste.
- Se a colônia tem melhorias (mais taxa, mais teto) e o que elas custam.

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
  dinheiro de verdade → TC         TC queimada na taxa do mercado
                                   (gold: SEM ralo — ver "TC", item 1)
```

A maioria dos MMOs quebra por torneira demais e ralo de menos, e a inflação
come o jogo em seis meses. O mercado passou a ser em TC, e isso mexeu no
diagrama em dois pontos: a taxa agora queima TC, e **o gold perdeu o único
ralo que tinha**.

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
e a venda é em **TC**, com taxa em TC.

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

Com a TC na conta, o alt que vende recebe TC na conta DELE. O único caminho
pra levar isso pra principal é o próprio mercado: a principal põe um item
qualquer à venda e o alt compra caro — e a taxa come uma fatia a cada
passagem. **A taxa virou a alavanca contra multi-conta:** quanto maior, mais
caro lavar TC de uma conta pra outra.

### A taxa sai do valor da venda, não do bolso

Cobrada adiantado, o jogador que não paga — e que não tem TC nenhuma, porque
ela só entra comprando — **nunca começaria a vender**, e o mercado viraria só
de quem já pagou. Descontando do que ele recebe, a mesma TC é queimada e a
porta fica aberta. Com a TC, essa regra deixou de ser detalhe: é ela que
deixa quem não paga participar da economia.

### Vender o que é irregular, vincular o que é constante

É o critério pra escolher os ~10% comerciáveis. Darksteel todo mundo consegue
no mesmo ritmo — no mercado viraria só um índice de preço. Merece mercado o
que **sai torto**: o material raro que um jogador tirou três e o outro nenhum
na mesma semana.

**Com a TC, o mercado converte tempo em dinheiro — de propósito.** É o modelo
do MIR4 e do Lineage W: quem paga compra TC, quem farma vende item por TC. Isso
deixou de ser efeito a evitar e virou o desenho — e muda o que o vínculo
protege: não é mais só a economia do jogo, é dinheiro de verdade.

## TC — Tempest Coin

**DECIDIDO:**

- **TC é da CONTA e vale em todos os servidores.** Um saldo só, em qualquer
  realm.
- **TC é a moeda do mercado.** Todo item negociado no mercado é negociado em
  TC — ela é a base da economia entre jogadores.
- **Por enquanto, TC só entra comprando com dinheiro.** Quem não paga
  consegue TC vendendo no mercado pra quem pagou.
- **Skins** de armadura e de arma são compradas com TC e ficam presas ao
  personagem (ver `docs/PERSONAGEM.md`).

### O ciclo

```
dinheiro ──► TC ──► compra item no mercado ──► TC vai pro vendedor
                                               (menos a taxa, que QUEIMA)
                    vendedor gasta a TC em skin ou em item
```

Quem paga compra tempo; quem tem tempo vende o que farmou. A taxa é o único
ralo de TC — é ela que impede a massa de TC em circulação de só crescer.

### O que isso muda, e precisa de resposta

**1. O gold ficou sem ralo.** A taxa do mercado era o único ralo de gold do
diagrama; os outros dois gastam darksteel e item. O gold continua entrando
(drop, coleta) e não sai mais por lugar nenhum — é inflação de gold garantida.
Precisa de um ralo novo (conserto, custo em gold no refino ou no craft,
serviço de NPC), ou o gold deixa de existir como moeda.

**2. Farmar passou a render dinheiro, e isso atrai bot.** Antes, multi-conta
não rendia porque o recurso não atravessava de uma conta pra outra. Agora
atravessa — pelo próprio mercado, que é pra isso que ele existe. Foi o que
encheu o MIR4 de fazenda de bot. As defesas passam a ser três: o **vínculo**
(o que NÃO pode ser vendido continua sendo a maioria), a **taxa** (o custo de
lavar TC entre contas) e, se precisar, um **portão pra vender** (nível,
progresso de história).

**3. TC global exige um livro-caixa fora dos realms.** Hoje cada realm tem
banco próprio e a conta mora nele — o `merge-realms.sh` até trata conta em
colisão. Um saldo que vale em todo servidor não pode morar em nenhum deles:
precisa de um **cadastro de conta único** e de um **livro-caixa central**. E
como é dinheiro de verdade, cada movimento vira uma linha de razão (quem,
quanto, por quê, saldo depois), nunca um número sobrescrito. O mercado
continua por realm — o item mora no banco do realm —, mas o pagamento sai e
entra no livro central.

### O que ainda falta decidir sobre ela

- O ralo novo do gold (item 1).
- A taxa do mercado — é ao mesmo tempo o ralo da TC e a defesa contra
  multi-conta.
- Mercado por realm ou global? Proposta: por realm (o item mora no banco do
  realm); só o dinheiro é global.
- A TC um dia entra por outro caminho além da compra (evento, conquista)?
- Portão pra vender no mercado (nível, história, verificação)?

## O que falta decidir

- Quais peças exatamente entram nos ~10% vendáveis.
- Nível mínimo (ou progresso de main quest) pra abrir a colônia — é a defesa
  secundária contra alt descartável, caso o vínculo não baste.
- Se a colônia tem melhorias (mais taxa, mais teto) e o que elas custam.

# Loja de cash

Menu → Comércio → **Loja**. Quatro abas: **Montarias**, **Skins** (de
montaria), **Materiais** e **Tempest Points**. O saldo de TP fica no topo. Código:
`crates/shared/src/loja.rs` (catálogo, regras, mensagens),
`crates/server/src/loja.rs` (banco central), `crates/server/src/world/loja_mundo.rs`
(pedidos no loop do mundo), `crates/client/src/loja_tp.rs` (janela).

## Produtos e preços iniciais ⚠️

A TP é comprada com **dinheiro de verdade**; pergaminhos de invocação e skins
são comprados com **TP** (a TP é a moeda da loja, como no MIR4). Regra da economia
(ECONOMIA.md): montarias e skins não dão poder de combate; a aba Materiais
vende atalhos opcionais para craft e evolução de habilidades.

| pacote | TP | bônus | total | preço |
|---|---:|---:|---:|---:|
| Punhado de TP | 100 | — | 100 | R$ 4,90 |
| Bolsa de TP | 500 | 50 | 550 | R$ 24,90 |
| Baú de TP | 1.000 | 200 | 1.200 | R$ 49,90 |
| Tesouro de TP | 2.000 | 600 | 2.600 | R$ 99,90 |

| pergaminho | preço | resultado ao abrir |
|---|---:|---|
| Invocação: Montaria | 500 TP | Lobo 55%, Tigre 30%, Urso 15% |
| Invocação: Chaves | 120 TP | Escama, Garra, Chifre ou Couro; cor aleatória |
| Invocação: Tomos | 150 TP | habilidade aleatória; Verde 75%, Roxo 20%, Lendário 5% |

| skin | montaria | preço |
|---|---|---:|
| Lobo da Meia-Noite | Lobo | 300 TP |
| Lobo Dourado | Lobo | 450 TP |
| Tigre de Brasa | Tigre | 400 TP |
| Tigre Espectral | Tigre | 600 TP |
| Urso Polar | Urso | 350 TP |
| Urso de Obsidiana | Urso | 550 TP |

Os pergaminhos são repetíveis e entram na bolsa; comprar **não sorteia nem abre**.
Ao usar o item, o servidor decide o prêmio e o cliente mostra a abertura animada.
O de chaves sorteia a família e a cor: **55% cinza, 28% verde, 12% azul e 5%
roxa**. Se a bolsa estiver cheia na compra, o pergaminho vai para as Entregas;
se ela encher entre abrir e receber a chave, a chave vai para as Entregas.
Na bolsa, **Abrir 10+1** consome 10 pergaminhos e entrega 11 prêmios. O
servidor credita todos antes de iniciar a animação; fechá-la não perde nada.
As onze montarias são registradas numa única transação no banco central.

O pergaminho de montaria pode entregar uma montaria que a conta já possui.
A primeira cópia libera a montaria e a skin padrão; duplicatas ficam em
`loja_montarias.quantidade`, reservadas para o futuro sistema de combinar e
aprimorar montarias. A compra direta de `Produto::Montaria` é recusada pelo
servidor; essa variante permanece apenas para representar posses antigas.

A aba Materiais tem **quatro pergaminhos**: chaves, tomos, pet e montaria. As
abas **Montarias** e **Skins** saíram: a montaria virou item de bolsa e a cor
dela é a variação (docs/MONTARIAS.md), então não há mais skin para vender.

O terceiro pergaminho da aba é o **Pergaminho de Invocação: Pet**
(`PERGAMINHOS_PET`, `Produto::PergaminhoPet`), 250 TP: sorteia espécie e grau
de um pet coletor (docs/PETS.md). Cinza 55%, Verde 28%, Azul 12%, Roxo 4%,
Laranja 1%.

Moedas e Energia ficam na aba **Moedas**, ao lado: com o terceiro pergaminho,
cinco colunas na mesma aba espremiam os cartões e o preço sumia atrás do
botão. Lá estão as **moedas do jogo** (`MOEDAS`, `Produto::Moeda`), repetíveis
e entregues na hora no personagem que está jogando:

| pacote | vem | preço |
|---|---:|---:|
| Saco de Ouro | 10.000 ouro | 50 TP |
| Saco de Cobre | 20.000 cobre | 40 TP |
| Barras de Darksteel | 2.000 darksteel | 60 TP |

O ouro entra no saldo; cobre e darksteel, na carteira (docs/BANCO.md) — nunca
falta espaço. Não viram posse da conta (`loja_posses`), como os pergaminhos.

Na mesma aba, a **Energia** (`ENERGIAS`, `Produto::Energia`),
também repetível e entregue na hora:

| pacote | vem | preço | rendimento |
|---|---:|---:|---:|
| Fagulha de Energia | 2.000 Energia | 40 TP | 20,0 TP/mil |
| Cristal de Energia | 12.000 Energia | 200 TP | 16,7 TP/mil |
| Núcleo de Energia | 70.000 Energia | 1.000 TP | 14,3 TP/mil |

Energia não é item de bolsa: cai no saldo de evolução do personagem
(`skill_progress.energia`), o mesmo que a coleta enche e que paga tier de
habilidade (docs/SKILLS.md) e ponto de atributo. O pacote maior sempre rende
mais Energia por TP — o cartão mostra o TP por mil e destaca o melhor.

Skin só se compra tendo a montaria. Toda montaria corre igual
(`VEL_MONTADO`): pagar mais compra aparência, não vantagem.

## Comprar em lote

`PedidoLoja::ComprarItem` leva `vezes` (1..=`LOTE_MAX`, 99). Tudo no catálogo é
repetível, então o lote vale pra todos os itens — **menos pacote de TP**, que é
compra de dinheiro de verdade e onde lote é cobrança repetida sem querer.

O lote é **um pedido só**: um id de idempotência, uma transação, um débito do
total. Dez pedidos separados podiam falhar no meio e deixar o jogador sem saber
quantos entraram. `pode_comprar(produto, vezes, saldo)` devolve o preço **total**
e o saldo tem que cobrir ele — cobrar o unitário e entregar dez seria o jeito
óbvio de a loja virar fábrica de TP, e `o_lote_cobra_o_total_e_nao_o_unitario`
cobra isso.

Na entrega, moeda e Energia entram multiplicadas de uma vez; pergaminho e item
de pet vão um evento por unidade, porque `Evento::Consumivel` entrega UM item e
empilhar é trabalho do `add_to_inventory`. O texto do resultado diz quantos
("10x Pergaminho entregue na bolsa!") — é a única confirmação de que o lote
inteiro entrou.

### Desconto por quantidade

| a partir de | desconto |
|---|---|
| 5 | 5% |
| 10 | 10% |
| 25 | 15% |
| 50 | 20% |

As faixas (`DESCONTO_DO_LOTE`) casam com os atalhos **1x / 10x / 50x** da
janela: tocar no "10x" e ver o "−10%" aparecer é o que ensina a regra sem
texto. A conta é do **servidor** (`preco_do_lote`) — o `pedido` guarda o valor
cobrado e o razão debita por ele; o cliente só desenha. A divisão inteira
**trunca, e a sobra fica com o jogador**: arredondar pra cima seria cobrar por
um TP que o desconto disse que ele não ia pagar.

`o_desconto_do_lote_so_barateia` amarra o que importa: as faixas em ordem
decrescente (fora de ordem, `desconto_pct` pega a errada), nunca cobrar mais
que o cheio, nunca sair de graça, e o preço **por unidade** nunca subir ao
comprar mais — sem essa última o desconto podia inverter numa faixa e comprar
mais sairia mais caro por peça.

Na janela: **−**, **+**, o número, e os atalhos **1x / 10x / 50x**. O preço
mostra `Preço · 10x 500` com o total à direita, o `−10%` em verde ao lado dos
atalhos e o preço cheio **riscado** acima do total — número menor sozinho
ninguém compara de cabeça com o que teria pagado. A quantidade volta a 1 a cada
abertura da confirmação: lote herdado da compra anterior é compra sem querer.
O servidor clampa de qualquer jeito (`cat::lote`) — cliente pode mandar o que
quiser.

## Pagamento

```
cliente ── ComprarTp{pacote, pedido} ──► canal ──► central: loja_pedidos (pendente)
                                                   │
                                        Provedor::iniciar
                                         ├─ Aprovado ─► confirmar_pagamento ─► tp_razao (+TP, ref loja:<pedido>) ─► creditado
                                         ├─ Pendente{url} ─► (provedor real) webhook ─► confirmar_pagamento
                                         └─ Recusado ─► recusado
```

- **Id do pedido** gerado no cliente (`pedido_valido`: 8–64 chars). É a
  chave primária de `loja_pedidos`: clique duplo, reenvio e reconexão com o
  mesmo id viram a mesma linha.
- **`confirmar_pagamento` é idempotente**: trava a linha do pedido
  (`FOR UPDATE`) e lança o crédito com a referência única `loja:<pedido>`
  no livro-caixa (`tp_razao`). Webhook repetido ou dois canais ao mesmo
  tempo creditam uma vez só. Pedido com o mesmo id de outra conta é recusado.
- **Compra de item com TP**: uma transação do central — trava a conta,
  confere posse e saldo (`pode_comprar`), debita (`loja:item:<produto>`,
  referência `loja:<pedido>`), grava a posse e marca `entregue`. A montaria
  grava junto a skin padrão. Recusa (sem saldo, já possui, falta a montaria)
  fica registrada no pedido com o motivo. Consumíveis são repetíveis e não
  entram em `loja_posses`. Abrir uma invocação de montaria atualiza
  `loja_montarias` e então garante a posse e a skin padrão.
- **Posse é da CONTA** (`loja_posses`, chave `REALM:id_da_conta`, a mesma da
  TP): vale para todos os personagens da conta no realm.

### Provedor simulado (hoje)

`PAGAMENTO_SIMULADO` ligado por padrão (só `=0` desliga): aprova na hora e a
janela mostra **PAGAMENTO SIMULADO** e "nada é cobrado". A receita que o
panóptico mostra é simulada. Com `PAGAMENTO_SIMULADO=0` o provedor fica
`Desligado` e a compra de TP é recusada.

### Provedor real (a fazer)

Uma variante nova de `loja::Provedor` (Mercado Pago, Stripe…):

1. `iniciar` cria a cobrança na API do provedor e devolve `Pendente { url }`
   (checkout / Pix). O cliente abre a URL (`nativo::abrir_url`).
2. O provedor chama um **webhook HTTPS no `web`**, que confere a assinatura e
   chama `loja::confirmar_pagamento(pedido, id_externo)`.
3. Estorno/chargeback: movimento negativo no razão com referência própria
   (não implementado).

**App Store**: no iOS, bem digital (TP, montaria, skin) **tem que** ser
vendido por In-App Purchase. Lá o "provedor" é o StoreKit: o app manda o
recibo, o servidor confere com a Apple e chama `confirmar_pagamento`. Checkout
externo (Mercado Pago/Stripe) só vale fora do app iOS (web/PC/Android
conforme a loja de cada um). Não implementado.

## Tabelas (banco central)

- `loja_pedidos (id PK, conta, produto, tipo tp|item, valor, moeda
  BRL_CENTAVOS|TP, status pendente|creditado|entregue|recusado, provedor,
  externo, motivo, criado_em, atualizado_em)`
- `loja_posses (conta, produto, pedido UNIQUE, quando, PK (conta, produto))`
- `loja_montarias (conta, montaria, quantidade, PK (conta, montaria))`
- `tp_razao` (já existia: MERCADO.md)

Criadas no boot do canal quando `DATABASE_URL_CENTRAL` existe. Sem o central
a loja aparece desligada.

## Testes

- `shared::loja` (catálogo consistente, códigos, regras de compra, skin
  válida, preço em reais, id de pedido).
- `server::loja::tests::pedidos_idempotentes_e_posses_no_postgres`
  (`DATABASE_URL_CENTRAL_TESTE`): pedido repetido, webhook repetido, dois
  canais com o mesmo pedido, id de outra conta, provedor desligado, montaria,
  compra repetida, skin sem montaria, sem saldo, duas compras simultâneas.
- Bot `cargo run --bin lojabot` (servidor de teste): compra de TP com pedido
  repetido, montaria, compra repetida recusada, velocidade a pé × montado,
  desmonta lutando, reconexão, dois processos com o mesmo pedido.

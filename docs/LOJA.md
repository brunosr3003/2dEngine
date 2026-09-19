# Loja de cash

Menu → Comércio → **Loja**. Quatro abas: **Montarias**, **Skins** (de
montaria), **Materiais** e **Tempest Points**. O saldo de TP fica no topo. Código:
`crates/shared/src/loja.rs` (catálogo, regras, mensagens),
`crates/server/src/loja.rs` (banco central), `crates/server/src/world/loja_mundo.rs`
(pedidos no loop do mundo), `crates/client/src/loja_tp.rs` (janela).

## Produtos e preços iniciais ⚠️

A TP é comprada com **dinheiro de verdade**; montaria e skin são compradas
com **TP** (a TP é a moeda da loja, como no MIR4). Regra da economia
(ECONOMIA.md): montarias e skins não dão poder de combate; a aba Materiais
vende um atalho opcional para o craft.

| pacote | TP | bônus | total | preço |
|---|---:|---:|---:|---:|
| Punhado de TP | 100 | — | 100 | R$ 4,90 |
| Bolsa de TP | 500 | 50 | 550 | R$ 24,90 |
| Baú de TP | 1.000 | 200 | 1.200 | R$ 49,90 |
| Tesouro de TP | 2.000 | 600 | 2.600 | R$ 99,90 |

| montaria | bicho | preço | skin que vem junto |
|---|---|---:|---|
| Lobo da Clareira | lobo | 500 TP | Pelagem Cinza |
| Tigre das Neves | tigre | 800 TP | Listras Brancas |
| Urso de Carga | urso | 1.200 TP | Pelo Castanho |

| skin | montaria | preço |
|---|---|---:|
| Lobo da Meia-Noite | Lobo | 300 TP |
| Lobo Dourado | Lobo | 450 TP |
| Tigre de Brasa | Tigre | 400 TP |
| Tigre Espectral | Tigre | 600 TP |
| Urso Polar | Urso | 350 TP |
| Urso de Obsidiana | Urso | 550 TP |

| material | preço | conteúdo |
|---|---:|---|
| Baú de Chaves de Craft | 120 TP | 1 Escama, Garra, Chifre ou Couro aleatório |

O baú é repetível e pode entregar qualquer cor: **55% cinza, 28% verde,
12% azul e 5% roxa**. O servidor faz os dois sorteios, debita a TP uma vez
por pedido e entrega a chave na bolsa; se a bolsa estiver cheia, ela vai para
o correio de recompensas.

Na mesma aba, as **moedas do jogo** (`MOEDAS`, `Produto::Moeda`), repetíveis
e entregues na hora no personagem que está jogando:

| pacote | vem | preço |
|---|---:|---:|
| Saco de Ouro | 10.000 ouro | 50 TP |
| Saco de Cobre | 20.000 cobre | 40 TP |
| Barras de Darksteel | 2.000 darksteel | 60 TP |

O ouro entra no saldo; cobre e darksteel, na carteira (docs/BANCO.md) — nunca
falta espaço. Não viram posse da conta (`loja_posses`), como o baú.

Skin só se compra tendo a montaria. Toda montaria corre igual
(`VEL_MONTADO`): pagar mais compra aparência, não vantagem.

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
  entram em `loja_posses`.
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

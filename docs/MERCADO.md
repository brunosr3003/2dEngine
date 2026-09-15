# Mercado global

No molde do MIR4: **um mercado só pra todos os realms**. Menu → Comércio →
Mercado. Nada abre por tecla.

Decisões do usuário (15/09/2026):

- **Global entre todos os realms.**
- **Vende tudo que não é vinculado.**
- **Taxa de 5%**, descontada do que o vendedor recebe e **queimada** (some da
  economia).
- **Troca TP↔gold já agora**, com custódia e livro-caixa central.
- **Portão de nível pra vender: 20** (`shared::mercado::NIVEL_PARA_VENDER`).
  Comprar é livre.

Código: regras puras e tipos de rede em `crates/shared/src/mercado.rs`; banco
central, custódia, cartas e relay em `crates/server/src/mercado.rs`; livro da
TP em `crates/server/src/mercado_razao.rs`; o lado do loop do mundo em
`crates/server/src/world/mercado_mundo.rs`; painel em
`crates/client/src/mercado_ui.rs`.

## O painel

| Aba | O que faz |
|---|---|
| Comprar | Filtro por categoria (Todas, Equipamento, Material, Consumível), busca por nome, lista do mais barato, páginas de 20. Comprar abre a confirmação com a quantidade (compra parcial) e o total. |
| Vender | Itens da bolsa que não são vinculados. Quantidade e preço por botões (sem digitar), total, taxa de 5% e quanto recebe. Mostra o menor preço à venda do mesmo item. |
| Meus anúncios | Ativos (até 20) com Cancelar, e o histórico de vendas e compras. |
| Entregas | Tudo que espera ser recebido: item comprado, gold de venda, item de anúncio cancelado ou recusado, gold de compra que não fechou. "Receber tudo". |
| TP | Saldo de TP da conta, TP à venda (gold por TP) e o formulário pra vender TP. |

Venda fechada com o painel fechado avisa no chat.

## Vinculado

Coluna `items.vinculado` no banco do realm (padrão `false`) e
`ItemConfigEntry.vinculado` pro cliente. Item que o jogo não conhece conta
como vinculado. A migração `mercado_vinculados_v1` (em `migracoes_de_dados`)
vincula uma vez a **Poção de XP** (recompensa de missão). Pra vincular outro:

```sql
UPDATE items SET vinculado = TRUE WHERE id = …;
UPDATE economy_version SET version = version + 1 WHERE id = 1; -- hot-reload
```

Equipamento equipado não está na bolsa, então não aparece. Vínculo por
instância (ex.: vincula ao equipar) ainda não existe.

## Por que dois bancos e cartas

O gold e os itens moram no banco do REALM; o mercado é de todos. Dois bancos
não têm transação comum, então nunca "debita dos dois e torce":

```
 realm (DATABASE_URL)                              central (DATABASE_URL_CENTRAL)
 ───────────────────                               ──────────────────────────────
 anunciar: item sai da bolsa ─┐
 comprar:  gold sai           │ mesma transação
            mercado_saida ◄───┘ do save do personagem
                  │ relay (3 s ou na hora do save)
                  └──────────────────────────────► mercado_operacoes (id único)
                                                   mercado_anuncios (custódia)
                                                   mercado_cartas ──┐
                                                   tp_razao         │
 "Receber": carta aplicada na bolsa ─┐                              │
  mercado_cartas_aplicadas ◄─────────┘ mesma transação do save      │
                  │ relay                                           │
                  └──────────────────────────────► entregue = NOW() ┘
```

- **Custódia.** Ao anunciar, o item sai da bolsa e a operação vai pra
  `mercado_saida` na **mesma transação** do save do personagem. Ao comprar, o
  gold sai do mesmo jeito. Se o processo cair antes do save, nenhum dos dois
  aconteceu.
- **Uma vez só no central.** O relay manda cada linha de `mercado_saida`. O
  central grava o id em `mercado_operacoes` na mesma transação do efeito; a
  mesma linha mandada de novo (queda no meio, dois canais do mesmo realm)
  devolve o resultado guardado e não repete nada.
- **Cartas.** Tudo que volta pro personagem é uma carta com id derivado da
  operação (`<op>:item`, `<op>:venda`, `<op>:reembolso`, `<anuncio>:cancelado`,
  `<op>:devolve`), inserida com `ON CONFLICT DO NOTHING`. Aplicar a carta grava
  `mercado_cartas_aplicadas` na mesma transação do save; só depois o relay
  marca `entregue` no central. Carta já aplicada e ainda não avisada é filtrada
  da lista, e o processo lembra as que aplicou antes do save confirmar — **aplicar
  duas vezes vale uma**.
- **Compra que não fecha** (acabou, preço mudou, quantidade maior que a
  restante, anúncio próprio): nada muda no anúncio e o gold volta por carta.
  Não existe "compra o que sobrou".
- **Limite de 20 anúncios** é conferido no central (a contagem verdadeira), com
  trava por personagem. Anúncio recusado lá devolve o item por carta.
- **Save que falhou**: o writer guarda os registros do mercado e manda no
  próximo save — perder um é perder item ou gold de alguém.
- **Registro espera a linha do dono**: um registro só vai no save que leva a
  bolsa do mesmo personagem (`tomar_registros_mercado`), senão a operação
  podia gravar sem a bolsa mudada e duplicar item.

## TP

A TP é da **conta** e mora só no central, em `tp_razao`: **uma linha por
movimento** (conta, delta, saldo depois, motivo, referência única, quando). O
saldo é a última linha; nunca é sobrescrito. `CHECK (saldo_depois >= 0)` e
trava por conta (`pg_advisory_xact_lock`).

- **Anunciar TP**: a TP sai do livro (`<anuncio>:custodia`) na mesma transação
  que cria o anúncio.
- **Comprar TP**: o gold sai no realm (custódia, como item); no central a TP
  entra no livro do comprador (`<op>:tp`) e o gold líquido vai por carta ao
  vendedor.
- **Cancelar**: a TP que sobrou volta ao livro (`<anuncio>:cancelado`).
- A taxa de 5% vale também na troca TP↔gold, em gold.

**Chave da conta** no central: `REALM:id_da_conta` (`mercado::conta_global`).
Cadastro único de contas ainda não existe — cada realm tem sua `accounts`. Um
merge de realm tem que remapear essas chaves em `tp_razao` e
`mercado_anuncios.conta`.

### Creditar TP de teste

Enquanto não existe loja de TP, é o único jeito de TP entrar:

```sh
# pelo personagem (acha a conta no banco do realm)
DATABASE_URL_CENTRAL=… DATABASE_URL=… MMO_REALM=SA01 \
  cargo run --release --bin creditar_tp -- Fulano 500 "teste do mercado"

# pela chave da conta
DATABASE_URL_CENTRAL=… cargo run --release --bin creditar_tp -- --conta SA01:12 500
```

## Tabelas

**Realm** (`DATABASE_URL`, criadas no boot por `mercado::init`):

| Tabela | Pra quê |
|---|---|
| `mercado_saida` | Operações que saíram do personagem e ainda vão (ou já foram) ao central: `id`, `personagem`, `payload` (JSON), `criada`, `enviada`, `resultado`. |
| `mercado_cartas_aplicadas` | Cartas já aplicadas: `id`, `personagem`, `aplicada`, `avisada` (o central já marcou entregue). |
| `items.vinculado` | Coluna nova. |

**Central** (`DATABASE_URL_CENTRAL`, criadas no boot com trava 728432):

| Tabela | Pra quê |
|---|---|
| `mercado_anuncios` | Anúncios em custódia: tipo (item/TP), realm, conta, personagem, item, instância (JSON), quantidade total e restante, preço, estado (ativo/esgotado/cancelado). |
| `mercado_operacoes` | Id de toda operação vinda de realm, com o resultado — a idempotência. |
| `mercado_cartas` | Entregas: realm, personagem, item/quantidade/instância, gold, motivo, `entregue`. |
| `mercado_vendas` | Histórico: bruto, taxa, líquido, vendedor e comprador. |
| `tp_razao` | Livro-caixa da TP. |

## Variáveis de ambiente

| Variável | Onde | O quê |
|---|---|---|
| `DATABASE_URL_CENTRAL` | todo processo de canal (`tempest-campo`) | Banco central. Sem ela o mercado fica **desligado** (aviso no log, painel responde "desligado") e o jogo segue. |
| `DATABASE_URL_CENTRAL_TESTE` | só testes | Banco descartável pro teste `mercado::testes::custodia_cartas_e_tp_no_postgres`. Sem ela o teste não faz nada. |

## Deploy do banco central (VPS zone13)

Uma vez só. No mesmo Postgres dos realms serve, desde que seja **outro
banco** (e, com mais de um realm em VPS diferentes, todos apontam pro mesmo):

```sh
ssh zone13
sudo -u postgres psql -c "CREATE ROLE tempest_central LOGIN PASSWORD '<senha forte>'"
sudo -u postgres psql -c "CREATE DATABASE tempest_central OWNER tempest_central"
# em /etc/tempest.env:
#   DATABASE_URL_CENTRAL=postgres://tempest_central:<senha>@127.0.0.1:5432/tempest_central
sudo systemctl restart tempest-campo    # os canais (o supervisor repassa o env)
sudo systemctl restart tempest-web      # não usa o central hoje; reinicia junto do deploy
journalctl -u tempest-campo -n 50 | grep mercado   # "banco central conectado — mercado global ligado"
```

As tabelas nascem sozinhas no primeiro boot. Protocolo 95: servidor e app
sobem juntos.

## Testes

- `cargo test -p shared mercado`: taxa e arredondamento, portão de nível,
  vinculado, quantidade/preço fora da faixa, compra parcial, compra recusada,
  carta aplicada duas vezes, categorias.
- `cargo test -p server mercado`: ids, JSON da operação, chave da conta e o
  fluxo inteiro no Postgres (anunciar pela saída do realm, repetir, busca,
  compra parcial, repetir compra, carta aplicada e avisada, compra maior que o
  restante com reembolso, cancelar duas vezes, TP: crédito idempotente,
  anúncio sem saldo, anúncio, compra, cancelamento, histórico).
- `cargo test -p client mercado_ui` e `menu`.

```sh
docker exec mmo-pg psql -U solar -d mmo_dev -c "CREATE DATABASE tempest_central_teste"
DATABASE_URL_CENTRAL_TESTE=postgres://solar:solar_dev_123@localhost:5432/tempest_central_teste \
  cargo test -p server mercado
```

## Ainda não

- Anúncio **expira** (MIR4 tem prazo) — hoje fica até vender ou cancelar.
- Preço médio / gráfico de preço, e o preço da TP no panóptico.
- Cadastro único de contas (a chave da TP é por realm+conta).
- Vínculo por instância (equipar vincula).
- Loja de cash que gasta a TP.

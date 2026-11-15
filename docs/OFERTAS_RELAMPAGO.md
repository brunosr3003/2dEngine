# Ofertas relâmpago

Pacotes premium que aparecem quando o personagem chega num nível, por tempo
limitado. **Tudo é configurado no banco CENTRAL** (`tempest_central`), sem
mexer no código nem publicar build.

## Tabela `ofertas_relampago` (o catálogo)

| coluna | o que é |
|---|---|
| `id` | identificador (`nivel_5`). Não muda depois de criado. |
| `titulo` / `titulo_pt` | nome em inglês / português (o jogador vê o da língua dele). |
| `nivel` | abre quando o personagem atinge este nível. |
| `duracao_min` | quanto tempo fica aberta (padrão 60). |
| `produto_play` | id do produto na Google Play Console (ex. `oferta_099`). |
| `preco_centavos` | preço mostrado — o MESMO número em R$ e US$ (99 = R$ 0,99 / US$ 0.99). Tem que bater com o preço do produto na Play Console. |
| `valor_de_centavos` | "de" riscado (opcional). |
| `tp` | TP creditado. |
| `energia` | energia creditada. |
| `itens` | JSON: `[{"item_id":363,"qtd":1},{"item_id":345,"qtd":2000}]`. |
| `ativa` | `false` tira do ar (quem já tem aberta continua até expirar). |
| `nota` | anotação livre. |

Itens úteis: 363 pergaminho de pet, 361 pergaminho de montaria, 360
pergaminho de chave de craft, 362 pergaminho de tomo, 345 darksteel.

### Exemplos

```sql
-- nova oferta no nível 35 por R$ 9,99 / US$ 9.99 (produto já existente na Play)
INSERT INTO ofertas_relampago (id, titulo, titulo_pt, nivel, produto_play, preco_centavos, valor_de_centavos, energia, itens)
VALUES ('nivel_35', 'Hero''s Flash Offer', 'Oferta Relâmpago de Herói', 35, 'oferta_999', 999, 5990, 20000,
        '[{"item_id":361,"qtd":2},{"item_id":360,"qtd":4}]');

-- mudar o conteúdo
UPDATE ofertas_relampago SET itens = '[{"item_id":363,"qtd":3}]' WHERE id = 'nivel_15';

-- desligar
UPDATE ofertas_relampago SET ativa = false WHERE id = 'nivel_25';
```

## Tabela `ofertas_relampago_contas` (quem recebeu)

Uma linha por conta × oferta: `aberta_em`, `expira_em`, `comprada_em`, `pedido`.
Cada oferta aparece **uma vez por conta** (personagem novo não reabre).
Para dar de novo a alguém: `DELETE FROM ofertas_relampago_contas WHERE conta = '…' AND oferta = '…';`

## Regras

- Abre no máximo UMA oferta por vez, a de menor nível ainda não recebida.
- Só o app Android pede ofertas (é onde há pagamento real).
- Preço novo precisa de produto novo na Play Console (`oferta_<preço>`), com
  preço manual BR = R$ x,xx e EUA = US$ x.xx; os outros países convertem do
  preço dos EUA. Oferta nova com preço que já existe reusa o produto.
- Compra: o servidor confere o token na Google, credita uma vez (pedido do
  Google único), entrega TP + energia + itens e **consome** a compra na Google
  (o mesmo produto pode ser comprado de novo pela próxima oferta).

## Pacotes bônus (gatilhos) — 15/11/2026

Pedido do dono: "pacotes de centavos que aparecem quando o jogador morre … no
primeiro porão, quando ele morre ou não consegue … e conforme upa vai
ganhando pacotes bônus … as promoções são sempre: a quantidade de TP que
aquela compra daria + algum bônus (item, ouro, drop, invocação…)".

Usa a mesma máquina das ofertas relâmpago (mesmas tabelas, mesmos produtos
`oferta_<preço>`, mesma compra e entrega). Código: regras puras em
`crates/shared/src/pacotes_bonus.rs`, banco em `crates/server/src/ofertas.rs`
(`disparar`, `listar`, `comprar_simulado`, `creditar`), mundo em
`crates/server/src/world/ofertas_mundo.rs` (ganchos), janela em
`crates/client/src/ofertas_ui.rs` (`desenha_foco`).

### A regra

`pacotes_bonus::tp_do_pacote(preço, tp_bonus) = tp_do_preco(preço) + tp_bonus`.
`tp_do_preco` é o TP que o dinheiro compra na loja (`loja::PACOTES`): na taxa
do maior pacote que o preço alcança (bônus incluso), ou do menor pacote
abaixo do preço dele. Hoje: R$ 0,99 → 20 TP, 1,99 → 40, 4,99 → 101,
9,99 → 203, 15,00 → 306. Todo pacote dá pelo menos isso de TP **mais** um
bônus (TP extra, ouro, Energia, itens ou equipamento) — o teste
`todo_pacote_e_o_tp_do_preco_mais_um_bonus` trava. A regra vale para o
catálogo inicial; quem editar o banco na mão é responsável por mantê-la.

O pacote inicial do dono (400 TP por 0,99) fica dentro da regra como
20 TP do preço + 380 TP de bônus.

### Os pacotes

| id | gatilho | nível mín. (exibido → interno) | preço / produto | conteúdo |
|---|---|---|---|---|
| `bonus_porao_inicial` | `porao_inicial` | 1 → 1 | 0,99 `oferta_099` | 400 TP + 10.000 ouro + 10.000 Energia + 10.000 Darksteel |
| `bonus_morte_arma` | `morte` | 10 → 6 | 1,99 `oferta_199` (**novo**) | 40 TP + 20.000 ouro + a ARMA ATUAL dele em AZUL no nível dele |
| `bonus_morte_conjunto` | `morte` | 20 → 11 | 4,99 `oferta_499` | 101 TP + 30.000 ouro + conjunto AZUL (arma, secundária, armadura, brinco, amuleto, bracelete, cinto) |
| `bonus_nivel_20` | nível | 20 → 11 (aparece no 21) | 1,99 `oferta_199` (**novo**) | 40 TP + 15.000 ouro + 1 pergaminho de montaria |
| `bonus_nivel_50` | nível | 50 → 30 | 4,99 `oferta_499` | 101 TP + 20.000 Energia + 1 pergaminho de pet + 2 de chave + 5.000 Darksteel |
| `bonus_nivel_70` | nível | 70 → 42 | 9,99 `oferta_999` (**novo**) | 203 TP + 50.000 Energia + 2 pergaminhos de montaria + 3 de chave + 20.000 Darksteel |
| `bonus_nivel_100` | nível | 100 → 60 | 15,00 `oferta_1500` | 306 TP + 100.000 ouro + 2 pet + 2 montaria + 5 chave + 50.000 Darksteel |

As antigas `nivel_5`/`nivel_15`/`nivel_25` (interno = exibido ~9/30/~43)
continuam como estão (não seguem a regra: a 15 e a 25 não dão TP). Para
alinhar: `UPDATE ofertas_relampago SET tp = 101 WHERE id = 'nivel_15';`
`UPDATE ofertas_relampago SET tp = 306 WHERE id = 'nivel_25';`

Os pacotes de nível são linhas comuns de nível (`gatilho` NULL): abrem pelo
mesmo caminho das ofertas de nível (o cliente pergunta quando o nível muda).

### Gatilhos

- **`porao_inicial`**: no Porão do Naufrágio (`PORAO_INICIAL` = conteúdo 1)
  sem vencer — caiu lá dentro, saiu pela porta antes de vencer, ou o tempo
  acabou. Ganchos: `oferta_ao_cair` (world.rs, na transição para caído) e
  `oferta_ao_falhar_porao` (dungeon.rs: `dg_sair` sem vitória e
  `dg_terminar` por tempo).
- **`morte`**: caiu em qualquer lugar (campo, dungeon, PvP). Caindo no Porão
  do Naufrágio tenta primeiro o inicial, depois os de morte.
- A cada gatilho abre **no máximo um** pacote: o primeiro (por ordem de
  gatilho, depois nível) que a conta nunca recebeu e cujo nível mínimo ela
  já tem. Assim os de morte vêm em sequência: arma, depois conjunto.
- Pacote de equipamento só abre se REALMENTE aumentar o poder (já usa azul
  melhor? pula para o próximo).
- Um pacote de gatilho aberto por vez, e **dois nunca abrem a menos de
  2 h** (`INTERVALO_ENTRE_GATILHOS_MIN`, contado da abertura do anterior).
  Cada um dura **45 min** (`DURACAO_MIN`, coluna `duracao_min`).
- Um pacote de gatilho aberto não segura a oferta de nível (e vice-versa).
- **Por CONTA**, como as ofertas de nível: cada pacote aparece uma vez por
  conta; um personagem novo não reabre.
- Nunca para staff (`chat_global::staff`) nem bots de teste (nome `bot<dígito>…`,
  `pacotes_bonus::e_bot_de_teste`) — nem os de nível.
- Só dispara para quem pode pagar: a sessão cujo cliente pediu ofertas
  (Android com Google Play, ou qualquer cliente num servidor com pagamento
  simulado). Assim a hora não corre num PC que não compra.

### O equipamento

Montado na hora do gatilho, no nível dele, em AZUL (`Grau::Raro`, nível de
item `dungeon::nivel_da_peca(Raro, nível)` = 26..45, como peça de baú):

- **arma**: o TIPO da arma equipada (mão vazia = espada e escudo);
- **conjunto**: arma + a secundária do conjunto + a armadura que ele veste
  (sem armadura: pesada para espada, média para katana, leve para pistolas e
  anel) + brinco, amuleto, bracelete e cinto.

Os atributos de peça são fixos por modelo/cor/nível (`ItemInstance::fixar`),
então o que a janela mostra é o que chega. As peças e o poder ficam na linha
da conta (`ofertas_relampago_contas.pecas` = `[[item_id, nível_de_item], …]`,
`poder_antes`, `poder_depois`, `personagem`).

**Poder mostrado**: `effective_stats` → `dungeon::poder_de_stats`, a mesma
conta da ficha, com o equipamento atual e com as peças do pacote no lugar.

### Entrega

Igual às ofertas: na mesma transação da compra entra tudo em `loja_entregas`
(novas colunas `ouro` e `ilvl`), e o mundo entrega: TP no razão, Energia em
`skill_progress.energia`, **ouro direto na bolsa de ouro**, itens no
Correio (Entregas) como antes, e **as peças na bolsa primeiro** (para
equipar já) — bolsa cheia, Correio. Se o pagamento chega com o jogador em
outro personagem da conta, a entrega vai para quem estiver logado.

### Servidor de teste (pagamento simulado)

Com `Provedor::Simulado` (sem Google Play configurado), o cliente de
qualquer plataforma vê as ofertas e compra com `PedidoLoja::ComprarOfertaSimulada`:
aprovado na hora, ninguém é cobrado, `loja_pedidos.provedor = 'simulado'`,
`externo = 'sim-<pedido>'`, telemetria em `loja_pedido_teste` (não entra na
receita). Em servidor com Google Play o pedido simulado é recusado.

### Telemetria

`oferta_aberta` (chave = id, também para os de gatilho), `oferta_gatilho`
(chave `gatilho:id`), `oferta_comprada`, e o de sempre de receita
(`loja_pedido` / `loja_pedido_teste`, `loja_receita_centavos`), mais
`loja_moeda` (chave `1`) para o ouro entregue. O Panóptico continua lendo
`loja_pedidos`; a aba de campanhas só lista campanhas.

### Para dinheiro de verdade

- Criar na Play Console os produtos **`oferta_199`** (R$ 1,99 / US$ 1.99) e
  **`oferta_999`** (R$ 9,99 / US$ 9.99), consumíveis, como os outros
  `oferta_*`. `oferta_099`, `oferta_499` e `oferta_1500` já existem.
- App Store: a compra de oferta ainda não existe no iOS (só Android).
- Subir o `PROTOCOL_VERSION` na release (campos novos em `OfertaRelampago` e
  a variante nova `PedidoLoja::ComprarOfertaSimulada`).

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

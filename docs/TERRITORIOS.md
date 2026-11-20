# Territórios: ilhotas dos clãs

Desenho fechado com o Bruno em 20/11/2026. Ilhas pequenas no mar aberto, sem
nada a ver com as ilhas principais, que os clãs ocupam, fortificam e disputam.

## Decisões

| Tema | Decisão |
|---|---|
| Acesso | As ilhotas ficam **dentro do mar** (mesmo processo `mar_aberto`). Ancora perto da praia e **desembarca andando**; volta pro navio do mesmo jeito. Cerco mistura batalha naval e invasão a pé. |
| Clã sem território | **Clã nômade**: o clã existe, mas sem as vantagens (loja, passivas, banco, chat etc. ficam travados) até ocupar uma ilhota. Clãs que já existem viram nômades até conquistar. |
| Disputa | **Cerco agendado**: o atacante declara guerra; a ilhota só é atacável numa janela fixa (dia/hora escolhidos pelo dono). Fora da janela, as estruturas são invulneráveis. |
| Fortes | **Lotes fixos com módulos**: cada ilhota tem lotes pré-definidos (núcleo/bandeira, muralha, portão, torres de canhão, cais, armazém). O clã escolhe e sobe o nível de cada módulo com materiais. |

## Fases

1. **As ilhotas existem** (feito 20/11/2026): `mar::ilhotas()`, 12 ilhotas
   geradas de forma determinística em água PvP, longe das ilhas principais,
   das rotas e umas das outras. Relevo pelo mesmo gerador do mar (o cliente
   desenha sozinho), sem cais, sem bicho e sem baú em cima. Nome próprio.
2. **Desembarque** (feito 20/11/2026): com a praia a até 8 u do corpo ou
   da borda do casco (`mar::praia_perto` / `praia_perto_de`), o botão do
   leme vira "Go ashore" (`PedidoNavio::Desembarcar`), com prioridade sobre
   o leme (dá pra descer direto da roda). Sem exigir navio parado: ele é
   parado na hora. Em terra: `Session::em_terra`
   (ilhota + navio ancorado), o corpo anda pelo mover de terra sobre o
   relevo do mar, o toque usa o A* de terra, o cliente recebe
   `ServerMessage::EmTerra` e segue as regras de terra (não é leme). O
   navio do capitão fica ancorado (não vira autônomo, não é relançado).
   Perto do navio: "Board ship" (`PedidoNavio::VoltarAoNavio`; navio meu
   ou do grupo). Sem navio por perto e na beira d'água: "Set sail" lança o
   meu. Morrer em terra leva o navio ancorado junto (`largar_terra`).
3. **Ocupação e clã nômade**: tabela `territorios` (ilhota, clã, desde),
   reivindicar ilhota livre (ritual no núcleo), vantagens do clã travadas
   sem território, aviso pros clãs existentes.
4. **Fortes**: lotes por ilhota, módulos com nível e custo, render no
   cliente, armazém do clã.
5. **Cerco**: declaração, janela agendada pelo dono, estruturas com HP
   (canhão do navio e ataque a pé), captura do núcleo, recompensa e trava
   de re-ataque.

## Armadilha

O relevo do mar tem CACHE em disco (servidor `data/ilhas/*-marN.alt`, e o
cliente também), com o nome vindo de `mar::REVISAO`. Mexeu no relevo (ilhota
nova, tamanho, posição): **suba `mar::REVISAO`**. Sem isso o mover de terra lê
o relevo velho (água embaixo da ilhota) e ninguém anda em terra, enquanto o
navio (que usa `mar::navegavel` direto) parece certo. Foi o bug de 20/11/2026.

## Onde está no código

- `crates/shared/src/mar.rs`: `ilhotas()`, `IlhaVista::territorio`,
  `terra_em` / `bloco_da_coluna` / `agua_livre` olham as duas listas.
  `ilhas()` continua só com as ilhas principais (rotas, cais, nível,
  águas pacíficas não mudam).

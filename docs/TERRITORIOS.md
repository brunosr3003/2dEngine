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
3. **Ocupação e clã nômade** (feito 20/11/2026): tabela `territorios`
   (ilhota PK, clã UNIQUE com ON DELETE CASCADE, desde). Em terra, a até
   `territorio::RAIO_DO_NUCLEO` (12 u) do centro de uma ilhota livre, o
   botão vira "Claim island" (`PedidoNavio::ReivindicarIlhota`): o mar
   checa em terra/centro/clã/livre/clã sem ilha (`pode_reivindicar`), o
   banco pré-checa líder ou oficial (`social::pode_reivindicar`) e começa
   a TOMADA: 2 minutos (`TEMPO_DE_TOMADA_S`) com pelo menos um membro do
   clã no anel e ninguém de fora. Alguém de fora no anel pausa
   ("Contested"); nenhum membro do clã dentro cancela. No fim o banco
   recheca e grava (`social::reivindicar`). `ServerMessage::Tomadas` a cada
   mudança: faixa no topo da tela pra quem está a 120 u, anel pulsando.
   Mastro com bandeira e anel no centro de cada ilhota (cor do dono);
   ilhotas sem props (só terra). O mar
   guarda a cópia (`GameWorld::territorios`), recarrega a cada 30 s e manda
   `ServerMessage::Territorios` pros clientes (mapa: dourado livre, verde
   meu, vermelho de outro clã). **Clã nômade** = sem ilhota: no máximo
   `territorio::MAX_NOMADE` (10) membros; com ilhota, 50. Loja, banco e
   passivas do clã, quando existirem, exigem território. Painel do clã
   mostra "Territory: X" ou "Nomad clan". Ainda sem abandonar ilhota pela
   UI (dissolver o clã libera).
4. **Fortes**: lotes por ilhota, módulos com nível e custo, render no
   cliente, armazém do clã.
5. **Cerco**: declaração, janela agendada pelo dono, estruturas com HP
   (canhão do navio e ataque a pé), captura do núcleo, recompensa e trava
   de re-ataque. A visão do Bruno (20/11/2026): pra dominar uma ilha o clã
   faz um **cerco de barcos**, não morre (quem morre no mar volta pro
   próprio barco com respawn crescente — já vale em terra na ilhota desde
   20/11), invade, derrota os defensores, **abre os portões** e **domina a
   bandeira** (os 2 minutos no anel sem inimigo). Afundar os barcos do
   outro lado tira o ponto de volta dele.

   **Barcos no cerco (implementado 20/11/2026):** navio ancorado com o
   capitão em terra que chega a 0 vira destroço em chamas como qualquer
   outro (`dano_no_casco` acha o navio por `navio_do_capitao`); morrer na
   ilha enquanto ele queima volta pro destroço (`morte_em_terra`, respawn
   crescente); quando o destroço afunda (12 min ou abandono) com o capitão
   vivo em terra, ele fica na ilha sem navio e sem penalidade
   (`afundar_derrotado`); morrer na ilha sem navio = morte naval de verdade
   (navio avariado) e volta pro cais, só voltando ao mar depois do reparo.

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

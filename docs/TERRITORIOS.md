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
4. **Fortes e economia da ilha** (desenho fechado com o Bruno em 21/11/2026,
   ver a seção "Fase 4 em detalhe").
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

## Fase 5: regras do cerco (fechadas com o Bruno em 21/11/2026)

- **Guerra só no fim de semana** (sábado ou domingo, horário de Brasília).
- **Declaração** de segunda até **sexta ao meio-dia**; depois disso vale só
  pro fim de semana seguinte.
- **O defensor escolhe o horário**, dentro de uma **faixa** (ex.: 14h–22h de
  sábado ou domingo). Sem escolha até sábado 0h, vale um horário padrão
  (sábado 20h).
- **Vitória pela bandeira**: quem segurar a bandeira 2 min dentro da janela
  da guerra (1–2 h) leva a ilha. Muros, portões e defensores só atrasam; não
  precisa demolir tudo.
- **Manutenção semanal**: cada ilha cobra uma taxa semanal do banco do clã
  (sobe com o nível). Sem pagar, as defesas enfraquecem e, depois de 2
  semanas, a ilha fica livre.
- **Escudo depois da troca de dono**: a ilha que acabou de trocar de dono
  não pode ser atacada no fim de semana seguinte.
- **Duração da janela** por porte: pequena **45 min**, média **60 min**,
  grande **90 min**. O atacante vence ao segurar a bandeira 2 min sem
  ninguém do defensor no anel; acabou o tempo sem isso, o defensor fica.
  **Prorrogação sem limite**: se no fim da janela houver atacante no anel
  (disputando ou com a contagem dos 2 min correndo), a guerra continua
  enquanto houver atacante lá. Ela só acaba quando os defensores expulsam
  todos os atacantes do anel (o defensor fica) ou quando o atacante
  completa os 2 min (toma a ilha).
- Já existente: **1 ilhota por clã** (com 12 ilhotas, sempre sobra ilha) e
  tomar ilha **livre** não precisa de guerra (o claim de 2 min, a qualquer
  hora).

## Vantagens do território pro membro (decididas em 22/11/2026)

O Bruno perguntou "por que alguém pagaria essa taxa?": ter ilha precisa dar
vantagem que o MEMBRO sente. Pacote aprovado:

1. **Manutenção pela metade**: 50k × vila + 10k × cada nível das outras trilhas.
2. **Passivas do território** (todo membro, enquanto o clã tiver ilha):
   XP e drop +2%/+3%/+4% por nível da vila (pequena/média/grande, até +16%
   na Fortaleza); velocidade de coleta e ouro de baú +5% por nível da vila.
3. **Loja do clã**: pontos de clã por membro (depósito de ouro na
   manutenção = 1 ponto por 1.000; coleta na ilha do clã; defesa na guerra),
   trocados por chaves, materiais raros, cosméticos e montaria exclusiva.
4. **Ilha como base**: teleporte pra ilha do clã pelo menu, reparo de navio
   grátis lá; depois baú pessoal e chefe exclusivo nas grandes.
5. **Dividendos semanais**: toda segunda, depois da manutenção, 20% do
   material do banco é dividido entre os membros que jogaram na semana.

## Fase 6: fortes nas ilhas principais (ideia aprovada em 21/11/2026)

Depois do cerco (fase 5), o mesmo sistema da fase 4 vai pras ilhas
principais, **em todas, inclusive as iniciais**: um forte por ilha, num
ponto fixo fora da cidade, com Governador, muros, portões, canhões e
defensores, e cerco agendado **a pé** (sem PvP de barco; quem morre volta
pra cidade com o respawn crescente).

- **Só quem tem clã participa**: na guerra de forte, só membros de clã
  atacam e são atacados; quem não tem clã passa sem ser alvo e sem poder
  atacar (decisão do Bruno: "esses PvPs são opcionais").
- **Porte grande**: o forte de ilha principal sobe até Fortaleza (trilhas
  até nível 10, janela de guerra de 90 min).
- **Taxa menor que a das ilhotas**: **3%** do que se coleta e dropa naquela
  ilha vai pro banco do clã dono (ilhota do mar: **10%** do que se coleta
  nela). A ilha principal tem muito mais movimento, então rende mesmo assim.
- **Um forte de ilha principal por clã**, além da ilhota do mar.
- Exige generalizar território, cerco e banco (hoje no processo do mar)
  pra qualquer zona.

## Fase 4 em detalhe (21/11/2026)

**Portes das ilhotas** (12 no total): 3 **grandes** (raio ~105) só na faixa
norte (águas fortes, perto de Skyreach/Kōgen-tō/Abyssia), 3 **médias** (~77)
e 6 **pequenas** (~53). Relevo **plano** (platô liso, praia curta) pra caber
construção e evitar bug de degrau. Layout fixo por ilha: centro da vila,
anel da muralha, portão virado pra praia de desembarque, lotes de canhão na
costa, lotes de recurso.

**NPC Governador** (centro da vila): upgrada a VILA (Acampamento → Vila →
Forte → Fortaleza; teto: pequena Vila, média Forte, grande Fortaleza) e as
DEFESAS: **muros** e **portões** (HP, destrutíveis), **canhões** (HP,
destrutíveis, atiram em navio inimigo ancorado na costa) e **defensores**
(guerreiros, arqueiros, magos; nível = nível da ilha + nível da defesa).

**NPC Fazendeiro**: upgrada os RECURSOS da ilha (árvores, pedreiras,
energia…). **Qualquer um coleta**, mas uma **taxa vai pro banco do clã**
dono; o banco pode ser **saqueado numa invasão**.

**Defensores atacam só invasores**: quem bateu em muro/portão/canhão,
defensor ou membro do clã naquela ilha (marca de invasor por um tempo).
Coletar não é invadir.

**Destruição só na janela do cerco** (decisão da fase 5); no teste, um botão
no Panóptico abre o cerco na hora.

**Custo**: materiais + ouro = **20.000 × N²** no nível N (escala calculada
pela renda: ~10–20k/h por jogador ativo, clã de 10–20 junta ~1–2M/dia).
Tetos por trilha: pequena 4, média 7, grande 10 → maxar as 7 trilhas
(muro, portão, canhão, defensor, madeira, pedra, energia) custa ~4,2M /
~19,6M / ~54M. Vila: Vila 500k, Forte 2M, Fortaleza 6M.

**Etapas**: 4.1 portes + relevo plano + layout · 4.2 Governador, upgrades e
banco de dados · 4.3 muros/portões/canhões com HP e colisão · 4.4
defensores · 4.5 canhões atirando · 4.6 Fazendeiro, recursos, taxa e banco
do clã.

## Bandeiras de clã (21/11/2026)

Cada clã tem uma bandeira, que vai no mastro principal dos navios do clã (o
cliente acha o clã pelo capitão: nome do navio = nome do capitão, tag na
entidade do capitão), no mastro da ilhota que o clã domina e no mapa.

- **Preset** (`shared::bandeira::Bandeira`, 5 campos de 4 bits num `u32`):
  cor de fundo, padrão (9), segunda cor, emblema (9, bitmaps 12×12) e cor
  do emblema; rasterizado em 48×32 RGBA igual em servidor e cliente.
  `social_clas.bandeira` (NULL = `Bandeira::do_cla(id)`). Editor no painel
  do clã (mesma permissão da tag), `Pedido::BandeiraCla`.
- **Imagem própria**: "Upload image" no painel → `Pedido::CodigoBandeira`
  gera código de uso único (15 min, `cla_bandeira_codigos`) → o jogo abre
  `<site>/api/bandeira?c=CÓDIGO` (página em `crates/web/estatico`; sob /api porque os proxies só repassam /api/*) → `POST
  /api/bandeira` (PNG/JPEG até 3 MB) corta 3:2, reduz pra 48×32 RGBA e
  grava JÁ APROVADA em `cla_bandeira_imagem` (auto-approve, decisão de
  21/11/2026) e zera as denúncias antigas → ganha do preset em até 30 s.
- **Denúncias**: aba "Reports" no painel Social lista as imagens no ar;
  `Pedido::DenunciarBandeira` (1 por jogador por bandeira, não a do próprio
  clã) grava em `cla_bandeira_denuncias`; o Panóptico (aba "bandeiras")
  ordena pelas mais denunciadas e "recusar" tira a imagem (volta o preset).
- O mar manda `ServerMessage::Bandeiras` (todas, com a imagem aprovada) a
  cada mudança; o cliente faz uma textura por bandeira (`bandeiras.rs`).

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

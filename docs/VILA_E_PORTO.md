# Vila e porto

Cada ilha grande tem uma **cidade** e um **porto**. Os dois são funções da
semente, como o resto do terreno: servidor e cliente calculam igual, e nada
viaja pela rede.

## Relevo (`shared::terreno`)

- **Cidade**: platô cheio até `Cidade::RAIO_PLATO` (28 u), rampa smoothstep até
  `Cidade::RAIO` (42 u), num sítio plano e seco perto da enseada.
- **Porto** (`SitioPorto`): pátio aplainado na costa (18/28 u), a ≥ 150 u da
  cidade, com água funda na ponta do cais. O **píer** é erguido no relevo
  (a faixa do cais vai ao nível do pátio), porque o movimento barra água.
- **Só em mar aberto.** Os 48 rumos consideram TODA passagem de terra pra água
  (não só a primeira: saindo do centro, a primeira água pode ser um lago), e o
  candidato só vale se a ponta do cais e 6 u além dela forem oceano —
  `Gerador::mar_aberto`: busca gulosa pra fora numa grade de 2 u, só por água,
  até passar do raio da ilha (lago esgota o contorno e é recusado). O teste
  `o_porto_da_no_oceano` confere as 4 ilhas.
- Nada de árvore, planta ou pedra nos dois (`Gerador::na_cidade`).
- Mudou o relevo: `VERSAO` do cache de altura sobe (hoje 4).

## Construções (`shared::construcao`)

Port das receitas do zone14: `Casebre` (casinha, média, sobrado, salão de
loja), `Armazem`, `Doca`, `Cabana`/cabana do cartógrafo, e os props `Lampiao`,
`Poco`, `Banco`, `Caixas`, `Barril`. Extras do Tempest: soleira, floreira sob a
janela, diagonais do enxaimel, lampião na porta das lojas, piso do sobrado.

O banco atende no porto e também na praça da cidade: a Banqueira usa o mesmo depósito do Banqueiro.

- `gerar(tipo, papel, seed)` / `gerar_prop(tipo, seed)` → `Construcao`.
- Voxel: `B_CASA = 0.5` (prédio), `B_PROP = 0.125` (prop). Cor em
  `BlocoCasa::rgb()`; `brilha()` = chama sem sombreamento.
- Eixos locais: célula `(ix,iy,iz)` ocupa `[i·escala, (i+1)·escala]`; fachada
  da porta em `z = 0`, olhando pra `−z`.
- Giro de 90 em 90 (`yaw_q`), pivô no centro do volume:
  `Construcao::local_para_mundo(pos, yaw_q, local)`.

## Disposição (`shared::vila`)

`Gerador::vila()` (ou `Ilha::vila()`) devolve `Vila { predios, props, npcs,
porto }`.

- Cidade: anel de ofícios virado pro poço (Alquimista, Ferreiro, Armaduras,
  Taberna, Alfaiate, Treinador, Identificador) e 2–3 casas de morador. Praça
  com poço, bancos e lampiões. Lote só onde o relevo já está no nível.
- Porto: doca, armazém, cabana do cartógrafo, caixas, barris, lampiões.
- NPC na porta de todo prédio de ofício, olhando pra fora; Capitão do Porto
  no cais olhando o mar. Só o Alquimista tem loja (3).
- `Predio.pos`: `xz` = pivô, `y` = origem do volume (alicerce 2 cm acima do
  chão; o piso interno fica no nível do terreno).

## Colisão

`vila::caixas_solidas` funde os voxels em caixas e guarda só o que fica na
altura do corpo (entre 0,31 e 1,7 acima do chão do prédio): paredes, ombreiras,
floreiras, cabeços do cais e o poço. Beiral, toldo, placa e telhado passam por
cima; alicerce e tabuado ficam no plano do chão. A porta é um vão, então dá pra
entrar. Vale em `sem_estorvo`, `cabe`, `trecho_sem_estorvo`, no A* e no
`mover_com_degrau` — só no servidor (o cliente não prediz).

## Servidor

`montar_cidade` cria as zonas seguras (cidade e porto) e um NPC por
`NpcDaVila`: `Npc(1)` + `VendorTag` pra quem tem loja, `Npc(9)` pro resto. Mob
não nasce na cidade nem no porto. O rumo do NPC vai no `EntityMeta.kind`
(`shared::kind_de_npc_yaw` / `npc_yaw_de_kind`).

## Viajar e Pergaminho de Teleporte

Tocar no **Capitão do Porto** abre o menu **Viajar** (`ServerMessage::Viagem`):
uma linha por ilha do `ARQUIPELAGO` — "você está aqui", **Embarcar**,
"sem barco agora" (servidor da ilha fora do ar) ou bloqueada com o passo da
história que libera ("Rumo à Geleira"). Liberada = a história chegou ao passo
de viagem daquela ilha (`viagem::liberada`); a inicial, sempre. Ir e voltar é
de graça. `ClientMessage::Viajar` confere de novo no servidor: perto do
Capitão (`PERTO_DO_CAPITAO`), ilha liberada, canal no ar; aí `embarcar` grava
a chegada na praça de lá e manda `TrocarZona`. Se o Capitão tem missão a
oferecer, o diálogo vem antes e o menu abre quando ele fecha.

O **Pergaminho de Teleporte** (id 359) foi retirado em 26/09/2026.
As unidades antigas viram 100 cobre cada. Viagens dentro da ilha usam a rota
do mapa e a montaria; o botão de teleporte saiu. Ver COMBATE_MONTADO_E_VIAGEM.md.

# Calendário de presença

Um prêmio por dia de login, no molde do check-in de 28 dias do MIR4
([28-Day Attendance](https://cs.mir4global.com/post/1113),
[Ringring's 28-Day Check-in](https://forum.mir4global.com/post/1734)): conta
no resgate, volta ao dia 1 no começo do mês e dá prêmio maior nos dias 7, 14,
21 e 28.

Código: regras e tabela em `crates/shared/src/presenca.rs` (fonte única),
resgate no servidor em `crates/server/src/world/presenca.rs`, janela em
`crates/client/src/presenca_ui.rs`.

## Regras

- **28 prêmios por mês.** O mês vira no dia 1 às **04:00 de Brasília** (07:00
  UTC, o mesmo reset das diárias e dungeons, `dungeon::RESET_UTC_S`) e a grade
  volta ao dia 1, mesmo incompleta.
- **Um resgate por dia de jogo** (dia vira às 04:00).
- **Dia perdido não quebra a sequência:** o próximo resgate pega o próximo
  prêmio da lista. Depois do 28º, espera o mês virar.
- **Por conta:** o progresso é da conta. O personagem que resgata recebe; os
  outros da conta veem o dia resgatado.
- **O banco decide** (`crate::presenca`, tabela `presenca_resgates`, uma linha
  por resgate): o pedido trava a conta (`pg_advisory_xact_lock`), lê as linhas,
  planeja com `shared::presenca::planejar` e insere, numa transação. Dois
  personagens da conta em canais (processos) diferentes no mesmo instante: um
  insere, o outro é recusado. `UNIQUE (conta, calendário, dia de jogo)` e
  `UNIQUE (conta, calendário, ciclo, dia da grade)` seguram mesmo sem o lock.
- **Entrega idempotente** (molde da carta do mercado): a linha nasce
  `aplicado = FALSE`; o canal põe o prêmio na bolsa uma vez por id e o save do
  personagem marca `aplicado` na **mesma transação** da bolsa. Se o canal cair
  antes do save, a linha fica pendente e o próximo login da conta (depois de
  2 min, `PENDENTE_APOS_S`) reserva a linha (UPDATE … RETURNING, atômico) e
  entrega de novo. Duplo toque, reconexão e segundo personagem: "Você já
  resgatou hoje".
- **Teste:** `MMO_PRESENCA_TESTE_OFFSET_S` adianta o relógio do calendário no
  servidor de teste; `MMO_PRESENCA_TESTE_PENDENTE_S` muda a janela do
  pendente. Bot: `cargo run --bin presencabot`.
- **Ouro** vai direto pro ouro do personagem; **item** vai pra bolsa; o que não
  couber vira carta nas **Entregas do Mercado** (motivo "Calendário de
  presença").
- Nada de TP, equipamento ou item negociável.

## Prêmios do mês ⚠️

| dia | prêmio | | dia | prêmio |
|---:|---|---|---:|---|
| 1 | 500 ouro | | 15 | 1.500 ouro |
| 2 | Poção de Vida ×10 | | 16 | Poção de Mana ×15 |
| 3 | Poção de Mana ×10 | | 17 | Poção de Vigor ×8 |
| 4 | Poção de Vigor ×5 | | 18 | Poção de Fortuna ×1 |
| 5 | 800 ouro | | 19 | 1.800 ouro |
| 6 | Poção de Vida+ ×5 | | 20 | Poção de Vida+ ×10 |
| **7** | **Poção de XP ×1 + Marcas ×10** | | **21** | **Poção de XP ×2 + Marcas ×20** |
| 8 | 1.000 ouro | | 22 | 2.000 ouro |
| 9 | Poção de Vida ×15 | | 23 | Poção de Mana+ ×8 |
| 10 | Poção de Mana+ ×5 | | 24 | Poção de Sorte ×1 |
| 11 | Poção de Fortuna ×1 | | 25 | 2.500 ouro |
| 12 | 1.200 ouro | | 26 | Poção de Vida+ ×12 |
| 13 | Poção de Vida+ ×8 | | 27 | Poção de Fortuna ×1 |
| **14** | **Poção de XP ×2 + Poção de Sorte ×1** | | **28** | **Poção de XP ×3 + Marcas ×40** |

Total do mês: 11.300 ouro, 8 Poções de XP, 70 Marcas da Tempestade, 3 de
Fortuna, 2 de Sorte e poções comuns.

**Vinculado:** Poção de XP e Marcas já eram vinculadas; Fortuna e Sorte passam
a ser (migração `presenca_vinculados_v1`, só saem de recompensa). As poções
comuns são as que o Alquimista vende — dar algumas não cria nada que não se
compre. Cobre, Darksteel, Pó Cintilante e chaves de craft **ficaram de fora**:
são negociáveis no mercado, e a pilha de item não tem vínculo por unidade
(só a peça de equipamento tem `vinculado` na instância). O teste
`marcos_sao_os_melhores_e_a_grade_so_tem_permitido` barra item fora da lista
`PERMITIDOS`.

## Eventos

`EventoDePresenca { id, nome, inicio_unix, fim_unix, substitui, grade }` em
`shared::presenca::EVENTOS` — **vazio = desligado**.

- Durante o período o evento aparece como aba própria na janela, com
  progresso próprio na conta (`DadosPresenca::eventos`).
- `substitui: false` complementa: os dois calendários valem no mesmo dia.
- `substitui: true` esconde o mensal enquanto durar (o mensal fica parado,
  sem perder o progresso).
- A grade do evento segue as mesmas regras (um por dia, sem quebra, marcos
  pelo dia da grade) e só pode ter item de `PERMITIDOS`.

Para ligar um evento: adicionar a entrada em `EVENTOS` com a grade e subir
servidor e app (a grade vai pelo protocolo).

## Interface

- **Ícone de presente** no topo do HUD, à esquerda do MENU, com selo vermelho
  quando há resgate hoje.
- **Menu → Aventura → Presença.**
- **Sozinha no login**, uma vez por sessão do app, quando há resgate.
- Grade de 7 × 4 com ícone e quantidade; marcos com borda dourada e ícone
  maior (o segundo prêmio no canto); o dia de hoje destacado; os já
  resgatados escurecidos com visto; toque num dia mostra os prêmios. Rodapé:
  resgatados X/28, contador até o próximo reset e o botão **Resgatar**.
- "Onde obter": item que sai do calendário mostra a fonte "Calendário de
  presença" com os dias, e o "Ir" abre a janela.

## Limites conhecidos

- Personagem que sai do jogo entre o pedido e a resposta do banco não recebe
  na hora: a linha fica pendente e chega no próximo login da conta (depois de
  2 min).

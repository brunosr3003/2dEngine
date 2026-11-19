# Barcos novos + PvP naval — status da implementação

Design: `docs/BARCOS.md` (barcos, tiers, pontos) e `docs/PVP-NAVAL.md` (combate,
abordagem, saque). Este arquivo diz **o que já está no código e o que falta**.

Branch: `worktree-tag-cla-preco-mercado`. Atualizado em 19/11/2026.

---

## Resumo

| Fase | O quê | Status |
|------|-------|--------|
| 1 | Dados, stats, migração, upgrade | **Começada** (só a base de dados) |
| 2 | UI no Estaleiro (pontos, galeria) | Não começada |
| 3 | Projéteis desviáveis | Não começada (já existe uma base no código) |
| 4 | Barco derrotado (fogo, reparo de vela) | Não começada |
| 5 | Abordagem + respawn com delay | Não começada |
| 6 | Saque + Naval Tokens + loja | Não começada |
| 7 | IA do barco (auto-return / auto-defend) | Não começada |
| 8 | Alianças entre tripulações | Não começada |

**Hoje nada disso muda o jogo.** Os tipos novos existem, mas nenhuma parte do
servidor ou do cliente lê eles ainda. Os barcos atuais (Sloop, Schooner,
Galleon, Man-o'-War) continuam funcionando como antes.

---

## O que já foi feito

### Commitado (`7bdb507`)

`crates/shared/src/navio.rs`, bloco "NOVO SISTEMA DE BARCOS":
- `Tamanho` (Pequeno / Médio / Grande) e `Variante` (Veloz / Ataque / Tanque).
- `TipoBarco`: as 9 combinações, com `id()` 0–8, `de_id()`, `nome()` em inglês
  ("Swift Sloop", "Ironclad Galleon"…) e `capacidade()` (2 / 4 / 8 passageiros).
- `StatsBase` e `stats_base(tipo)` com valores para os 9 tipos.
- `PONTOS_POR_TIER`: T1 10, T2 18, T3 26, T4 34.

`crates/shared/src/items.rs`, no fim de `NavioData`:
- `tipo_novo: Option<u8>` (`None` = barco antigo).
- `pontos: [u8; 4]` (velocidade, HP, dano, virada).

O `cargo check -p client` passou no brunjilinux com isso.

### Feito mas ainda não commitado

Em `navio.rs`, logo acima de `stats_base`:
- `tier_mult(tier)`: 0,6 / 0,9 / 1,2 / 1,5.
- `valida_pontos(tier, pontos)`: a soma não passa do limite do tier.
- `stats_finais_novo(tipo, tier, pontos)`: soma base + pontos e multiplica pelo tier.

---

## Problemas conhecidos no que já existe

1. **Os números de `stats_base` são provisórios e estão na escala errada.**
   O jogo mede velocidade em u/s (10 a 13,4; o fio aguenta 15,5) e virada em
   rad/s (0,7 a 1,5). Os valores escritos (velocidade 120, virada 180) não
   batem com isso, e "1 ponto = +1" quase não faz diferença no HP (2.000+).
   **Decisão pendente:** passar para unidades reais, com cada ponto valendo
   um % do stat (ex.: HP +4%, dano +4%, velocidade +2%, virada +3% por ponto).
2. **Aplicar o multiplicador do tier na velocidade fica estranho:** um T1 a
   60% andaria a ~6 u/s. Proposta: o tier pesa cheio em HP e dano e pouco em
   velocidade e virada (ex.: 0,85 / 0,95 / 1,05 / 1,15). E toda velocidade
   continua presa em `VELOCIDADE_MAX` (15,5).
3. **O tier do barco não está guardado em lugar nenhum.** As funções do navio
   só recebem o `NavioData`, não o item inteiro. Proposta: um campo `tier`
   em `NavioData`.
4. **Os testes de `navio.rs` não compilam.** A linha ~1353 monta um
   `NavioData` campo a campo, sem `..Default::default()`, e agora faltam os
   campos novos. O `check_cli.sh` só roda `cargo check -p client` e não pegou.
   Falta rodar `cargo test -p shared`.
5. **Protocolo:** `NavioData` vai no fio em postcard (posicional), então os
   campos novos mudam o formato. Produção está na **198** e a 199 é só de
   teste, então tudo bem ficar na 199 enquanto ela não for lançada. Se a 199
   sair antes disto ficar pronto, o protocolo tem que subir.

---

## O que falta fazer

### Fase 1 — base (próximo passo)
- [ ] Resolver os problemas 1 a 4 acima (unidades reais, tier no `NavioData`,
      testes compilando).
- [ ] Ligar o sistema novo nas funções que o jogo já usa: `casco_total`,
      `velocidade_do_navio`, `giro_max`, `mult_do_canhao` e `Visual::de`
      (escolher modelo/escala pelo tamanho). Se `tipo_novo` for `Some`, usa a
      conta nova; se for `None`, a de hoje. Assim o servidor, o HUD e o
      combate usam os stats novos sem mexer em cada chamada.
- [ ] **Capacidade:** achar onde o convés aceita passageiros e limitar por
      `TipoBarco::capacidade()`.
- [ ] **Migração dos barcos antigos:** função que converte Sloop / Schooner /
      Galleon / Man-o'-War para tipo + tier novos e as estrelas em pontos,
      sem o jogador perder nada. Falta decidir o mapeamento exato.
- [ ] **Upgrade T1→T4:** 2 barcos iguais (tipo + tier) + ouro = 1 do tier de
      cima. Falta decidir se os pontos se mantêm ou resetam.
- [ ] **Respec:** zerar os pontos por um custo em ouro.
- [ ] **Como se ganha cada um dos 9 tipos:** craft no Estaleiro? Com quais
      materiais? Hoje só existe a escada Sloop → Man-o'-War.
- [ ] Item id / nome / ícone para os tipos novos (ou um item só, com o tipo
      dentro da instância).

### Fase 2 — UI (Estaleiro, `client/src/estaleiro_ui.rs`)
- [ ] Painel de pontos: 4 stats com +/−, pontos usados/disponíveis, preview
      dos stats finais, botão aplicar e botão respec.
- [ ] Galeria 3×3 dos tipos com stats base, capacidade e preview 3D.
- [ ] Tela de upgrade de tier.
- [ ] Skins de casco (estilo skin de montaria). Velas e figuras já existem.

### Fase 3 — projéteis desviáveis
Já existem `tempo_de_voo`, `mira_com_avanco` e `acertou` em `navio.rs`.
**Primeiro passo:** ver o que já é projétil de verdade hoje e o que é acerto
instantâneo.
- [ ] Projétil com tempo de voo, que erra se o alvo desviar.
- [ ] Broadside em leque, Long Gun rápido, Chain Shot com lentidão (skill nova).
- [ ] Efeito visual de disparo, rastro e impacto/splash.

### Fase 4 — barco derrotado
- [ ] HP 0 deixa o barco parado em vez de ele sumir: velas rasgadas, fogo,
      fumaça, inclinado, à deriva.
- [ ] Timer para afundar sozinho (10–15 min).
- [ ] Capitão escolhe: **Afundar** (todos vão para o cais da última ilha) ou
      **Reparar vela** (30–60 s parado no mastro, cancela com dano ou se andar;
      ao terminar o barco volta com 10% de HP). Ilimitado, sem material e sem
      cooldown.

### Fase 5 — abordagem e PvP no convés
- [ ] Pular para o convés de um barco a 5–10 u. Vários barcos ao mesmo tempo.
- [ ] Respawn com delay crescente no mesmo combate (5 s, 10 s, 15 s…), no
      próprio barco se ele estiver inteiro.
- [ ] Barco afundado: morte real (−2% XP, −50 de proficiência naval, nunca
      desce de nível) e volta ao cais da última ilha.

### Fase 6 — saque e Naval Tokens
- [ ] Baú do barco derrotado: canalizar 10 s, interrompido por dano.
- [ ] Loot só gerado pelo sistema (500–2.000 de ouro, 1–3 Naval Tokens,
      chance de material raro). **Nunca** sai item do jogador.
- [ ] Barril boiando quando o barco afunda: 15 min, coleta de 5 s, aparece no
      mapa depois de 5 min, o dono também pode pegar.
- [ ] Naval Tokens: item/moeda, NPC loja no cais (skins, velas, figuras, pets)
      e fontes PvE (entregas, exploração, daily).

### Fase 7 — IA do barco
- [ ] Todos abordaram outro barco: o seu fica virando para ser difícil de acertar.
- [ ] Tripulação morta ou desconectada: o barco volta sozinho para o cais.
- [ ] Caiu a conexão: 2 min no convés e depois vai para o cais em segurança.

### Fase 8 — alianças
- [ ] Convite entre líderes de tripulação, aceitar ou recusar.
- [ ] Aliados não se atacam e aparecem marcados.
- [ ] Loot dividido igual entre os aliados a até 100 u.

---

## Ainda falta decidir
- Mapeamento dos barcos antigos para os tipos novos.
- Se os pontos se mantêm no upgrade de tier.
- Quanto vale cada ponto em cada stat (itens 1 e 2 acima).
- Como se consegue cada tipo de barco e quanto custa.
- O que exatamente acontece com HP 0 além do fogo (o dono tem uma ideia
  própria; por enquanto fica só o gatilho).

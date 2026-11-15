# Reset de nível e árvore de caminhos

Desenho fechado com o dono em 14/11/2026. **Nada disto está implementado
ainda.** Substitui o desenho de 12/11 (reset com subclasse escolhida de cara e
skills 4–6 por arma).

## A ideia

O jogo vira um laço curto e animado: subir do 1 ao 100 rápido, resetar,
ganhar poder permanente e subir de novo. A progressão longa vem do
**equipamento e da árvore**, não de um muro de XP. O motivo: muita gente
abandona no começo, e o começo era justamente o trecho mais lento (dados de
produção, 08–14/11: casuais 3–5× mais lentos que o melhor caso no 30–60).

## Regras

### Nível

- **Teto de nível: 100.** Tudo que hoje passa de 100 (Porão 102, Grutas 110 e
  118, a cadeia de Abyssia 100–117) vira a faixa **"100+"**.
- **Na faixa 100+ a métrica é o poder**, não o nível: o conteúdo pede um
  poder (a conta da ficha, `dungeon::poder_de_stats`). Mobs continuam podendo
  ter nível acima de 100 por dentro.
- **Sem trava de nível em equipar** (já não há desde a 198), **em craft**
  (`craft::conferir` ainda checa `nivel_min`) **nem em viagem** (a única é a
  Geleira no 20, `world.rs` `abrir_menu_viagem`). Quem passa a frear é o poder
  e o custo dos materiais.
- **PvP protegido por poder, não por nível** (`pk::pvp_mult_nivel`): um
  resetado de nível baixo com equipamento de 100 não pode caçar iniciante.

### Ritmo da primeira subida

- Alvo: **tryhard (15–16 h/dia) do 1 ao 100 em ~2 semanas.**
- Já é o ritmo atual: `RITMO_MAX` 1,25 (melhor caso 1→100 ≈ 170 h; tryhards
  reais rodam a ~1,3× o melhor caso ≈ 220 h). Não mexer na curva por ora.

### O reset

- Disponível no **nível 100**, num NPC (a definir qual).
- **Zera:** nível, XP, pontos de atributo distribuídos.
- **Mantém:** itens e equipamento vestido, ouro, TP, pets, montarias,
  proficiências, história concluída (as ilhas continuam abertas).
- **XP depois do reset:** continua pelo nível/poder do mob. O resetado caça o
  mais forte que aguenta e sobe muito rápido no começo (o custo de cada nível
  cresce com o quadrado), desacelerando sozinho — é a "sensação de poder" que
  o dono quer. Trocar o XP por **poder do mob** em vez de nível do mob é
  desejável, mas não bloqueia nada.
- **Divisor por reset** em cima do ritmo, pra cada subida pesar mais. Ainda a
  calibrar: `2^resets` dobra rápido demais pra "vários resets" (3ª subida ≈ 8
  semanas pro tryhard); sugestão `1,5^resets` (2, 3, 4,5, 7 semanas).
- **Bônus por reset** (do desenho de 12/11, mantidos): +3% atributos,
  +15% coleta / material / pó / Darksteel, e aura de corpo inteiro estilo DBZ
  (1 verde, 2 azul, 3 roxa, 4 dourada, 5 buraco negro, 6+ só intensidade).
- **Ranking de nível** ordena primeiro por resets, depois por nível.

### Árvore de caminhos (no lugar da subclasse)

- **Pontos:** 1 a cada 10 níveis, **a partir do primeiro reset** (10 pontos
  por subida).
- **Quatro caminhos.** Cada um é uma trilha de passivas que, indo fundo,
  libera **ativas**; a build pode misturar caminhos (passivas variadas) ou ir
  fundo num (chega nas ativas):
  - **Assassino:** crítico, velocidade, perfuração… → ativa de
    **invisibilidade**.
  - **Mago:** dano mágico, mana, área… → ativa de magia em área.
  - **Guardião:** defesa, vida, bloqueio… → ativa de provocar / escudo.
  - **Corsário:** pirataria e economia — saque (drop), coleta, mineração,
    ouro… → ativa ligada a saque/tesouro.
- As ativas da árvore ocupam **novos slots de skill (4, 5, 6)**, além das 3
  da arma.
- Redistribuir os pontos custa ouro.
- Nós, números e ativas exatas: a desenhar com o dono.

### Porões de resetado

- **Cortado.** Com o 100+ por poder e sem trava de nível/viagem, o desafio do
  resetado é o conteúdo 100+ que já existe (e estágios de poder maiores nele).

## Etapas

1. **Release A — base (ajuda já quem está abandonando):** teto 100, "100+" por
   poder, sem trava de nível em craft/viagem, PvP por poder.
2. **Release B — o reset:** coluna `resets` no personagem, NPC, divisor por
   reset, bônus de atributo e coleta, aura, ranking, e a árvore com as
   passivas.
3. **Release C — as ativas:** slots 4–6 e as ativas de cada caminho.

## Armadilhas mapeadas no código (14/11)

- **Skills em array fixo:** `ProgressoDeSkills { tiers: [u8; 12], … }` é JSON
  com tamanho fixo; crescer o array sem migração faz o `unwrap_or_default` do
  load **zerar as skills de todo mundo**.
- **Slots de skill fixos em 3:** `skills::DESTRAVA_EM`, `destravada()`,
  `client/habilidades.rs` (`0..3`), `desktop.rs` (teclas 1–3), e
  `world.rs` (`.min(2)` no gesto).
- **Admin `SetLevel`** (molde do reset) não manda `ProgressUpdate` nem marca
  `nivel_mudou`: copiar como está deixa o cliente com o nível velho.
- **Coisas derivadas do nível que mudam quando ele cai:** firmeza no 60,
  bônus da pistola (`nivel/3`), `for_da_armadura`, acesso à Ilha Mágica,
  `min_level` de missão, preço do perdão de PK, perda de XP na morte.
- **Aura e ranking por reset precisam de campo novo** no protocolo (o `u64`
  de auras do `EntityMeta` está cheio) → sobe `PROTOCOL_VERSION`.
- **Cópias da conta de stats** em `pvp_sim.rs`, `balanceamento.rs` e
  `world/ranking.rs` chamam `effective_stats` / `destravada` direto: todas
  precisam do número de resets.
- **Poder de referência das dungeons usa a ladder**, que subestima o jogador
  real (~1,6×, medido com `server --poder`): calibrar antes de travar o 100+
  por poder.

# Reset de nível e árvore de caminhos

Desenho fechado com o dono em 14/11/2026. Substitui o desenho de 12/11
(reset com subclasse escolhida de cara e skills 4–6 por arma).

**Estado:** etapa A feita no branch da 199 (15/11/2026): teto 100
(`progressao::NIVEL_MAX`, `somar_xp`), "100+" na tela (`texto_de_nivel` /
`texto_de_faixa`), dungeon e missão pedem no máximo nível 100
(`dungeon::cadeado`, `quests::nivel_pedido`) e passam disso só por poder,
sem trava de nível em craft (`craft::conferir`, `craft_ui::motivo`) nem em
viagem (Geleira), PvP por poder (`pk::pvp_mult_poder`,
`pk::protegido_por_poder`). Os três pontos que ficaram de fora também
(15/11): o poder de referência das dungeons segue o jogador REAL
(`dungeon::fator_real`, 1,0 até o nível 20 subindo a 1,6 no 40), o
simulador de PvP do Panopticon usa o gap por poder (`Regras::por_poder`), e o
poder de cada jogador vai no `EntityMeta.poder` pra o cliente aplicar a mesma
proteção. Etapas B e C: não começadas.

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

## Mundo novo: 1 ao 100 em uma semana (decidido 15/11/2026)

O dono: "colocar realmente a progressão bem rápida, tipo 1 semana no
tryhard pro lvl 100", com o mundo encurtado e mais denso.

- **Faixas por ilha (níveis NOVOS):** Bosque 1–30, Geleira 30–50, Ermo
  50–70 (precisa de muito mais mob), Planalto 70–100. Celeste, Kōgen e
  Abissal: "100+" por poder.
- **Dentro da ilha o nível sobe do porto (norte) pro sul.**
- **A força não muda (opção b):** o 100 novo tem a força do 60 de hoje (fim
  do Planalto). Uma conversão única nível novo → antigo, por faixas
  (1–30 → 1–15, 30–50 → 15–30, 50–70 → 30–42, 70–100 → 42–60), alimenta tudo
  que é força (ladder, mob, item, chave, drop, poder de referência), pra
  balança calibrada ilha por ilha continuar valendo.
- **Ilha Mágica:** continua como está; só a trava dela muda de nível pra
  poder.
- **Curva de XP:** melhor caso 1→100 ≈ 75–80 h (tryhard ≈ 100 h reais ≈ 1
  semana).
- **Missões:** história e missões de ilha vão pros níveis novos; Ermo e
  Planalto ganham missões pros níveis que ganharam; linha de missões do
  reset (até o NPC no 100, e a de volta apresentando a árvore).
- **Personagens existentes:** ficam com o nível/XP que têm, e **todo mundo
  volta pra ilha inicial**; cada um descobre onde aguenta upar.
- **Lançamento:** nada vai pra produção agora. Tudo é planejado e testado na
  versão de teste; o dono decide depois quando lançar.

### Como foi feito (15/11/2026, versão de teste)

- **As faixas novas são as de hoje renumeradas**, então o jogo continua com
  os níveis INTERNOS (mob, chefe, dungeon, missão, item, chave, curva de XP)
  e só a TELA converte: `progressao::ANCORAS_DA_ESCALA` (1→1, 15→30, 30→50,
  42→70, 60→100), `nivel_exibido(xp)`, `xp_do_nivel_exibido(n)` (barra de
  XP), `texto_de_nivel` / `texto_de_faixa` ("100+" acima do 60 interno).
  Personagens existentes ficam coerentes sozinhos.
- **Teto:** `NIVEL_MAX` = 60 interno (aparece 100); `NIVEL_MAX_EXIBIDO` = 100.
- **Ritmo:** o 1,25 atual já dá ~57 h no melhor caso até o 60 interno (~5
  dias de tryhard a 15 h/dia): a curva não precisou mudar.
- **Ilha Mágica por poder:** `NivelMagico::poder_exigido` (poder de
  referência do nível que abria o andar: I ≈ 2 mil, VI ≈ 11,8 mil);
  `maior_liberado(poder)`.
- **Ermo mais denso:** `world::densidade_da_ilha` — hordas a 70 u (eram 90)
  e 26 mobs por horda (eram 18): ~2,4× mais mob.
- **Degradê dentro da ilha:** já existe — o nível das zonas sobe com a
  distância da cidade (a mais fraca perto do porto/cidade).
- **Missões novas:** Ermo 1100–1109 (8 em cadeia + 2 contratos), Planalto
  1110–1129 (14 em cadeia + 3 contratos), no Mestre de Missões, com
  tradução.
- **Voltar todo mundo pra ilha inicial:** `scripts/voltar-ilha-inicial.sh`
  (com o jogo PARADO; `--sim` pra ver antes). Não roda na produção até o
  dono decidir lançar.

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

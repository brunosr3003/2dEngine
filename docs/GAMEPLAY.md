# Gameplay Design

Referências: **Realm of the Mad God** (loop MMO, permadeath, AOI),
**Wind Waker** (exploração de ilhas, navegação),
**Albion Online** (classless, full-loot PvP, economia),
**Sea of Thieves** (espírito pirata coop).

Visual: **Mana Seed** — sprites 64×64px modulares (paper doll), tiles 16×16px, paleta vibrante aventureiro.

---

## Pilares

1. **Ação direta, sem turnos.** Player mira com mouse/touch, atira, desvia. Skill > stats.
2. **Morte importa.** Downed State → execução por outro player → perda de loot e durabilidade.
3. **Sem classes.** Build é definida por arma equipada + proficiências levantadas pelo uso.
4. **Exploração aberta.** Ilhas navegáveis, conteúdo descoberto — não "escolhe portal num menu".
5. **Coop com estranhos.** Grupos formados na hora. Veteranos viram Raid Bosses ambulantes.

---

## Loop principal

```
spawn no Porto Central (hub, safe zone)
  ↓
pega barco → navega até ilha (tier escolhido)
  ↓
explora, mata mobs, coleta loot, ganha XP de proficiências
  ↓
é derrubado (Downed) OU limpa a ilha
  ↓
volta ao Porto, deposita loot permanente no baú, repete
```

---

## Sistema Classless — Proficiências por Uso

Todos os jogadores começam iguais. A build emerge do que vc usa.

### Nível Principal
- Ganho por XP geral (matar mobs, completar ilhas).
- Cada level dá **Pontos de Atributo** pra distribuir livremente em:
  `FOR` (dano físico, HP), `DES` (velocidade, crítico), `INT` (dano mágico, MP), `VIT` (HP max, regeneração), `SPD` (velocidade de movimento).
- **Level cap: 100.** O nível principal define o **cap máximo** de cada Proficiência.

### Proficiências
- Evoluem pelo uso real: bater com espada → XP em "Espada". Lançar fogo → XP em "Magia de Fogo".
- Cada proficiência desbloqueia passivas e melhora multiplicadores daquele estilo.
- Exemplos: Espada, Arco, Cajado, Magia de Fogo, Magia de Cura, Navegação, Pesca, Ferraria.
- Cap de proficiência = f(nível principal) → impede ser bom em tudo ao mesmo tempo.
- **Balanceamento natural:** ter 100 em Espada E 100 em Magia exige atributos divididos — espada fraca se INT alta, magia fraca se FOR alta.

### Equipamento define estilo
- **Espada/Machado:** melee, dano direto, low range.
- **Arco/Besta:** ranged, projéteis retos, kite.
- **Cajado/Varinha:** magia (fogo, cura, lightning), AOE ou seeking.
- **Slots:** arma + armadura + 2 anéis. Anéis dão habilidades ativas swappable.

---

## Sistema de Morte Hardcore — Downed State

### Fase 1: Downed
- HP chega a 0 → jogador **não morre imediatamente**. Cai no chão (Downed).
- No chão: velocidade 20% do normal, sem habilidades, pode rastejar para se esconder.
- Barra de "Downed HP" (dhp): outros players podem reduzi-la atacando o corpo.

### Fase 2: Execução
- **Monstros não executam.** Apenas outros players podem executar (cast de 3s no corpo).
- Se ninguém executar: auto-revival após timer (30s–120s dependendo da zona), levanta com 5% HP.
- Se executado: **morte real** → perde todo o loot coletado na sessão + dano massivo de durabilidade no equipamento vestido (pode quebrar se chegar a zero).

### Mecânica de Carregar
- Player vivo pode carregar um Downed nas costas.
- Quem carrega: não pode atacar, penalidade de movimento –40%.
- **Resgate:** aliado te tira do meio do combate e te carrega para safe zone.
- **Sequestro:** inimigo te carrega para area deles para saquear ou pedir resgate.

---

## Sistema de Aura / Poise (Prestígio PvP)

- **Aura** é uma segunda barra de XP, ganha exclusivamente em PvP.
- Ganho: vencer combate contra player (derrubá-lo) dá XP de Aura proporcional à Aura do inimigo.
- Perda: ser executado causa **perda massiva** de Aura (parcialmente absorvida pelo executor).
- **Efeito em combate:** Aura alta = resistência a stagger de players com Aura inferior. Efeito visual ao entrar em combate.
- **Pirâmide de prestígio:** veteranos com Aura alta evitam lutas desnecessárias (risco de perder semanas de XP). Viram Raid Bosses ambulantes — derrubá-los gera enorme ganho de Aura.

---

## Navegação e Ilhas

- **Porto Central (hub):** safe zone, NPCs (vendedor, reparador, banco), doca de barcos, baú pessoal.
- **Barco:** item equipável de transporte. Permite navegar entre ilhas. Pode ser atacado em zonas PvP.
- **Ilhas Rasas (tier 1):** piratas fracos, caranguejos, tartarugas. Farm de XP seguro.
- **Ilhas Vulcânicas (tier 2):** golems de lava, serpentes, armadilhas. PvP ativo.
- **Ilhas Amaldiçoadas (tier 3):** mortos-vivos navais, brumas, low-visibility. Full-loot PvP.
- **Ilhas de Boss:** portal raro, entrada em grupo de até 6, boss único com mecânicas específicas.

---

## Controles

### Desktop
- **WASD:** mover.
- **Mouse:** mirar.
- **LMB:** ataque básico.
- **RMB / Space:** habilidade ativa (do anel equipado).
- **Shift:** dash (I-frames 300ms, cooldown 2s).
- **E:** interagir (loot, NPC, barco, reviver aliado).
- **F:** carregar/largar Downed.
- **T:** chat. **Tab:** mapa. **Esc:** menu.

### Touch (mobile)
- Joystick virtual esquerdo: mover.
- Joystick virtual direito: mirar + atacar ao soltar.
- Botões dedicados: habilidade, dash, interagir.

---

## Progressão e Morte

- Level cap 100. Morre (executado) → perde o personagem em modo **permadeath configurável** (pode ser soft).
- Loot coletado na sessão é perdido na morte. Loot depositado no baú do Porto é permanente.
- **Fama** (meta-currency): acumula mesmo na morte. Libera slots de personagem extra, skins, pets cosméticos.
- **Sistema de desbloqueio de ilhas:** ilhas high-tier só aparecem no mapa se alguém no shard já atingiu determinada Fama.

---

## Loot

- Raridade: Common → Uncommon → Rare → Epic → Legendary → Relic.
- Drops ficam visíveis no chão como sprite. Free-for-all (qualquer um pode pegar).
- Equipáveis: arma, armadura, anel ×2.
- Consumíveis: poção HP, poção MP, scroll de teleporte, kit de reparo.
- Durabilidade: todos os equipamentos têm durabilidade. Reduz com uso e na morte.

---

## Combate

- Servidor autoritativo. Cliente manda apenas intent (InputFrame).
- Cooldown de ataque: 250ms–800ms dependendo da arma.
- Damage: `(ATK - DEF) * variance(0.9..1.1)`, mínimo 1.
- Projéteis: `lifetime`, `speed`, `damage`, `owner`. Broadphase pelo spatial grid.
- Dash: I-frames 300ms, cooldown 2s.

---

## Social / Party

- `/invite <username>` para convidar. Party de até 6.
- XP compartilhado dentro da AOI comum.
- Chat: `/s` (local), `/p` (party), `/g` (guild), `/w <user>` (privado).
- Trade P2P direto (sem auction house inicial).

---

## Economia

- Sem gold de mercado no início. Troca P2P + fee de reparação como sink.
- Fama não é vendável.
- Itens têm durabilidade — cria demanda por kits de reparo e ferraria.

---

## Balanceamento

- Stats, cooldowns e tabelas de loot em arquivos TOML em `data/balance/` — lidos em runtime.
- Analytics: kills/deaths/dps/tempo-por-zona → ajuste sem recompilar.

---

## Anti-padrões evitados

- ❌ P2W. Zero vantagem paga em combate. Só cosméticos.
- ❌ Classes fixas. Build é escolha, não prisão.
- ❌ Lobby de matchmaking. Porto Central É o lobby.
- ❌ Grinding sem fim. Cap 100 + permadeath empurra rotatividade de personagens.
- ❌ 3D. Nunca.

# Gameplay design

Referências estéticas/mecânicas: **Realm of the Mad God**, **Forager**,
**Crystalshire**, **Stardew Valley (combate)**.

## Pilares

1. **Ação direta, sem turnos.** Player mira com o mouse/touch, atira,
   desvia. Skill > stats.
2. **Morte importa.** Quando morre, perde progresso da sessão (*roguelike
   lite* — configurável). Motiva jogar com cautela em zonas altas.
3. **Coop com estranhos.** Pequenos grupos formados na hora. Sem
   raid-planejamento pesado.
4. **Pixel art simples, silhuetas legíveis.** 16×16 por tile, sprites de
   16–32px. Cores saturadas distintas por inimigo/projétil para leitura
   rápida em telas cheias.

## Loop principal

```
spawn no Nexus (hub social, safe zone)
  ↓
escolher portal (biome / dificuldade)
  ↓
explorar, matar mobs, coletar loot, subir nível
  ↓
morre OU limpa a zona
  ↓
volta ao Nexus, deposita loot permanente, repete
```

## Controles

### Desktop
- **WASD / setas**: mover.
- **Mouse**: mirar.
- **Botão esquerdo**: ataque básico.
- **Botão direito / Space**: habilidade da classe.
- **E**: interagir (loot, portal, NPC).
- **Shift**: dash (cooldown).
- **T**: abrir chat.
- **Tab**: mapa.
- **Esc**: menu.

### Touch (mobile)
- **Joystick virtual** canto inferior esquerdo: mover.
- **Joystick virtual** canto inferior direito: mirar + atacar (solta = atira).
- **Botão** habilidade + dash dedicados.

### Gamepad (opcional)
- Stick L move, stick R mira + atira ao puxar.
- A/X habilidade, B/O dash, Y/△ interagir.

## Classes iniciais (Fase 4)

| Classe | Arma | Habilidade | Estilo |
|---|---|---|---|
| Archer | arco (projétil reto rápido) | múltiplas flechas em cone | kite |
| Warrior | espada (melee curto) | charge com dano | close-combat tankier |
| Wizard | varinha (projétil seeking lento) | AOE radial | burst |

Stats base por classe: HP, MP, ATK, DEF, SPD, DEX, WIS. Subir nível dá
pontos para distribuir.

## Progressão

- **Level 1–20** por personagem. Morre → perde o personagem (soft
  permadeath). Loot guardado no "fame/vault" permanente.
- **Fame** (meta-currency) acumula entre personagens. Libera slots de
  personagem, skins cosméticas, pet cosmético.
- **Sistema de "desbloqueio de zonas"**: portais aparecem aleatórios;
  zonas high-tier só aparecem se alguém no shard já chegou em X fama.

## Zonas

- **Nexus (hub):** safe, NPCs vendedores, bau pessoal, portais para zonas.
- **Grassland (tier 1):** goblins, slimes, lobos. Farm de XP fácil.
- **Desert (tier 2):** escorpiões, cacti, sand-worms. Projéteis mais rápidos.
- **Cavern (tier 3):** morcegos, golems, cristais explosivos. Low-visibility.
- **Boss dungeons:** rare portal spawn, entrada em party de até 6.

## Loot

- Tier de raridade (Common, Uncommon, Rare, Epic, Legendary, Relic).
- Drops visíveis no chão como sprite ao morrer mob. Qualquer um pega.
- Items equipáveis: arma + 2 anéis + armadura.
- Consumíveis: poção de HP, MP, scroll de teleporte.

## Combate

- **Servidor autoritativo.** Cliente só manda intent.
- **Cooldown de ataque:** 250ms–800ms por classe.
- **Damage calc:** `(ATK - DEF) * variance(0.9..1.1)`, clamp min 1.
- **Projéteis** têm `lifetime`, `speed`, `damage`. Colisão broadphase
  pelo spatial grid.
- **Dash:** immunidade I-frames 300ms, cooldown 2s.

## Party / social

- Invite por /invite username.
- Party compartilha XP quando dentro de AOI comum.
- Loot table "roll" vs "livre" (free-for-all por default).
- Chat por canal: /s (say — local AOI), /p (party), /g (guild), /w user.

## Economia

- Sem gold, sem AH inicial. Troca P2P só (player-to-player trade janela).
- Meta-currency (fame) não é vendável.
- Futuro: mercado com fee para desestimular hoarding.

## Dificuldade e balanceamento

- Ticks, cooldowns e stats em arquivos TOML em `assets/balance/`, leitos
  em runtime pelo servidor. Permite balance pass sem recompilar.
- Analytics: log de kills/deaths/dps/tempo-por-zona → tabela para ajustar.

## Anti-pattern deliberadamente evitados

- **Grinding infinito sem conteúdo novo.** Level cap baixo (20) empurra
  alternância entre classes.
- **P2W.** Zero vantagens pagas em combate. Só cosméticos.
- **Lobby antes do jogo.** Nexus É o lobby — ninguém olha tela de
  matchmaking.

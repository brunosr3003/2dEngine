# Combat Animation System — Master Plan

> Criado: 2026-04-24
>
> Plano de arquitetura e roadmap para levar o jogo de "walk+idle" para um
> sistema completo de combate animado (melee, ranged, polearm) usando os
> assets Mana Seed Character Base 2.5c inteiros.
>
> Ler antes: [CHARACTER_ASSETS.md](./CHARACTER_ASSETS.md) — inventário de
> arte disponível e limitações conhecidas.

---

## TL;DR

Substituir o código ad-hoc de walk+idle no `PlayerPaperDoll` por um
**framework de animação data-driven** (AnimationDef + CharacterAnimator)
que consome os sheets Mana Seed em qualquer página (`p1`, `pONE1-3`,
`pBOW1-3`, `pPOL1-3`), acoplado a uma **máquina de estados de combate**
(casual → draw → combat_idle → attack → hurt/dead) sincronizada via
**eventos de ação autoritativos do servidor**. Entregue em 8 milestones
incrementais — cada um jogável por si só.

---

## 1. Visão (End State)

O jogador entra no mundo com seu personagem customizado (pele + cabelo +
roupa + arma). Em paz, anda em `p1B` com a arma visível nas costas. Ao
pressionar um botão de ataque, entra em guarda (`pONE2` idle): braço
erguido com a espada à frente, escudo à esquerda. Clicar LMB executa
slash1 → slash2 → thrust alternando. RMB executa parry/shield-bash.
Dash executa dodge. Tomar dano reproduz uma animação curta de hurt; cair
para 0 HP reproduz a dead sequence. Outros jogadores, NPCs e bosses
usam o mesmo sistema — com variações de arma (arco usa `pBOW`, lança
usa `pPOL`), cor de pele/cabelo/roupa e palette de arma por raridade.
Tudo autoritativo no servidor; o cliente prediz para latência zero.

---

## 2. Estado Atual

| Componente | Status |
|------------|--------|
| `PlayerPaperDoll.cs` | ✅ Reescrito para Character Base 2.5c (p1 idle+walk) |
| 3 layers (body, outfit, hair) como child SpriteRenderers | ✅ Funcionando |
| Direção de facing em 4 cardinais (hysteresis para diagonais) | ✅ Funcionando |
| Input: LMB/RMB/Dash já enviados ao servidor | ✅ |
| Dano visual = flash branco (`FlashHit` em `EntityRenderer`) | ⚠️ Placeholder |
| Morte = entidade some | ⚠️ Placeholder |
| Arma visível no personagem | ❌ Não implementado |
| Animações de combate (slash/thrust/parry/dodge) | ❌ Não implementado |
| Stance de combate (guarda vs casual) | ❌ Não implementado |
| Customização visual (skin/hair/outfit) | ❌ Fixo em dap1 + fstr + v01 |
| Visual dos outros players | ⚠️ Todos iguais ao self |

---

## 3. Arquitetura

### 3.1 Cliente — Pipeline de Renderização do Personagem

```
Input (teclado/mouse)
  │
  ├─► Local prediction ──────────┐
  │                               ▼
  └─► Network (SendInput) ──► NetClient ◄── Server Snapshot
                                  │              │
                                  │              ├─ position, velocity
                                  │              ├─ hp, hp_max
                                  │              └─ action: {kind, dir, start_tick}
                                  │                 visual_config
                                  ▼
                          EntityRenderer
                                  │
                                  ▼
                     ┌─ CharacterAnimator ─┐
                     │  ▲                  │  state machine
                     │  │                  │  (Casual→Combat→Attack→…)
                     │  └── AnimationDef   │
                     └──────────┬──────────┘
                                │ chooses page + cell per dir
                                ▼
                          ┌─ PaperDoll ─┐
                          │             │
                          │ 9 layers    │ body, bot, out, clo, fac,
                          │ as child    │ har, hat, 6tla (wpn A),
                          │ SpriteRenderers  7tlb (wpn B / shield)
                          └─────────────┘
                                │
                                ▼
                      Per-layer sprite from
                      Resources.LoadAll<Sprite>(page sheet)[cell_index]
```

**Três novas classes:**

- **`AnimationDef`** (struct, imutável): identifica *um* clip de animação.
  Define a página (sheet), os índices de célula por direção (S/N/E/W),
  a quantidade de frames, duração por frame, loop, prioridade.

- **`CharacterAnimator`** (MonoBehaviour): executa o AnimationDef atual,
  avança frame por Time.deltaTime, sinaliza `OnComplete`. Implementa a
  máquina de estados via tabela de transições (ver §3.3). Não sabe nada
  sobre sprites — apenas expõe `CurrentAnimation`, `CurrentFrameInDir`.

- **`PaperDoll`** (MonoBehaviour): compõe `SpriteRenderer` por camada
  visual e, a cada frame, lê `animator.CurrentAnimation.page` +
  `animator.CurrentFrameInDir` e aplica o sprite correto de cada layer.
  Também gerencia sortingOrder por direção (layer-order tables).

> `PlayerPaperDoll.cs` atual fica deprecated na M1 e é substituído pelo
> par CharacterAnimator + PaperDoll.

### 3.2 Layer Stack (PaperDoll)

Mana Seed tem 9 layers possíveis identificadas pelo prefixo no nome do
arquivo. Nem todas são sempre visíveis; carregadas sob demanda:

| Ordem | Layer | Prefixo arquivo | Quando carregar | sortingOrder base |
|------:|-------|-----------------|-----------------|-------------------|
| 0 | Back cloak | `0bot` | Se cloak equipado | 8 |
| 1 | Body | `0bas` | Sempre | 9 |
| 2 | Outfit | `1out` | Sempre | 10 |
| 3 | Front cloak | `2clo` | Se cloak equipado | 11 |
| 4 | Face | `3fac` | Se goggles/bandana | 12 |
| 5 | Hair | `4har` | Sempre | 13 |
| 6 | Hat | `5hat` | Se chapéu | 14 |
| 7 | Weapon A | `6tla` | Se arma (sword/axe/mace) | 15 |
| 8 | Weapon B / Shield | `7tlb` | Se escudo ou aljava | 16 |

**Ordem por direção** — para algumas direções (ex. N = costas) o escudo
precisa ficar *atrás* do corpo. Cada page + layer tem um offset de
sortingOrder por direção, extraível dos `_guides/layer order, page X.png`.
Implementação: tabela `int[4]` (S/N/E/W) de offset por layer+página.
Fallback seguro: todos layers em ordem natural quando não houver exceção.

### 3.3 Máquina de Estados de Combate

**Prioridade** (maior vence ao conflitar):

| Prioridade | Estado | Origem |
|-----------:|--------|--------|
| 100 | Dead | Servidor (hp ≤ 0) |
| 90 | Hurt | Servidor (dano) |
| 80 | Attack (slash/thrust/bash/shoot) | Input + Servidor |
| 75 | Dodge | Input (Dash) |
| 70 | Parry | Input (RMB) |
| 60 | Draw / Sheath | Transição stance |
| 30 | Combat_Moving | Velocity > 0 em stance combate |
| 20 | Combat_Idle | Em stance combate, parado |
| 15 | Casual_Walking | Velocity > 0, out-of-combat |
| 10 | Casual_Idle | Fora de combate, parado |

**Transições principais:**

```
Casual_Idle ──LMB/RMB──▶ Draw ──(end)──▶ Combat_Idle
                                              │
                          ┌───────────────────┤
                          │                   │
                        LMB                   │◀── (combat_timeout 3s)
                          ▼                   │
                        Attack ──(end)────────┤
                                              │
                         RMB ─▶ Parry ────────┤
                         Dash ─▶ Dodge ───────┤
                         Move ─▶ Combat_Moving─┤
                          │                   │
                          │                   ▼
                          │                 Sheath ──▶ Casual_Idle
                          │
                         Dmg ──▶ Hurt ──(end)──▶ Combat_Idle
                                      │
                                   hp ≤ 0
                                      ▼
                                     Dead (hold)
```

**Combat timeout** (default 3s): se o jogador não atacar/tomar dano,
auto-sheath e volta para Casual. Evita andar com a espada na mão o
jogo inteiro.

### 3.4 Servidor — Eventos de Ação

Hoje o servidor envia `position`, `velocity`, `hp` em cada snapshot.
Adicionar:

```rust
// crates/shared/src/protocol.rs (ou similar)
#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum ActionKind {
    Slash1, Slash2, Thrust, ShieldBash,
    Parry, Dodge,
    ShootStraight, ShootUp,
    Hurt, Dead,
    Draw, Sheath,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct ActionFrame {
    pub kind: ActionKind,
    pub dir:  u8,    // 0=S 1=N 2=E 3=W
    pub start_tick: u64,
}

pub struct EntitySnapshot {
    // … campos atuais …
    pub action:  Option<ActionFrame>,  // último action disparado
    pub stance:  Stance,                // casual ou combat
    pub visual:  Option<VisualConfig>,  // enviado 1ª vez + em mudanças
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VisualConfig {
    pub skin:        u8,       // 0-10 (v00-v10)
    pub outfit:      String,   // "fstr"
    pub outfit_v:    u8,       // 1-5
    pub hair:        String,   // "dap1"
    pub hair_v:      u8,       // 0-13
    pub hat:         Option<(String, u8)>,
    pub weapon_a:    Option<(String, u8)>,  // ("sw01", 1)
    pub weapon_b:    Option<(String, u8)>,  // ("sh01", 1) shield
}

pub enum Stance { Casual, Combat }
```

**Quando o servidor emite um `ActionFrame`:**

- **Attack** — input LMB recebido + cooldown OK → servidor calcula dano,
  cria projétil se ranged, emite `ActionFrame{kind: Slash1, dir, start_tick}`.
  Variante slash1/slash2/thrust escolhida por combo counter stored per
  entity (cicla).
- **Hurt** — ao aplicar dano a uma entidade, anexa `ActionFrame{Hurt}` no
  próximo snapshot dela.
- **Dead** — ao transitar para hp=0, emite `ActionFrame{Dead}`; entidade
  permanece no downed state por N segundos antes de ser removida.
- **Parry/Dodge** — input correspondente → servidor valida cooldown +
  emite action. Durante a animação, o servidor aplica imunidade/redução
  de dano.

**Tempo de animação** — server tick + `frame_count * frame_duration_ms`
determina quando a ação termina. Cliente lê `elapsed = current_tick - start_tick`,
converte para `frame_index`, renderiza.

---

## 4. Dados — Animation Reference Card

Mapping de animações → (página, células por direção, frames, duração).
Células indexadas `row * 8 + col` (0-63).

### p1 — Casual Movement

| Anim | Frames | Dur/frame | Cells (S, N, E, W) | Loop |
|------|--------|-----------|-------------------|------|
| CasualIdle | 1 | — | 0, 8, 16, 24 | — |
| CasualWalk | 6 | 135ms | 32+i, 40+i, 48+i, 56+i | ✓ |
| Run (substituição frames 3,6) | 2 | 135ms | 38+i, 46+i, 54+i, 62+i | ✓ |
| Push | 2 | 150ms | 1+i, 9+i, 17+i, 25+i | ✓ |
| Pull | 2 | 150ms | 3+i, 11+i, 19+i, 27+i | ✓ |
| Jump | 4 | 100ms | 4+i, 12+i, 20+i, 28+i | — |

### p1B — Weapon-on-back (usar ao invés de p1 quando stance=Casual + arma equipada)

Mesma grade que p1 mas com braço livre — *layout igual*. Textura diferente,
mesmas células.

### pONE1 — Sword+Shield, transições/reações

| Anim | Frames | Dur/frame | Cells | Loop |
|------|--------|-----------|-------|------|
| Draw | 4 | 80ms | (a verificar) | — |
| Sheath | 4 | 80ms | Draw invertido | — |
| Parry | 2 | 100ms | — | — |
| Dodge | 3 | 100ms | — | — |
| Hurt | 2 | 150ms | — | — |
| Dead | 3 | 200ms | — (last holds) | — |

### pONE2 — Sword+Shield, combat idle/movement

| Anim | Frames | Dur/frame | Cells | Loop |
|------|--------|-----------|-------|------|
| CombatIdle | 4 | 200ms | — | ✓ |
| CombatMove | 4 | 120ms | — | ✓ |
| Crouch | 1 | — | — | — |
| Retreat | 2 | 150ms | — | — |
| Lunge | 2 | 100ms | — | — |

### pONE3 — Sword+Shield, attacks (16 frames/dir, fits 8×8)

| Anim | Frames | Dur/frame | Cells per dir (row first) | Loop |
|------|--------|-----------|---------------------------|------|
| Slash1 | 4 | 80ms | row0 cols 0-3 | — |
| Slash2 | 4 | 80ms | row0 cols 4-7 | — |
| Thrust | 4 | 80ms | row1 cols 0-3 | — |
| ShieldBash | 4 | 80ms | row1 cols 4-7 | — |

Onde S=(rows 0,1), N=(rows 2,3), E=(rows 4,5), W=(rows 6,7).

### pBOW1-3 — Bow combat

Estrutura espelha pONE. pBOW3:

| Anim | Frames | Cells per dir |
|------|--------|---------------|
| ShootStraight | 4 | row0 cols 0-3 |
| ShootUp | 4 | row0 cols 4-7 |

(4 dirs ocupam rows 0-3; rows 4-7 possivelmente reservadas para anim extra).

### pPOL1-3 — Polearm combat

Mesma estrutura que pONE. pPOL3:

| Anim | Frames | Cells |
|------|--------|-------|
| SlashWide | 4 | — |
| Thrust1 | 4 | — |
| Thrust2 | 4 | — |

---

> **Tarefa de pré-implementação (M0.5):** verificar visualmente cada guide
> PNG (`_guides/animations, page X.png`) para completar as colunas "Cells"
> acima. Salvar resultado como tabela constante em C# no `PageCellMap.cs`.

---

## 5. Wire Protocol — Mudanças

### 5.1 Envio (client → server)

Sem mudanças no início. Combate usa os botões existentes:
- `Buttons.Primary` (LMB) = attack básico
- `Buttons.Secondary` (RMB) = parry/block
- `Buttons.Dash` (Space/Shift) = dodge

Servidor escolhe a variante (slash1/slash2/thrust) com base em combo counter
e weapon kind. Input continua minimalista.

### 5.2 Recebimento (server → client)

`EntitySnapshot` ganha 3 campos:

```json
{
  "id": 123,
  "pos": [10.0, 20.0],
  "vel": [0.0, 2.0],
  "hp": 80,
  "hp_max": 100,
  "action": { "kind": "Slash1", "dir": 0, "start_tick": 12345 },
  "stance": "combat",
  "visual": { ... }
}
```

**Compressão:** `visual` enviado na primeira vez que uma entidade entra no
AOI do cliente. Depois só em mudanças (delta). `action` só presente quando
uma ação foi iniciada dentro da janela visível (~500ms) — depois volta a
`null`.

### 5.3 Auth timing

Servidor roda a 30Hz (já definido). Uma animação de 4 frames × 80ms = 320ms
≈ 10 ticks. O cliente compara `current_tick - start_tick` para saber em
que frame está. Sincronização tolera jitter via `Mathf.Clamp` do frame
index.

---

## 6. Roadmap — 8 Milestones

Cada milestone é independentemente jogável (commit + teste antes do próximo).

### **M1 — Animation Framework (3-4 dias)**

**Goal:** Refatorar `PlayerPaperDoll.cs` em `CharacterAnimator` + `PaperDoll`
data-driven. Output idêntico ao atual (Walk + Idle), mas agora qualquer
nova animação só requer adicionar um `AnimationDef` ao registry.

**Files:**
- `Assets/_Project/Scripts/Game/Animation/AnimationDef.cs` (novo)
- `Assets/_Project/Scripts/Game/Animation/CharacterAnimator.cs` (novo)
- `Assets/_Project/Scripts/Game/Animation/PaperDoll.cs` (novo)
- `Assets/_Project/Scripts/Game/Animation/PageCellMap.cs` (novo — tabela
  estática de cell indices por página)
- `Assets/_Project/Scripts/Game/PlayerPaperDoll.cs` (delete)
- `Assets/_Project/Scripts/Game/EntityRenderer.cs` (substitui chamada)

**Acceptance:** player anda e idle exatamente como antes. Zero mudança
visível. Código agora aceita adicionar nova anim com `AnimationDef` em 3
linhas de constante.

### **M2 — Weapon + Shield Layers (1-2 dias)**

**Goal:** Personagem carrega espada visível na p1B (mão nas costas) quando
em casual. Implementar layers 6tla + 7tlb no `PaperDoll`.

**Files:**
- `PaperDoll.cs` (estende para 9 layers; só ativa se `visualConfig.weapon_a != null`)
- `VisualConfig.cs` (novo — struct local que espelha o do servidor; hardcoded por ora)

**Acceptance:** self-player aparece com sword `sw01_v01` visível nas costas.
Se commentar o `weapon_a`, personagem aparece sem espada.

### **M3 — Combat State Machine (Client-Local) (3-4 dias)**

**Goal:** Pressionar LMB → transição Casual → Draw → Combat_Idle → Slash1
→ Combat_Idle → (3s timeout) → Sheath → Casual. Tudo client-side apenas —
outros players não veem. Usa `pONE2` + `pONE3` para stance e attacks.

**Files:**
- `CharacterAnimator.cs` (state machine completa)
- `InputHandler.cs` (detecta click → dispara `animator.TriggerAttack()`)
- `PageCellMap.cs` (adiciona pONE2, pONE3 com cells verificadas)

**Acceptance:** ciclo de combate funcional no SELF. LMB repetido alterna
slash1 ↔ slash2. RMB → parry. Dash → dodge. 3s sem ação → volta casual.

### **M4 — Server Action Events (2-3 dias)**

**Goal:** Replicar ações para outros jogadores. Servidor emite
`ActionFrame` no snapshot; client renderiza para outros players.

**Files:**
- `crates/shared/src/protocol.rs` (adiciona `ActionKind`, `ActionFrame`,
  `Stance`, `VisualConfig`)
- `crates/server/src/world.rs` ou `tick.rs` (process input → emit action)
- `crates/server/src/tick.rs` (include action no snapshot)
- `Assets/_Project/Scripts/Network/` (deserializa, feeds no EntityRenderer)
- `EntityRenderer.cs` (recebe action e chama `animator.PlayFromServer(action)`)

**Acceptance:** dois clients conectados; um ataca, o outro vê. Sword
timing aproximadamente sincronizado.

### **M5 — Hurt & Dead (2-3 dias)**

**Goal:** Substituir flash branco por animação Hurt (2 frames). Morte
reproduz Dead animation (3 frames) antes de remover a entidade. Downed
state do jogador mostra Dead persistido.

**Files:**
- Servidor: ao aplicar dano → `snapshot.action = Hurt`. Ao hp=0 →
  `snapshot.action = Dead` + stance/marker Downed.
- `EntityRenderer.FlashHit()` removido — substituído por anim.
- `CharacterAnimator` — Hurt/Dead com prioridade alta interrompem outras.

**Acceptance:** inimigo ataca → personagem executa Hurt e tela pisca
levemente (DamageNumber mantido). Death animation visível antes do corpo
sumir.

### **M6 — Ranged (Bow) (3-4 dias)**

**Goal:** Arco funcional. pBOW1-3 completo. Quiver overlay (7tlb). Seta
como projétil visual alinhado ao snapshot do server.

**Files:**
- `PageCellMap.cs` (pBOW1, pBOW2, pBOW3)
- `PaperDoll.cs` (7tlb = quiver em vez de shield quando weapon_b = "qv01")
- `ProjectileRenderer.cs` (usa `aro_comn_v0X` sprite em vez de dot amarelo)
- `CharacterAnimator.cs` (transições: se weapon_a kind=bow → páginas pBOW)

**Acceptance:** trocar arma do char para `bow_long` → personagem muda
stance, anima ShootStraight ao clicar, aljava visível nas costas, flecha
voa com sprite.

### **M7 — Polearm + Weapon Mapping (2-3 dias)**

**Goal:** pPOL1-3 para Spear. Mapeamento server-weapon-id → page set
configurável no VisualConfig. Great Sword / Staff / Wand ficam como
proxy (pONE + ax01 para GS; a definir para Staff/Wand).

**Files:**
- `PageCellMap.cs` (pPOL*)
- `WeaponSpriteMap.cs` (tabela: `WeaponKind::Sword` → `pageset = pONE`, `sprite = sw01`)
- Documentar no CHARACTER_ASSETS.md os proxies usados.

**Acceptance:** todos os 6 tipos de arma do servidor têm alguma
representação visual jogável. Limitações conhecidas documentadas.

### **M8 — Character Customization (4-5 dias)**

**Goal:** UI de criação de personagem escolhe pele + cabelo + roupa +
cor. `VisualConfig` persiste no server (Postgres). Outros players veem
seu visual corretamente.

**Files:**
- UI nova: `CharacterCreatorUI.cs`
- `crates/server/src/persistence.rs` (adiciona colunas `skin, hair, hair_v,
  outfit, outfit_v, weapon_a, weapon_b` na tabela `characters`)
- Migration SQL nova
- Spawn flow: servidor envia VisualConfig no primeiro snapshot do entity

**Acceptance:** 3 players com visuais distintos aparecem corretamente
uns aos outros. Relogar preserva escolha.

---

## 7. Riscos e Decisões em Aberto

| # | Risco | Mitigação |
|---|-------|-----------|
| 1 | **Cell indices por página** não conferidos na doc | M0.5: inspeção visual dos guides + tabela canonical |
| 2 | **Memória** — 9 layers × N players × múltiplas páginas em RAM | Lazy-load de páginas de combate só quando entra em stance; texturas compartilhadas via Resources cache |
| 3 | **Layer order por direção** — shield atrás/frente do body | Tabela `DirLayerOffset[page,layer,dir]`; fallback = ordem natural |
| 4 | **Timing sync** — frame drift entre server tick e client render | `elapsed_ms = (current_tick - start_tick) * tick_ms`; clamp frame ∈ [0, frameCount-1] |
| 5 | **Input prediction** — LMB deve responder instantâneo | Client dispara animação localmente; server confirma via ActionFrame no próximo snapshot. Se divergência, snap para server state. |
| 6 | **Combo alternation** slash1/slash2 — client ou server decide? | Server decide (authoritativo), mas client prediz com mesmo counter local; reconcilia se divergir |
| 7 | **Great Sword / Staff / Wand** sem sprites Mana Seed | Proxy temporário: GS → ax01+pONE; Staff → sw01+pONE; Wand → sw01 menor. Arte custom pode vir depois sem quebrar o sistema. |
| 8 | **Animações sem direção** (Jump arco circular) | Adicionar `cellsPerDir` vazio + `sharedCells[]` no AnimationDef; apenas walk/attack usam 4 dirs |
| 9 | **Hurt cancela attack próprio?** | Sim — Hurt(prio 90) > Attack(80). Server trava input durante hurt window. |
| 10 | **Character resizing** — char é 32px visível com PPU 16 = 2 unidades | Já correto; paper doll pivot 0.40 alinha pés na grid |

---

## 8. Out of Scope

Explicitamente **não** incluído neste plano (possíveis adições futuras):

- **Armaduras visíveis** (chestplate, helmet, boots como overlays
  distintos da outfit) — requer arte custom; outfit cobre role visual
  suficiente no curto prazo.
- **Emotes sociais** (dormir, beber, sentar — página p4) — depois de
  combate estar firme.
- **Pesca** (p3) — atividade fora-de-combate, post-MVP.
- **Skills especiais / magia com efeitos** (VFX layer) — sistema
  separado, não entra aqui.
- **Hit stun / recoil frames** além da animação Hurt básica.
- **Stamina / bloqueio limitado** — game design dimension, não rendering.
- **Sistema de clima / sombras dinâmicas / ToD** — iluminação, não char.
- **Lip sync / dialog portraits** — out of scope.
- **Raças não-humanas** (p1_0bas_demn, _gbln) — arte existe, mas
  integrar raça como axis do VisualConfig é uma extensão do M8.

---

## 9. Apêndice — Interfaces Principais

```csharp
// AnimationDef.cs — imutável, construído uma vez
public sealed class AnimationDef {
    public readonly string Name;
    public readonly string Page;           // "p1", "pONE3", etc.
    public readonly int FrameCount;
    public readonly float FrameDuration;   // seconds
    public readonly bool Loops;
    public readonly int Priority;
    public readonly int[] StartCellPerDir; // [S, N, E, W]
    public readonly int CellStride;        // usually 1 (cols 0,1,2,3) or 8 (rows for multi-row)
    public AnimationDef(string name, string page, int frames, float dur,
                        int[] startPerDir, int stride = 1,
                        bool loops = false, int priority = 0);
}

// CharacterAnimator.cs
public class CharacterAnimator : MonoBehaviour {
    public AnimationDef Current { get; private set; }
    public int CurrentFrameInDir { get; private set; } // actual cell index
    public Dir CurrentDir { get; private set; }
    public event Action<AnimationDef> OnComplete;

    public void Play(AnimationDef anim, Dir dir, bool forceRestart = false);
    public void Trigger(string name, Dir dir);  // convenience
    public void UpdateFromVelocity(Vector2 vel); // sets dir + selects walk/idle
    public void ApplyServerAction(ActionFrame action, long currentServerTick); // for remote entities
    // State machine internals: casual <-> combat, interruption rules
}

// PaperDoll.cs
public class PaperDoll : MonoBehaviour {
    public void SetVisual(VisualConfig cfg);
    public void Init(CharacterAnimator animator);
    // LateUpdate reads animator.Current + CurrentFrameInDir, applies sprites
    // to each active layer's SpriteRenderer.
}

// VisualConfig.cs
[Serializable]
public struct VisualConfig {
    public byte   skin;           // v00-v10
    public string outfit;         // "fstr"
    public byte   outfitColor;    // v01-v05
    public string hair;           // "dap1"
    public byte   hairColor;      // v00-v13
    public Option hat;
    public Option weaponA;        // sword/axe/mace/bow
    public Option weaponB;        // shield/quiver
    public struct Option { public string code; public byte color; }
}
```

```rust
// crates/shared/src/protocol.rs (adições)
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum ActionKind {
    Slash1, Slash2, Thrust, ShieldBash,
    Parry, Dodge,
    ShootStraight, ShootUp,
    Hurt, Dead, Draw, Sheath,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ActionFrame {
    pub kind: ActionKind,
    pub dir:  u8,
    pub start_tick: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stance { Casual, Combat }
```

---

## 10. Como Usar Este Plano

1. **Leitura e aprovação** — revisar §1, §3, §6. Ajustar prioridades se
   algo estiver fora de ordem para o ciclo atual do projeto.
2. **M0.5 (pré-work, ~2h)** — abrir cada `_guides/animations, page X.png`
   e preencher as tabelas da §4 com cell indices confirmados. Salvar em
   `Assets/_Project/Scripts/Game/Animation/PageCellMap.cs`.
3. **M1 first** — começar pela refatoração. Nenhuma mudança visível =
   commit seguro. Só então partir para M2+.
4. **Checkpoints** — após cada milestone, testar com 2 clients conectados.
   Rollback fácil se algo quebrar para players remotos.
5. **Documentar decisões no caminho** — CHARACTER_ASSETS.md já existe;
   adicionar `COMBAT_DECISIONS.md` para registros se surgirem tradeoffs
   não previstos aqui.

---

*Fim do plano. Pronto para revisão e ajustes antes da implementação.*

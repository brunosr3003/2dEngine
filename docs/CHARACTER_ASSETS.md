# Character Assets — Escopo e Possibilidades

> Atualizado: 2026-04-24
>
> Mapeamento completo dos assets Mana Seed Character Base disponíveis no projeto e o
> que é possível implementar com eles no cliente.
>
> Todos os sprites estão em `Assets/_Project/Resources/Character/ManaBase/`.
> Formato: sheets 512×512px, células 64×64px (8×8 grid), PPU=16.
> Convenção de nome: `char_a_<página>_<camada>_<item>_<cor>.png`

---

## 1. Corpo Base

11 tons de pele disponíveis (`v00`–`v10`, sendo `v00` o mais claro e `v10` o mais escuro).

O corpo base é **separado por página de animação** — cada página é um sheet 512×512 com
frames numa grade 8×8. Todas as camadas (roupa, cabelo, arma) têm o layout idêntico
e se sobrepõem pixel a pixel.

---

## 2. Páginas de Animação e Frames

### Página 1 (`p1`) — Movimentação Base
Animações disponíveis em **4 direções** (S/N/E/W):

| Animação | Frames | Observações |
|----------|--------|-------------|
| Stand (idle) | 1 | Posição parada |
| Push | 2 | Empurrar objeto pesado |
| Pull | 2 | Puxar objeto pesado |
| Jump | 4 | Arco: preparar → salto → queda → pouso |
| Walk | 6 | Loop principal de movimento |
| Run (frames 3 e 6) | 2 | Substituição parcial do walk para corrida |

> **Implementado atualmente:** Walk (6 frames @ 135ms) + Idle. Faltam: Run, Push, Pull, Jump.

### Página 1B (`p1B`) — Segurando objeto nas costas
Mesmas animações da p1 com pose de braço adaptada para carregar arma/ferramenta nas costas.
Usado para mostrar arma equipada durante navegação/idle fora de combate.

### Página 1C (`p1C`) — Arma 2 mãos
Variante da p1 para armas de duas mãos (lanças com 2 mãos, machado grande, etc.).

### Página 2 (`p2`) — Ações de Mundo
Animações em **4 direções**:

| Animação | Frames | Ferramenta sprite |
|----------|--------|-------------------|
| Tilling/Mining/Woodcut (overhead strike) | 4 | `farm`, `mine`, `wood` |
| Plantar sementes | 4 | — |
| Regar | 4 | `roda` (regador) |
| Colher | 4 | — |
| Bug net | 4 | `bnet` |

> Não relevante para o MMO de combate no curto prazo, mas disponível.

### Página 3 (`p3`) — Pesca
8 frames de pesca (lançar, aguardar, recolher, mostrar peixe). Ferramenta: vara + bóia.

### Página 4 (`p4`) — Emotes e Posições
Ferreiro, escalar, sentado, dormindo, beber, look around, idle alternativo.

---

## 3. Páginas de Combate

Cada set de combate tem **3 páginas** (`pXXX1`, `pXXX2`, `pXXX3`) que juntas cobrem
todas as animações de combate em 4 direções.

### Espada + Escudo (`pONE1`, `pONE2`, `pONE3`)

| Animação | Frames | Página |
|----------|--------|--------|
| Draw/Sheath (sacar/guardar) | 4 | ONE1 |
| Parry (aparar) | 2 | ONE1 |
| Dodge (esquiva) | 3 | ONE1 |
| Hurt (levar dano) | 2 | ONE1 |
| Dead (morte/knockdown) | 3 | ONE1 |
| Idle em guarda | 4 loop | ONE2 |
| Move em guarda | 4 loop | ONE2 |
| Crouch / Retreat / Lunge | 1 / 2 / 2 | ONE2 |
| Slash 1 (forehand) | 4 | ONE3 |
| Slash 2 (backhand) | 4 | ONE3 |
| Thrust (estocada) | 4 | ONE3 |
| Shield Bash | 4 | ONE3 |

**Armas disponíveis** (sheets separados, sobrepostos à animação):
- `sw01` — espada clássica (5 paletas: prata, dourado, azul, vermelho, roxo)
- `sw02` — twin-blade (espada dupla, 5 paletas)
- `ax01` — machado (5 paletas)
- `mc01` — mace/maça (5 paletas)

**Escudos disponíveis** (camada separada, pode ir na frente ou atrás do corpo):
- `sh01`, `sh02`, `sh03` — 3 formatos × 5 paletas = 15 sprites de escudo

> **Relevante para o jogo:** Sword no servidor mapeia para `sw01`/`sw02`.
> Dagger poderia usar `sw01` menor ou `ax01`. Great Sword precisaria de sprite próprio
> (não incluso — mais próximo do `ax01` em peso).

### Arco (`pBOW1`, `pBOW2`, `pBOW3`)

| Animação | Frames | Página |
|----------|--------|--------|
| Draw/Sheath | 4 | BOW1 |
| Parry | 2 | BOW1 |
| Dodge | 3 | BOW1 |
| Hurt | 2 | BOW1 |
| Dead | 3 | BOW1 |
| Idle em guarda | 4 loop | BOW2 |
| Move em guarda | 4 loop | BOW2 |
| Retreat / Lunge | 2 / 2 | BOW2 |
| Atirar (reto) | 4 | BOW3 |
| Atirar (diagonal cima) | 4 | BOW3 |

**Arcos disponíveis** (sheets separados):
- `bow_shrt` — shortbow (5 paletas)
- `bow_long` — longbow (5 paletas)
- `bow_curv` — recurve bow (5 paletas)

**Acessórios de arco:**
- `qv01` — quiver/aljava (8 paletas)
- `aro_comn` — flecha em voo (8 paletas)

> **Relevante:** Bow no servidor mapeia direto para `bow_long`/`bow_shrt`.

### Lança / Spear (`pPOL1`, `pPOL2`, `pPOL3`)

| Animação | Frames | Página |
|----------|--------|--------|
| Draw/Sheath | 4 | POL1 |
| Parry | 2 | POL1 |
| Dodge | 3 | POL1 |
| Hurt | 2 | POL1 |
| Dead | 3 | POL1 |
| Idle em guarda | 4 loop | POL2 |
| Move em guarda | 4 loop | POL2 |
| Retreat / Lunge | 2 / 2 | POL2 |
| Slash (wide) | 4 | POL3 |
| Thrust 1 (curto) | 4 | POL3 |
| Thrust 2 (longo) | 4 | POL3 |

**Armas disponíveis:**
- `sp01` — short spear (5 paletas)
- `sp02` — winged spear (5 paletas)
- `hb01` — halberd (5 paletas)

> **Relevante:** Staff/Wand no servidor são ranged mágicos — visualmente poderiam
> usar a pose de lança (pPOL) como proxy ou precisariam de sprite próprio.
> No curto prazo: Staff usa pONE (uma mão) com sprite de cajado.

---

## 4. Cabelos

7 estilos × 13–14 paletas de cor cada. Cada estilo vem em sheets separados por página.

| Código | Estilo | Páginas compatíveis |
|--------|--------|---------------------|
| `bob1` | Bob curto (feminino) | p1, p1B, p2, p3, p4 + pBOW1-3 + pONE1-3 + pPOL1-3 |
| `bob2` | Bob médio | p1, p1B, p2, p3, p4 |
| `dap1` | Dapper (masculino liso) | p1, p1B, p2, p3, p4 + pBOW1-3 + pONE1-3 + pPOL1-3 |
| `flat` | Liso curto | p1, p1B |
| `fro1` | Afro | p1, p1B |
| `pon1` | Rabo de cavalo | p1, p1B |
| `spk2` | Espinhado | p1, p1B |

> **Atenção:** `bob2`, `flat`, `fro1`, `pon1`, `spk2` NÃO têm sheets de combate (pONE/pBOW/pPOL).
> Para combate, apenas `bob1` e `dap1` estão completos.

---

## 5. Roupas / Outfits

Cada outfit vem em sheets por página. Outfits com suporte a combate marcados com ✓.

| Código | Roupa | p1/p1B/p1C | pONE | pBOW | pPOL |
|--------|-------|-----------|------|------|------|
| `fstr` | Forester (túnica + chapéu pontudo) | ✓ | ✓ | ✓ | — |
| `pfpn` | Peasant Farmer Pants + Hat | ✓ | ✓ | ✓ | — |
| `pfdr` | Peasant Farmer Dress + Bonnet | ✓ | — | — | — |
| `angl` | Angler (calças + chapéu chuva) | ✓ | — | — | — |
| `bksm` | Blacksmith (avental + bandana) | ✓ | — | — | — |
| `alch` | Alchemist (casaco + óculos) | ✓ | — | — | — |
| `boxr` | Boxer (cueca) | ✓ | — | — | — |
| `undi` | Underwear | ✓ | — | — | — |

> **Para o MMO de combate:** `fstr` (Forester) é o mais completo — suporta espada e arco.
> Peasant Pants também. Os demais são somente para NPCs/Villagers.

---

## 6. Chapéus / Headwear

Disponíveis separadamente como camada extra sobre o cabelo:

| Código | Item | Paletas |
|--------|------|---------|
| `pnty` | Pointy hat (forester) | 5 |
| `pfht` | Peasant farm hat | 5 |
| `pfbn` | Peasant bonnet | 5 |
| `rnht` | Rain hat | 5 |
| `band` | Bandana | 5 |
| `hddn` | Hood down | 10 |
| `hdpl` | Hood up (cloak) | 10 |

> Cloak & Hood (`hddn`/`hdpl`) vem com mantos frontais e traseiros — sistema mais
> complexo com camada "0bot" (embaixo de tudo) + "2clo" (capa) em 10 paletas.

---

## 7. Mapeamento Arma Servidor → Sprite

| Arma (servidor) | Tipo | Página base | Weapon sprite | Observações |
|-----------------|------|-------------|---------------|-------------|
| Sword | Melee | `pONE` | `sw01` v01 (prata) | Pronto |
| Dagger | Melee | `pONE` | `sw01` (menor) | Mesmo set, sprite menor |
| Great Sword | Melee | `pONE` ou `p1C` | `ax01` (proxy) | Falta sprite próprio |
| Staff | Ranged | `pONE` | sprite próprio | Falta; pode usar bastão custom |
| Wand | Ranged | `pONE` | sprite próprio | Falta; menor que staff |
| Bow | Ranged | `pBOW` | `bow_long` v01 | Pronto |

---

## 8. O que é possível implementar

### Imediato (assets prontos, só código)

- [x] Walk/Idle já implementados (sistema antigo, 512×512)
- [ ] **Migrar para o novo sistema** (Character Base 2.5c, layout por número de célula)
- [ ] **Run** — frames 3 e 6 do walk (velocidade > threshold)
- [ ] **Hurt** — 2 frames ao levar dano (já tem flash branco, pode virar animação)
- [ ] **Dead/Knockdown** — 3 frames (usar no Downed State)
- [ ] **Espada + Escudo** — páginas pONE completas: idle, move, slash1, slash2, thrust, shield bash, parry, dodge
- [ ] **Arco** — páginas pBOW: idle, move, atirar (reto e cima), parry, dodge
- [ ] **Lança** — páginas pPOL: slash, thrust1, thrust2
- [ ] **Weapon sprite overlay** — camada adicional no PaperDoll para arma na mão
- [ ] **Shield overlay** — segunda camada de arma (escudo, atrás ou frente do corpo)
- [ ] **Customização visual** — escolha de cor de pele + cabelo + roupa + chapéu na criação de personagem
- [ ] **NPC visuals** — usar outfits Angler/Blacksmith/Alchemist nos NPCs da cena

### Médio prazo (requer trabalho adicional)

- [ ] **Jump** ao usar Dash — 4 frames de jump como animação de esquiva
- [ ] **Emotes** — `/emote sleep`, `/emote drink`, `/emote sit` via página p4
- [ ] **Staff/Wand sprite** — desenhar ou adaptar um sprite compatível
- [ ] **Great Sword sprite** — desenhar sprite 2-mãos compatível com p1C
- [ ] **Cor de arma por raridade** — mapear paletas de sw01 para raridades do item
- [ ] **Múltiplos NPCs com visuais distintos** — Vendor = Blacksmith apron, Vault NPC = Alchemist coat

### Longo prazo / Design

- [ ] **Customização in-game** — player escolhe pele/cabelo/roupa no client, servidor replica `visual_config`
- [ ] **Armaduras visíveis** — sheet de armadura como camada sobre o outfit (requer arte custom)
- [ ] **Emote social** — botão no HUD para acionar emotes
- [ ] **Pesca** — minigame usando p3 se quiser adicionar atividade não-combate

---

## 9. Limitações Conhecidas

- Apenas `bob1` e `dap1` têm cabelo completo para todas as páginas de combate.
  Os outros 5 estilos ficam restritos a conteúdo não-combate (NPCs, idle).
- Apenas `fstr` e `pfpn` têm outfits para combate com arco (`pBOW`).
  Nenhum outfit tem suporte para lança (`pPOL`).
- Great Sword e Staff/Wand não têm sprites Mana Seed compatíveis — precisariam
  de arte custom ou usar sprites proxy.
- O sistema atual (`PlayerPaperDoll.cs`) usa o layout do sheet antigo (512×512,
  grade sequential). O Character Base 2.5c usa numeração de célula non-sequential
  (ex: walk S = células 001–006, walk N = células 017–022). Requer reescrita do
  `PlayerPaperDoll` para o novo mapeamento.

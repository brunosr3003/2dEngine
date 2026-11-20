# Sistema de Barcos

**Status:** barcos, Estaleiro, combate, economia naval, autonomia e alianças
via grupo estão implementados na versão de teste; skins completas de casco e
polimento audiovisual continuam pendentes (20/11/2026). Ver
`BARCOS-IMPLEMENTACAO.md`.

## Visão Geral

Barcos como **items upgradáveis** com escolhas de progressão e trade-offs. Sistema de **alocação de pontos** em stats permite builds especializados ou balanceados.

---

## Estrutura dos Barcos

### 3 Tamanhos (capacidade fixa de passageiros)

| Tamanho  | Capacidade | Características Gerais                      |
|----------|------------|---------------------------------------------|
| Pequeno  | 1-2        | Rápido, ágil, frágil                       |
| Médio    | 3-4        | Balanceado (quality)                        |
| Grande   | 6-8        | Lento, resistente, alta capacidade          |

### 3 Variantes por Tamanho (afinidade de stats)

Cada tamanho tem 3 variantes com **stats base diferentes**:

1. **Veloz** — afinidade em Velocidade + Virada
2. **Ataque** — afinidade em Dano
3. **Tanque** — afinidade em HP

**Total: 9 barcos base** (3 tamanhos × 3 variantes)

---

## Sistema de Tiers (T1 → T2 → T3 → T4)

Cada barco segue o sistema padrão de items do jogo:

| Tier | Multiplicador | Cor    | Pontos Livres |
|------|---------------|--------|---------------|
| T1   | 60%           | Cinza  | 10 pontos     |
| T2   | 90%           | Verde  | 18 pontos     |
| T3   | 120%          | Azul   | 26 pontos     |
| T4   | 150%          | Roxo   | 34 pontos     |

**Upgrade:**
- 2 barcos **iguais** (mesmo tipo/tier) = 1 barco do tier acima
- Exemplo: Pequeno Veloz T1 + Pequeno Veloz T1 = Pequeno Veloz T2
- Custo em ouro (cresce por tier)

---

## Sistema de Pontos de Stat

### 4 Stats Alocáveis

1. **Velocidade** — velocidade de navegação (u/s)
2. **HP** — pontos de vida do casco
3. **Dano** — multiplicador de dano das habilidades (canhões)
4. **Virada** — agilidade de rotação (rad/s)

### Alocação de Pontos

- Cada tier dá **pontos livres** para distribuir
- Player aloca manualmente nos 4 stats
- **Trade-offs forçados**: pontos limitados = escolhas

### Builds Exemplo (barco T4 com 34 pontos)

```
Build Balanceado (Quality)
├─ Velocidade: 8 pontos
├─ HP: 10 pontos
├─ Dano: 8 pontos
└─ Virada: 8 pontos

Build Tanque
├─ Velocidade: 4 pontos
├─ HP: 20 pontos
├─ Dano: 5 pontos
└─ Virada: 5 pontos

Build DPS
├─ Velocidade: 6 pontos
├─ HP: 6 pontos
├─ Dano: 18 pontos
└─ Virada: 4 pontos

Build Speed
├─ Velocidade: 20 pontos
├─ HP: 4 pontos
├─ Dano: 4 pontos
└─ Virada: 6 pontos
```

### Respec (Realocação)

- **Permitido** com custo em **ouro**
- Permite experimentar builds diferentes
- Custo aumenta com tier do barco

---

## Exemplo Completo

```
╔═══════════════════════════════════════════╗
║ Pequeno Veloz T2                          ║
╠═══════════════════════════════════════════╣
║ Capacidade: 2 passageiros                 ║
║                                           ║
║ Stats Base (afinidade velocidade):        ║
║ • Velocidade: 50 (afinidade)              ║
║ • HP: 100                                 ║
║ • Dano: 10                                ║
║ • Virada: 8 (afinidade)                   ║
║                                           ║
║ Pontos Alocados: 18/18                    ║
║ ├─ Velocidade: +6 → 56 total              ║
║ ├─ HP: +4 → 104 total                     ║
║ ├─ Dano: +2 → 12 total                    ║
║ └─ Virada: +6 → 14 total                  ║
║                                           ║
║ Multiplicador T2: 90%                     ║
║ [Realocate] (custo: 500 ouro)             ║
╚═══════════════════════════════════════════╝
```

---

## Sistema de Skins

Similar ao sistema de **montarias**:

- **Barco base** = stats funcionais
- **Skin cosmética** = visual independente
- Player pode ter várias skins
- Aplica/troca skins no Estaleiro
- Skins não afetam stats

**Skins incluem:**
- Cor das velas (já existe)
- Figura de proa (já existe)
- **Novo**: Casco completo (modelo 3D diferente)

---

## Progressão e Obtenção

### Obtenção do Barco Base

1. **Quest inicial** — primeiro barco (Pequeno T1)
2. **Craft no Estaleiro** — outros barcos base
3. **Drop/Compra** — barcos de outros players

### Progressão

```
Tier 1 → Tier 2 → Tier 3 → Tier 4
   ↓        ↓        ↓        ↓
+8 pts   +8 pts   +8 pts
(18)     (26)     (34)

Upgrade = 2 barcos iguais + ouro
```

### Materiais de Craft

- **Madeira** (T1/T2/T3/T4)
- **Aço** (por cor/tier)
- **Pranchas de Naufrágio**
- **Darksteel** (tiers altos)

---

## Mecânicas de Combate Naval

### HP do Barco

- **HP → 0** = barco **avariado**
- Player retorna ao dock de origem
- **Reparo** no Harbour Captain (custo em cobre)
- Gatilho preparado para mecânicas futuras (ex: afundar)

### Dano no Mar

- Mobs marinhos atacam o **casco**, não o player
- Skills de barco (canhões) usam stat **Dano**
- Habilidades especiais (Broadside, Long Gun, etc.)

---

## Comparação com Sistema Atual

### Sistema Atual (a ser substituído)

- 4 cascos em progressão **linear** (Sloop → Schooner → Galleon → Man-o'-War)
- Upgrade consome barco anterior + materiais
- 3 eixos de estrelas **independentes** (Hull/Sail/Cannon)
- Pode upar tudo sem trade-offs
- Sem escolha de build

### Sistema Novo (proposto)

- 9 barcos base em 3 **classes paralelas**
- Variantes com afinidades diferentes
- Sistema de **pontos limitados** com trade-offs
- **Escolhas** de progressão (especialização vs balanceado)
- Tiers T1-T4 como items normais
- Mais profundidade e replayability

---

## Considerações de Implementação

### Dados Necessários

```rust
pub struct BarcoInstance {
    /// Tipo base (1-9: tamanho × variante)
    pub tipo: u8,
    
    /// Tier atual (1-4)
    pub tier: u8,
    
    /// HP atual do casco
    pub hp_atual: u16,
    
    /// HP máximo (stats_base + pontos alocados) × tier_mult
    pub hp_max: u16,
    
    /// Pontos alocados em cada stat [vel, hp, dano, virada]
    pub pontos: [u8; 4],
    
    /// Skin cosmética equipada (vela + figura + casco)
    pub skin_vela: u8,
    pub skin_figura: u8,
    pub skin_casco: u8,
}

pub struct BarcoBase {
    pub id: u8,
    pub nome: &'static str,
    pub tamanho: Tamanho,      // Pequeno/Médio/Grande
    pub variante: Variante,     // Veloz/Ataque/Tanque
    pub capacidade: u8,         // Passageiros
    
    /// Stats base (antes de pontos e tier mult)
    pub velocidade_base: u16,
    pub hp_base: u16,
    pub dano_base: u16,
    pub virada_base: u16,
}
```

### UI Necessária

1. **Painel de Alocação de Pontos** (no Estaleiro)
   - Sliders para cada stat
   - Preview de stats finais
   - Custo de respec

2. **Comparação de Barcos**
   - Side-by-side dos 9 tipos base
   - Filtros por tamanho/variante

3. **Upgrade/Craft**
   - Visualização de materiais
   - Preview do resultado

4. **Skins**
   - Galeria de skins desbloqueadas
   - Preview 3D

### Balanço

- **Pontos por tier**: ajustar para evitar power creep
- **Stats base**: balancear afinidades entre variantes
- **Capacidade**: impacto no PvP (mais players = mais forte?)
- **Custo de respec**: não pode ser muito barato (decisões importam)

---

## Fases de Implementação

### Fase 1: Dados e Backend
- [x] Definir structs de barco novo
- [x] Migração de barcos antigos
- [x] Sistema de pontos e cálculo de stats
- [x] Lógica de upgrade T1→T4

### Fase 2: UI
- [x] Painel de alocação de pontos
- [x] Galeria de barcos (9 tipos)
- [x] Sistema de respec
- [x] Tooltips e preview dos stats

### Fase 3: Visual
- [x] Visual procedural por tamanho/variante integrado ao preview e ao mar
- [ ] Variações por tier (textura/detalhes)
- [x] Velas e figuras de proa cosméticas
- [ ] Skins completas de casco

### Fase 4: Balanço
- [x] Testes automatizados de tipos, tiers, migração e limites
- [x] Primeira passagem de pontos/stats em unidades reais
- [ ] Playtest e ajuste fino das nove builds
- [ ] Feedback de players

---

## Notas de Design

- **Simplicidade**: 4 stats é mais claro que 3 eixos + virada + capacidade
- **Escolhas**: trade-offs forçam decisões interessantes
- **Variação**: 9 barcos base × 4 tiers × N builds = alta replayability
- **Progressão**: sistema familiar (igual items de equip)
- **Social**: capacidade diferente por tamanho incentiva grupos

---

**Documento criado:** 18/11/2026  
**Autor:** Bruno + Claude  
**Status:** Implementação funcional em teste; arte por tier e playtest ainda pendentes

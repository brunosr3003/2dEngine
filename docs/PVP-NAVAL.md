# PvP Naval e Sistema de Abordagem

**Status:** Proposta de design (18/11/2026)

---

## Visão Geral

Sistema completo de **combate naval multi-fase** com pirataria real: projéteis desvíáveis, abordagem de navios, PvP no convés e saque de espólios.

**Filosofia:** Skill-based, high risk/high reward, interação real entre tripulações.

---

## Fase 1: Combate Naval (Projéteis)

### Mecânica de Projéteis

**Projéteis físicos desvíáveis** em vez de auto-target:

```
Disparo do canhão
    ↓
Projétil físico (arco balístico)
    ↓
Tempo de viagem (2-4s dependendo distância)
    ↓
Alvo pode DESVIAR navegando
    ↓
Acerto = dano no casco do barco
```

### Skills de Barco Adaptadas

**1. Broadside (Canhões Laterais)**
- Dispara múltiplos projéteis em **leque**
- Precisa **prever** movimento do alvo (lead targeting)
- Dano total alto se acertar todos
- Cooldown: ~15s

**2. Long Gun (Canhão Frontal)**
- 1 projétil **rápido**, longo alcance
- Mais preciso, menos dano
- Snipe de longa distância
- Cooldown: ~8s

**3. Chain Shot (Correntes)**
- Projétil especial que **desacelera** (slow)
- Se acertar: reduz velocidade do alvo em 50% por 5s
- Setup para combo (facilita acertar próximo Broadside)
- Cooldown: ~20s

**4. Emergency Repair (Já existe)**
- Reparo rápido durante combate
- Restaura % de HP
- Pode usar em movimento

### Skill Expression

- **Aim:** prever onde alvo VAI estar (lead)
- **Dodge:** virar/acelerar para evitar projéteis
- **Positioning:** ângulo certo para Broadside
- **Timing:** acertar alvo desacelerado por Chain Shot

---

## Fase 2: Barco Derrotado (HP → 0)

### Estado "Afundando"

Quando HP do barco chega a **0**, ele NÃO desaparece:

```
Visual do Barco "Morto":
├─ Velas RASGADAS/caídas no mastro
├─ FOGO espalhado pelo casco (shader + partículas)
├─ FUMAÇA subindo (visível de longe)
├─ Barco lista pro lado (inclinado ~15°)
└─ Deriva LENTA (incontrolável, movimento natural)

Estado Mecânico:
├─ Habilidades desabilitadas
├─ Não pode navegar (controles não funcionam)
├─ Vulnerável a boarding (abordagem)
├─ Timer de afundamento: 10-15 minutos
└─ Após timer: afunda de verdade (some)
```

### Escolhas do Capitão

Quando seu barco é derrotado, você tem **2 opções**:

**Opção 1: Afundar Imediatamente**
```
├─ Desiste da luta
├─ Barco afunda na hora (some)
├─ Tripulação toda respawna no DOCK da última ilha
├─ Conta como MORTE REAL:
│   ├─ Perde XP
│   ├─ Perde proficiência
│   └─ Drop de items? (a definir)
└─ Loot do barco fica flutuando? (ver seção Loot)
```

**Opção 2: Reparar Vela**
```
├─ Tenta salvar o barco
├─ Interagir com mastro quebrado
├─ CHANNEL 30-60s (barra de progresso)
├─ Pode ser INTERROMPIDO:
│   ├─ Levar dano
│   ├─ Se mover
│   └─ Se morrer
├─ VULNERÁVEL durante todo o channel:
│   ├─ Inimigos podem abordar
│   ├─ Podem matar você durante reparo
│   └─ Se morrer: opções acima novamente
└─ Se COMPLETAR:
    ├─ Vela volta (visual restaurado)
    ├─ Fogo apaga
    ├─ Barco fica com 10% HP
    ├─ Pode navegar (lentamente)
    └─ Foge pro dock para reparo completo
```

**Reparo:**
- **Ilimitado**: pode tentar reparar quantas vezes quiser
- **Sem custo** de materiais
- **Sem cooldown**
- Já é **muito punitivo**: vulnerável, lento, pode ser morto

---

## Fase 3: Boarding (Abordagem)

### Como Abordar um Barco

```
Condições para Boarding:
├─ Barco inimigo com HP 0 (afundando/pegando fogo)
├─ OU barco vulnerável (reparando, tripulação pequena)
├─ Aproximar SEU barco a 5-10u
└─ Botão "Board" ou simplesmente pular

Mecânica:
├─ Animação de pular (gangplank visual?)
├─ Entra no CONVÉS do barco inimigo
├─ Agora é PvP tradicional (combate player)
└─ Múltiplos jogadores podem abordar ao mesmo tempo
```

### Risco do Boarding

```
Enquanto você aborda barco inimigo:
├─ SEU barco fica "desprotegido"
├─ Outro pirata pode abordar SEU barco
├─ Tripulação precisa ficar de guarda
└─ Dividir forças: alguns abordam, outros defendem
```

### PvP no Convés

**Combate tradicional** entre players no convés do barco:

```
Respawn durante PvP Naval:
├─ Estilo DUNGEON (delay crescente)
├─ Primeira morte: respawn em 5s
├─ Segunda morte (mesmo combate): 10s
├─ Terceira: 15s
├─ E assim por diante...
└─ Reseta quando combate acaba

Onde Respawn:
├─ SEU BARCO VIVO: respawn no seu convés
├─ SEU BARCO AFUNDOU: respawn no dock última ilha
│   └─ Conta como MORTE REAL (perde XP/prof)
└─ Delay dá tempo pro inimigo saquear
```

**Estratégias:**
- Matar tripulação inimiga antes de saquear
- Defender seu barco enquanto outros abordam
- Alianças entre múltiplas tripulações

---

## Fase 4: Saquear

### Mecânica de Saque

```
Interagir com baú/inventário do barco inimigo:
├─ CHANNEL de 10 segundos
├─ Barra de progresso VISÍVEL
├─ VULNERÁVEL durante todo o channel:
│   ├─ Pode levar dano
│   ├─ Inimigos podem respawnar
│   ├─ Outros piratas podem te matar
│   └─ Interrompido = perde o loot
└─ Se COMPLETAR: pega loot do barco
```

**Por que 10s?**
- Tempo = risco
- Inimigo respawna e te mata se não for rápido
- Outros piratas podem roubar o loot de você
- **High risk, high reward**

### Sistema de Loot

**[A DEFINIR - segurar por enquanto]**

Opções a considerar:
- Gold fixo do barco?
- % do inventário da tripulação?
- Itens especiais apenas de PvP naval?
- Materiais de craft raros?

---

## Mecânicas Avançadas

### Múltiplos Atacantes

**Livre pra todos** — qualquer um pode abordar:

```
Cenário: 3 barcos atacando 1
├─ Barco A derrota Barco D
├─ Barco D fica afundando
├─ Barcos A, B e C podem TODOS abordar
├─ PvP no convés vira "battle royale"
├─ Quem matar a tripulação D pega o loot
└─ Mas pode ser morto por B ou C depois
```

**Alianças entre Tripulações:**
```
Via menu in-game:
├─ Líder da tripulação A convida tripulação B
├─ Se aceitar: formam aliança
├─ Não podem atacar uns aos outros
├─ Dividem loot? (a definir)
└─ Podem desfazer aliança a qualquer momento
```

### Timer de Afundamento

```
Barco com HP 0:
├─ Timer de 10-15 minutos (a definir exato)
├─ Depois do timer: afunda automaticamente
├─ Loot NÃO some quando afunda
│   └─ Senão capitão se auto-afunda pra negar loot
├─ Loot fica FLUTUANDO na água
│   ├─ Visível como item no mar
│   ├─ Pode ser coletado por qualquer um
│   └─ Desaparece após X tempo (5min?)
└─ Incentiva saquear ANTES de afundar
```

**Por que timer longo?**
- Dá tempo pro combate de boarding acontecer
- Múltiplas tentativas de reparo
- Tensão: "quanto tempo tenho?"

### Morte Real vs Respawn

```
MORTE REAL (perde XP/proficiência):
├─ Seu barco afundou completamente
├─ Escolheu "Afundar Imediatamente"
└─ Timer de afundamento acabou

RESPAWN NO BARCO (sem penalidade):
├─ Morreu durante PvP no convés
├─ Mas seu barco ainda está vivo
└─ Delay crescente (5s/10s/15s...)
```

---

## Tesouro Colossal (Phase 4 existente)

Integração com sistema de **baú de world boss**:

```
Baú Colossal no Convés:
├─ Dropa de world boss
├─ Cabe apenas no DECK do barco
├─ MARCA o barco (skull flag visível de longe)
├─ Qualquer um pode atacar barco marcado
│   └─ Mesmo em zona "segura"
├─ Multiplicador de loot por distância:
│   ├─ Abrir onde dropou: x1.0
│   ├─ 1 ilha de distância: x1.6
│   └─ 3+ ilhas: x3.0
└─ Risco/recompensa alto

Mecânica de Saque (mesmo sistema):
├─ Matar tripulação inimiga
├─ Abordar barco
├─ Channel 10s no baú
└─ Pegar tesouro (muito valioso)
```

---

## Considerações de Balanço

### Vantagens de Tamanhos de Barco

```
Barco Pequeno (1-2 passageiros):
├─ Rápido, ágil
├─ Difícil de acertar com projéteis
├─ MAS: pouca tripulação pra defender boarding
└─ Risco: pode ser overwhelmed por números

Barco Médio (3-4 passageiros):
├─ Balanceado
├─ Tripulação razoável pra PvP
└─ Versátil

Barco Grande (6-8 passageiros):
├─ HP alto, tanky
├─ MUITA tripulação pra PvP no convés
├─ Domina boarding
├─ MAS: lento, fácil de acertar projéteis
└─ Pode ser kited por barcos pequenos
```

### Counterplay

```
Contra Boarding:
├─ Manter tripulação no convés (não todos abordando)
├─ Reparar rápido e fugir
├─ Matar atacantes durante channel de saque
└─ Alianças com outros barcos

Contra Projéteis:
├─ Virar/acelerar para desviar
├─ Usar Chain Shot para slow inimigo
├─ Barcos pequenos = mais ágeis
└─ Prever trajetória e contra-atacar
```

### Progressão e Meta

```
Early Game:
├─ Barcos pequenos/médios T1-T2
├─ Evitar PvP (fugir)
└─ Farmar materiais/subir tier

Mid Game:
├─ Barcos T3, tripulação maior
├─ Começar a fazer PvP
└─ Praticar aim/dodge

Late Game:
├─ Barcos T4, builds especializados
├─ Alianças entre guildas
├─ Domínio de rotas marítimas
└─ Pirataria organizada
```

---

## Edge Cases e Polimentos

### Tripulação Desconectada

```
Se player desconecta durante combate naval:
├─ Personagem fica no convés (AFK)
├─ Pode ser morto normalmente
├─ Barco continua navegando (IA básica? ou para?)
└─ Reconecta: volta no mesmo barco (se vivo)
```

### Barco Sem Tripulação

```
Se todos os tripulantes morreram/desconectaram:
├─ Barco para de navegar
├─ Fica deriva lenta
├─ Pode ser abordado livremente
└─ Timer de afundamento continua
```

### Zonas Seguras

```
Perto de Docks/Cidades:
├─ "Harbour Corridor" (corredor seguro)
├─ PvP desabilitado
├─ EXCETO: barcos com Tesouro Colossal (marcados)
│   └─ Sempre attackáveis, mesmo em zona segura
└─ Radius de 100-200u do dock
```

### Loot Flutuante

```
Quando barco afunda:
├─ Loot fica flutuando na água (barril/destroços)
├─ Visível de longe (luz/partículas)
├─ Pode ser coletado navegando por cima
├─ CHANNEL 10s (mesmo risco)
├─ Despawna após 5 minutos
└─ Incentiva outros piratas a chegar rápido
```

---

## Fases de Implementação

### Fase 1: Projéteis
- [ ] Sistema de projéteis físicos (arco, velocidade)
- [ ] Lead targeting (calcular onde alvo vai estar)
- [ ] Hitbox de barco para projéteis
- [ ] VFX de impacto e splash
- [ ] Adaptar skills existentes (Broadside, Long Gun)

### Fase 2: Barco Derrotado
- [ ] Estado "afundando" (HP 0)
- [ ] Visual: velas quebradas, fogo, fumaça
- [ ] Física: deriva lenta, lista pro lado
- [ ] Timer de afundamento (10-15min)
- [ ] Opções: Afundar vs Reparar

### Fase 3: Reparo
- [ ] Interação com mastro
- [ ] Channel 30-60s (interruptível)
- [ ] VFX de reparo (ferramentas, partículas)
- [ ] Restaurar vela + HP 10%
- [ ] Som ambiente (marteladas, cordas)

### Fase 4: Boarding
- [ ] Mecânica de aproximar barcos
- [ ] Botão/input para abordar
- [ ] Animação de pular/gangplank
- [ ] Entrar em outro convés
- [ ] Múltiplos players simultâneos

### Fase 5: PvP no Convés
- [ ] Sistema de respawn com delay crescente
- [ ] Tracking de "mesmo combate" (reset)
- [ ] Respawn condicional (barco vivo vs afundado)
- [ ] Morte real quando barco afunda

### Fase 6: Saque
- [ ] Baú/inventário no barco inimigo
- [ ] Channel 10s (interruptível)
- [ ] Sistema de loot (a definir)
- [ ] VFX de saque
- [ ] Loot flutuante quando afunda

### Fase 7: Alianças
- [ ] Menu de convite entre tripulações
- [ ] Friendly fire desabilitado
- [ ] Visual de aliados (cor/marker)
- [ ] Divisão de loot (a definir)

### Fase 8: Balanço
- [ ] Testar projéteis (velocidade, arco)
- [ ] Ajustar timers (reparo, afundamento, channel)
- [ ] Balancear tamanhos de barco
- [ ] Feedback de players

---

## Questões em Aberto

### 1. Sistema de Loot
- **O que sai do barco derrotado?**
  - Gold fixo?
  - Itens do inventário da tripulação?
  - Materiais especiais de PvP?
  - Tesouro aleatório?

### 2. Loot Flutuante
- **Como funciona exatamente?**
  - Barril visível flutuando?
  - Precisa abordar ou só passar perto?
  - Quanto tempo fica disponível?
  - Desaparece ou fica pra sempre?

### 3. Divisão de Loot em Aliança
- **Como dividir loot entre aliados?**
  - Automático (split igual)?
  - Manual (negociar)?
  - Quem acertar killing blow leva tudo?

### 4. Penalidade de Morte Real
- **Quanto perde ao morrer de verdade (barco afundou)?**
  - % de XP?
  - Proficiência específica (naval)?
  - Drop de items equipados?
  - Durability dos items?

### 5. IA de Barco
- **O que acontece com barco sem ninguém no leme?**
  - Para completamente?
  - Deriva aleatória?
  - IA básica (segue último input)?

---

**Documento criado:** 18/11/2026  
**Autor:** Bruno + Claude  
**Status:** Proposta em revisão

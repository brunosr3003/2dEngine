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

Uma por conjunto de arma: espada e escudo, katana, duas pistolas e anel
mágico. As skills usam o nível do personagem para desbloqueio; não há
passivas ou árvores de compra de habilidades. Ver [SKILLS.md](SKILLS.md).

### Equipamento define estilo
- **Conjuntos:** espada e escudo, katana, duas pistolas e anel mágico.
- **Skills:** três por conjunto, liberadas nos níveis 1, 5 e 10 do personagem.
  Regras e catálogo em [SKILLS.md](SKILLS.md); equipamentos em [COMBATE.md](COMBATE.md).

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

HUD no molde do MIR4 (ver [HUD.md](HUD.md)). **Nenhum painel abre por tecla:**
tudo abre por ícone do HUD ou pelo **≡ MENU** (canto superior direito). Esc
só **fecha** (o painel aberto pelo Menu volta ao Menu); sem nada aberto,
limpa o alvo e desliga o AUTO.

- **Abrir coisas (só clique):** 🎒 Bolsa e 📜 Missões no topo direito; nome da
  zona ou ⤢ do minimapa abrem o **Mapa**; título "Missões ›" do rastreador abre
  o diário e o rodapé "Todas as missões" abre a lista completa; o **MENU** tem
  Bolsa, Missões, Todas, Craft, Forja, Mapa, Vendedores (só "Ir" até o NPC; a Loja do Menu é a de cash, em breve) e Sair.
- **WASD / setas:** mover. **Espaço:** pular. **Shift:** correr.
- **F** ou botão **ATACAR:** ataca o alvo; sem alvo, mira o inimigo mais perto.
- **Tab:** próximo inimigo perto.
- **1 / 2 / 3:** skills (também clicáveis no arco; arrastar ↑ AUTO / ↓ manual).
- **Z:** auto combate. **X:** auto coleta (também pelos botões).
- **C:** poção de vida. **8 / 9 / 0:** poção de mana, vigor e experiência.
- **Alt:** mostra a tecla de cada botão. **Alt+Enter:** tela cheia.
- **Q / E** giram a câmera; **roda** aproxima; arrastar com o botão do meio (ou
  o direito) gira e inclina.
- **LMB:** seleciona alvo ou anda até o chão clicado; ataque básico automático.
- **Mapa** (nome da zona, ⤢ do minimapa ou Menu; Esc, X ou clique fora
  fecham). O minimapa fica no canto superior direito; a roda sobre ele muda o zoom.
  Clicar num ponto de terra do mapa ou do minimapa **viaja** até lá: desliga o
  auto combate, solta o alvo e anda sozinho por etapas de até 160 m. Andar no
  teclado, clicar no mundo, ligar o auto combate ou abrir a loja cancelam.
  O mapa grande mostra as **zonas de mob** (círculo na cor do bicho dominante,
  "Lobo · Nv 1–3"; o hover lista todos os bichos da zona com a chance de cada
  um) e as **regiões de recurso** (losango na cor: madeira, pedra cinza, verde,
  azul, roxa; o hover diz quantos corpos e o que rende). O painel à direita tem
  os **filtros** (Mobs, cada bicho, cada recurso, Vila) — a escolha vale a
  sessão — e o **Ir para**: cada bicho da ilha com a faixa de nível e a
  distância da zona mais perta, cada recurso com a região mais perta. "Ir" (ou
  clicar numa zona/região no mapa) fecha o mapa, anda até lá e, ao chegar, liga
  o **auto combate** na zona ou a **auto coleta** só daquele recurso. O
  minimapa mostra zonas e regiões de leve, respeitando os filtros.
- **Todas as missões** (rodapé do rastreador ou Menu → Progresso → Todas),
  por ilha e na ordem da cadeia: Disponível, Em andamento (com progresso),
  Pronta pra entregar, Concluída ou Bloqueada. Bloqueada mostra o cadeado e, no
  hover, o pré-requisito exato ("Requer nível 3", "Conclua: …", "Na ilha …").
  "Ir" numa liberada desta ilha: disponível anda até o Mestre e abre a oferta;
  em andamento ou pronta liga a auto missão. Clicar numa bloqueada só avisa.
  Missões antigas do mapa de tiles (sem quem as dê nas ilhas) ficam fora.
- **LMB num NPC vendedor:** longe, o personagem anda até ele; perto, abre a loja.
  Clique num item compra 1, Shift+clique compra 5. A loja fecha com Esc, no X
  ou ao se afastar do vendedor. O servidor valida ouro, bolsa e alcance.
- **X do alvo** (no painel do alvo) ou **Esc:** limpa o alvo.
- Reservados pro que ainda não existe (igual ao MIR4): **R** golpe letal,
  **Shift** esquiva (hoje é correr), **4** quarta skill.

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

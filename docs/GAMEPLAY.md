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
fala com o Capitão do Porto → viaja até a ilha (tier escolhido)
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
  `FOR` (dano físico, HP), `DES` (ataque rápido, crítico), `INT` (cada ponto
  alocado dá dano ao Anel Mágico — ataque básico e habilidades — e MP),
  `VIT` (HP max, regeneração),
  `SPD` (mais vigor, regeneração de vigor e recarga menor do Dash).
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

- **Porto Central (hub):** safe zone, NPCs (vendedor, reparador, banco), doca, baú pessoal.
- **Viagem entre ilhas:** pelo Capitão do Porto (`ClientMessage::Viajar`), grátis e imediata.
  O **barco navegável foi apagado em 20/09/2026** (ver `docs/ECONOMIA.md`): existia como
  veículo completo no servidor — vela, leme, vento, canhões — sem uma linha de cliente e sem
  nada pra fazer no mar. Vai ser refeito do zero quando houver destino.
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
- **Joystick virtual (toque):** encostar o dedo na metade **esquerda de baixo**
  (fora de botão/painel) faz aparecer um joystick flutuante onde o dedo tocou;
  empurrar anda como o WASD, relativo à câmera, com intensidade (empurrão leve
  anda devagar; zona morta de 12%). Some ao soltar. Conta como andar na mão:
  pausa auto missão, viagem, "Ir para" e a ida até o NPC; o auto combate segue
  ligado. O dedo do joystick nunca gira a câmera nem clica no mundo — outro dedo
  na direita continua girando, e os botões funcionam ao mesmo tempo. **Soltar o
  polegar não é clique:** no quadro do soltar o joystick já largou o id, então o
  `main` tira da lista o dedo de antes *e* o de depois (`joystick::sem_dedos`),
  e aperto de mouse simulado até 0,3 s depois de um toque não vale como clique.
- **Câmera suave:** mouse, dedo, pinça e roda mexem num alvo e a câmera persegue
  (aproximação exponencial, K = 18/s); o delta do dedo passa por uma média de
  50 ms e, ao soltar, sobra uma inércia que morre em ~0,35 s.
- **Toque (iOS):** um dedo arrastando no **mundo** gira (horizontal) e inclina
  (vertical) a câmera, com a mesma sensibilidade do mouse; **pinça** com dois
  dedos aproxima/afasta na faixa da roda. Toque curto parado é o clique normal
  (andar/selecionar) e sai no **soltar** — arrastar ou pôr o segundo dedo cancela
  o clique. Começar o gesto em cima de botão ou painel do HUD não mexe na câmera.
- **LMB:** seleciona alvo ou anda até o chão clicado; ataque básico automático.
- **Mapa** (nome da zona, ⤢ do minimapa ou Menu; Esc, X ou clique fora
  fecham). O minimapa é um **disco** no canto superior direito; os botões **−** e
  **+** nos cantos de baixo (ou a roda sobre ele) mudam o zoom, o botão ao lado
  do ⤢ alterna entre **compacto e expandido** e o **×** do canto de cima
  **fecha** o minimapa — fica um botão redondo no lugar, que reabre. Tamanho,
  zoom e aberto/fechado ficam guardados no personagem. Clicar num ponto de terra do mapa, ou
  **dentro do disco** do minimapa, **viaja** até lá: desliga o
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

### Morte (como está implementado, molde MIR4)

- HP chega a 0: o personagem cai e aparece a **tela de derrota** por cima de tudo. Mob para de mirar nele. Auto missão, auto combate e auto coleta param e os painéis fecham.
- **Reviver na cidade** (só clique; Esc não revive): renasce na cidade da **ilha onde está**, com HP cheio, sem custo.
- **Perda de XP:** 10% do XP que o nível atual pede pra subir (`lvl² × mult`), limitado ao que o jogador tem acima do início do nível — morrer **nunca** derruba nível nem deixa XP negativo. Recém-chegado no nível perde pouco ou nada.
- **Recuperar XP:** cada morte fica recuperável por **24 h** (guardam-se as 10 mais recentes). As **3 primeiras recuperações do dia são grátis** (reset à meia-noite UTC); depois custa **100 + 50 × nível + metade do XP devolvido** em ouro. Sem ouro, não recupera. O XP devolvido não ganha o bônus da Poção de Experiência.
- Onde recuperar: botão na tela de derrota (a morte mais recente) e **Menu → Aventura → Recuperar XP** (lista todas, com prazo e preço).
- Persistido no personagem: `mortes_json`, `recuperacoes_dia`, `recuperacoes_usadas`. Regras em `crates/server/src/morte.rs`.

### Planejado

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

## Barra de itens

Quatro espaços no canto inferior direito — **C, 8, 9 e 0** (teclas de ação, não abrem painel). Cada espaço guarda um consumível, se usa sozinho (AUTO) e o limiar.

- **Clique** usa. **Arrastar pra cima** liga o AUTO daquele espaço, **pra baixo** desliga (mesmo gesto das skills). Botão direito ou clique num espaço vazio abre o configurador.
- **Configurador** (Menu → Sistema → Barra): escolha um consumível da bolsa e um espaço (ou o contrário), ligue o AUTO, ajuste o limiar em passos de 5% e limpe espaços. Fica salvo no personagem, no servidor.
- **AUTO**: vida/mana/vigor bebem abaixo do limiar (padrão 60/40/30%) com o grupo fora da recarga; o tier é o menor cujo total cobre o que falta pro máximo, senão o maior disponível. Experiência, Fortuna e Sorte bebem quando o buff não está ativo. Morto não bebe; sem estoque o botão fica apagado.
- **Poções curam ao longo do tempo** (parte na hora + ticks de 1 s) e têm **recarga por grupo** (Vida/Vida+ 8 s, Mana/Mana+ 8 s, Vigor 15 s) que cobre a cura inteira: não dá pra tomar outra no meio. O botão mostra a recarga e brilha enquanto cura. Ver `docs/ITENS.md`.

## Coleta

- **Clique numa pedra ou tronco:** o personagem anda até o alcance e coleta aquele nó, ciclo a ciclo, golpeando; a barrinha "Coletando · tipo · N s" mostra o próximo ciclo. Andar cancela. O nó esgota, some (e para de barrar a passagem) e volta no respawn.
- **AUTO COLETA** (X ou botão): vai de nó em nó dos tipos marcados dentro do raio; sem nó, "Aguardando recursos…". **Botão direito** no AUTO COLETA configura tipos e raio (salvo no personagem). Ver `docs/COLETA.md`.
- Padrão de quem nunca configurou: Vida (AUTO ligado), Mana, Vigor e Experiência.

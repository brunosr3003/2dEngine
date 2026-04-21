# Roadmap

> Cada fase tem um **goal** testável. Não avançar antes do goal estar
> demonstrável (não "quase pronto").

---

## Fase 1 — Scaffold e loop end-to-end ✅ (em andamento)

**Goal:** cliente conecta, vê a si mesmo e outros clientes como quadrados
coloridos movendo-se sincronizados pelo servidor autoritativo.

Entregue no scaffold inicial:

- [x] Workspace Cargo (engine / shared / server / client)
- [x] Protocolo bincode (Handshake, Login, Input, Snapshot, Chat)
- [x] Servidor tokio com tick loop 30Hz e AOI naive
- [x] Cliente winit + wgpu renderizando sprites instanciados
- [x] Input WASD + envio de InputFrames
- [x] Câmera smooth-follow

Pendências desta fase:

- [ ] `cargo check` verde no workspace (user precisa instalar Rust).
- [ ] Teste manual com 2 clientes na mesma máquina.
- [ ] Smoke test automático: spawn server, conecta 1 cliente, recebe
      snapshot, desconecta limpo.

---

## Fase 2 — Mundo jogável mínimo (2 semanas)

**Goal:** um mapa navegável com tiles, sprites de verdade, múltiplos
jogadores se vendo com sprites animados, chat funcional.

### Render
- [ ] `Atlas` carregado de PNG + JSON (sub-retângulos).
- [ ] `TilemapRenderer`: chunks instanciados de tiles (não um quad
      por tile).
- [ ] Z-ordering por `position.y` (top-down com profundidade).
- [ ] Sprite animado: componente `AnimState { clip, frame, time }`
      avançado no update.
- [ ] UI mínima: HP bar sobre cada jogador, nome, caixa de chat.

### Simulação
- [ ] Tiles bloqueantes (paredes). Colisão AABB vs grid.
- [ ] **Predição client-side + reconciliação** (ver NETWORKING.md).
- [ ] Interpolação entre snapshots para entidades remotas
      (buffer de 2-3 snapshots, renderiza 100-150ms no passado).

### Net
- [ ] Spatial hash grid no servidor, substitui O(E×S) do AOI.
- [ ] `last_input_seq` → reconciliação client-side.
- [ ] Ping RTT medido com `server_time_ms`.

---

## Fase 3 — Combate e NPCs (3 semanas)

**Goal:** atirar em inimigos, tomar dano, morrer, ressuscitar. Mobs com
IA básica que perseguem e atacam.

- [ ] `Projectile` entity + componente `Lifetime`, `Damage`, `OwnedBy`.
- [ ] Ataque: `ClientMessage::Input.buttons & PRIMARY` → servidor gera
      projétil no cooldown + mira.
- [ ] Colisão projétil vs entidade (broadphase grid, narrow AABB).
- [ ] `Enemy(kind)` com componente `AiState { target, patrol_origin }`.
- [ ] State machine simples: idle → chase → attack.
- [ ] Death + respawn: entidade despawna, envia `Kick`? não, spawna
      de novo após timer.
- [ ] Loot: ao morrer, mob dropa entity `Loot(kind)`; jogador coleta com
      `INTERACT` se estiver em alcance.

---

## Fase 4 — Progressão e persistência (3 semanas)

**Goal:** contas persistentes. Sair e voltar mantém nível, inventário,
posição.

- [ ] `sqlx` + SQLite para dev (schema: accounts, characters, inventory).
- [ ] Postgres como opção de prod via env var.
- [ ] `AuthService`: username/password hashed (argon2), sessão por token.
- [ ] Inventário (24 slots) + equipamento (arma, armadura, anel).
- [ ] XP, level, stats base (HP, MP, DEX, WIS).
- [ ] Classes iniciais: Archer, Warrior, Wizard.
- [ ] Snapshot periódico do personagem → DB (a cada 30s + no logout).

---

## Fase 5 — Cross-platform (4 semanas)

**Goal:** roda nativo em desktop (Win/Mac/Linux) e mobile (Android + iOS)
com **crossplay** no mesmo shard. **Sem build para browser** — o cliente
é um app nativo em todas as plataformas.

### Desktop (Win/Mac/Linux)
- [x] Mac: funciona (wgpu/Metal).
- [ ] Windows: validar em GitHub Actions `windows-latest` (DX12).
- [ ] Linux: validar em `ubuntu-latest` (Vulkan).
- [ ] Instaladores/binary artifacts por plataforma (cargo-bundle / MSI / dmg).

### Android
- [ ] `cargo-apk` ou projeto Gradle embedando o crate.
- [ ] `NativeActivity` via `android-activity`.
- [ ] UI de touch: joystick virtual (canto esquerdo), botão de atacar
      (direito). Adicionar `TouchInput` no engine/input.
- [ ] Ciclo de lifecycle (Pause/Resume) → reconectar no servidor.
- [ ] wgpu → Vulkan (backend preferido) com fallback GLES 3.

### iOS
- [ ] `cargo-mobile2` (gera projeto Xcode) ou Xcode wrapper custom.
- [ ] Metal backend do wgpu.
- [ ] Mesma UI de touch (compartilhada com Android).
- [ ] App Store exige: LaunchScreen, ícones, TestFlight para beta.

### Multiplatform concerns
- [ ] Assets compatíveis (evitar texturas >2048² em GL ES 2 / WebGL2).
- [ ] Controles adaptativos: `InputSource { Keyboard, Touch, Gamepad }`.
- [ ] Build script CI (GitHub Actions) que compila todos os targets
      em cada PR.

---

## Fase 6 — Conteúdo e lançamento (∞)

**Goal:** um jogo que vale a pena jogar.

- [ ] Gerador procedural de dungeons (BSP ou drunkard's walk).
- [ ] Biomas: grassland, desert, cavern, nexus (hub social).
- [ ] 15+ tipos de mobs com comportamento distintos.
- [ ] 3+ bosses com scripts específicos.
- [ ] Sistema de raridade de loot (5 tiers).
- [ ] Party-finding + chat por canal.
- [ ] Trade P2P entre jogadores.
- [ ] Servidor de matchmaking para múltiplos shards.
- [ ] Leaderboard / logros persistentes.

---

## Infra / operacional (paralelo a tudo)

- [ ] CI: `cargo check`, `cargo test`, `cargo clippy` em PR.
- [ ] `cargo deny` para checar licenças e CVEs.
- [ ] Docker/Nomad para o servidor.
- [ ] Observabilidade: `tracing` → OTLP → Grafana/Tempo.
- [ ] Backup do DB periódico.
- [ ] Rate-limit + DDoS mitigation (Cloudflare na frente do WS).

---

## Decisões a tomar quando a hora chegar

- [ ] Monetização: F2P com cosméticos? B2P? Early access?
- [ ] Hospedagem: VPS simples (Hetzner) vs Kubernetes? Resposta depende
      da escala — começa com 1 VPS.
- [ ] Shards: 1 servidor = 1 mapa = 1 processo? Ou 1 processo com N
      mundos? Primeiro caso é mais simples, depois pode evoluir.
- [ ] Suporte a mods? Luau/Rhai embarcado para scripts de enemy/item?

---

## Anti-goals (coisas que NÃO vamos fazer)

- ❌ Implementar bindings de Vulkan/Metal do zero.
- ❌ Refazer hecs/glam/winit — "not invented here" é cilada.
- ❌ Bevy (resolve demais — engine própria virou só camada fina).
- ❌ 3D. Nunca.
- ❌ Voice chat. Terceirizar se quiserem (Discord).

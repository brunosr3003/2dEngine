# Barcos novos + PvP naval — status da implementação

Design: `docs/BARCOS.md`, `docs/PVP-NAVAL.md` e `docs/NAVEGACAO.md`.
Este arquivo registra o que está realmente ligado ao jogo e o que ainda falta.

Branch: `worktree-tag-cla-preco-mercado`. Atualizado em 20/11/2026.

## Resumo

| Fase | Escopo | Status real |
|---|---|---|
| 1 | Tipos, stats, migração, pontos e tiers | **Implementada** |
| 2 | Estaleiro e UI dos barcos | **Implementada** |
| 3 | Combate com projéteis desviáveis | **Implementada** |
| 4 | Estado derrotado, fogo e reparo de vela | **Implementada** |
| 5 | Abordagem hostil e respawn naval | **Implementada** |
| 6 | Saque PvP e Naval Tokens | **Implementada** |
| 7 | IA do barco abandonado | **Implementada** |
| 8 | Alianças entre tripulações | **Implementada via grupos** |

O sistema novo já altera o jogo. Barcos legados continuam aceitos e são
migrados; barcos novos usam tipo, tier e pontos nas contas compartilhadas por
cliente e servidor.

## Implementado

### 1. Barcos, progressão e migração

Em `crates/shared/src/navio.rs` e `crates/shared/src/items.rs`:

- nove tipos (`Tamanho` × `Variante`), com nomes, capacidade e stats base;
- tiers T1–T4, limites de pontos e stats finais em unidades reais do jogo;
- `NavioData` guarda `tipo_novo`, `tier` e quatro eixos de pontos;
- casco, velocidade, giro, arranque, canhão, visual e capacidade leem os dados
  novos, preservando o caminho legado quando necessário;
- migração dos quatro cascos antigos preserva cosméticos, tesouro e proporção
  de HP;
- validação de alocação, custo de respec e custo de upgrade;
- upgrade exige outro barco idêntico e ouro; pontos são preservados;
- capacidade 2/4/8 é aplicada ao embarque da tripulação;
- construção, alocação, respec e upgrade são validados no servidor.

### 2. Estaleiro e apresentação

Em `crates/client/src/estaleiro_ui.rs`:

- galeria dos nove tipos e construção dos cascos T1;
- preview 3D;
- painel de quatro stats com pontos usados/disponíveis e preview;
- respec com custo e upgrade T1→T4;
- cosméticos de vela e figura de proa continuam integrados.

### 3. Combate naval — concluído nesta rodada

Base já existente:

- balas são entidades temporárias do servidor e acertam apenas ao pousar;
- avanço da mira e raio de acerto permitem desviar virando ou saindo do ponto;
- dano de PvP vai ao casco e respeita PK, águas pacíficas e zona segura;
- cliente desenha disparo, fumaça, trajetória, impacto e splash;
- todos os canhões saem do costado, nunca da proa.

Finalizado agora:

- **Broadside:** cinco balas em leque por alvo, com voo e colisão próprios; o
  dano total foi dividido entre as cinco;
- **Long Gun:** bala a 60 u/s, teto de voo de 0,7 s e avanço específico;
- **Chain Shot:** quinta habilidade, alcance 18, dano baixo e recarga de 20 s;
  acertar um casco válido reduz a velocidade a 50% por 5 s;
- a lentidão só entra quando o dano ao casco foi autorizado e aplicado;
- barra naval ampliada para cinco botões, atalhos 1–5, AUTO, cooldown,
  tooltip, cor e ícone da Chain Shot;
- VFX usa o tempo rápido correto na Long Gun e tiro lateral na Chain Shot;
- piloto automático usa Chain Shot contra navios, sem gastá-la em mobs;
- `Session` mantém os relógios de Smoke e Chain Shot separadamente;
- testes cobrem leque, voo rápido, definição da Chain Shot e lentidão.

### 4. Barco derrotado, abordagem e respawn

- casco a zero permanece por 12 minutos como destroço abordável;
- destroço para, deriva, recebe fogo/fumaça no cliente e mantém o convés;
- capitão pode abandonar imediatamente ou reparar a vela por 45 s;
- movimento, dano, saída do ponto ou estado inválido interrompem o reparo;
- reparo de vela devolve 10% do casco e o reparo completo continua no cais;
- abordagem hostil é liberada a curta distância contra destroços fora da zona
  segura; o invasor não passa a integrar a tripulação inimiga;
- mortes no combate naval usam espera crescente e retornam ao convés enquanto
  o navio ainda existe; o afundamento aplica a morte naval definitiva.

### 5. Navegação, convés e tripulação

- casco com giro, inércia, aceleração e rota marítima A*;
- piloto automático do servidor orbita, mira e dispara;
- convés andável, leme, capitão e membros da tripulação;
- membro do mesmo grupo pode embarcar ao navegar ao lado;
- o próprio barco do passageiro é recolhido durante o embarque;
- lotação respeita o tamanho do barco;
- sem capitão, a tripulação é levada ao cais.

### 6. Saque naval e tesouro

- invasor aborda o destroço e canaliza o saque por 10 s;
- mover, tomar dano, sair do destroço ou outro jogador concluir primeiro
  interrompe/recusa o canal;
- a recompensa é gerada pelo sistema e não remove itens da vítima;
- o saque paga ouro, 1–3 Naval Tokens e fragmentos navais, com entrega pelo
  correio quando a bolsa está cheia;
- cada destroço só pode ser saqueado uma vez;
- o Estaleiro oferece uma loja naval que consome tokens;
- elites do mar e, com chance menor, criaturas comuns fornecem tokens em PvE;
- destroço não saqueado deixa um barril temporário, canalizável e marcado no
  mapa por 15 minutos.

Infraestrutura adicional do Baú do Colosso:

- a maquinaria do Baú do Colosso permanece no código, mas
  `BAU_DO_COLOSSO_LIGADO = false`: chefes não geram esse baú atualmente;
- multiplicador cresce ao atravessar ilhas;
- barco com tesouro fica marcado e pode ser atacado;
- ao afundar, o tesouro boia; em abate PvP ele é reservado ao vencedor;
- coleta é canalizada e pode ser interrompida;
- abrir paga ouro/materiais conforme o conteúdo e o multiplicador.

### Fase 7 — IA do barco

- ao abordar um inimigo, o casco próprio continua no mar sob comando
  autônomo em vez de desaparecer;
- o barco responde com bordadas aos agressores válidos;
- sem capitão, navega de volta ao cais depois da tolerância de dois minutos;
- reconectar ou usar “voltar ao meu navio” recupera o casco autônomo próximo.

### Fase 8 — alianças

- o convite/aceite e a dissolução usam o sistema de grupo já exposto no menu;
- capitães aliados podem continuar em cascos distintos;
- regras de PK impedem fogo amigo e o HUD identifica os membros do grupo;
- o saque de destroço é dividido automaticamente entre aliados a até 30 u.

## O que ainda falta

- skins completas de casco e variações visuais próprias por tier;
- VFX/áudio de polimento para reparo, abordagem e saque;
- playtest e ajuste fino de builds, recompensas e timers.

## Validação de 20/11/2026

- `cargo check -p shared -p server -p client`: **passou**;
- 24 testes de `shared::navio`: passaram;
- 42 testes de `server::world::navio`: passaram;
- 14 testes de convés, incluindo abordagem, autonomia e reconexão, passaram.

Warnings antigos do projeto permanecem, sem erro de compilação.

# Tempest

MMO de mundo aberto em Rust: servidor autoritativo, cliente próprio em
macroquad e um arquipélago de ilhas voxel geradas por semente.

```
crates/
  shared/      regra e terreno — o que os dois lados PRECISAM concordar
  server/      simulação autoritativa, um processo por canal
  client/      cliente 3D (macroquad/miniquad)
  web/         cadastro, login e diretório de canais
  panoptico/   painel de observabilidade (só leitura)
  admin_cli/   ferramentas de operação
tools/voxrender/  pipeline de arte voxel → sprite
```

## O que define este projeto

**O terreno nunca viaja pela rede.** Cliente e servidor geram a mesma ilha da
mesma semente, com o mesmo código em `shared::terreno`. O servidor guarda 2
bytes por coluna (~16 MB/km² contra ~416 MB de volume cheio); o cliente gera o
volume só em volta do jogador. O que trafega é posição, e nada mais.

**A colisão é uma comparação de inteiros.** Não existe malha de navegação nem
colisão 3D: um bloco de subida anda, dois a três só pulando, quatro é parede.
Descer é livre. Tronco, matação e toco barram; flor e capim não.

**O cliente não prediz.** Ele suaviza. Toda decisão de jogo — mover, pular,
atacar, achar caminho — acontece no servidor, e o cliente desenha o resultado.
Ver [docs/NETWORKING.md](docs/NETWORKING.md).

**Uma verdade por pergunta.** Quando duas partes do código respondem a mesma
pergunta por contas diferentes, elas divergem — e o sintoma aparece longe da
causa. O histórico deste repositório é feito disso: o desenho da árvore e a
colisão dela, a altura do pulo e a regra de degrau, a câmera e o desvio de
morro. Cada um foi um bug com o mesmo formato, e a correção foi sempre a
mesma: uma função só, chamada pelos dois lados.

## Rodar

```bash
# banco + web (cadastro, lista de canais)
cargo run --release --bin web

# um canal
MMO_ZONA=ilha_inicial BIND_ADDR=0.0.0.0:9200 cargo run --release --bin server

# cliente, lado a lado com o terminal
./scripts/run-client.sh --build
```

Detalhes de build em [docs/BUILD.md](docs/BUILD.md); operação de canais em
[docs/SERVIDORES_E_CANAIS.md](docs/SERVIDORES_E_CANAIS.md).

## Documentos

| Onde | O quê |
|---|---|
| [ARCHITECTURE](docs/ARCHITECTURE.md) | as peças e por que estão separadas |
| [MUNDO](docs/MUNDO.md) | geração das ilhas, movimento, vegetação |
| [NETWORKING](docs/NETWORKING.md) | protocolo, AOI, o que trafega |
| [SERVIDORES_E_CANAIS](docs/SERVIDORES_E_CANAIS.md) | realm, canal, auto-escala |
| [ECONOMIA](docs/ECONOMIA.md) / [ITENS](docs/ITENS.md) | moeda, forja, grau e refino |
| [GAMEPLAY](docs/GAMEPLAY.md) / [NEW_MECHANICS](docs/NEW_MECHANICS.md) | design |
| [PANOPTICO](docs/PANOPTICO.md) | painel de observabilidade |
| [PIPELINE_ARTE](docs/PIPELINE_ARTE.md) | voxel → sprite |
| [COMBATE](docs/COMBATE.md) | conjuntos de arma, peso de armadura, skills |
| [SKILLS](docs/SKILLS.md) | as 12 habilidades e desbloqueio por nível |
| [COLETA](docs/COLETA.md) | coleta automática por densidade do lugar |
| [ECONOMIA_DE_CRAFT](docs/ECONOMIA_DE_CRAFT.md) | materiais, receitas, síntese de cor |
| [PERSONAGEM](docs/PERSONAGEM.md) | rig, animação, montaria |
| [character create](docs/character%20create.md) | what to model: parts, sizes, pivots, palette (EN) |
| [COMBATE_POR_ALVO](docs/COMBATE_POR_ALVO.md) | combate e wire binário |
| [BOSSES](docs/BOSSES.md) | chefes de campo, golpes telegrafados, lugar e respawn |
| [VILA_E_PORTO](docs/VILA_E_PORTO.md) | cidade, porto, casas voxel, NPCs de porta |
| [MISSOES](docs/MISSOES.md) | Mestre de Missões, cadeia da ilha inicial, diário (J) |
| [HISTORIA](docs/HISTORIA.md) | a missão principal sem fim: capítulos por ilha, travas de nível, Crônicas da Tempestade |
| [DUNGEONS_E_RAIDS](docs/DUNGEONS_E_RAIDS.md) | desenho: dungeons, raids, chefes, matchmaking entre realms, recompensas por faixa |
| [HUD](docs/HUD.md) | HUD e Menu Principal no molde do MIR4: layout, painéis, nada abre por tecla |

## Stack

`hecs` (ECS) · `tokio` + WebSocket binário · `postcard` · `glam` ·
`sqlx`/Postgres · `argon2` · `axum` · `macroquad`

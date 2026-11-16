# Captura de filmes e prints para anúncio (`MMO_FILME`)

Código: `crates/client/src/filme.rs` (cenas, câmeras, captura) e
`crates/client/src/relogio.rs` (relógio fixo). Só existe em build **debug de
desktop** (`#![cfg(all(debug_assertions, not(ios/android)))]`) e só roda com
`MMO_FILME` nomeando uma cena; sem a variável o jogo abre como sempre
(teste `filme::testes::o_filme_so_liga_com_a_variavel`).

As cenas são as do teste de desempenho (`docs/BENCHMARK.md`) estendidas: mesmo
mundo local sem servidor, mesma câmera do jogo, luz, neblina, corpos, auras,
efeitos de habilidade e números de dano, em várias zonas. Sem HUD, sem painel.

## Como funciona

- Cada quadro é desenhado num render target com MSAA (`MMO_FILME_MSAA`,
  padrão 8) do tamanho pedido, lido de volta e mandado em RGBA cru pro stdin do
  `ffmpeg` (H.264, yuv420p, CRF 18, 60 fps, `+faststart`, sem áudio).
- **Passo fixo de 1/60 s**: `relogio::fixa` segura o relógio que o desenho lê
  (`get_time`/`get_frame_time` importados de `crate::relogio` em render3d,
  auras, água/mar, luzes, abismo, efeitos, renascimento, etc.). Fora da captura
  o relógio é o do macroquad, intocado. Por isso o vídeo é liso mesmo que cada
  quadro leve 100 ms pra renderizar.
- O 2D por cima do mundo (números de dano, barras) usa
  `render3d::define_escala_do_alvo` pra sair no tamanho de 1080p em qualquer
  resolução; `render3d::world_to_screen` passou a usar `tela()` (no jogo, sem
  alvo, é a tela de sempre).
- Gráficos no Ultra (`config_graficos::publica_para_teste`, nada é salvo).
- Antes de cada tomada o terreno é assado inteiro e há 1 s de aquecimento não
  gravado.
- Retrato: a câmera recua `sqrt((16/9)/aspecto)` (o FOV vertical é fixo, então
  o quadro estreito precisa de mais distância pra caber a ação). iPad 3:4 recua
  menos que 9:16.

## Variáveis

| Variável | O quê |
|---|---|
| `MMO_FILME` | cena(s): `bosque,geleira,ermo,planalto,celeste,kogen,abissal,magica,mar,auras,chefe` ou `todas` |
| `MMO_FILME_TAM` | `WxH` ou preset (abaixo); sufixo `-paisagem` deita |
| `MMO_FILME_SAIDA` | pasta (padrão `target/filme`) |
| `MMO_FILME_PRINTS` | N prints espalhados por cena |
| `MMO_FILME_PRINTS_EM` | tempos em segundos, ex. `3.5,8,12.2` |
| `MMO_FILME_SO_PRINTS` | `1` = não grava vídeo, para depois do último print |
| `MMO_FILME_MSAA` | amostras do MSAA (padrão 8) |

Presets de `MMO_FILME_TAM` (retrato, salvo os deitados por natureza):

| Nome | Tamanho | Uso |
|---|---|---|
| `16x9` | 1920×1080 | anúncio horizontal (padrão) |
| `4k` | 3840×2160 | anúncio 4K |
| `9x16` | 1080×1920 | Reels/Shorts/TikTok |
| `iphone67` | 1290×2796 | App Store iPhone 6.7" |
| `iphone65` | 1242×2688 | App Store iPhone 6.5" |
| `ipad13` | 2064×2752 | App Store iPad 13" |
| `ipad129` | 2048×2732 | App Store iPad 12.9" |
| `android` | 1080×1920 | Play Store |
| `android-alto` | 1080×2400 | Play Store 20:9 |

`iphone67-paisagem` = 2796×1290, e assim por diante.

Saída: `tempest-<cena>-<tam>.mp4` e `tempest-<cena>-<tam>-<t>s.png`.

## Rodar (brunjilinux, display :1, RTX 3060)

```sh
cd ~/lote-mercado && export CARGO_TARGET_DIR=~/lote-mercado-target
cargo build -p client --bin client
DISPLAY=:1 MMO_WIN=320x240 MMO_FILME=todas MMO_FILME_TAM=16x9 MMO_FILME_PRINTS=1 \
  MMO_FILME_SAIDA=/tmp/tempest-filme $CARGO_TARGET_DIR/debug/client
DISPLAY=:1 MMO_WIN=320x240 MMO_FILME=todas MMO_FILME_TAM=9x16 MMO_FILME_SAIDA=/tmp/tempest-filme \
  $CARGO_TARGET_DIR/debug/client
scripts/filme-montagem.sh /tmp/tempest-filme 16x9
scripts/filme-montagem.sh /tmp/tempest-filme 9x16
# prints de loja, sem vídeo
DISPLAY=:1 MMO_WIN=320x240 MMO_FILME=planalto,kogen,celeste,abissal,chefe,mar \
  MMO_FILME_TAM=iphone67 MMO_FILME_PRINTS=1 MMO_FILME_SO_PRINTS=1 \
  MMO_FILME_SAIDA=/tmp/tempest-prints $CARGO_TARGET_DIR/debug/client
```

A janela é só um contexto GL (o quadro vai pro render target); a captura força
X11 (`window_conf`) pra o Wayland não suspender os quadros com a janela coberta.

## Cenas

| Cena | Zona | O quê |
|---|---|---|
| bosque | Bosque (`ARQUIPELAGO[0]`) | borda da vila, caça: sobrevoo e órbita |
| geleira | Geleira | neve, luta |
| ermo | Ermo | dunas e mesas, luta |
| planalto | Planalto | a luta grande do benchmark (32 mobs, 6 jogadores, buraco negro) |
| celeste | Skyreach | da ilha 1 até a 0 por cima das nuvens, luta |
| kogen | Kōgen-tō | cruzamento de Shibuya à noite, neon, robôs |
| abissal | Abyssia | reino da bolha, luta |
| magica | Ilha Mágica I | chegada, luta |
| mar | mar aberto | frota com esteira; bordada contra tubarões e elite; chegada ao cais do farol do Bosque |
| auras | Bosque | fila com os 6 tiers de aura de renascimento (R1, 3, 5, 10, 15, 55), dolly |
| chefe | Planalto | Thunderhide Behemoth (kind 22) contra 7 jogadores |

Montagem: `scripts/filme-montagem.sh` (trechos fixos por cena, crossfade 0,4 s).

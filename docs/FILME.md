# Captura de filmes e prints para anúncio (`MMO_FILME`)

Código: `crates/client/src/filme.rs` (cenas, câmeras, captura) e
`crates/client/src/relogio.rs` (relógio fixo). Só existe em build **debug de
desktop** (`#![cfg(all(debug_assertions, not(ios/android)))]`) e só roda com
`MMO_FILME` nomeando uma cena; sem a variável o jogo abre como sempre
(teste `filme::testes::o_filme_so_liga_com_a_variavel`).

As cenas são as do teste de desempenho (`docs/BENCHMARK.md`) estendidas: mesmo
mundo local sem servidor, mesma câmera do jogo, luz, neblina, corpos, auras,
efeitos de habilidade e números de dano (`efeitos::numeros`, sem as placas de
nível/vida), em várias zonas. Sem HUD, sem painel. Os prints de loja saem
limpos, sem números (`MMO_FILME_LIMPO`).

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
- Gráficos no Ultra (`config_graficos::publica_para_teste`, nada é salvo), com
  a distância de visão **além do Ultra**: `RAIO_DA_CAPTURA` = 20 pedaços
  (`MMO_FILME_RAIO`), 18 no mar (a faixa de dither de `mar_cena` acompanha o
  raio). A neblina fecha um pedaço DENTRO do quadrado carregado (antes ela era
  empurrada pra longe com a câmera afastada e mostrava a borda: árvores
  cortadas em linha). As casas vão até a mesma distância
  (`construcoes::define_alcance_da_captura`).
- Os modelos pedidos no meio da cena (roupas pagas, chapéus, skins de arma,
  `rig_ou_pede`) são atendidos a cada quadro: antes só antes da primeira cena,
  e toda roupa saía a camisa padrão.
- Antes de cada tomada o terreno é assado inteiro e há 1 s de aquecimento não
  gravado.
- **Profundidade (17/11/2026)**: o render target da captura tinha depth de
  16 bits (`TextureFormat::Depth` do miniquad = `GL_DEPTH_COMPONENT16`) e a
  câmera o near plane de 0,01 do macroquad, com a neblina a ~300 u: z-fighting
  e geometria "cortando" outra (nuvem furada, borda da ponte serrilhada,
  degraus de terreno vazando por cima da grama). Agora o depth de render
  target é 24 bits no desktop (`vendor/miniquad/src/graphics/gl.rs`; Android,
  iOS e web seguem 16) e o near da captura é 15% da distância olho→alvo
  (0,5–3 u, `planos_da_captura`). O jogo continua com o near de sempre.
  Com o depth exato, as camadas coplanares do telegráfico do chefe (fundo,
  preenchimento, borda, setas) brigavam e a área vermelha saía em pedaços; na
  captura cada vértice sobe um fio sobre o anterior (`chao_em_camadas`). No
  jogo (janela já com 24 bits) o mesmo deve acontecer: ver `telegrafico.rs`.
- **Câmera sem colisão (17/11/2026)**: a câmera da captura não é mais
  empurrada pra fora do chão e não segue a altura crua do bloco sob o alvo
  (função degrau: pulava 0,5 u a cada borda de bloco, 6 u na revelação do
  Planalto e 15 u na Skyreach, ao passar do vazio pra ilha). A altura é a
  média do chão num disco de 6 u (`chao_suave`) numa mola de 0,5 s no relógio
  fixo (`mola`). No mar o navio e os bichos ficam exatamente no caminho
  (antes os snapshots a cada 0,25 s davam 43 trancos na bordada).
- **Conferir a câmera**: `MMO_FILME_CAMERA=cam.csv` grava olho/alvo de todo
  quadro (com o olho do código antigo, pra comparar) e
  `scripts/filme-camera.py cam.csv` mostra por tomada a folga mínima sobre o
  chão e os "trancos" (segunda diferença da posição). `MMO_FILME_SO_CAMERA=1`
  não grava vídeo (rode pequeno, `MMO_FILME_TAM=640x360 MMO_FILME_MSAA=1`).
- Retrato: a câmera recua `sqrt((16/9)/aspecto)` (o FOV vertical é fixo, então
  o quadro estreito precisa de mais distância pra caber a ação). iPad 3:4 recua
  menos que 9:16. A fila de heróis (`auras`) recua só a raiz disso (1,33× em
  9:16): com o recuo cheio eram seis bonecos pequenos numa faixa e o olho
  entrava na casa atrás no fim do dolly.
- Retrato v2 (17/11/2026): mesmo comando do anúncio com `MMO_FILME_TAM=2160x3840
  MMO_FILME_ESCALA=1080:1920` e a montagem com `2160x3840`.

## Variáveis

| Variável | O quê |
|---|---|
| `MMO_FILME` | cena(s): `bosque,geleira,ermo,planalto,celeste,kogen,abissal,magica,mar,auras,chefe` ou `todas` |
| `MMO_FILME_TAM` | `WxH` ou preset (abaixo); sufixo `-paisagem` deita |
| `MMO_FILME_SAIDA` | pasta (padrão `target/filme`) |
| `MMO_FILME_PRINTS` | N prints espalhados por cena |
| `MMO_FILME_PRINTS_EM` | tempos em segundos, ex. `3.5,8,12.2` |
| `MMO_FILME_SO_PRINTS` | `1` = não grava vídeo, para depois do último print |
| `MMO_FILME_LIMPO` | `1` = sem números de dano (prints de loja). Ligado por padrão com `MMO_FILME_SO_PRINTS`; `0` desliga |
| `MMO_FILME_MSAA` | amostras do MSAA (padrão 8; a RTX 3060 aceita 16) |
| `MMO_FILME_RAIO` | raio de terreno em pedaços (padrão 20) |
| `MMO_FILME_ESCALA` | `W:H`: reduz (lanczos) ao gravar cada cena; renderizar em 2x e guardar 1080p |
| `MMO_FILME_CRF` / `MMO_FILME_PRESET` | x264 das cenas (18 / slow) |
| `MMO_FILME_CAMERA` | CSV com a câmera de cada quadro |
| `MMO_FILME_SO_CAMERA` | `1` = só o CSV, sem vídeo |

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

## As lutas (`filme_luta.rs`)

Sem servidor, mas como o jogo: clareiras espalhadas pelo mapa
(`clareiras`: chão livre, seco, plano, longe umas das outras), cada uma com
2-4 jogadores e 3-6 mobs que chegam pelas bordas. Jogador corre até o alvo,
bate o combo (`acao::GOLPE`, variante 0-1-2), solta skill (`Ent::skill` + os
efeitos do próprio `Habilidades`, no relógio fixo), salta (Leap), recua quando
o mob encosta (pistola/anel); mob avança e morde (`Ent::ataque_mob` + bote);
todo mundo sente o golpe (`Ent::ferido`), mob morre (hp 0 tomba) e outro
chega. O chefe telegrafa (`chefe_anim::Carga` + `telegrafico`) e os jogadores
saem da área. Visual dos jogadores: `HEROIS`/`OUTROS` (roupas e chapéus pagos,
skins de arma, aura de equipamento azul/roxo T4 +12 e verde +5, R1..R15).

## Cenas

| Cena | Zona | O quê |
|---|---|---|
| bosque | Bosque (`ARQUIPELAGO[0]`) | borda da vila, caça: sobrevoo e órbita |
| geleira | Geleira | neve, luta |
| ermo | Ermo | dunas e mesas, luta |
| planalto | Planalto | 6 clareiras de luta em volta da cidade: voo baixo por três, depois a câmera sobe e mostra todas |
| celeste | Skyreach | da ilha 1 até a 0 por cima das nuvens, luta |
| kogen | Kōgen-tō | cruzamento de Shibuya à noite, neon, robôs |
| abissal | Abyssia | reino da bolha, luta |
| magica | Ilha Mágica I | chegada, luta |
| mar | mar aberto | frota com esteira; bordada contra tubarões e elite; chegada ao cais do farol do Bosque; chegada ao porto neon de Kōgen-tō |
| auras | Bosque | vitrine: 6 heróis com roupas/chapéus pagos, skins de arma, aura de equipamento e R1, 2, 3, 5, 10, 15, dolly |
| chefe | Planalto | Thunderhide Behemoth (kind 22) contra 7 jogadores |

Montagem: `scripts/filme-montagem.sh` (trechos fixos por cena, crossfade 0,4 s).
`FILME_CORTES="cena:início:segundos ..."` troca os trechos, `FILME_FADE` o
crossfade, `FILME_ESCALA=1920:1080` reduz (captura em 2560×1440 sai mais
nítida) e `FILME_CRF`/`FILME_PRESET` a qualidade (20 / slow). O anúncio v2 de
17/11/2026 (4K, MSAA 16, raio 24, lanczos pra 1080p ao gravar, CRF 8; montagem
veryslow CRF 15):

```sh
DISPLAY=:1 MMO_WIN=320x240 MMO_FILME=mar,celeste,planalto,chefe,auras MMO_FILME_TAM=3840x2160 \
  MMO_FILME_MSAA=16 MMO_FILME_RAIO=24 MMO_FILME_ESCALA=1920:1080 MMO_FILME_CRF=8 \
  MMO_FILME_SAIDA=~/filme-ad/final $CARGO_TARGET_DIR/debug/client
FILME_CORTES="mar:0.3:8.5 celeste:0.5:7 planalto:4.5:8 planalto:13.2:9.5 chefe:1:8 auras:1:9 mar:20.5:6.5 mar:27.5:5.5" \
  FILME_FADE=0.5 FILME_CRF=15 FILME_PRESET=veryslow \
  scripts/filme-montagem.sh ~/filme-ad/final 3840x2160 ~/filme-ad/tempest-ad-16x9.mp4
```

O v1 (mesmo dia, antes das correções):

```sh
FILME_CORTES="mar:0.3:8.5 celeste:0.5:7 planalto:4.5:8 planalto:13.2:9.5 chefe:1:8 auras:1:9 mar:20.5:6.5 mar:27.5:5.5" \
  FILME_FADE=0.5 FILME_ESCALA=1920:1080 scripts/filme-montagem.sh ~/filme-ad/final 2560x1440 ~/filme-ad/tempest-ad-16x9.mp4
```

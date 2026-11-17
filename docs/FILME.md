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
| `MMO_FILME_LIMPO` | `1` = sem números de dano (prints de loja). Ligado por padrão com `MMO_FILME_SO_PRINTS`; `0` desliga |
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
crossfade e `FILME_ESCALA=1920:1080` reduz (captura em 2560×1440 sai mais
nítida). O anúncio de 17/11/2026:

```sh
FILME_CORTES="mar:0.3:8.5 celeste:0.5:7 planalto:4.5:8 planalto:13.2:9.5 chefe:1:8 auras:1:9 mar:20.5:6.5 mar:27.5:5.5" \
  FILME_FADE=0.5 FILME_ESCALA=1920:1080 scripts/filme-montagem.sh ~/filme-ad/final 2560x1440 ~/filme-ad/tempest-ad-16x9.mp4
```

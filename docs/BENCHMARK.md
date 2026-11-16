# Teste de desempenho do primeiro login

Código: `crates/client/src/benchmark.rs` (lógica, cenas, máquina de estados) e
`crates/client/src/benchmark_ui.rs` (telas). Ganchos: `main.rs` (`Jogo::desenhar`,
limite de quadros no laço principal, entrada do mundo) e `config_graficos.rs`
(presets públicos ao crate, `aplica_nivel`, `baixar_um_nivel`, botão
"Run performance test again").

## Fluxo

1. Primeira vez que a tela de personagens aparece num aparelho sem
   `benchmark.prefs` (ao lado de `graficos.prefs`, `lembranca::caminho`), o teste
   começa sozinho. Desligado com `MMO_SEM_BENCHMARK`, `MMO_CHAR` ou
   `MMO_PREVIA_LOGIN` (testes automáticos).
2. Painel de progresso por cima das cenas ("Checking your device…", barra,
   cena · preset, botão Skip). Roda um quadro por vez dentro de `desenhar`, então
   a conexão continua sendo bombeada. Os limites de fps (economia e Gráficos)
   ficam desligados enquanto mede.
3. Tela de escolha: Performance / Balanced / Quality (recomendado em destaque),
   alvo 30 ou 60 fps, a linha "Your device: about N fps on High" e o que será
   aplicado. Apply aplica via `ConfigGraficos::aplica_nivel` e grava
   `benchmark.prefs`; não roda de novo.
4. Skip aplica o padrão da plataforma e também grava: celular = Medium (Balanced)
   a 30 fps, desktop = High a 60.
5. Menu › System › Graphics › "Run performance test again" refaz o teste (no jogo, ele
   desenha no lugar do mundo; o painel de Gráficos aberto segura a entrada).

## Cenas (sem servidor)

- **Luta grande**: Planalto (`ARQUIPELAGO[3]`), praça da cidade; o jogador com a
  aura de renascimento mais pesada (buraco negro, R15), 6 jogadores com auras de
  equipamento, 32 monstros do Planalto andando em anéis e atacando, efeitos de
  habilidade (só os meus quando "efeitos dos outros" está desligado) e números
  de dano.
- **Cidade cheia**: mesma ilha e multidão, câmera no zoom máximo com a menor
  inclinação que a distância de visão permite (o pior caso de terreno, forração,
  árvores e casas).
- **Mar aberto**: `benchmark::cena_mar` — terreno do mar na rota a partir do cais
  do Bosque, `agua::desenha` e 6 cascos `mar_ui::desenha_casco`. TODO: quando o
  mar ganhar um construtor de cena próprio, chamar daqui.

Cada cena: o terreno é assado sem medir (6 pedaços/quadro no celular, 16 no
desktop), 8 quadros de aquecimento, depois 30–75 quadros no celular (20–45 no
desktop). Mede o tempo entre quadros apresentados (média e p95) e o tempo de CPU
do desenho até o `flush` (p95). Corta cedo quando a resposta é óbvia: folgado a
60 ou abaixo de ~19 fps. Teto total de 45 s.

## Presets e escolha

Os níveis são os presets de `config_graficos` (Low, Medium, High, Ultra), varridos
do mais pesado ao mais leve. O celular começa no High (Ultra = ~400 MB de malha,
arriscado na tela de personagens; o painel continua oferecendo). Um preset que
passa em tudo a 60 fps no objetivo Performance encerra a varredura (os mais
leves não importam). Uma cena abaixo de ~19 fps reprova o preset na hora.

Regra de "passa" por cena, com orçamento T = 1000/alvo ms:
média ≤ 1,05·T, p95 apresentado ≤ tolerância·T, p95 de CPU ≤ folga·T.

| Objetivo    | tolerância p95 | folga CPU |
|-------------|----------------|-----------|
| Performance | 1,10           | 0,60      |
| Balanced    | 1,25           | 0,80      |
| Quality     | 1,50           | 0,95      |

Escolhido = o preset mais pesado cujas 3 cenas passam. Nada passa: Low, e a tela
avisa ("can't hold 30 fps even on Low"). O alvo inicial é 60 no desktop; no
celular 60 só se Balanced chega a 60 acima do Low. Recomendado: Balanced, ou
Performance quando nem Balanced segura o alvo. O MSAA nunca sobe (é fixado ao
abrir a janela, então foi o medido); o limite de fps vira o alvo escolhido.

## Guarda em jogo

`benchmark::Guarda`: janelas de 1 s; se o fps fica abaixo de 70% do alvo (limite
de fps, ou 60 sem limite) por mais de 20 s seguidos, aparece um cartão
não-modal "The game is running slowly. Lower the graphics one step?"
(Lower / Not now). Not now cala por 10 min. Quadro > 0,5 s (carregamento, app
voltando do fundo) reinicia a janela; carregando ou em economia não conta.
**Nunca aparece no nível mais baixo** (`config_graficos::ha_nivel_abaixo`): sem
nada mais leve, não há o que oferecer.

## Prévia

`MMO_PREVIA_BENCHMARK=1` (debug, desktop) roda o teste DE VERDADE na máquina sem
aplicar nem gravar nada, imprime as medidas e salva em `target/benchmark_shots/`
(ou `MMO_PREVIA_SAIDA`): progresso de cada cena, escolha com o medido, um
celular bom e um fraco inventados, e o cartão da guarda.

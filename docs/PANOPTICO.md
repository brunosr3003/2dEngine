# Panóptico — observabilidade do jogo

Um painel que mostra o jogo inteiro de cima: o mapa ao vivo de cada canal, e
tudo que os bancos e a telemetria sabem sobre jogadores, economia, mercado,
dungeons, missões e infraestrutura.

```
crates/panoptico               o painel (axum + uma página, sem framework)
  src/auth.rs                  senha, sessão em cookie, freio de tentativas
  src/observa.rs               jogadores, mercado, dungeons, atividade, missões, itens, infra
  src/economia.rs              previsão (shared::forja) × medição
crates/server/src/panoptico.rs o retrato ao vivo que cada canal publica
crates/server/src/telemetria.rs contadores, medidas e erros agregados por minuto
scripts/run-panoptico.sh       como subir local
```

## De onde vem cada coisa

| fonte | o que é | custo pro jogo |
|---|---|---|
| **retrato** (`/estado` do canal, 5×/s) | posição, vida, estado de cada jogador e mob; chefes de campo (vivo/renasce); dungeons abertas; fila e salas; buffs, mortes a recuperar, preferências e banda de cada conexão; build, uptime, últimos avisos | serializa a cada 200 ms fora do cadeado; só com `PANOPTICO_BIND` |
| **telemetria** (`telemetria`, por minuto) | contadores `(tipo, chave)` que o mundo soma no ponto em que a coisa acontece | uma soma num mapa com cadeado curto; uma task grava por minuto |
| **medidas** (`telemetria_medidas`, por minuto) | o valor de agora: online, mobs vivos, chefes vivos, instâncias, fila, ouro online, nós de coleta, p99 do tick | a cada 30 s |
| **erros** (`telemetria_erros`) | toda linha WARN/ERROR do log do canal | camada do `tracing` |
| **banco do realm** | contas, personagens, missões, dungeons (`dungeon_json`, `dungeon_contas`), itens, migrações, heartbeat de canais, saídas e cartas do mercado | consultas do painel, nunca do jogo |
| **banco central** | anúncios, vendas, cartas, operações, razão da TP | idem |

Retenção: telemetria e medidas 30 dias, erros 7 dias (limpeza de hora em hora
pela própria task). Gravação que falha devolve os contadores pro mapa: um
minuto de banco fora não apaga o minuto.

### Contadores de telemetria

| tipo | chave | onde |
|---|---|---|
| `kill`, `chefe_morto`, `kill_dungeon`, `chefe_dungeon_morto` | nome do bicho | morte de inimigo |
| `drop` | item | saque spawnado |
| `chave_drop` | `chefe:item` / `dungeon:item` | chave de craft que caiu |
| `morte_jogador`, `xp_perdido` | — | `Session::morrer` |
| `recuperar_xp` (grátis/pago), `xp_recuperado` | — | recuperar XP |
| `ouro_fonte` | saque, missão, venda npc, troca npc, baú do tesouro, mercado carta | toda entrada de ouro |
| `ouro_ralo` | loja, troca npc, recuperar xp, mercado compra, dungeon entrada | toda saída de ouro |
| `skill_pedida` (id), `skill_recusada` (motivo) | | habilidades |
| `item_usado` | item | usar item |
| `coleta` | `Rock:T2` etc. | nó coletado |
| `coleta_item` | item | o que a coleta deu |
| `craft` (receita), `craft_falha` (motivo), `selo_craftado` | | craft |
| `refino` | `subiu:+7`, `destruiu:+8`, `sem_material` | forja |
| `loja_compra`, `loja_venda` | item | lojas de NPC |
| `mercado` | anunciar, comprar, carta aplicada | lado do canal |
| `missao_entregue`, `missao_historia`, `missao_abandonada` | id | missões |
| `dungeon_entrada` | `conteudo:estagio:normal|ajudante` | início da instância |
| `dungeon_resultado` | `conteudo:estagio:vitoria|tempo_esgotado` | fim |
| `dungeon_wipe`, `dungeon_tempo_s`, `dungeon_bonus_tempo`, `dungeon_bau` | `conteudo:estagio` | |
| `dungeon_bau_item` | item | o que o baú deu |
| `dungeon_entrada_comprada`, `selo_usado` | | |
| `save` (ok/falha), `save_ms`, `save_linhas`, `save_mercado`, `save_atrasado_mercado` | | writer de persistência |
| `conexao` (aberta/fechada), `banda` (enviados/recebidos bytes) | | sessão WebSocket |
| `log` (warn/error) | | camada de log |

Não medido ainda: latência por jogador (o servidor não sabe o RTT — o ping é
do cliente) e o uso do "Onde obter"/"Ir" (é só cliente; medir exige mensagem
nova no protocolo).

## As abas

* **mundo** — canais à esquerda com carga de tick; mapa arrastável com zoom no
  ponteiro (`F` enquadra); jogador azul (roxo dentro de dungeon), mob vermelho,
  chefe amarelo, chefe de campo morto é um × com o tempo pra voltar. Anel
  vermelho em quem está abaixo de 35% de vida. Sem ninguém selecionado, a
  coluna da direita mostra o canal: build, uptime, chefes, dungeons abertas,
  fila e últimos avisos.
* **jogadores** — contas (Google, novas), online, ativos 24 h/7 d, por zona, e
  a lista de personagens: nível, ouro, visto, capítulo da história, dungeon
  (vitórias, estágio, entradas, correio), buffs, mortes a recuperar, auto.
* **economia** — fluxo de ouro (fontes × ralos por hora, saldo), estoque de
  chaves e itens especiais, chaves que caíram por origem, materiais por cor,
  peças por raridade, e o que já existia: moeda, progressão, curva de demanda,
  escadas, curva de loot, drops medidos, itens no mundo.
* **mercado** — anúncios ativos (item e TP), vendas e taxa queimada, preço
  médio/mediana por item, vendas por hora, últimas vendas, entregas
  pendentes, operações e recusadas, razão da TP (saldos, motivos, movimentos),
  e o lado do realm (saídas pendentes, cartas aplicadas).
* **dungeons** — ao vivo (instâncias, fila, salas) e na janela: por conteúdo e
  estágio, entradas, ajudantes, vitórias, tempo esgotado, taxa, duração média,
  wipes, bônus, baús, vitórias de sempre, personagens liberados; o que os baús
  deram; correio pendente; Selos e primeiras vitórias da semana.
* **atividade** — todos os contadores agrupados (combate, habilidades, ouro,
  drops, coleta, craft e forja, lojas, mercado, missões, dungeons, rede e
  saves), com nomes resolvidos e gráfico por hora. Janela de 1 h a 30 dias.
* **missões** — onde a história de cada um parou, por capítulo; por missão,
  ativas, prontas, entregues e abandonadas.
* **infra** — heartbeat dos canais, processos ao vivo (build, uptime, avisos),
  medidas das últimas 24 h, saves e rede, erros recentes, migrações aplicadas,
  tamanho dos bancos.

## Segurança

Ele vê conta, ouro e posição de todo mundo.

* **Sessão obrigatória.** Nada passa sem login, exceto a própria tela de
  login. API sem sessão responde 401; página redireciona.
* **Senha** em `PANOPTICO_SENHA` (8+ caracteres; o processo não sobe com
  menos), comparada em tempo constante.
* **Cookie** aleatório de 256 bits, `HttpOnly`, `SameSite=Strict`, `Secure`
  (desligável com `PANOPTICO_COOKIE_SEGURO=0` só pra teste local em http), no
  caminho do prefixo, 12 h. Nada de token na URL.
* **Freio**: 5 senhas erradas em 15 min travam o IP (429). Atrás de proxy o IP
  vem de `X-Real-IP`/`X-Forwarded-For` só com `PANOPTICO_CONFIAR_PROXY=1`.
* **Cabeçalhos**: CSP só do próprio host, `X-Frame-Options: DENY`,
  `nosniff`, `no-referrer`, `no-store` nas respostas.
* **Só leitura.** Não existe rota que escreva no jogo.
* **Escuta em loopback.** Acesso remoto é pelo nginx com HTTPS (ou túnel SSH).
* O `/estado` do canal continua exigindo `MMO_ADMIN_TOKEN` e escuta só em
  127.0.0.1 (`PANOPTICO_BIND`).

## Variáveis

| variável | painel | exemplo |
|---|---|---|
| `PANOPTICO_SENHA` | obrigatória (8+) | — |
| `PANOPTICO_WEB_BIND` | onde escuta | `127.0.0.1:18095` |
| `PANOPTICO_PREFIXO` | caminho atrás do proxy | `/panoptico` |
| `PANOPTICO_COOKIE_SEGURO` | `0` só em http local | (omitir em produção) |
| `PANOPTICO_CONFIAR_PROXY` | `1` atrás do nginx | `1` |
| `DATABASE_URL` | banco do realm | |
| `DATABASE_URL_CENTRAL` | banco central (opcional) | |
| `MMO_ADMIN_TOKEN` | token do `/estado` dos canais (opcional: sem ele o mapa fica mudo) | |
| `PANOPTICO_HOST_CANAIS` | host pra falar com os canais quando o anunciado é público | `127.0.0.1` |
| `PANOPTICO_OFFSET` | porta do painel do canal = porta do jogo + offset | `1000` |

No canal: `PANOPTICO_BIND=127.0.0.1:<porta do jogo + 1000>` e
`MMO_ADMIN_TOKEN` pro retrato ao vivo; `TELEMETRIA_FLUSH_S` (60) muda o passo
da gravação. A telemetria grava sempre, com ou sem `PANOPTICO_BIND`.

## Subir local

```bash
MMO_ZONA=ilha_inicial BIND_ADDR=0.0.0.0:9200 \
PANOPTICO_BIND=127.0.0.1:10200 MMO_ADMIN_TOKEN=<32 chars> \
  ./target/release/server

MMO_ADMIN_TOKEN=<o mesmo> PANOPTICO_SENHA=<8+ chars> ./scripts/run-panoptico.sh
```

## Um aviso que o painel já se deu

A primeira versão desenhava uma linha pra todo mob com `ai_target`, e a tela
mostrou 54 bichos "caçando" um jogador a 124 unidades com detecção de 9. A IA
estava certa: `ai_target` é só o cache do jogador mais próximo. Hoje o retrato
separa `alvo` (cache) de `perseguindo` (indo atrás de verdade), e só o segundo
vira linha. **Painel que mente é pior que painel nenhum.**

## Aba loja

Lê o banco central (`loja_pedidos`, `loja_posses`, `tp_razao`) e a
telemetria do realm: receita de pacotes de TP (simulada enquanto
`PAGAMENTO_SIMULADO` estiver ligado — não é dinheiro recebido), TP vendida e
TP gasta na loja, pedidos por tipo e status (30 dias), itens mais comprados,
pacotes vendidos, receita e itens por hora (48 h), últimos pedidos e
**montados agora** (medida `montados` que o canal grava junto do tick p99).
Contadores novos da telemetria: `loja_pedido`, `loja_receita_centavos`,
`loja_tp_vendida`, `loja_item`, `loja_tp_gasta`, `montaria`.

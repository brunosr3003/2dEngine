# Sessão de 20/09/2026 — protocolo 110

Registro das mudanças desta sessão, a partir do commit `10bd059`. O histórico
colado na conversa descreve trabalhos anteriores; não implica que tenham sido
refeitos nesta sessão.

## Combate e controles

- INT alocado acrescenta 1 de ataque por ponto quando o personagem usa arma
  mágica (atualmente o Anel Mágico). O ataque básico e as habilidades usam esse
  ataque. A distribuição continua concedendo 2 de mana por INT.
- A primeira implementação convertia toda a Sabedoria em ataque; a simulação
  encontrou desequilíbrio. A versão final converte os pontos alocados em INT.
- Dash ligado ao botão na posição antes reservada ao Pulo e à tecla Ctrl.
  O servidor já possuía a mecânica: custo de 30 de vigor, duração de 0,2 s,
  velocidade 14 e recarga base de 1,5 s, reduzida por SPD.
- Pulo movido para cima do Dash. Layout e áreas de toque ajustados.
- Ficha atualizada para explicar INT e SPD, incluindo a recarga do Dash.

## Pergaminhos e livros

- Novo Pergaminho de Invocação de Tomos (item 362), vendido por 150 TP.
- Cada abertura sorteia uma das 12 habilidades e um tomo: Verde 75%, Roxo 20%,
  Lendário 5%. O tomo vai ao estoque de evolução do personagem.
- Compra entrega o pergaminho na bolsa; abrir é uma ação separada.
- Chaves, montarias e tomos permitem abrir um pergaminho ou usar **Abrir 10+1**:
  são consumidos 10 pergaminhos da pilha selecionada e entregues 11 prêmios.
- O servidor valida quantidade e saldo; somente lotes de 1 ou 10 são aceitos.
- Montarias do lote são registradas em uma transação no banco central, com
  quantidade acumulada por montaria. Duplicatas continuam contando.
- Falha na transação devolve o lote pago; reembolso com bolsa cheia vai para
  Entregas, usando inserção de tudo ou nada para evitar devolução parcial.
- Chaves sem espaço também vão para Entregas.

## Interface e animação

- Pergaminhos redesenhados em vetores, com papel, dobras, hastes e lacre.
  Selos distinguem chaves, montarias e tomos na bolsa e na loja.
- Animação com brilho, partículas, abertura luminosa e revelação do prêmio.
- Abertura 10+1 revela uma grade de 11 prêmios; Guardar libera após a revelação.
- Resultado de tomo informa habilidade, grau e quantidade acumulada.
- Loja Materiais organizada com pergaminhos de chaves, tomos e pacotes de moeda.
  Títulos e posicionamento de botões ajustados após inspeção das prévias.

## Código, documentação e verificação

- Protocolo atualizado de 109 para 110; cliente e servidor devem ser publicados
  juntos. Novas mensagens para abertura em lote e resultados múltiplos.
- Cadastro persistente do novo pergaminho incluído na inicialização do servidor.
- Documentação atualizada em GAMEPLAY.md, LOJA.md e SKILLS.md.
- Testes do catálogo, distribuição de tomos, regra 10+1, INT e layout do HUD.
- Teste pelo despachante do servidor verifica 10 tomos consumidos, 11 entregues
  e recusa de nova abertura sem saldo.
- Suíte do workspace passou: cliente 320, servidor 143, shared 178 e demais
  alvos. Dois testes ficaram ignorados; testes condicionais de banco não
  equivalem a uma validação completa com PostgreSQL.
- Prévias locais inspecionadas: `/tmp/tempest-invocacao-preview` e
  `/tmp/tempest-loja-110`. Não houve teste desta versão em iPhone físico.

## Publicação

- Código publicado no Git: commit `8d46cb1`.
- Build release de servidor, API e painel concluído sem erro.
- Teste adicional pelo despachante (10 pagos/11 entregues e repetição sem saldo)
  passou após a suíte completa.
- Mac atualizado por fast-forward e build iOS executado pelo script do projeto.
- TestFlight: **Tempest 1.1 (2609201402)**, `UPLOAD SUCCEEDED with no errors`.
  Delivery UUID: `718ae1e0-3b6c-49ea-8267-ecf998a3d62f`.
  O upload foi confirmado; processamento e disponibilidade pela Apple não
  foram verificados no App Store Connect.
- Backups: `tempest-prod/dumps/tempest_sa01-pre-110-2609201402.dump` e
  `tempest-prod/dumps/tempest_central-pre-110-2609201402.dump`.
- Binários anteriores preservados em `tempest-prod/bak-110-2609201402/`.
- Instalados os binários server, supervisor, web e panoptico. Reiniciados
  `tempest-prod-campo`, `tempest-prod-geleira`, `tempest-prod-web` e
  `tempest-prod-panoptico` às 11:26 de 20/09/2026 (São Paulo).
- API pública confirmou protocolo 110 e anunciou Bosque (9000) e Geleira (9100).
  Os quatro serviços ficaram ativos; logs confirmaram os dois mundos iniciados.
- As portas públicas 9000 e 9100 responderam `101 Switching Protocols` à
  negociação WebSocket. A conexão aberta foi encerrada pelo limite de 3 s
  do teste. Hashes de server e web instalados conferem com o build release.
- Consulta somente leitura confirmou item 362, Pergaminho de Invocação: Tomos,
  com limite de pilha 99 no banco do realm.
- É necessário atualizar o cliente para o protocolo 110. Não foi feito reset
  de personagens nesta publicação.

## Correções posteriores — Dash (protocolo 111, ainda não publicado)

- Botão mostra setor de recarga, arco e segundos restantes. A contagem começa
  quando o servidor aceita o Dash e já inclui a redução por SPD.
- Dash é processado antes do bloqueio de comandos da skill: interrompe cast,
  recuperação do ataque, salto, defesa e reação de dano. Continua exigindo
  vigor e recarga disponível e não permite escapar do estado caído/carregado.
- Golpes e habilidades ainda pendentes são removidos. Skill cancelada antes
  do impacto devolve mana e libera sua recarga; efeito já aplicado não é desfeito.
- Pose baixa de impulso com braços para trás, pernas assimétricas e rastro
  curto. Estado de Dash replicado para os demais jogadores.
- Teste de mundo confirmou interrupção de skill antes do impacto, reembolso
  de mana e ausência de nova cobrança ao apertar Dash durante a recarga.
- Estas correções são posteriores ao TestFlight 2609201402 descrito acima.

## Ponto vermelho da Ficha

- Ponto de atributo sobrando passa a marcar a Ficha no MENU e o próprio botão
  MENU do HUD, no mesmo padrão já usado por Missões, Diárias e Presença.
- O selo vem do último retrato enviado pelo servidor: sem retrato não há selo,
  e ele some quando a distribuição zera os pontos.

## Publicação do protocolo 111

- Commit `6ce00ac` publicado no Git. Build release de server, supervisor, web
  e panóptico concluído sem erro.
- TestFlight: **Tempest 1.1 (2609201451)**, `UPLOAD SUCCEEDED with no errors`.
  Delivery UUID: `c309c92e-fa26-462a-ad0d-cc799d582b46`. O processamento pela
  Apple não foi verificado no App Store Connect.
- Backups: `tempest-prod/dumps/tempest_sa01-pre-111-2609201451.dump` e
  `tempest_central-pre-111-2609201451.dump`. Binários anteriores em
  `tempest-prod/bak-111-2609201451/`.
- Reiniciados `tempest-prod-web`, `tempest-prod-panoptico`, `tempest-prod-campo`
  e `tempest-prod-geleira` às 11:55 de 20/09/2026 (São Paulo). Havia uma sessão
  ativa no Bosque, derrubada pelo restart com autorização do usuário.
- API pública respondeu protocolo 111; portas 9000 e 9100 devolveram
  `101 Switching Protocols`. Os dois mundos subiram sem erro no log.

## Energia no ponto de atributo

- Alocar ponto de atributo passou a cobrar Energia do mesmo saldo da evolução
  de habilidades, com preço crescente: `10 + 5 × pontos já alocados`.
- Sem saldo o servidor recusa sem consumir o ponto livre e devolve o texto
  "faltam N de Energia para este ponto"; a Ficha desliga o `+` e mostra saldo
  e custo do próximo ponto no cabeçalho de ATRIBUTOS.
- O reset continua grátis e devolve só os pontos — a escada de Energia começa
  do zero e é paga de novo.
- Não houve mudança de mensagens: o protocolo continua 111.
- Prévia local conferida em `/tmp/tempest-ficha-111/ficha.png`.

## Publicação da Energia no atributo

- Commit `a83086f` publicado no Git. Suíte completa antes do build: cliente
  322, servidor 146, shared 179, nenhum falho.
- TestFlight: **Tempest 1.1 (2609201503)**, `UPLOAD SUCCEEDED with no errors`.
  Delivery UUID: `77b1bcc5-c3db-4e76-a4d0-e4a743f663c1`. O processamento pela
  Apple não foi verificado no App Store Connect.
- Backups: `tempest-prod/dumps/tempest_sa01-pre-energia-2609201503.dump` e
  `tempest_central-pre-energia-2609201503.dump`. Binários anteriores em
  `tempest-prod/bak-energia-2609201503/`.
- Reiniciados os quatro serviços às 12:05 de 20/09/2026 (São Paulo), sem
  ninguém conectado. API pública respondeu protocolo 111 e as portas 9000 e
  9100 devolveram `101 Switching Protocols`.

## Energia na loja de cash

- Três pacotes de Energia na aba Materiais da loja de TP: Fagulha (2.000 por
  40 TP), Cristal (12.000 por 200 TP) e Núcleo (70.000 por 1.000 TP). O pacote
  maior rende mais Energia por TP e o cartão mostra o TP por mil.
- `Produto::Energia` é repetível: não vira posse da conta, e o servidor entrega
  no saldo de evolução do personagem logado, nunca na bolsa.
- A aba Materiais passou de três para quatro colunas. O cartão de lista foi
  refeito — o COMPRAR desceu para a base porque, na coluna estreita, ele cobria
  o preço (isso já acontecia com a quantidade das moedas antes da mudança).
- Prévias conferidas em `/tmp/tempest-loja-energia4/`.
- Nenhuma mensagem nova: o protocolo continua 111.

## Publicação da Energia na loja

- Commit `0d3b59e` publicado no Git. Suíte completa antes do build: cliente
  322, servidor 146, shared 180, nenhum falho.
- TestFlight: **Tempest 1.1 (2609201519)**, `UPLOAD SUCCEEDED with no errors`.
  Delivery UUID: `091bcd90-2aa0-4be1-b93b-9cede78f02c0`. O processamento pela
  Apple não foi verificado no App Store Connect.
- Backups: `tempest-prod/dumps/*-pre-loja-energia-2609201519.dump`; binários
  anteriores em `tempest-prod/bak-loja-energia-2609201519/`.
- Reiniciados os quatro serviços às 12:21 de 20/09/2026 (São Paulo), sem
  ninguém conectado. API pública respondeu protocolo 111 e as portas 9000 e
  9100 devolveram `101 Switching Protocols`.
- A compra de Energia em si não foi exercitada contra o banco central nesta
  publicação; o que rodou foram os testes de catálogo e de regra de compra.

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

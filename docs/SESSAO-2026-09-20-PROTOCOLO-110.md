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

Em andamento. Os resultados do build iOS, envio ao TestFlight, backup e
reinício da produção serão registrados abaixo após confirmação.

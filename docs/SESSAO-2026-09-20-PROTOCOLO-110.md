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

## Pets coletores (protocolo 112)

- Sistema novo em `docs/PETS.md`: o saque continua caindo no chão e o pet vai
  buscá-lo fisicamente, creditando no dono ao encostar.
- O pet é **item de bolsa**, não posse de conta — é isso que o torna
  negociável no mercado e combinável na aba Combinar do Craft sem sistema
  novo. O grau mora no `item_id` (cinco ids por espécie), a convenção dos
  materiais coloridos.
- Cinco espécies, cinco graus. Velocidade 90%→150% de `PLAYER_SPEED`, busca de
  8 a 16 tiles, 5 a 17 pontos de atributo pela curva `items::tier_stat_mult`.
  Os pontos entram como ponto ALOCADO, então a afinidade da espécie combina
  com a classe sem regra nova.
- Quatro fontes: missão 509 da história (1 Lobinho Cinza), Pergaminho de
  Invocação: Pet (250 TP), recompensa diária vinculada nos dias 14 e 28, e
  combinar 3 do mesmo grau.
- Combinar é aposta, não conta fechada: 60/40/25/10% por degrau e falhar
  consome os três — a mesma regra da chave de craft.
- AOI: pet dos outros leva penalidade na ordenação e é o primeiro a cair
  quando o teto de 60 enche. O pet do dono nunca cai.
- Protocolo subiu para 112: `EntityKind::Pet`, `EntityTag::Pet`,
  `EquipSlot::Pet` e `Produto::PergaminhoPet` mudam o wire.
- A loja ganhou a aba **Moedas**: com o terceiro pergaminho, cinco colunas na
  aba Materiais espremiam os cartões e o preço sumia atrás do botão.
- Duas regras escritas foram alteradas de propósito, e estão anotadas em
  PETS.md: item de TP que muda atributo (MONTARIAS.md) e pergaminho vinculado
  na recompensa diária (CALENDARIO.md).
- Não implementado: dono e prazo do saque no chão. Com pet de 16 tiles, quem
  tem o grau melhor alcança o drop de quem matou. Também não foi medido o
  efeito do pet no AOI com o canal cheio — isso precisa de bots.

## Publicação dos pets

- Commit `00746f9` publicado no Git. Suíte antes do build: cliente 324,
  servidor 150, shared 186, nenhum falho.
- TestFlight: **Tempest 1.1 (2609201633)**, `UPLOAD SUCCEEDED with no errors`.
  Delivery UUID: `9eed157b-694f-4339-bb2d-bb4e1a7c3610`. O processamento pela
  Apple não foi verificado no App Store Connect.
- Backups: `tempest-prod/dumps/*-pre-pets-2609201633.dump`; binários anteriores
  em `tempest-prod/bak-pets-2609201633/`.
- Reiniciados os quatro serviços às 13:36 de 20/09/2026 (São Paulo), sem
  ninguém conectado. API pública respondeu protocolo 112 e as portas 9000 e
  9100 devolveram `101 Switching Protocols`.
- Consulta somente leitura confirmou os 25 pets cadastrados (ids 420–444, slot
  `pet`, pilha 1) e o Pergaminho de Invocação: Pet vinculado.
- **É necessário atualizar o cliente para o protocolo 112.** Nenhum pet foi
  exercitado num cliente de verdade nesta publicação: o que rodou foram os
  testes de mundo (nascer, andar, coletar, bolsa cheia) e as prévias da loja.

## Prioridade de quem matou (2 s)

- `LootTag` passou a guardar quem deu o golpe final, e `LOOT_PRIORIDADE_S`
  (2 s) reserva o saque pra ele e pro pet dele nesse tempo. Depois, é de quem
  chegar.
- Pickup por proximidade e pet perguntam pra mesma `LootTag::liberado_para`.
- Saque sem dono (coleta, item largado, morte de jogador) continua livre desde
  o primeiro quadro.
- Resolve o buraco anotado na publicação dos pets: pet de raio grande limpava
  o drop de quem matou o bicho.
- Três testes novos: a regra nas bordas da janela, o pet de quem não matou
  esperando, e o pet de quem matou pegando na hora. Nada disso foi exercitado
  com dois jogadores de verdade — falta teste com bots.

## Publicação da prioridade no saque

- Commit `76b1828` publicado no Git. Suíte antes do build: cliente 324,
  servidor 153, shared 186, nenhum falho.
- TestFlight: **Tempest 1.1 (2609201644)**, `UPLOAD SUCCEEDED with no errors`.
  Delivery UUID: `ee3ffc2b-6420-4ff9-ac8e-ab93159b1078`. O processamento pela
  Apple não foi verificado no App Store Connect.
- Backups: `tempest-prod/dumps/*-pre-prioridade-2609201644.dump`; binários
  anteriores em `tempest-prod/bak-prioridade-2609201644/`.
- Reiniciados os quatro serviços às 13:46 de 20/09/2026 (São Paulo), sem
  ninguém conectado. API pública respondeu protocolo 112 e as portas 9000 e
  9100 devolveram `101 Switching Protocols`.
- O protocolo continua 112: a janela de prioridade é regra de servidor, não
  mexe no wire.

## Nível, ração e skills de pet (protocolo 113)

- O pet ganhou **nível 1–30**, guardado em `ItemInstance::pet` — viaja com o
  item, então pet vendido no mercado leva o que o dono criou junto.
- Ele só recebe XP **alimentado**: a Ração de Pet dá 2 h e usos seguidos
  somam. Alimentado, entra 20% da XP que o dono ganha matando.
- O nível dobra o atributo que o pet dá, do 1 ao 30, e abre slot de skill no
  10, no 20 e no 30.
- Cinco skills passivas (raio, velocidade, duração da ração, XP do pet e
  pontos de atributo) e um Removedor, todos itens de loja **negociáveis**.
- Aba **Pets** no Menu → Personagem: modelo 3D girando, barra de XP, estado da
  fome com botão ALIMENTAR, atributos, Poder e os três slots.
- O ícone do pet na bolsa passou a ser o modelo 3D, tingido pela cor do grau.
  Sem palco (célula pequena, modelo carregando) cai numa silhueta vetorial.
- A loja ganhou a aba **Pets** (pergaminho + ração + removedor + 5 skills) e o
  pergaminho saiu de Materiais, que voltou a ter dois cartões.
- Protocolo 113: `ItemInstance` ganhou campo e `Produto::ItemDePet` é novo.
- Prévias conferidas em `/tmp/tempest-pets-ui3/pets.png` e
  `/tmp/tempest-loja-petsaba/`.
- **Não exercitado**: ninguém alimentou nem subiu um pet num cliente de
  verdade. O que rodou foram os testes de mundo e as prévias.

## Skills de regeneração e de atributo (protocolo 114)

- Seis skills novas de pet, uma por atributo (+4 de FOR, DES, INT, VIT, SPD ou
  RES), que somam DIRETO no stat escolhido, fora da afinidade da espécie.
- Duas de regeneração: Sopro Curativo (+1,5 de vida/s) e Fonte Interior
  (+2 de mana/s).
- O regen de mana era a constante `MP_REGEN_PER_SEC` no tick; virou o stat
  `PlayerStats::mp_regen`, com o valor antigo como padrão. Ficha salva no banco
  sem o campo lê o valor base. A Ficha do personagem passou a mostrar os dois
  regens.
- A aba Pets da loja virou grade 5x3 (treze consumíveis) com cartão compacto;
  o pergaminho voltou para Materiais, que ficou com três cartões.
- Protocolo 114: `PlayerStats` ganhou campo.
- Prévia conferida em `/tmp/tempest-loja-skills/2532x1170-5-pets.png`.

## Publicação das skills de pet

- Commit `9dc93d5` publicado no Git. Suíte antes do build: cliente 325,
  servidor 155, shared 191, nenhum falho.
- TestFlight: **Tempest 1.1 (2609201720)**, `UPLOAD SUCCEEDED with no errors`.
  Delivery UUID: `139b6f62-171b-494c-9de3-fdfb10098e12`. O processamento pela
  Apple não foi verificado no App Store Connect.
- Backups: `tempest-prod/dumps/*-pre-petskills-2609201720.dump`; binários
  anteriores em `tempest-prod/bak-petskills-2609201720/`.
- Reiniciados os quatro serviços às 14:22 de 20/09/2026 (São Paulo), sem
  ninguém conectado. API pública respondeu protocolo 114 e as portas 9000 e
  9100 devolveram `101 Switching Protocols`.
- Consulta somente leitura confirmou as 13 skills cadastradas (ids 445–457).
- **É necessário atualizar o cliente para o protocolo 114.**

## A borda da célula voltou a ter a cor do item

- `Peca::grau()` lia a cor SÓ da `ItemInstance`. Só equipamento rolado tem
  instância, então material colorido, chave e pet — que guardam a cor no
  próprio `item_id` — apareciam todos com borda cinza. Na prática, quase toda
  a bolsa ficava sem cor.
- `item_id::cor_de_id` é a fonte nova: cobre os doze materiais coloridos (as
  quatro cores contíguas), as chaves (incluindo a lendária, que ficou fora da
  faixa) e os pets. A instância continua valendo para equipamento rolado.
- Teste novo cobre as três famílias, as bordas da faixa e o equipamento.
- O protocolo não muda: é conta de cliente.
- Cheguei a escrever uma prévia da bolsa para conferir na tela, mas no
  offscreen ela pega roda do mouse fantasma e rola sozinha, mostrando a grade
  fora da posição. Preferi removê-la a deixar um harness que mente sobre o que
  está na bolsa; a confirmação visual que serviu foi a das células de pet, com
  as cinco cores distintas.

## Publicação da correção da borda

- Commit `8baa486`. TestFlight: **Tempest 1.1 (2609201736)**,
  `UPLOAD SUCCEEDED with no errors`. Delivery UUID:
  `c8bea7a0-576f-4c04-ab93-a5c677bd81af`.
- Prod reiniciada às 14:39 de 20/09/2026, sem ninguém conectado, só para o
  binário ficar em sincronia com o commit: a correção é de cliente e o
  servidor não mudou de comportamento. Protocolo continua 114.
- Backups em `tempest-prod/dumps/*-pre-borda-2609201736.dump`.

## Montaria virou item, e a skin saiu (protocolo 115)

- A montaria seguiu o caminho do pet: **item de bolsa**, cor no `item_id` (3
  espécies × 5 cores), slot `EquipSlot::Montaria`, negociável no mercado e
  combinável na aba Combinar (3 do mesmo grau, 60/40/25/10%).
- **Sem nível**, por decisão: o pet trabalha, a montaria só anda.
- A **cor dá velocidade** (150% no cinza até 190% no laranja) e a **espécie dá
  atributo** (SPD/DES no lobo, FOR/DES no tigre, VIT/RES no urso). O cinza vale
  o que a montaria única valia antes, então ninguém ficou mais lento.
- **O sistema de skins acabou**: a tinta do grau é a variação. Saíram
  `loja::Skin`, `SKINS`, `loja::Montaria`, `MONTARIAS`, `Produto::Montaria`,
  `Produto::Skin`, `Posses` inteiro e as abas Montarias e Skins da loja.
  A loja ficou com quatro abas e quatro pergaminhos em Materiais.
- Sumiu junto todo o caminho de transação central da invocação de montaria: o
  pergaminho agora sorteia e entrega na bolsa, como o de pet.
- `montarias_ui` foi reescrita no molde de `pets_ui`: modelo 3D da equipada na
  cor dela, velocidade, atributos, Poder e o botão MONTAR.
- **Migração `montarias_viraram_item_v1`**: apaga as posses de montaria e skin
  e devolve em TP tudo que foi comprado nelas. Roda uma vez, marcada no
  livro-caixa. A decisão foi começar do zero, não converter.
- Protocolo 115: `Produto`, `EstadoLoja`, `PremioInvocacao`, `EquipSlot` e
  `Equipment` mudaram.
- Duas regras escritas caíram de propósito, anotadas em MONTARIAS.md: item de
  TP que muda atributo, e "toda montaria corre igual".
- Prévias conferidas em `/tmp/tempest-loja-mont/`.
- **Não exercitado**: ninguém equipou nem montou num cliente de verdade, e a
  migração não rodou contra a prod ainda — ela roda no próximo start.

## Publicação da montaria como item

- Commit `b8f2f59` publicado. TestFlight: **Tempest 1.1 (2609201812)**,
  `UPLOAD SUCCEEDED with no errors`. Delivery UUID:
  `78f5a196-fd1a-40ba-8c74-83282363f724`.
- **A primeira subida derrubou a prod**: a migração somava `valor` com
  `SUM(...)`, que no Postgres devolve NUMERIC, e o decode em `i64` falhava —
  o servidor entrou em crash-loop no start, às 15:13. Corrigido com
  `::BIGINT` no `SELECT` e republicado; o mundo subiu limpo às 15:17.
  Ninguém estava conectado.
- A migração rodou: `loja_posses` de montaria e skin zerada,
  `loja_montarias` zerada, marcador `montarias_viraram_item_v1` gravado e
  **3.350 TP devolvidos** à conta `SA01:1` (saldo foi de 100 para 3.450).
- Consulta somente leitura confirmou as 15 montarias cadastradas (ids
  460–474, slot `montaria`).
- Backups: `tempest-prod/dumps/*-pre-reset-montaria.dump`; binários anteriores
  em `tempest-prod/bak-montaria-item/`.

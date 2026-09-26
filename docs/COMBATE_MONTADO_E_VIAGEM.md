# Combate montado e fim do Pergaminho de Teleporte — implementação

## Objetivo

Fazer a travessia do mundo ter valor sem transformar cada encontro com um mob
em uma parada para desmontar. O pergaminho que salta até um ponto marcado na
mesma ilha sai de circulação. O Capitão do Porto continua levando gratuitamente
entre ilhas liberadas; portais de história, Ilha Mágica, dungeon, retorno após
morte e saídas de emergência continuam com suas funções próprias.

## Regras para o jogador

- Montado, o personagem pode usar o ataque básico contra mobs comuns. Dano,
  alcance, cadência, custo de munição e ganho de proficiência são iguais aos
  do ataque a pé. A montaria não adiciona outro multiplicador de dano.
- Skills ofensivas, defesa ativa e coleta pedem desmontar. Ao tocar numa skill
  montado, o cliente solicita desmontar e a usa assim que o servidor confirma;
  não deve parecer que o botão falhou. Skills de movimento não disparam montado.
- Atacar, ter alvo hostil ou estar em combate recente mantém a velocidade
  normal da montaria, incluindo os bônus de grau, tier e refino. Sprint
  continua disponível com o consumo normal de stamina.
- Tomar dano não desmonta nem inicia bloqueio de remontagem. O HP do jogador
  continua caindo normalmente.
- Chefes do mundo aberto e das dungeons podem ser enfrentados montado.
  O ataque básico mantém seu dano normal. Zonas seguras continuam proibindo ataque.
  PvP montado fica desativado nesta primeira versão.
- Montar leva 1 s mesmo durante combate. Iniciar coleta, cair, carregar
  outro jogador, perder a montaria equipada ou mudar de zona desmonta como hoje.
- A missão automática pode atacar mobs comuns montada e permanece montada ao
  terminar um alvo. Não deve entrar em ciclo de montar/desmontar. Ao precisar
  de skill, coleta ou NPC, desmonta antes da ação e prossegue.

O dano básico igual a pé é uma regra fixa.
Avaliar tempo para matar, dano recebido e
velocidade de progressão com equipamentos típicos dos níveis 20, 30 e 40.

## Viagem sem pergaminho

- Remover da loja do Alquimista o Pergaminho de Teleporte (item 359), o botão
  `Teleportar ×N` da faixa de viagem, e a aceitação de pedidos `Teleportar`
  pelo servidor. O marcador, a rota automática e `Ir` de missões permanecem.
- Na primeira entrada após a atualização, converter cada pergaminho da bolsa,
  banco, correio e mercado em 100 cobre, seu preço de compra, com operação
  idempotente e registro de migração. Anúncios de mercado devem ser cancelados
  e o item devolvido antes da conversão. Confirmar todas as fontes de inventário
  antes de executar a migração; nenhuma cópia deve ficar usável depois.
- O Capitão do Porto segue como viagem grátis entre ilhas já liberadas. A Ilha
  Mágica mantém entrada, saída e mudança de tier pelos NPCs próprios. O retorno
  por morte e o resgate de posição travada continuam; não são atalhos de farm.
- Mostrar distância estimada e destino na faixa de viagem. Se o caminho falhar,
  permitir cancelar e traçar de novo. Revisar rotas até portos, cabanas, NPCs
  e objetivos de missão para evitar destinos inacessíveis.

## Mudanças técnicas

1. **Servidor:** trocar a regra global `luta_desmonta` em `world/loja_mundo.rs`
   por eventos explícitos de desmontagem. Separar ataque básico, skill, dano,
   coleta e entrada em instância. O servidor decide se o alvo permite combate
   montado, aplica dano e velocidade de combate, consome firmeza e sincroniza
   o estado. Rejeitar ataque montado em PvP, chefes e zonas seguras.
2. **Cliente:** preservar alvo, input e auto ataque ao montar; exibir firmeza e
   aviso de queda no HUD. Ajustar pose do cavaleiro, arma, impacto, som e
   efeitos para os três corpos de montaria. Ao usar skill, esperar confirmação
   de desmontagem para disparar uma única vez.
3. **Auto missão/auto combate:** manter estado de montaria entre alvos comuns;
   proibir tentativas de montar durante luta e espera de 5 s após queda.
   Desmontar antes de tarefas manuais, coleta, chefe ou instância.
4. **Rede e persistência:** adicionar firmeza e bloqueio de remontagem ao
   estado autoritativo se o HUD precisar deles. Versionar protocolo de cliente
   e servidor juntos. Firmeza reinicia ao montar; bloqueio por queda não deve
   ser burlado por reconexão ou troca de zona.
5. **Economia:** remover venda, receitas e recompensas futuras do item 359;
   executar migração de saldo e mercado; tirar o item das fontes de busca.

## Ordem de entrega e validação

1. Instrumentar tempo de viagem e uso atual do pergaminho; mapear referências
   ao item 359 e os lugares onde atacar ou apanhar desmonta.
2. Implementar combate básico montado no servidor e cliente, com testes de
   alvo permitido, dano, velocidade, firmeza e reconexão. Testar manualmente
   as três montarias, auto combate, auto missão e iPhone/Android.
3. Revisar trajetos e estados de falha de navegação. Só então executar a
   migração e retirar o pergaminho, para a viagem ter uma alternativa funcional.
4. Lançar cliente e servidor na mesma janela. Medir tempo de deslocamento,
   mortes em trânsito e tempo para matar mobs de níveis 20, 30 e 40; ajustar
   os parâmetros iniciais sem mexer silenciosamente nos atributos dos mobs.

## Critérios de aceite

- Jogador montado mata mob comum sem desmontar e recebe dano normalmente.
- Atacar jogador, chefe ou zona segura montado não causa dano indevido.
- Skills e coleta desmontam e executam uma vez; auto missão não trava nem
  fica remontando entre dois mobs.
- Após cair, reconectar ou trocar de ilha não ignora o bloqueio de remontagem.
- Nenhuma forma do Pergaminho de Teleporte continua vendável/usável; todos os
  exemplares antigos são convertidos exatamente uma vez.
- Ir para missão, Capitão, Ilha Mágica, morte e resgate de posição continuam
  alcançáveis sem depender do pergaminho.

## Implementação de 26/09/2026

- Protocolo 141. Ataque básico usa o mesmo cálculo de dano, crítico, alcance
  e cadência a pé ou montado; teste de simulação compara o dano real das duas situações.
- O servidor desmonta antes de validar o impacto em chefe/PvP e antes de
  executar uma skill válida. O cast e a desmontagem são atômicos no servidor,
  evitando um segundo pedido ou uma skill perdida por espera do cliente.
- Área do chefe: raio de 12 m de um chefe vivo. Montar nessa área é recusado.
- Firmeza perde no mínimo 15 por golpe que tira HP; poise, parry e invulnerabilidade
  existentes continuam funcionando conforme suas regras, sem criar HP de montaria.
- O cooldown de queda usa UTC no estado persistente do personagem. O HUD mostra
  firmeza junto ao botão da montaria. As animações usam os braços e a arma do
  ataque básico sobre a pose sentada; sons e impactos são os da arma equipada.
- Migração `viagem_montada_v1`: converte bolsa, banco e Entregas em 100 cobre por
  pergaminho; o mercado cancela ofertas e devolve cobre. Entregas antigas por outros
  caminhos também convertem no ingresso do inventário. Presente de atualização:
  1 Pergaminho de Invocação: Montaria nas Entregas de cada personagem existente.
- A missão 794, “O mapa mostra o caminho”, entrega 1 pergaminho de montaria.
- Calendário: dias 2/9/16/23 dão mais 1 pergaminho de pet; 4/11/18/25 dão mais
  1 de montaria. Dias 7/14/21/28 dão 10 de cada. Progresso e resgates anteriores
  são preservados; prêmios já resgatados não são distribuídos de novo.

## Validação e publicação

- 283 testes de shared, 222 testes do servidor e 3 testes da janela de presença
  passaram. O teste de migração em PostgreSQL isolado passou com duas execuções.
- Produção: protocolo 141, sete canais e API ativos, sem reinícios automáticos.
- Banco conferido: item 359 inativo, zero cópias em bolsa/banco e zero anúncios
  ativos ou cartas desse item no mercado central. `filep` recebeu uma entrega
  com Pergaminho de Invocação: Montaria.
- iPhone: versão 1.1 (2609261601) instalada por Wi-Fi.
- Android: APK assinado, cópia verificada por SHA-256 no Mac em
  `~/Downloads/Tempest-Android-20260926-montaria.apk`.
- Resta observação de uso real para ajustar velocidade/firmeza e avaliar a
  animação nas diferentes montarias. O dano básico igual a pé permanece fixo.

## Controle de sprint na HUD

Botão ao lado da montaria: um toque liga, outro desliga, com indicação
`LIGADO`/`SPRINT`. Shift usa a mesma alternância no desktop. A preferência
manual prevalece sobre a corrida automática das rotas; desligar não é
anulado pelo próximo quadro de auto missão. Antes da primeira escolha,
a corrida automática mantém seu comportamento anterior. O servidor segue
validando stamina e impedimentos de movimento. Funciona a pé e montado.

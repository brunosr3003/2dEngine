# Social — grupos, amigos, correio e clãs

Os quatro botões em Menu → Social abrem um painel com abas. Interface por
clique/toque, campos ligados ao teclado virtual, listas paginadas, notificações
no menu e confirmação das ações destrutivas.

## Grupo

- Até 5 personagens, no mesmo canal e instância ao aceitar o convite.
- Qualquer membro pode convidar pelo nome ou pela lista de amigos/clã.
- Aceitar, recusar, sair; convite expira em 60 segundos. A entrada reconfere
  as vagas e não permite trocar de grupo silenciosamente.
- Grupo temporário: sair do canal ou desconectar remove o membro; com apenas
  um jogador restante o grupo dissolve. Grupos não atravessam canais.
- Mantém a partilha existente de XP em raio de 25 unidades: bônus total de
  20%, dividido entre os participantes próximos, na mesma instância.

## Amigos

Pedido por nome (busca sem distinguir maiúsculas), aceitar/recusar, cancelar
pedido enviado, remover amizade bilateral, convidar para grupo e escrever
carta. Limite de 100 relações por personagem, incluindo pedidos pendentes.
A presença é explicitamente **neste canal / fora deste canal**; não declara
que alguém em outro canal esteja offline. Amizades funcionam entre canais do
mesmo realm e persistem ao reconectar/reiniciar.

## Correio

Cartas de texto entre personagens do mesmo realm, incluindo destinatários
offline. Assunto até 60 caracteres, mensagem até 1000, caixa com até 100
cartas, intervalo mínimo de 10 segundos entre envios por remetente. Ler,
responder e apagar; leitura e exclusão exigem ser o destinatário. Cartas
longas têm paginação. O rascunho só é limpo após confirmação de envio.

**Recompensas** abre diretamente a aba de entregas já existente do
Mercado, que também contém as recompensas de dungeons. Cartas pessoais não
transportam itens ou moedas; as regras de resgate existentes são preservadas.

## Clã

Criação gratuita, nome único sem distinguir maiúsculas (3–24 caracteres),
até 50 membros. O líder convida, edita aviso (200 caracteres), expulsa,
transfere liderança ou dissolve. Os convidados aceitam/recusam; membros podem
sair. O líder deve transferir ou dissolver antes de sair. Cada personagem só
pode pertencer a um clã; aceitar limpa os demais convites. Limites e cargos
são reconferidos em transação, inclusive em pedidos simultâneos de canais
diferentes. Não há banco, moedas, nível ou guerra de clãs nesta implementação.

## Servidor e compatibilidade

`shared::social` define os pedidos e respostas. As variantes `Social` são
**anexadas** ao final dos enums postcard, e o protocolo atual é **105**,
conforme a regra do projeto. Atualizar servidor, web (`/api/version`) e
cliente juntos. Clientes 103/104 devem ser atualizados antes de voltar a jogar.


`server::social::init` cria sete tabelas `social_*` no banco do realm, de
forma idempotente. Não modifica os saves/inventários. A persistência roda em
tasks fora do tick, com uma operação em voo por sessão, limite de frequência,
SQL parametrizado e respostas vinculadas ao personagem que iniciou a ação.
As mutações usam um advisory lock transacional do subsistema para proteger
limites e liderança entre processos. Leituras usam snapshot consistente.
Para escalar volumes altos de operações sociais, substituir esse lock global
por locks ordenados por relação/clã, preservando as mesmas invariantes.

A interface consulta notificações a cada 20 segundos fechada e 5 segundos
aberta, além das atualizações após ações. Grupo é atualizado imediatamente.

## Validação

```sh
cargo test -p shared social
cargo test -p server --bin server social
# Banco PostgreSQL: schema temporário separado dos personagens reais.
TEST_DATABASE_URL=... cargo test -p server --bin server social_postgres -- --ignored
cargo test -p client menu::tests
cargo check -p server -p client
bash scripts/build-android.sh
```

O teste PostgreSQL cobre amizade bilateral, pedido recíproco concorrente,
privacidade e limites de cartas, cargos, convites, transferência de liderança,
expulsão e dissolução. O teste de mundo cobre convite, limite de grupo,
expiração, isolamento de instâncias e dissolução ao sair.

Preview visual sem rede (build de desenvolvimento):
`MMO_PREVIA_SOCIAL=1 target/debug/client`. Capturas em
`/tmp/tempest-social-preview/`, com dados fictícios, sem mudar personagens.

## Correio administrativo — protocolo 105

Em **Menu → Correio → Envio admin/mod**, contas autorizadas podem enviar texto
com até 8 tipos de item, de 1 a 100.000 unidades por tipo. A busca usa o catálogo
do jogo; o servidor reconfere que cada ID existe e está ativo. Pacotes que não
caberiam nem numa bolsa vazia são recusados antes de enviar. Não há envio de
ouro por ID especial nem equipamento com atributos customizados nesta tela.

O destinatário pode ser um personagem (inclusive offline ou o próprio remetente)
ou **todos os personagens existentes no banco do realm no instante do envio**.
É por personagem, não por conta, e não inclui personagens criados posteriormente.
O total exibido antes de enviar é estimado; a resposta confirma o total real.
O envio exige confirmação com destino, assunto e anexos por destinatário.

- A tabela `social_staff` vincula o cargo `admin` ou `mod` à conta autenticada.
  Nome de personagem, campos do cliente e chamadas diretas não concedem permissão.
  Ambos os cargos podem fazer envios individuais e globais com itens.
- `social_campanhas` mantém autor, conta, cargo, conteúdo, anexos, instante e
  quantidade de destinatários. Repetir o mesmo identificador de envio não duplica
  cartas; reutilizá-lo com outro conteúdo é recusado.
- A caixa **Oficiais** é separada da caixa pessoal de 100 cartas. A caixa oficial
  mostra até 500 por vez, priorizando anexos pendentes; apagar cartas antigas
  permite acessar as próximas. Nenhuma campanha é descartada por caixa cheia.
- **Receber itens** é tudo ou nada: o servidor simula a inserção numa cópia da
  bolsa. Sem espaço para todos os anexos, nada é consumido ou parcialmente recebido.
- Uma conexão dedicada mantém um advisory lock por carta até o save concluir.
  Dois canais não podem resgatar o mesmo anexo simultaneamente; não há timeout de
  reserva que libere duplicação enquanto o banco está lento.
- O recibo e o inventário são gravados na mesma transação de persistência. Um token
  identifica a reserva e impede um save atrasado de confirmar outra reserva. O
  retorno “recebidos e salvos” só sai depois da confirmação no banco. Se o processo
  cair antes do commit, o lock é liberado e os anexos continuam disponíveis.
- Carta com anexos pendentes não pode ser apagada; a exclusão de oficial é lógica,
  preservando auditoria e recibos. Recebimento repetido é recusado.

### Conceder/revogar acesso da equipe

Ferramenta **local do operador**, sem endpoint aberto para promover contas:

```sh
# DATABASE_URL deve estar configurada no ambiente do servidor.
cargo run -p server --bin social_staff -- PERSONAGEM admin
cargo run -p server --bin social_staff -- PERSONAGEM mod
cargo run -p server --bin social_staff -- PERSONAGEM remover
```

A mudança vale para todos os personagens da conta. Remover o cargo bloqueia
novos envios imediatamente, mesmo com uma tela antiga aberta. O cliente atualiza
o botão na próxima consulta social (até 20 s fora do painel, 5 s dentro).

Servidor, API/web e APK desta extensão usam protocolo **105** e devem ser
atualizados em conjunto. As tabelas são aditivas. Os testes de Postgres usam schema
isolado e cobrem autorização, revogação, destinatário offline, broadcast,
idempotência concorrente, anexos inválidos, leitura/exclusão indevida, rollback
de recibo, bloqueio simultâneo e nova tentativa após bolsa cheia.

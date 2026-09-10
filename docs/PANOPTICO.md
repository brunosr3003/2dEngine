# Panóptico — observabilidade do mundo

Um painel que mostra o jogo inteiro de cima: onde está cada jogador, cada
bicho, o que cada um está fazendo, e tudo que o servidor sabe sobre eles.

```
crates/panoptico          o painel (axum + uma página, sem framework)
crates/server/panoptico.rs o retrato que cada canal publica
scripts/run-panoptico.sh  como subir
```

## Como o estado chega lá

O jogo roda um processo por canal, cada um simulando 30 vezes por segundo
dentro de 33 ms. Servir painel de dentro desse laço seria deixar uma aba de
navegador competir com o tick.

Então o caminho é o contrário: o tick **publica** um retrato já serializado
num `RwLock` a cada 200 ms, e quem atende a requisição só clona um ponteiro.
Painel lento, dez abas abertas ou cliente travado nunca viram jogo lento.

Cinco retratos por segundo e não trinta: o olho lê a cinco, e serializar mil
entidades trinta vezes por segundo seria trabalho jogado fora.

O painel busca todos os canais **em paralelo**. Em série, onze canais com um
lento no meio dariam um painel no ritmo do pior deles. Canal que não responde
aparece como **mudo**, não some — canal que some parece canal que fechou, e a
diferença importa pra quem opera.

## O mapa

Não existe imagem guardada em lugar nenhum. O panóptico gera a ilha do mesmo
`shared::terreno`, da mesma semente, e desenha o PNG (~2,5 MB, ~5 s, cacheado
por zona). Se o gerador mudar, o mapa muda junto — não há versão velha pra
desincronizar.

A cor é o material da superfície, mais duas coisas que só fazem sentido visto
de cima: sombra de encosta (relevo de cima não se vê por cor, se vê por luz) e
curva de nível a cada oito blocos, discreta.

## Segurança

Ele vê conta, ouro e posição de todo mundo.

* **Fechado por padrão.** Sem `PANOPTICO_BIND`, o processo de jogo não abre
  porta nenhuma a mais.
* **Token obrigatório.** Nem o canal nem o painel sobem com token de menos de
  16 caracteres.
* **Só leitura.** Não existe rota que escreva; o módulo não tem `&mut
  GameWorld` em lugar nenhum.
* **Nunca em endereço público.** `PANOPTICO_WEB_BIND` fica em `127.0.0.1` e
  acesso remoto é por túnel: `ssh -L 8090:127.0.0.1:8090 zone13`.

## Subir

```bash
# cada canal, com a porta do painel = porta do jogo + PANOPTICO_OFFSET (1000)
MMO_ZONA=ilha_inicial BIND_ADDR=0.0.0.0:9200 \
PANOPTICO_BIND=127.0.0.1:10200 MMO_ADMIN_TOKEN=<32 chars> \
  ./target/release/server

# o painel
MMO_ADMIN_TOKEN=<o mesmo> PANOPTICO_TOKEN=<outro> ./scripts/run-panoptico.sh
```

## A tela

Canais à esquerda com carga de tick, mapa arrastável com zoom no ponteiro
(`F` enquadra a ilha), inspeção à direita.

Jogador é azul com nome, mob é vermelho, chefe é amarelo, cadáver é cinza.
**Anel vermelho em quem está abaixo de 35% de vida**: é a informação que
decide se alguém precisa de atenção agora, e ela não pode exigir um clique.

Linha tracejada é rota de jogador; linha vermelha é mob perseguindo.

## A aba de economia

Duas colunas lado a lado de propósito: **o que o desenho prevê** e **o que o
banco mede**. Painel só com a previsão é a planilha de novo; painel só com a
medição não diz se o número é alto ou baixo.

O que é previsão sai de `shared::forja` — a mesma função que o jogo usa pra
refinar. Nada é recalculado com fórmula própria aqui: painel com a sua versão
da regra vira uma segunda regra, e as duas divergem.

| seção | o que é | de onde vem |
|---|---|---|
| moeda | ouro no mundo, mediana, p90, maior fortuna, ouro em mãos | `characters` + retratos dos canais |
| progressão | personagens por nível | `characters.xp` |
| escada do refino | peças, tentativas, darksteel e horas por nível e grau | `shared::forja::escada` |
| curva de loot | valor esperado por morte, por bicho | `loot_drops` × `items` |
| drops medidos | o que de fato caiu nos últimos 7 dias | `item_drops_log` |
| itens no mundo | estoque por item, mochila e baú somados | `inventory` + `vault` |

**Ouro em mãos contra ouro no mundo.** O primeiro é de quem está logado; o
segundo inclui quem está fora. A diferença é ouro parado — ele não circula,
mas volta a circular quando o dono voltar, e é o tamanho dessa volta que
importa saber antes que ela aconteça.

**A escada mostra onde o jogo muda de natureza.** Até o +5 a falha só come
material e a peça sempre chega: o custo é tempo. Do +6 em diante cada
tentativa arrisca a peça, e a coluna de *peças* descola — 1, depois 3,3,
depois 17, depois 111. É a mesma tabela que decidiu a colônia offline existir:
Raro +7 são 1,51 milhão de darksteel, ou **106 horas** de mineração ativa.

`DARKSTEEL_POR_HORA` mora em `shared::forja` e não só no `ECONOMIA.md` porque
o painel calcula tempo a partir dele — número de desenho que mora em dois
lugares vira dois desenhos diferentes.

## Um aviso que o painel já se deu

A primeira versão desenhava uma linha pra todo mob com `ai_target`, e a tela
mostrou 54 bichos "caçando" um jogador a 124 unidades com detecção de 9.

A IA estava certa: `ai_target` é só o cache do jogador mais próximo — a IA
guarda o mais perto pra não varrer todos os jogadores por tick, e a
perseguição é barrada por `detect_range`. Quem mentia era o painel.

Hoje ele separa `alvo` (cache) de `perseguindo` (indo atrás de verdade), e só
o segundo vira linha. **Painel que mente é pior que painel nenhum**, porque a
confiança que ele dá é real e a informação não.

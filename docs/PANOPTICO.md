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

## Um aviso que o painel já se deu

A primeira versão desenhava uma linha pra todo mob com `ai_target`, e a tela
mostrou 54 bichos "caçando" um jogador a 124 unidades com detecção de 9.

A IA estava certa: `ai_target` é só o cache do jogador mais próximo — a IA
guarda o mais perto pra não varrer todos os jogadores por tick, e a
perseguição é barrada por `detect_range`. Quem mentia era o painel.

Hoje ele separa `alvo` (cache) de `perseguindo` (indo atrás de verdade), e só
o segundo vira linha. **Painel que mente é pior que painel nenhum**, porque a
confiança que ele dá é real e a informação não.

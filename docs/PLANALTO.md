# Planalto da Tormenta — níveis 40–60

A quarta ilha continua usando `ilha_planalto`, a mesma semente e a rota liberada
pelo passo 752. O Último Abrigo fica a 180 unidades do porto. O terreno desenhado
sobre o relevo original contém cinco terraços conectados por estradas e um atalho
entre Mosteiro e Forja. Cliente, colisão, mapa e população consultam
`shared::planalto::Plano`. O cache de altura inclui a revisão do desenho.

| Região | Níveis | Conteúdo |
|---|---|---|
| Encostas dos Sentinelas | 40–44 | Primeira caça e ruínas de vigia |
| Mosteiro dos Ventos | 44–49 | Ruínas e acesso à dungeon 13 (entrada a partir de 40) |
| Vale do Trovão | 48–53 | Cristais de Energia e Owlbear Primevo (52) |
| Forja Partida | 52–57 | Mineração e acesso à dungeon 14 (entrada a partir de 50) |
| Olho da Tempestade | 57–60 | Farol e Arquimago da Tormenta (60) |

Os botões das duas dungeons no mapa abrem o conteúdo correspondente no sistema
existente de grupos e instâncias. As entradas continuam acessíveis pelo Menu.
A raridade de baús e chaves mantém a escada atual: Épica no 40 e Lendária no 50.

Os pontos 40–44 da história são os centros das regiões. Cinco novos passos
855–859 entram no capítulo IV sem renumerar passos existentes. O realinhamento
existente pelo id da missão mantém o progresso salvo. Contratos 860/861 voltam
dez minutos após a entrega; diárias 862/863 exigem a vitória na dungeon correta.
Não há requisito de grupo ou de vitória em chefe de campo para concluir a história.

## Tempestade

A cada meia hora UTC, durante os primeiros dez minutos, um campo de raio 32 fica
carregado. Alterna entre Energia no Vale e minério na Forja. O campo recebe até
oito inimigos extras, respeitando terreno plano e afastamento da estrada. Os
extras desaparecem quando o evento termina; as áreas normais continuam ativas.
O mapa e um círculo no chão mostram o campo ativo.

Uma coleta de nó dentro do campo rende 50% a mais (arredondado para cima nos
itens; Energia segue arredondamento do saldo). O servidor usa a posição do nó e
seu relógio, independentemente do relógio ou da seleção do cliente. O bônus não
altera probabilidades de drop nem recompensa de missões.

## Validação e operação

- Teste compartilhado verifica todas as estradas, seus degraus e faixas.
- Teste do servidor gera a ilha completa e verifica caça nas cinco regiões,
  recursos reais nos dois campos, vagas do evento e posição dos dois chefes.
- `MMO_PREVIA_PLANALTO=1 target/debug/client` exporta seis vistas e o mapa para
  `/tmp/tempest-planalto`; `MMO_PREVIA_SAIDA` permite outro diretório.
- Canal de produção: `tempest-prod-planalto`, porta 9102, observabilidade 10102,
  túnel 19102 na VPS e caminho público `/z/ilha_planalto`.
- Protocolo 156 impede que clientes com o relevo antigo entrem na nova ilha.

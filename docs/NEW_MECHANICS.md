# Proposta de Novas Mecânicas (Hardcore MMO)

Este documento guarda as ideias de Game Design elaboradas para estruturar a progressão, combate e sistema de morte do jogo, com foco em uma experiência hardcore baseada em risco e recompensa.

## 1. Sistema de Progressão "Classless" (Sem Classes)

A ideia central é não prender o jogador a uma classe. A evolução ocorre fluidamente com base no uso.

*   **Atributos Base Zerados:** Todos os jogadores começam exatamente iguais (ex: 10 pontos em Força, Destreza, Inteligência, etc.).
*   **Progressão Baseada no Uso (Proficiências):** 
    *   Bater com uma espada dá XP para "Habilidade de Espada".
    *   Usar magias de cura dá XP para "Magia de Cura".
    *   Usar magias de fogo dá XP para "Magia de Fogo".
*   **A Relação Nível Principal vs. Proficiências:**
    *   O jogador possui um Nível Principal, obtido ganhando XP geral no mundo.
    *   O Nível Principal dita a distribuição de Pontos de Atributo (Força, Inteligência, etc.) e **define o limite máximo (cap)** do nível que uma Proficiência específica pode atingir.
    *   *Balanceamento:* Um jogador pode ter nível 100 em Espada e 100 em Magia, mas como os pontos de Atributo globais são limitados, a Espada só dará um dano absurdo se ele tiver investido seus atributos em Força (deixando a magia fraca, e vice-versa).

## 2. Sistema de Morte Hardcore (Downed State & Execução)

A morte não deve ser instantânea, especialmente no PvE. Ela gera tensão social e espaço para escolhas morais no PvP.

*   **O Estado de Agonia ("Downed"):** Quando o HP chega a zero, o jogador **não morre**. Ele cai no chão, incapacitado.
*   **Rastejamento:** No chão, o jogador fica lento (ex: 20% da velocidade) e não pode usar habilidades, mas pode se esconder.
*   **A Cadeia Alimentar (Monstros vs. Jogadores):**
    *   **PvE:** Monstros **não executam** jogadores. Eles apenas derrubam o jogador. Um jogador derrubado por monstros pode esperar alguns segundos/minutos para levantar sozinho (com uns 5% de HP), se não for encontrado por ninguem.
    *   **PvP (A Execução):** O perigo real de cair para um monstro é que você vira uma presa fácil em uma área PvP. Apenas **outros jogadores** podem realizar uma "Execução" (um cast de alguns segundos no seu corpo) para efetivamente matá-lo.
*   **A Punição de Morte (Pós-Execução):**
    *   Se for executado, o jogador sofre consequências duras, estilo *Albion*: perda do inventário coletado (loot, ouro) e um dano massivo de durabilidade no equipamento vestido (que custará caro para reparar, ou poderá quebrar se chegar a zero).

## 3. Resgate e Sequestro (Mecânica de Carregar)

Como o jogador caído fica vulnerável, adiciona-se uma interação física entre os jogadores vivos e caídos.

*   **Interação "Carregar":** Um jogador pode colocar um personagem caído nas costas.
*   **Efeitos:** Quem carrega não pode atacar e sofre penalidade de movimento.
*   **Aplicações Práticas:**
    *   *Resgate:* Um aliado tira você do meio da briga de guilda ou do agro dos monstros e te carrega até uma zona segura para te reviver.
    *   *Sequestro:* Inimigos podem te derrubar, te jogar nos ombros e te levar para uma área restrita deles, pedindo resgate ou apenas para saquear num local seguro.

## 4. Sistema de Aura/Poise (O "Haki" do PvP)

Uma mecânica para premiar sobrevivência e vitórias em combates contra jogadores reais, gerando prestígio e resistência através do combate PvP.

*   **Ganho de XP Condicional (Risco Altíssimo):**
    *   Enfrentar jogadores reais ativa sua "Aura de Batalha" (Poise).
    *   Você só adquire essa XP para aumentar o nível da Aura se **vencer** a luta (incapacitar o inimigo).
    *   Se você **perder** (for derrubado), você sofre uma **perda enorme** da sua XP de Aura.
*   **Efeito em Combate:**
    *   Ter uma Aura alta funciona como *Poise/Armamento*. Jogadores com muita Aura dificilmente sofrem "stagger" (interrupção) de magias ou ataques de jogadores com Aura muito inferior. A tela pode até ter efeitos visuais quando eles entram em combate.
*   **Ecossistema:**
    *   Criará uma "pirâmide de prestígio". Veteranos no topo não vão querer lutar sem motivo banal, pelo medo de perder um Poise que demorou semanas para upar caso sejam encurralados por muitos jogadores ("zerg"). 
    *   Veteranos viram Raid Bosses ambulantes, pois quem conseguir derrubá-los possivelmente irá absorver uma fatia gigantesca dessa Aura perdida.

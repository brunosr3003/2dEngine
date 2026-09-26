# Som do Tempest — primeiro pacote

## Objetivo

Dar retorno claro às ações do jogador sem transformar o combate automático em
ruído constante. O primeiro pacote é de efeitos curtos; música, vozes e
ambientes completos ficam para uma etapa posterior.

## Inventário inicial

| Grupo | Sons | Quando tocar |
|---|---|---|
| Combate | golpe leve, golpe pesado, disparo, acerto, crítico, dano recebido, morte do inimigo | No evento confirmado pelo jogo; crítico substitui acerto comum |
| Habilidades | preparação, lançamento mágico, impacto mágico | Uma família genérica no início; habilidades especiais podem ganhar variação depois |
| Coleta e itens | picareta, machado, recurso obtido, item recolhido | Coleta confirmada e entrada efetiva na bolsa |
| Missões | progresso, objetivo pronto, recompensa recebida | Mesmos momentos dos avisos visuais; conclusão tem prioridade sobre progresso |
| Interface | clique, ação recusada | Só em botões importantes; não tocar em cada quadro ou hover |

**Meta:** cerca de 18 arquivos curtos. Dois ou três sons alternativos para
golpes e acertos frequentes evitam repetição sem exigir um arquivo por arma ou
por monstro.

## Regras de reprodução

- Sons do próprio jogador ficam em destaque. Outros jogadores e mobs ficam
  mais baixos e perdem volume com a distância.
- No máximo um som de impacto comum por pequena janela de tempo por fonte.
  Crítico, dano recebido e objetivo concluído têm prioridade.
- Não tocar som de ataque apenas porque o botão foi pressionado se o servidor
  recusou a ação. Evitar som duplicado quando o evento reaparecer na rede.
- Coleta automática usa som discreto: o golpe de ferramenta pode repetir, mas
  o ganho de recurso só toca quando o servidor confirmar.
- Volume separado para **efeitos** e **música**, com opção de silenciar. Os
  valores devem persistir entre sessões.
- O jogo deve continuar funcionando se um arquivo de som não carregar.

## Ordem de implementação

1. Reprodutor central no cliente e controles de volume. Integrar clique,
   recusa, missão pronta e recompensa; validar iOS antes de expandir.
2. Combate e habilidades: usar os eventos de cast/impacto e as mudanças de
   vida confirmadas. Testar com combate automático e vários mobs próximos.
3. Coleta e itens. Depois, considerar um ambiente leve por ilha e música para
   cidade, exploração e chefe, com transições suaves.

## Aceite do primeiro pacote

- O jogador distingue acerto comum, crítico, dano recebido e conclusão de
  missão sem olhar o texto.
- Dez mobs lutando perto não cobrem os sons do próprio personagem.
- Auto combate e auto coleta não produzem som contínuo ou repetido em excesso.
- O iPhone toca os efeitos sem atraso perceptível e respeita os volumes salvos.

## Estado atual

Implementado em `crates/client/src/sons.rs`: 18 efeitos prontos CC0,
embutidos no executável. Fontes e licenças em `assets/audio/README.md`.

- Combos confirmados escolhem corte, disparo ou magia pelo conjunto da arma.
- Acertos, críticos, dano e morte usam eventos/estados do servidor.
- Habilidades, ciclos de coleta, itens recebidos, missões e botões têm efeitos.
- Volume padrão 60%; botão **Som** no painel **Interface** alterna em passos
  de 20% até 100% e desligado. Preferência persistida no desktop e iOS.
- Até seis vozes, no máximo duas de terceiros, atenuação até 32 unidades,
  intervalo por efeito e descarte de impactos de snapshot repetido.
- Falha ao carregar um efeito não impede o restante do jogo.

Música e ambientes continuam previstos para uma etapa posterior. O controle
separado de música será acrescentado junto com as faixas. A persistência no
Android depende do caminho de preferências da plataforma, ainda indisponível.

## Revisão de combate (26/09/2026)

Cortes, acertos, crítico, disparo e magia substituídos por efeitos CC0 de
StarNinjas, rubberduck e Free Firearm Sound Library. Fontes e processamento
em `assets/audio/combat/README.md`. Menu e HUD preservados. Efeitos de
combate têm ganho menor; impacto mágico só acompanha habilidades do anel,
pois golpes físicos já recebem seu acerto confirmado pelo Snapshot.

## Mobs e identidade por skill (26/09/2026)

`MobAttackFx` agora dispara som: criaturas vocalizam, pistoleiros disparam,
magos conjuram e arqueiros usam passagem de flecha. Dano não letal e morte
confirmada têm vocalizações próprias. Mobs no combate do jogador recebem
prioridade, atenuação espacial e canais separados dos ataques do personagem.

Todas as 12 skills possuem dois arquivos exclusivos (cast/impact), associados
aos eventos confirmados e IDs do catálogo. Ver `assets/audio/skills/README.md`
e a receita de edição em `tools/audio/preparar_skills.py`. Este mapeamento
substitui o som genérico por conjunto e a regra anterior de impacto só mágico.

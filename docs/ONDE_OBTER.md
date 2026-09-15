# Onde obter

No MIR4, todo material pedido tem um atalho para "onde consigo isso". Aqui é
a **lupa** ao lado do item: abre um popup com de onde ele sai e um botão para
ir até lá. Nada abre por tecla; é só tocar na lupa.

Código: `crates/client/src/onde_obter.rs` (lógica pura + popup),
`crates/server/src/economy.rs` (`fontes_de_itens`),
`crates/server/src/world/chefes.rs` (`onde_obter_snapshot`, `itens_do_chefe`).

## Onde a lupa aparece

| Lugar | Em quê |
|---|---|
| Craft | cada ingrediente da receita selecionada |
| Forja | Darksteel e Cobre do próximo refino |
| Bolsa | cartão de qualquer item (menos ouro) |
| Diário de missões | objetivo de juntar/entregar item ainda não cumprido |
| Diárias | diária de juntar item |

Com o popup aberto, os painéis de baixo ficam parados (não desenham), para o
toque no popup não cair num botão deles. O X volta ao painel.

## De onde vêm as fontes

O servidor monta tudo das tabelas reais e manda no login e a cada recarga da
economia (`ServerMessage::ResourceSources`, protocolo 96). Só o que existe:

| Fonte | Tirada de | Mostra |
|---|---|---|
| **Coleta** | `farm_node_drops`: árvore (linha 1) e pedra. A tabela da pedra é por cor do **material**; a pedra de cada cor entrega as cores na proporção de `RENDIMENTO_DA_PEDRA`, então a fonte é a pedra, com a chance já multiplicada | tipo de nó, quantidade, chance por coleta |
| **Bicho** | `loot_drops` dos kinds comuns e de praia (sem chefes, sem equipamento) | nome, nível da zona, chance por morte |
| **Chefe do mundo** | `itens_do_chefe` (o mesmo que `loot_de_chefe` rola; teste garante) | nome, nível, chance |
| **Vendedor** | lojas que existem num NPC da vila (hoje a do Alquimista) | NPC, preço |
| **Craft** | receitas (`craft_recipes`) pelo item que sai | receita, nível mínimo |
| **Missão** | recompensas de `QUESTS` (item 1 e 2) | título, se é diária |
| **Dungeon/raid** | chaves de craft (Escama/Garra/Chifre/Couro) | "Em breve", com cadeado |
| **Mercado** | o cliente acrescenta para todo item **não vinculado** | abre o Mercado buscando o nome |

## O que o "Ir" faz

| Fonte | Ir |
|---|---|
| Coleta | vai à região mais perto daquele tipo na ilha e liga o **auto coleta só daquele tipo** (o mesmo "Ir" do mapa) |
| Bicho | vai à zona mais perto onde ele nasce com chance boa (≥ 15%) e liga o **auto combate** |
| Chefe | vai até o chefe e liga o auto combate. **Nível do chefe > seu nível + 5**: só chega (nada liga sozinho) e avisa |
| Vendedor | vai até o NPC e fala com ele (a loja abre) |
| Craft | abre o Craft na receita |
| Mercado | abre o Mercado na aba Comprar, buscando o item |
| Missão, dungeon/raid | sem Ir |

O Ir fecha os painéis, desliga o que brigaria pela rota (como o mapa) e diz no
chat "Indo: …"; a faixa do HUD mostra "INDO · … · N m". Fonte que não existe na
ilha atual aparece sem Ir ("Não há nesta ilha", "Em outra ilha").

Zona com nível mínimo acima do seu nível + 5 aparece em vermelho, depois das
opções do seu nível.

## Ordem

1. Coleta e bicho do seu nível, do mais perto.
2. Chefe do seu nível e vendedor.
3. Craft.
4. Bicho acima do nível, chefe muito acima.
5. Mercado.
6. Missão.
7. Dungeon/raid (em breve).

## Fora do escopo por enquanto

- Fonte em outra ilha não diz qual ilha (o cliente só conhece o mapa da ilha
  atual).
- Loja de facção e peixe da pesca não entram.
- Dungeon/raid entra quando existir.

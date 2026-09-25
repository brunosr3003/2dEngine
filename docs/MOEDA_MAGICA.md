# Moeda Mágica

Nas três zonas da Ilha Mágica, cada coleta concluída e cada peixe pescado
rendem 1 Moeda Mágica. Isso inclui árvores, pedras e Energia.
Um monstro abatido deixa 1 moeda
no chão; um mob Forte deixa 2 e um chefe deixa 10. Na Ilha Mágica I,
aproximadamente um mob em cada cinco pontos de nascimento é Forte: tem
25% mais vida e 15% mais dano que o valor base, aparece com `[FORTE]` no nome
e rende 50% mais XP. Os demais têm 85% da vida e 80% do dano base.
O saque segue as mesmas regras de propriedade
dos outros itens do monstro. A moeda fica na bolsa e empilha até 999.999.

O Mercador Mágico fica na ilhota de chegada, perto do Alquimista Errante.
Cada compra é um pacote; a interface mostra a quantidade recebida e o custo.

| Pacote | Moedas |
| --- | ---: |
| 1 Pó Cintilante | 12 |
| 1.000 Cobre | 200 |
| 200 Darksteel | 600 |
| 1 chave cinza (Escama, Garra, Chifre ou Couro) | 2.500 |
| 1 Pergaminho de Invocação: Chave | 12.000 |
| 1 Pergaminho de Invocação: Pet | 25.000 |
| 1 Pergaminho de Invocação: Montaria | 50.000 |
| 1 Passe da Ilha Mágica | 5.000 |

Calibração: no ritmo observado de cerca de 100 moedas por minuto, 100
moedas equivalem a 1 TP da loja cash para os itens que ela vende. Assim,
o pergaminho de pet (250 TP) custa cerca de 4 h 10 min de atividade na ilha,
e o de montaria (500 TP), cerca de 8 h 20 min. O Pó Cintilante mantém seu
preço de acesso de 12 moedas.

Os pacotes e preços moram em `shared::magica::TROCAS`; o servidor valida
saldo, quantidade e espaço na bolsa antes de debitar as moedas.

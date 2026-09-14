# Seleção e criação de personagem

Após o login, a lista mostra personagens da conta por nome, com nível,
arma equipada e prévia 3D. Clique num personagem ou use as setas para
selecionar; Enter ou **Entrar no mundo** envia `SelectCharacter`.
Listas longas podem ser roladas. **Novo personagem** fica disponível mesmo
quando a conta já tem personagens. Contas vazias abrem a criação diretamente.

A criação oferece as quatro armas anunciadas pelo servidor: espada e
escudo, katana, duas pistolas e anel mágico. Cada opção muda a arma e a
postura da prévia e mostra seu estilo, as três skills e os níveis 1/5/10.
O personagem pode ser girado arrastando com o mouse. A arma inicial não
impede trocar de conjunto durante o jogo.

O nome segue a regra atual do servidor: 2–24 caracteres ASCII, aceitando
letras, números e `_`; espaços nas extremidades são removidos no envio.
Também é possível escolher Peacemain ou Morganeers. A aparência usa o
modelo padrão do jogo; esta tela não oferece personalização cosmética.

**Criar personagem** envia `CreateCharacter` com nome, arma e facção. O
servidor já persiste a escolha e equipa a arma inicial. O cliente bloqueia
novos envios até a resposta. Erros aparecem na criação e preservam os
campos; o sucesso retorna à seleção com o novo personagem destacado.
O protocolo continua na versão 81.

Prévia local, sem login nem alterações no servidor:

```sh
MMO_PREVIA_PERSONAGENS=1 ./scripts/run-client.sh
```

F10 encerra a prévia. `MMO_PREVIA_EXPORTAR=1` exporta a seleção e as quatro
opções de criação para `/tmp/2dengine-personagem-0.png` até `-4.png`.

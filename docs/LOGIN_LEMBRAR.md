# Lembrar o login

Estado: implementado no cliente e no servidor de jogo. Não depende de nada
externo — funciona já.

O dono: *"tem que ter um lembrar do último login na hora de logar"*. Até aqui
a tela de login nascia **vazia**: o usuário só vinha de `MMO_USER`, que é
variável de ambiente e no celular não existe. Quem jogava pelo TestFlight
redigitava usuário e senha a cada abertura.

## O que fica guardado no aparelho

| dado | quando | por quê |
|---|---|---|
| nome de usuário | **sempre**, depois de um login que deu certo | é ele que faz o campo vir preenchido, e não é segredo |
| sessão (token) | só com **"lembrar de mim"** marcado | é o que dispensa digitar a senha |
| senha | **nunca** | ver abaixo |

**A senha não é guardada, e isso é decisão, não esquecimento.** Guardar senha
em claro no aparelho troca a comodidade de um pela exposição de todos os
lugares onde aquela senha se repete — e o jogador não escolheu isso ao marcar
uma caixinha. O token faz exatamente o mesmo serviço, com três vantagens: vence
sozinho (30 dias), vale só para este jogo, e dá para revogar no banco sem
obrigar ninguém a trocar de senha.

O arquivo é texto `chave=valor`, com permissão `0600` no unix:

- **iOS**: `$HOME/Documents/tempest.prefs` (a caixa de areia do app).
- **desktop**: `$XDG_DATA_HOME/tempest/prefs`, ou `~/.local/share/tempest/prefs`.
- **Android**: não há caminho garantido sem JNI, então **não guarda nada** —
  `caminho()` devolve `None` e todo o resto degrada em silêncio. Pendência.

## O fluxo

```
tela de login  --Login { usuario, senha, lembrar: true }-->  servidor
                                                              |
                          argon2 confere a senha  <-----------+
                                                              |
                                   emite_sessao (OsRng, 256b) |
                                   banco guarda só o SHA-256  |
                                                              v
cliente  <--CharacterList { ..., sessao: Some(token) }--  servidor
   |
   +-- grava usuário + token no arquivo
   |
   ... próxima abertura ...
   |
   +-- token no arquivo -> LoginToken { token }, e a tela de login nem aparece
```

A sessão reusa `login_tokens`, a **mesma** tabela e o **mesmo** caminho de
verificação (`auth::authenticate_token`) que o login com Google já usava. Não
há um segundo jeito de entrar: há um jeito, com duas maneiras de conseguir o
token.

### Detalhes que custaram para acertar

- **`OsRng`, não `fastrand`.** O resto do servidor sorteia com `fastrand`, que
  é um PRNG de jogo: rápido e semeado de forma previsível. Quem adivinha a
  semente adivinha a sequência — e aqui a sequência *é* a credencial.
- **Só no primeiro login.** Na troca de zona o cliente reconecta e reenvia
  usuário e senha; pedir sessão ali emitiria um token por zona visitada. O
  `ja_entrou` é o que separa os dois casos.
- **Quem entra por token não ganha outro.** Renovar a cada conexão encheria a
  tabela de linha morta.
- **Sessão vencida apaga o arquivo.** Sem isso a próxima abertura tentaria o
  mesmo token morto e cairia no mesmo erro para sempre.
- **Desmarcar apaga agora**, não no próximo login: quem desmarca está pedindo
  para esquecer já.
- **O auto-login exige `host`.** `conectar()` sai calado sem host e sem mexer
  na tela, e `tela_login` roda por quadro — sem a guarda, um token guardado com
  o host ainda indefinido vira laço infinito com a tela nunca aparecendo.

## Protocolo

`PROTOCOL_VERSION` 130 → 131. Postcard é posicional, então os dois campos
novos são o fim de cada struct e ambos têm `#[serde(default)]`:

- `ClientMessage::Login.lembrar: bool`
- `ServerMessage::CharacterList.sessao: Option<String>`

**O servidor tem que subir antes do cliente.**

## Pendências

- Android: sem lugar para escrever (precisa de JNI para o `filesDir` do app).
- Revogar sessão / "sair de todos os aparelhos" — hoje só vencem em 30 dias.
- HTTPS/WSS no cliente: o token trafega em claro no mesmo canal em que a senha
  já trafega hoje (pendência antiga, ver `LOGIN_GOOGLE.md`).

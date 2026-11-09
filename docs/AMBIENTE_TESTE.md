# Ambiente de TESTE (brunjilinux)

Servidor separado da produção, só para o dono e jogadores de teste, acessível
**só pela tailnet** (Tailscale/headscale). Pagamentos SIMULADOS (sem chave do
Google: comprar funciona, ninguém é cobrado). E-mail desligado; contas com
domínio `gmail.com`, `teste.local` ou `bot.local` já nascem confirmadas.

| O quê | Onde |
|---|---|
| Endereço (cliente, API, panóptico) | `http://100.64.0.23:8095` — panóptico em `/panoptico/` |
| Arquivos | `~/tempest-teste` no brunjilinux (`bin/`, `data/`, `static/`, `logs/`) |
| Banco | container `tempest-teste-pg` (postgres:16, `127.0.0.1:45432`, `max_connections=300`, volume `tempest-teste-pg`) |
| Senhas | `~/tempest-teste/.db-senha`, `tempest.env`, `panoptico.env` (chmod 600; senha do panóptico em `PANOPTICO_SENHA`) |
| Proxy | container `tempest-teste-caddy` (`~/tempest-teste/Caddyfile`, mesmas rotas do nginx de produção, só no IP da tailnet) |
| Serviços | `systemctl --user … 'tempest-teste-*'` (16, iguais aos de produção; gerados por `~/tempest-teste/gerar_teste.py` a partir das units de prod) |

## App de teste no iPhone

App separado **"Tempest TESTE"** (`com.brunji.tempest.teste`), ao lado do de
verdade. Perfil de desenvolvimento: `python3 scripts/ios-perfil-teste.py`
(cria o App ID e o perfil pela API do App Store Connect; rodar de novo quando
expirar ou entrar aparelho novo). Instalar:

```sh
APP_BUNDLE_ID=com.brunji.tempest.teste APP_NOME="Tempest TESTE" IOS_SAIDA=ios-teste \
  MMO_API_PADRAO=100.64.0.23:8095 scripts/build-ios.sh --iphone
```

O app de verdade continua sendo `scripts/build-ios.sh --iphone` SEM essas
variáveis (o `APP_BUNDLE_ID` passado na linha de comando vence o do
`~/MMORPG/.env`).

## Atualizar o servidor de teste com o código atual

No brunjilinux, no `~/2dEngine` (na branch a testar):

```sh
cargo build --release -p server --bin server --bin supervisor && cargo build --release -p web -p panoptico
systemctl --user stop 'tempest-teste-*'
cp target/release/{server,supervisor,web,panoptico} ~/tempest-teste/bin/
systemctl --user start tempest-teste-web tempest-teste-panoptico
systemctl --user start $(find ~/.config/systemd/user -maxdepth 1 -name 'tempest-teste-*.service' -printf '%f\n' | grep -v -e web -e panoptico)
```

Zerar os jogadores (mantém o catálogo): `bash ~/tempest-teste/zerar.sh`.
`static/releases.json` tem build 0 de propósito: o cliente de teste nunca é
mandado "atualizar na loja".

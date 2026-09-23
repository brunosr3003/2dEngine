# E-mail: confirmar conta e redefinir senha

Estado: implementado no `web`, no servidor de jogo e no cliente. **Desligado
sem `RESEND_API_KEY`** — sem ela o cadastro continua criando conta, só não
manda o link, igual ao login com Google antes das credenciais.

## Por que um serviço, e não SMTP na VPS

E-mail de servidor novo cai em spam por padrão. Quem entrega é reputação de IP
e assinatura (SPF/DKIM/DMARC), e é exatamente isso que um serviço transacional
já tem pronto. Um `sendmail` na VPS mandaria e-mail que ninguém recebe — o
pior dos casos, porque *parece* funcionar.

## A armadilha do domínio

`brunji.com.br` hoje declara, no DNS:

```
TXT  brunji.com.br   "v=spf1 -all"     ← nada pode enviar por este domínio
MX   brunji.com.br   0 .               ← este domínio não recebe e-mail
```

Os dois são declarações deliberadas de "aqui não tem e-mail", e o `-all` **faz
o destinatário recusar** qualquer mensagem enviada como `@brunji.com.br`.

Por isso o envio sai de um **subdomínio próprio**, `mail.brunji.com.br`: ele
ganha SPF e DKIM próprios, e a raiz continua com o `-all` intacto. É também o
que o Resend recomenda, por outra razão: se um dia a reputação do envio
transacional azedar, ela não contamina o domínio principal.

DNS fica no **Cloudflare**.

## As duas mecânicas são uma só

Confirmar e-mail e redefinir senha são a mesma coisa — um segredo de uso
único, com prazo, que prova posse da caixa de e-mail. Por isso uma tabela só:

```sql
email_tokens (token_hash PK, account_id, tipo, expires_at, usado_em, created_at)
--  tipo 0 = confirmação (48 h)
--  tipo 1 = reset        (1 h)
```

O banco guarda **só o SHA-256**, como `login_tokens`: quem ler a tabela não
consegue usar o que leu.

Prazos diferentes de propósito: o de confirmação só liga a conta e pode ser
generoso (e-mail demora, a pessoa abre no dia seguinte). O de reset **troca a
senha**, e uma hora já é tempo de sobra para quem está com a caixa aberta.

O uso único é **atômico**, num `UPDATE ... RETURNING` com `usado_em IS NULL` na
condição. Conferir e depois marcar, em dois passos, deixaria dois cliques
quase simultâneos passarem os dois.

## Decisões que não são óbvias

- **`email_confirmado` nasce `TRUE` no `DEFAULT`.** Só vale para a coluna nova:
  sem isso, toda conta que já existe nasceria "não confirmada" e ninguém mais
  entraria no jogo. Quem já estava dentro fica dentro; a exigência vale de
  agora em diante. Conta nova, com envio ligado, é marcada `FALSE` logo após o
  `INSERT`.
- **A falha do envio não derruba o cadastro.** A conta já existe no banco nesse
  ponto; responder erro faria o jogador tentar de novo e bater em "username já
  existe", sem conta nenhuma na mão dele.
- **`/api/auth/esqueci` responde 200 sempre.** Responder 404 para e-mail
  inexistente transformaria a rota num verificador de cadastro: qualquer um
  descobriria quem tem conta aqui. O cliente diz "**se** houver uma conta com
  esse e-mail".
- **Conta do Google não recebe link de reset.** A senha dela é `!google`, que
  nunca casa com argon2 — mandar o link seria prometer o que não acontece.
- **Trocar a senha apaga as sessões** (`login_tokens` da conta). "Esqueci minha
  senha" é o que a pessoa faz quando desconfia que alguém entrou; deixar de pé
  um "lembrar de mim" emitido antes manteria esse alguém dentro justamente
  depois do conserto.
- **A confirmação é conferida DEPOIS da senha.** Dizer "confirme seu e-mail"
  para quem errou a senha entregaria que aquele usuário existe.
- **O servidor de jogo tolera a coluna ausente** (`COALESCE` + fallback no erro
  `42703`). É o `web` quem cria o schema; se o servidor subir primeiro, sem o
  fallback a consulta falharia e **ninguém** entraria.
- **O link abre no navegador, não no app.** Quem clica no e-mail já está num
  navegador; pedir para voltar ao jogo e digitar um código seria trabalho que a
  página faz melhor. Por isso `/api/auth/reset` é formulário no GET e API no
  POST.

## O que existe em produção (23/09/2026)

- Conta no **Resend**, chave de envio em `~/tempest-prod/tempest.env` (modo
  `600`). A chave é do tipo **somente envio**: não lista nem cria domínios —
  o escopo certo para um servidor de jogo.
- Domínio de envio **`mail.brunji.com.br`**, região São Paulo (sa-east-1).
- Quatro registros no Cloudflare, criados pela API com um token de escopo
  mínimo (só esta zona, só `DNS:Edit`), **revogado depois de usar**:

  | Tipo | Nome | Conteúdo |
  |---|---|---|
  | TXT | `resend._domainkey.mail` | chave DKIM (218 caracteres) |
  | CNAME | `rsend.mail` | `rsend-sae1.forge.rmta.net` |
  | CNAME | `send.mail` | `send.forge.rmta.net` |
  | TXT | `_dmarc` | `v=DMARC1; p=none;` |

  Os CNAME são **`proxied: false`**. Com o proxy da Cloudflare ligado, ela
  responde o CNAME com IP próprio e o Resend não verifica nem entrega.

- Foi usado **"Manual setup"**, e não "Auto configure": o automático pede
  autorização para o Resend **escrever** no DNS da conta. Na mão, ninguém
  ganha esse acesso.

Se um dia a entrega parar, o primeiro lugar a olhar é `resend.com/logs` e
depois estes quatro registros — o DNS é o que costuma mudar sem aviso.

## Variáveis de ambiente (`web`)

| Variável | Obrigatória | O quê |
|---|---|---|
| `RESEND_API_KEY` | sim (senão desliga) | chave da API do Resend |
| `EMAIL_REMETENTE` | não | padrão `Tempest <nao-responda@brunji.com.br>` |
| `PUBLIC_BASE_URL` | não | padrão `https://mmo.brunji.com.br` |

## Pendências

- Reenviar a confirmação pelo jogo (hoje, quem não recebeu não tem botão).
- Trocar o e-mail da conta.
- Limite de pedidos por e-mail/IP no `esqueci` — hoje só há o apagamento do
  token anterior, que impede acumular chaves mas não impede insistir.

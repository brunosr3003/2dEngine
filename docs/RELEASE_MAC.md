# Mac: assinatura, notarização e o `xattr`

## O problema

Todo arquivo que um **navegador** baixa recebe o atributo estendido
`com.apple.quarantine`. App em quarentena que a Apple não notarizou não abre:
no macOS Sequoia nem pelo botão direito → Abrir, só por **Ajustes do Sistema →
Privacidade e Segurança → Abrir Mesmo Assim**. A saída na mão é

```
xattr -cr /caminho/Tempest.app
```

que apaga o atributo — e volta na próxima baixada. Era o que o dono fazia a
cada versão.

**Assinatura ad-hoc não resolve.** `codesign --sign -` (o que o build fazia até
28/09/2026) só faz o binário *rodar* no Apple Silicon, que exige assinatura
mesmo que anônima. Ela não diz nada sobre quem publicou, então o Gatekeeper
segue barrando o download. Quem tira a quarentena do caminho é o **ticket de
notarização** grampeado no `.app`.

## A solução: Developer ID + notarização + staple

`scripts/build-mac.sh` faz isso sozinho **se** existir uma identidade
`Developer ID Application` no login keychain:

1. `codesign --options runtime --timestamp --sign "Developer ID Application: …"`
   — hardened runtime e carimbo de tempo são exigidos pra notarizar.
2. `xcrun notarytool submit … --wait` — manda o zip e espera o veredito
   (normalmente 1–5 min). Usa a chave da App Store Connect que o
   `build-ios.sh` já usa (`ASC_API_KEY_ID`/`ASC_API_ISSUER_ID` +
   `~/.appstoreconnect/private_keys/AuthKey_<id>.p8`), e cai no
   `APPLE_ID`/`APPLE_APP_PASSWORD` se a chave não estiver lá.
3. `xcrun stapler staple Tempest.app` — grampeia o ticket **no app**, não no
   zip. É por isso que o zip final é feito depois.
4. `spctl --assess --type execute` — a prova: é a mesma resposta que o
   Gatekeeper vai dar na máquina de quem baixou.

Sem a identidade, o script cai no ad-hoc de antes e **avisa** no stderr, porque
build sem notarização é build que vai pedir `xattr` do outro lado.

## O certificado (uma vez só)

O keychain do Mac tem hoje `Apple Development` e `Apple Distribution` — as duas
são de App Store/TestFlight. Distribuir **fora** da App Store pede um tipo
diferente, `Developer ID Application`, que a conta paga do Programa de
Desenvolvedores dá direito e que precisa ser criado uma vez:

1. `ssh mac 'cd ~/2dEngine-release && python3 scripts/cert-developer-id.py --csr'`
   — gera a chave privada e o CSR em `~/.appstoreconnect/developer-id/`. A
   chave **nunca sai do Mac**.
2. No Mac, em
   `developer.apple.com/account/resources/certificates/add` → **Developer ID
   Application** → subir `developer-id.csr` → baixar o `.cer`.
3. `python3 scripts/cert-developer-id.py --instalar ~/Downloads/<o>.cer` —
   junta o certificado com a chave privada num `.p12` e importa no login
   keychain, autorizando o `codesign` a usá-la sem pedir senha.

**Pela API não dá.** Tentado em 28/09/2026: `POST /v1/certificates` com
`certificateType: DEVELOPER_ID_APPLICATION` responde
`403 FORBIDDEN_ERROR — This operation can only be performed by the Account
Holder`, e chave de API não pode ter esse papel. O passo 2 é do navegador,
logado como o dono da conta; não há como automatizar.

Detalhes que valem saber antes: o limite é de **dois** certificados Developer
ID ativos por conta, e a chave privada existe só onde o CSR nasceu — perdê-la é
ter que criar outro. Depois disso nada mais é manual: o `build-mac.sh` acha a
identidade sozinho e passa a notarizar.

## Enquanto não houver o certificado

Baixar por `curl` **não** põe quarentena — quem põe é o navegador. Então isto
abre de primeira num build ad-hoc, sem `xattr`:

```
curl -L -o ~/Downloads/Tempest.zip https://mmo.brunji.com.br/downloads/MMORPG-Mac.zip
unzip -o ~/Downloads/Tempest.zip -d ~/Applications
open ~/Applications/Tempest.app
```

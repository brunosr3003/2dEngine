#!/usr/bin/env python3
"""Cria (ou lista) o certificado Developer ID Application. Roda NO MAC.

    ssh mac 'cd ~/2dEngine-release && python3 scripts/cert-developer-id.py --listar'
    ssh mac 'cd ~/2dEngine-release && python3 scripts/cert-developer-id.py --criar'

E' o certificado que permite distribuir FORA da App Store. Sem ele o
`build-mac.sh` cai na assinatura ad-hoc, e quem baixa pelo navegador tem que
rodar `xattr -cr` a cada versao porque o app nao esta' notarizado -- ver
docs/RELEASE_MAC.md.

O que o `--criar` faz:

  1. gera uma chave privada RSA 2048 + CSR aqui no Mac (a chave NUNCA sai);
  2. pede o certificado pela API da App Store Connect, com a mesma chave .p8
     que o build-ios.sh usa;
  3. junta chave + certificado num .p12 e importa no login keychain.

CUIDADO, e' por isso que este script nao roda sozinho em build nenhuma: a
Apple permite no maximo DOIS certificados Developer ID Application ativos por
conta, so' o Account Holder cria, e revogar um quebra tudo que ele assinou e
ainda nao foi notarizado. Rode o `--listar` primeiro.
"""
import argparse, base64, json, os, pathlib, subprocess, sys, time, urllib.error, urllib.request

import jwt

TIPO = "DEVELOPER_ID_APPLICATION"
ENV = os.path.expanduser(os.environ.get("TEMPEST_ENV", "~/MMORPG/.env"))
SAIDA = pathlib.Path(os.path.expanduser("~/.appstoreconnect/developer-id"))


def credenciais():
    env = {}
    for linha in open(ENV):
        linha = linha.strip()
        if "=" in linha and not linha.startswith("#"):
            k, v = linha.split("=", 1)
            env[k.strip()] = v.strip().strip('"').strip("'")
    key_id, issuer = env["ASC_API_KEY_ID"], env["ASC_API_ISSUER_ID"]
    p8 = open(os.path.expanduser(f"~/.appstoreconnect/private_keys/AuthKey_{key_id}.p8")).read()
    token = jwt.encode(
        {"iss": issuer, "exp": int(time.time()) + 900, "aud": "appstoreconnect-v1"},
        p8,
        algorithm="ES256",
        headers={"kid": key_id, "typ": "JWT"},
    )
    return token, env


def chamar(token, caminho, corpo=None):
    dados = json.dumps(corpo).encode() if corpo else None
    req = urllib.request.Request(
        "https://api.appstoreconnect.apple.com/v1/" + caminho,
        data=dados,
        headers={
            "Authorization": "Bearer " + token,
            **({"Content-Type": "application/json"} if dados else {}),
        },
        method="POST" if dados else "GET",
    )
    try:
        return json.load(urllib.request.urlopen(req, timeout=60))
    except urllib.error.HTTPError as e:
        corpo = e.read().decode()
        sys.exit(f"API respondeu HTTP {e.code}:\n{corpo[:1200]}")


def listar(token):
    r = chamar(token, "certificates?limit=200")
    print(f"{'tipo':<34} {'nome':<38} {'expira':<22} id")
    achou = 0
    for c in r.get("data", []):
        a = c["attributes"]
        if a.get("certificateType") == TIPO:
            achou += 1
        print(
            f"{a.get('certificateType',''):<34} {a.get('displayName','')[:37]:<38} "
            f"{a.get('expirationDate',''):<22} {c['id']}"
        )
    print(f"\n{TIPO}: {achou} ativo(s). O limite da Apple e' 2.")
    return achou


def criar(token, env):
    SAIDA.mkdir(parents=True, exist_ok=True)
    os.chmod(SAIDA, 0o700)
    chave, csr = SAIDA / "developer-id.key", SAIDA / "developer-id.csr"
    if chave.exists():
        sys.exit(f"{chave} ja' existe. Ele guarda a chave privada de um certificado "
                 "que talvez esteja em uso: apague a mao se tem certeza.")
    nome = env.get("APPLE_ID", "tempest")
    subprocess.run(
        ["openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes",
         "-keyout", str(chave), "-out", str(csr),
         "-subj", f"/emailAddress={nome}/CN=Tempest Developer ID/C=BR"],
        check=True, capture_output=True,
    )
    os.chmod(chave, 0o600)
    print(f"chave privada: {chave}")

    r = chamar(token, "certificates", {
        "data": {
            "type": "certificates",
            "attributes": {"certificateType": TIPO, "csrContent": csr.read_text()},
        }
    })
    a = r["data"]["attributes"]
    cer = SAIDA / "developer-id.cer"
    cer.write_bytes(base64.b64decode(a["certificateContent"]))
    print(f"certificado:   {cer}\n  nome:   {a.get('displayName')}\n"
          f"  expira: {a.get('expirationDate')}")

    # Keychain: o codesign precisa da chave privada E do certificado juntos,
    # entao vai um .p12. Sem `-A`/`-T` o codesign pediria senha a cada uso.
    pem = SAIDA / "developer-id.pem"
    subprocess.run(["openssl", "x509", "-inform", "DER", "-in", str(cer),
                    "-out", str(pem)], check=True)
    p12 = SAIDA / "developer-id.p12"
    subprocess.run(["openssl", "pkcs12", "-export", "-out", str(p12),
                    "-inkey", str(chave), "-in", str(pem), "-passout", "pass:"],
                   check=True)
    os.chmod(p12, 0o600)
    subprocess.run(["security", "import", str(p12), "-k",
                    os.path.expanduser("~/Library/Keychains/login.keychain-db"),
                    "-P", "", "-T", "/usr/bin/codesign", "-T", "/usr/bin/security"],
                   check=True)
    print(f"importado no login keychain a partir de {p12}")

    idents = subprocess.run(["security", "find-identity", "-v", "-p", "codesigning"],
                            capture_output=True, text=True).stdout
    print("\n== identidades de assinatura agora ==")
    print(idents.rstrip())
    if "Developer ID Application" not in idents:
        sys.exit("O certificado entrou mas a identidade nao aparece: falta a "
                 "cadeia (Developer ID Certification Authority) no keychain.")
    print("\nPronto. `scripts/build-mac.sh` acha a identidade sozinho e passa a "
          "notarizar.")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--listar", action="store_true")
    p.add_argument("--criar", action="store_true")
    args = p.parse_args()
    token, env = credenciais()
    if args.criar:
        if listar(token) >= 2:
            sys.exit("\nJa' existem 2 certificados Developer ID Application: a Apple "
                     "nao deixa criar um terceiro. Revogue um no portal antes.")
        print()
        criar(token, env)
    else:
        listar(token)

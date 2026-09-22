#!/usr/bin/env python3
"""O estado de VERDADE das builds no TestFlight. Roda NO MAC.

    ssh mac 'python3 ~/2dEngine/scripts/testflight-estado.py'

O `altool` diz "UPLOAD SUCCEEDED" quando o arquivo CHEGOU -- nao quando a build
existe no TestFlight. Entre uma coisa e outra a Apple processa, e e' ali que ela
some calada: binario invalido, ITMS por e-mail, compliance de exportacao sem
resposta, ou 'Processing' eterno. Em 22/09/2026 o dono disse "nao subiu pro
testflight" com a build ja' em IN_BETA_TESTING ha' 14 minutos -- sem isto aqui
nao havia como saber de que lado estava o problema.

Le as credenciais do mesmo ~/MMORPG/.env que o build-ios.sh usa; a chave .p8
nunca sai do Mac.
"""
import json, os, time, urllib.request, urllib.error
import jwt

env = {}
for l in open(os.path.expanduser("~/MMORPG/.env")):
    l = l.strip()
    if "=" in l and not l.startswith("#"):
        k, v = l.split("=", 1); env[k.strip()] = v.strip().strip('"').strip("'")
KEY_ID, ISSUER = env["ASC_API_KEY_ID"], env["ASC_API_ISSUER_ID"]
BUNDLE = env.get("APP_BUNDLE_ID", "com.brunji.tempest")
p8 = open(os.path.expanduser(f"~/.appstoreconnect/private_keys/AuthKey_{KEY_ID}.p8")).read()
tok = jwt.encode({"iss": ISSUER, "exp": int(time.time())+900, "aud": "appstoreconnect-v1"},
                 p8, algorithm="ES256", headers={"kid": KEY_ID, "typ": "JWT"})

def get(path):
    req = urllib.request.Request("https://api.appstoreconnect.apple.com/v1/"+path,
                                 headers={"Authorization": "Bearer "+tok})
    for _ in range(3):
        try:
            return json.load(urllib.request.urlopen(req, timeout=30))
        except urllib.error.HTTPError as e:
            return {"erro": f"HTTP {e.code}: {e.read().decode()[:400]}"}
        except Exception as e:
            err = e
    return {"erro": str(err)}

app = get(f"apps?filter[bundleId]={BUNDLE}")["data"][0]["id"]

print("== grupos de teste ==")
for g in get(f"apps/{app}/betaGroups?limit=20").get("data", []):
    a = g["attributes"]
    testers = get(f"betaGroups/{g['id']}/betaTesters?limit=200").get("data", [])
    print(f"  {a['name']!r:<28} interno={a.get('isInternalGroup')} "
          f"auto={a.get('hasAccessToAllBuilds')} testers={len(testers)}")
    for t in testers[:8]:
        ta = t["attributes"]
        print(f"      - {ta.get('email')}  {ta.get('state')}")

print("\n== as 3 ultimas builds ==")
b = get(f"builds?filter[app]={app}&limit=3&sort=-uploadedDate&include=buildBetaDetail")
incl = {i["id"]: i for i in b.get("included", [])}
for d in b["data"]:
    ver = d["attributes"]["version"]
    bd = incl.get((d["relationships"].get("buildBetaDetail", {}).get("data") or {}).get("id"))
    ba = bd["attributes"] if bd else {}
    grupos = [g["attributes"]["name"] for g in get(f"builds/{d['id']}/betaGroups").get("data", [])]
    print(f"  {ver}: interno={ba.get('internalBuildState')} externo={ba.get('externalBuildState')}")
    print(f"      grupos ligados: {grupos or 'NENHUM'}")

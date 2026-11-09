#!/usr/bin/env python3
"""App ID + DEVELOPMENT profile for the Tempest TEST app (iPhone only).

The test app has its own bundle (`com.brunji.tempest.teste`) so it installs
NEXT TO the real one. This creates — through the App Store Connect API, with
the same key `upload-ios-api.py` uses — the bundle id if missing and an
IOS_APP_DEVELOPMENT profile for every registered iPhone and every Apple
Development certificate, and saves it where `build-ios.sh --iphone` looks.

    python3 scripts/ios-perfil-teste.py [bundle]

Runs on the Mac. Idempotent: an existing profile with the same name is
replaced (profiles expire and must list new devices).
"""
import base64
import json
import os
import sys
import time
import urllib.error
import urllib.request

import jwt

BUNDLE = sys.argv[1] if len(sys.argv) > 1 else "com.brunji.tempest.teste"
NOME_PERFIL = f"Tempest TESTE dev ({BUNDLE})"


def env_file():
    env = {}
    with open(os.path.expanduser("~/MMORPG/.env")) as f:
        for line in f:
            if "=" in line and not line.lstrip().startswith("#"):
                k, v = line.strip().split("=", 1)
                env[k.strip()] = v.strip().strip('"').strip("'")
    return env


ENV = env_file()
KEY_ID, ISSUER = ENV["ASC_API_KEY_ID"], ENV["ASC_API_ISSUER_ID"]
with open(os.path.expanduser(f"~/.appstoreconnect/private_keys/AuthKey_{KEY_ID}.p8")) as k:
    TOKEN = jwt.encode(
        {"iss": ISSUER, "exp": int(time.time()) + 900, "aud": "appstoreconnect-v1"},
        k.read(), algorithm="ES256", headers={"kid": KEY_ID, "typ": "JWT"},
    )


def api(method, path, body=None):
    req = urllib.request.Request(
        "https://api.appstoreconnect.apple.com/v1/" + path,
        data=None if body is None else json.dumps(body).encode(),
        method=method,
        headers={"Authorization": "Bearer " + TOKEN, "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            corpo = r.read()
            return json.loads(corpo) if corpo else {}
    except urllib.error.HTTPError as e:
        raise SystemExit(f"API {method} {path}: {e.code} {e.read().decode()[:600]}")


def main():
    achado = api("GET", f"bundleIds?filter[identifier]={BUNDLE}&filter[platform]=IOS")["data"]
    achado = [b for b in achado if b["attributes"]["identifier"] == BUNDLE]
    if achado:
        bundle_id = achado[0]["id"]
        print(f"bundle id existe: {BUNDLE}")
    else:
        bundle_id = api("POST", "bundleIds", {"data": {"type": "bundleIds", "attributes": {
            "identifier": BUNDLE, "name": "Tempest TESTE", "platform": "IOS"}}})["data"]["id"]
        print(f"bundle id criado: {BUNDLE}")
    certs = [c["id"] for c in api("GET", "certificates?limit=50")["data"]
             if c["attributes"]["certificateType"] in ("DEVELOPMENT", "IOS_DEVELOPMENT")]
    devices = [d["id"] for d in api("GET", "devices?limit=200")["data"]
               if d["attributes"]["platform"] == "IOS" and d["attributes"]["status"] == "ENABLED"]
    if not certs or not devices:
        raise SystemExit(f"faltam certificado de desenvolvimento ({len(certs)}) ou aparelho ({len(devices)})")
    for p in api("GET", f"profiles?filter[name]={urllib.request.quote(NOME_PERFIL)}")["data"]:
        api("DELETE", f"profiles/{p['id']}")
        print("perfil antigo removido")
    perfil = api("POST", "profiles", {"data": {
        "type": "profiles",
        "attributes": {"name": NOME_PERFIL, "profileType": "IOS_APP_DEVELOPMENT"},
        "relationships": {
            "bundleId": {"data": {"type": "bundleIds", "id": bundle_id}},
            "certificates": {"data": [{"type": "certificates", "id": c} for c in certs]},
            "devices": {"data": [{"type": "devices", "id": d} for d in devices]},
        },
    }})["data"]
    pasta = os.path.expanduser("~/Library/MobileDevice/Provisioning Profiles")
    os.makedirs(pasta, exist_ok=True)
    destino = os.path.join(pasta, f"tempest-teste-{perfil['attributes']['uuid']}.mobileprovision")
    with open(destino, "wb") as f:
        f.write(base64.b64decode(perfil["attributes"]["profileContent"]))
    print(f"perfil salvo: {destino} ({len(certs)} certificado(s), {len(devices)} aparelho(s))")


if __name__ == "__main__":
    main()

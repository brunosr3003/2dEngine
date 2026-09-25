#!/usr/bin/env python3
"""Upload direto de um IPA pela API de Build Uploads da Apple. Roda no Mac."""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

import jwt


def env_file():
    env = {}
    with open(os.path.expanduser("~/MMORPG/.env")) as file:
        for line in file:
            if "=" in line and not line.lstrip().startswith("#"):
                key, val = line.strip().split("=", 1)
                env[key.strip()] = val.strip().strip('"').strip("'")
    return env


ENV = env_file()
KEY_ID = ENV["ASC_API_KEY_ID"]
ISSUER = ENV["ASC_API_ISSUER_ID"]
with open(os.path.expanduser(f"~/.appstoreconnect/private_keys/AuthKey_{KEY_ID}.p8")) as key:
    TOKEN = jwt.encode(
        {"iss": ISSUER, "exp": int(time.time()) + 900, "aud": "appstoreconnect-v1"},
        key.read(),
        algorithm="ES256",
        headers={"kid": KEY_ID, "typ": "JWT"},
    )


def curl(method, url, body=None, headers=None):
    headers = headers or {}
    with tempfile.NamedTemporaryFile(mode="w", delete=False) as header_file:
        os.chmod(header_file.name, 0o600)
        for name, value in headers.items():
            header_file.write(f"{name}: {value}\n")
        header_path = header_file.name
    cmd = ["curl", "-sS", "--globoff", "--max-time", "90", "--retry", "2", "--fail-with-body",
           "-X", method, "-H", "@" + header_path]
    if body is not None:
        cmd += ["--data-binary", "@-"]
    cmd.append(url)
    try:
        result = subprocess.run(cmd, input=body, capture_output=True, timeout=300)
        if result.returncode:
            raise RuntimeError(f"HTTP upload: {result.stderr.decode()[-300:]} {result.stdout.decode()[:1500]}")
        return result.stdout
    finally:
        os.unlink(header_path)


def request(method, path, body=None):
    url = "https://api.appstoreconnect.apple.com/v1/" + path
    payload = None if body is None else json.dumps(body).encode()
    return json.loads(curl(method, url, payload,
                           {"Authorization": "Bearer " + TOKEN, "Content-Type": "application/json"}))


def main(ipa, resume_id=None):
    with open(ipa, "rb") as file:
        data = file.read()
    app = request("GET", f"apps?filter[bundleId]={ENV.get('APP_BUNDLE_ID', 'com.brunji.tempest')}")["data"][0]["id"]
    name = os.path.basename(ipa)
    build = name.rsplit("-", 1)[-1].removesuffix(".ipa")
    version = name.removeprefix("Tempest-").rsplit("-", 1)[0]
    print(f"Build {version} ({build}), {len(data)} bytes", flush=True)
    if resume_id:
        uid = resume_id
        file_rec = request("GET", f"buildUploads/{uid}/buildUploadFiles")["data"][0]
    else:
        upload = request("POST", "buildUploads", {"data": {
            "type": "buildUploads",
            "attributes": {"cfBundleShortVersionString": version, "cfBundleVersion": build, "platform": "IOS"},
            "relationships": {"app": {"data": {"type": "apps", "id": app}}},
        }})["data"]
        uid = upload["id"]
        print(f"Reserva de build: {uid}", flush=True)
        file_rec = request("POST", "buildUploadFiles", {"data": {
            "type": "buildUploadFiles",
            "attributes": {"assetType": "ASSET", "fileName": name, "fileSize": len(data), "uti": "com.apple.ipa"},
            "relationships": {"buildUpload": {"data": {"type": "buildUploads", "id": uid}}},
        }})["data"]
    fid = file_rec["id"]
    operations = [] if resume_id else file_rec["attributes"]["uploadOperations"]
    print(f"Enviando {len(operations)} parte(s)", flush=True)
    for operation in operations:
        offset = operation["offset"]
        length = operation["length"]
        headers = {h["name"]: h["value"] for h in operation["requestHeaders"]}
        curl(operation["method"], operation["url"], data[offset:offset + length], headers)
    checksums = {"file": {"algorithm": "MD5", "hash": hashlib.md5(data).hexdigest()}}
    result = request("PATCH", f"buildUploadFiles/{fid}", {"data": {
        "type": "buildUploadFiles", "id": fid,
        "attributes": {"uploaded": True, "sourceFileChecksums": checksums},
    }})
    print("Arquivo enviado:", result["data"]["attributes"].get("assetDeliveryState"), flush=True)
    state = request("GET", f"buildUploads/{uid}")["data"]["attributes"].get("state")
    print("Estado do envio:", state, flush=True)


if __name__ == "__main__":
    if sys.argv[1] == "--status":
        upload = request("GET", f"buildUploads/{sys.argv[2]}?include=build")
        print("Envio:", upload["data"]["attributes"].get("state"))
        print("Build:", upload["data"].get("relationships", {}).get("build"))
        for build in upload.get("included", []):
            print("Processamento:", build["attributes"].get("processingState"))
            groups = request("GET", f"builds/{build['id']}?include=betaGroups").get("included", [])
            print("Grupos TestFlight:", [g["attributes"].get("name") for g in groups])
    else:
        main(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else None)

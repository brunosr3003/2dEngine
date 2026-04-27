#!/usr/bin/env bash
# Setup inicial do VPS pra rodar o MMORPG server.
#
# Roda 1× só, COMO ROOT, num Ubuntu/Debian fresh:
#   wget https://raw.githubusercontent.com/.../vps-setup.sh
#   sudo bash vps-setup.sh
#
# O que faz:
#  - instala postgres
#  - cria DB + user `mmo` com senha aleatória → grava em /etc/mmorpg.env
#  - abre porta 9000/tcp no UFW
#  - cria usuário do sistema `mmo` (não-root)
#  - prepara /opt/mmorpg como home do server
#  - cria systemd unit `mmorpg-server.service` (não inicia ainda — falta o binário)
#
# Após esse script, do seu Mac:
#   ./scripts/build-linux.sh
#   scp dist/server-linux-x86_64.tar.gz mmo@VPS_IP:/opt/mmorpg/
#   scp data/maps/game.json mmo@VPS_IP:/opt/mmorpg/data/maps/
#   ssh mmo@VPS_IP
#     cd /opt/mmorpg && tar -xzf server-linux-x86_64.tar.gz
#   sudo systemctl start mmorpg-server
#   sudo systemctl enable mmorpg-server   # inicia no boot
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Rode como root: sudo bash $0"
    exit 1
fi

DB_PASS="$(openssl rand -base64 24 | tr -d '/+=' | head -c 24)"

echo "==> apt update + instalando deps..."
apt-get update -y
apt-get install -y postgresql ufw curl

echo "==> criando usuário sistema 'mmo'..."
if ! id -u mmo &>/dev/null; then
    useradd -m -s /bin/bash mmo
fi

echo "==> /opt/mmorpg/{data/maps,logs}..."
mkdir -p /opt/mmorpg/data/maps /opt/mmorpg/logs
chown -R mmo:mmo /opt/mmorpg

echo "==> configurando Postgres (DB=mmo_prod, user=mmo)..."
sudo -u postgres psql <<EOF
DO \$\$
BEGIN
   IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname='mmo') THEN
      CREATE ROLE mmo LOGIN PASSWORD '${DB_PASS}';
   ELSE
      ALTER ROLE mmo WITH PASSWORD '${DB_PASS}';
   END IF;
END
\$\$;
SELECT 'CREATE DATABASE mmo_prod OWNER mmo'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'mmo_prod')\gexec
EOF

cat > /etc/mmorpg.env <<EOF
DATABASE_URL=postgres://mmo:${DB_PASS}@localhost:5432/mmo_prod
MAP_FILE=/opt/mmorpg/data/maps/game.json
RUST_LOG=server=info
BIND_ADDR=0.0.0.0:9000
EOF
chmod 600 /etc/mmorpg.env
chown mmo:mmo /etc/mmorpg.env

echo "==> firewall (UFW): liberando 22 (ssh) + 9000 (game) + 80/443 (futuro nginx)..."
ufw allow 22/tcp || true
ufw allow 9000/tcp || true
ufw allow 80/tcp  || true
ufw allow 443/tcp || true
ufw --force enable || true

echo "==> systemd unit /etc/systemd/system/mmorpg-server.service..."
cat > /etc/systemd/system/mmorpg-server.service <<'EOF'
[Unit]
Description=MMORPG Game Server
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=mmo
WorkingDirectory=/opt/mmorpg
EnvironmentFile=/etc/mmorpg.env
ExecStart=/opt/mmorpg/server
Restart=on-failure
RestartSec=3
StandardOutput=append:/opt/mmorpg/logs/server.log
StandardError=append:/opt/mmorpg/logs/server.log
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload

echo
echo "================ SETUP COMPLETO ================"
echo "DB user=mmo  DB pass=${DB_PASS}"
echo "Senha SALVA em /etc/mmorpg.env (chmod 600)"
echo
echo "Próximo passo (do seu Mac):"
echo "  scp dist/server-linux-x86_64.tar.gz mmo@VPS_IP:/opt/mmorpg/"
echo "  scp data/maps/game.json mmo@VPS_IP:/opt/mmorpg/data/maps/"
echo "  ssh mmo@VPS_IP 'cd /opt/mmorpg && tar -xzf server-linux-x86_64.tar.gz && chmod +x server'"
echo "  ssh root@VPS_IP 'systemctl start mmorpg-server && systemctl enable mmorpg-server'"
echo "  ssh root@VPS_IP 'tail -f /opt/mmorpg/logs/server.log'"
echo
echo "Cliente conecta em: ws://VPS_IP:9000"

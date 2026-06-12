#!/usr/bin/env bash
# Setup (1x) do 2º processo do servidor = TUTORIAL, no VPS.
# Rodar como root no VPS: ssh mmo-vps 'bash -s' < scripts/vps-tutorial-setup.sh
#
# Sobe um 2º processo do MESMO binário /opt/mmorpg/server, com:
#   - MAP_FILE=data/maps/tutorial.json
#   - TUTORIAL_MODE=1
#   - BIND_ADDR=0.0.0.0:9001
#   - MESMA DATABASE_URL do mmorpg-server (mesmo Postgres/personagens)
# Fica atrás do nginx em /tutorial (não abre porta no firewall).
set -euo pipefail

# Reusa a DATABASE_URL do serviço principal pra não duplicar segredo.
DB_URL="$(systemctl show mmorpg-server -p Environment | tr ' ' '\n' | sed -n 's/^DATABASE_URL=//p')"
if [ -z "${DB_URL}" ]; then
  # Fallback: tenta o EnvironmentFile do serviço principal.
  echo "AVISO: DATABASE_URL não veio do systemd show. Edite /etc/mmorpg-tutorial.env manualmente."
fi

cat > /etc/mmorpg-tutorial.env <<EOF
DATABASE_URL=${DB_URL}
MAP_FILE=/opt/mmorpg/data/maps/tutorial.json
TUTORIAL_MODE=1
BIND_ADDR=0.0.0.0:9001
RUST_LOG=server=info
EOF
chmod 600 /etc/mmorpg-tutorial.env

cat > /etc/systemd/system/mmorpg-tutorial.service <<'EOF'
[Unit]
Description=MMORPG Tutorial (instância isolada, porta 9001)
After=network.target postgresql.service mmorpg-server.service

[Service]
Type=simple
WorkingDirectory=/opt/mmorpg
EnvironmentFile=/etc/mmorpg-tutorial.env
ExecStart=/opt/mmorpg/server
Restart=on-failure
RestartSec=3
StandardOutput=append:/opt/mmorpg/logs/tutorial.log
StandardError=append:/opt/mmorpg/logs/tutorial.log

[Install]
WantedBy=multi-user.target
EOF

mkdir -p /opt/mmorpg/logs
systemctl daemon-reload
systemctl enable mmorpg-tutorial
systemctl restart mmorpg-tutorial
sleep 2
systemctl is-active mmorpg-tutorial && echo "mmorpg-tutorial ATIVO em :9001"

cat <<'NGINX'

── FALTA (manual, 1x): nginx ───────────────────────────────────────────────
Adicione no server block de mmo.brunji.com.br
(/www/server/panel/vhost/nginx/mmo.brunji.com.br.conf), espelhando /game:

    location /tutorial {
        proxy_pass http://127.0.0.1:9001;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 3600s;
    }

Depois: nginx -t && systemctl reload nginx
NGINX

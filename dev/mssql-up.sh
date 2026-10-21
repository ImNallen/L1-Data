#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
docker compose -f dev/docker-compose.yml --profile mssql up -d --wait mssql
docker compose -f dev/docker-compose.yml exec -T mssql sh -c '/opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P "$MSSQL_SA_PASSWORD" -C -b -i /seed.sql'

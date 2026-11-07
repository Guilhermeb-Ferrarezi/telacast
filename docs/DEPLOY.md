# Colocar o Telacast na internet (VPS Oracle Ampere A1)

Caminho: **Caddy** (HTTPS automático) → `telacast-server` (só em `127.0.0.1:8080`) → **PostgreSQL**.
O app fala `wss://` com o Caddy; o servidor nunca fica exposto sem criptografia.

> Estado: o `wss://` do cliente foi testado contra um proxy TLS local com uma CA de teste. **Ainda não foi
> testado na VPS da Oracle**: use este guia como roteiro e confira cada passo.

## 1. Antes de começar

- VPS Ubuntu (ARM64 serve) com IP público; um nome de domínio (ex.: `telacast.duckdns.org`) apontando para ele.
- Portas **80 e 443** liberadas: na lista de segurança da Oracle (VCN) **e** no firewall da máquina
  (`sudo iptables -I INPUT -p tcp --dport 80 -j ACCEPT`, idem 443; a imagem da Oracle bloqueia por padrão).
- A porta **8080 não** deve ser aberta para fora.

## 2. PostgreSQL

```bash
sudo apt install postgresql          # ou Docker, como no desenvolvimento (docker-compose.dev.yml)
sudo -u postgres psql -c "CREATE USER telacast WITH PASSWORD 'TROQUE-ESTA-SENHA';"
sudo -u postgres psql -c "CREATE DATABASE telacast OWNER telacast;"
```

As migrações rodam sozinhas quando o servidor sobe.

## 3. Compilar e instalar o servidor (na própria VPS)

```bash
git clone <seu repositório> telacast && cd telacast
cargo build --release -p telacast-server           # 2 núcleos: leva alguns minutos
sudo install -m 755 target/release/telacast-server /usr/local/bin/
sudo useradd --system --home /var/lib/telacast --create-home telacast
```

`/etc/telacast.env` (permissão 600, dono `telacast`):

```
DATABASE_URL=postgres://telacast:TROQUE-ESTA-SENHA@127.0.0.1:5432/telacast
TELACAST_ADDR=127.0.0.1:8080
TELACAST_FILES_DIR=/var/lib/telacast/files
# opcionais: conexões simultâneas por IP (padrão 30) e tamanho máximo de anexo em MB (padrão 10)
# TELACAST_MAX_CONNS_PER_IP=30
# TELACAST_MAX_UPLOAD_MB=10
```

`/etc/systemd/system/telacast.service`:

```ini
[Unit]
Description=Telacast (servidor)
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
User=telacast
EnvironmentFile=/etc/telacast.env
ExecStart=/usr/local/bin/telacast-server serve
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/lib/telacast

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload && sudo systemctl enable --now telacast
journalctl -u telacast -f            # deve mostrar "telacast-server em ws://127.0.0.1:8080"
```

## 4. Caddy (TLS)

```bash
sudo apt install caddy
```

`/etc/caddy/Caddyfile`:

```
telacast.duckdns.org {
    reverse_proxy 127.0.0.1:8080
}
```

```bash
sudo systemctl reload caddy
```

O Caddy obtém e renova o certificado (Let's Encrypt) sozinho e repassa WebSocket sem configuração extra.
O servidor confia no `X-Forwarded-For` **só** quando a conexão vem do próprio `127.0.0.1`, que é o caso do Caddy:
os limites por IP enxergam o IP real do cliente.

## 5. Primeiro acesso

O cadastro é **fechado por convite**. Gere o primeiro na VPS:

```bash
sudo -u telacast env $(cat /etc/telacast.env | xargs) telacast-server invite --uses 5 --days 7 --label primeiros
```

No app, em "Servidor", escreva `wss://telacast.duckdns.org`, crie a conta com o código e pronto.
O app **recusa** `ws://` para qualquer servidor fora da sua máquina e da rede local.

## 6. Manutenção

- **Backup** diário: `pg_dump -Fc telacast > /var/backups/telacast-$(date +%F).dump` (cron) **e** a pasta dos anexos (`/var/lib/telacast/files`), guardando uma cópia fora da VPS.
- **Atualizar**: `git pull && cargo build --release -p telacast-server && sudo install ... && sudo systemctl restart telacast`.
- **Logs**: `journalctl -u telacast`. `RUST_LOG=info` (ou `debug`) no arquivo de ambiente aumenta o detalhe.
- **Certificado próprio** (CA interna, sem Let's Encrypt): no app, defina `TELACAST_CA_FILE=/caminho/ca.pem`.

## 7. Conferir antes de convidar gente

- [ ] `https://seu-dominio` responde (o Caddy mostra erro 400/426 no `/ws` aberto no navegador; é esperado).
- [ ] `ss -ltn` na VPS: `8080` só em `127.0.0.1`.
- [ ] Login pelo app com `wss://` funciona; com `ws://` o app recusa.
- [ ] Backup restaurado uma vez num banco de teste.
- [ ] Senha do Postgres trocada e `/etc/telacast.env` com permissão 600.

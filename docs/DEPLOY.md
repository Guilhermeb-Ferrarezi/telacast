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

Notificações push (opcional): seção 8. Sem `TELACAST_FCM_SERVICE_ACCOUNT` o push fica desligado e o log mostra um aviso na subida.

Miniaturas: não há variável nova. O servidor decodifica até 2 imagens ao mesmo tempo, cada uma de até 64 megapixels (pico de ~250 MB de RAM por imagem, mais a miniatura); numa VPS com pouca memória isso é o teto de uso extra.

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

- **Backup** diário: `pg_dump -Fc telacast > /var/backups/telacast-$(date +%F).dump` (cron) **e** a pasta dos anexos (`/var/lib/telacast/files`), guardando uma cópia fora da VPS. A subpasta `files/thumbs/` guarda as miniaturas geradas sob demanda (cada uma de ~20 a 150 KB, uma por imagem PNG/JPEG/WebP já pedida); pode ficar fora do backup e pode ser apagada a qualquer momento, o servidor gera de novo no próximo pedido.
- **Atualizar**: `git pull && cargo build --release -p telacast-server && sudo install ... && sudo systemctl restart telacast`.
- **Logs**: `journalctl -u telacast`. `RUST_LOG=info` (ou `debug`) no arquivo de ambiente aumenta o detalhe.
- **Certificado próprio** (CA interna, sem Let's Encrypt): no app, defina `TELACAST_CA_FILE=/caminho/ca.pem`.

## 7. Conferir antes de convidar gente

- [ ] `https://seu-dominio` responde (o Caddy mostra erro 400/426 no `/ws` aberto no navegador; é esperado).
- [ ] `ss -ltn` na VPS: `8080` só em `127.0.0.1`.
- [ ] Login pelo app com `wss://` funciona; com `ws://` o app recusa.
- [ ] Backup restaurado uma vez num banco de teste.
- [ ] Senha do Postgres trocada e `/etc/telacast.env` com permissão 600.

## 8. Notificações push (Android, Firebase Cloud Messaging)

O servidor manda o aviso ao FCM; o Google o entrega ao aparelho. **Opcional**: sem a credencial tudo funciona, só
não há push. Protocolo e regras de quem é avisado: `docs/PROTOCOL.md`, seção 11.

1. Em <https://console.firebase.google.com> crie um projeto (pode ser sem Analytics) e **adicione o app Android**
   (o nome do pacote é o do APK, `app.telacast`). Baixe o `google-services.json` e entregue a quem compila o app
   (ele vai no app, não no servidor; não vai para o repositório).
2. **Configurações do projeto > Contas de serviço > Gerar nova chave privada**. Baixa um JSON com `project_id`,
   `client_email` e `private_key`. Confira em *Cloud Messaging* que a **API Firebase Cloud Messaging (v1)** está
   ativa (a API antiga "legada" não serve).
3. Na VPS, guarde a chave fora do repositório e só legível pelo serviço:

```bash
sudo install -d -m 755 /etc/telacast
sudo install -m 600 -o telacast -g telacast service-account.json /etc/telacast/service-account.json
shred -u service-account.json            # apague a cópia baixada
```

4. Em `/etc/telacast.env` acrescente e reinicie (`sudo systemctl restart telacast`):

```
TELACAST_FCM_SERVICE_ACCOUNT=/etc/telacast/service-account.json
```

   O log deve mostrar `push ligado (FCM, projeto ...)`. Se aparecer `push desligado: ...` o motivo vem na mesma
   linha (arquivo ilegível, JSON sem `private_key`, chave que não é RSA). O conteúdo da chave nunca é escrito no log.
   Se o `ProtectSystem=strict` do serviço impedir a leitura, `/etc` já é legível; não precisa de `ReadWritePaths`.
5. Teste: instale o app num celular, entre na conta, bloqueie a tela e peça a outra pessoa que mande uma DM.

Detalhes:

- A chave de serviço dá poder de enviar push em nome do projeto: trate como senha (permissão 600, fora do git, fora
  do backup público). Se vazar, apague-a em *Contas de serviço* e gere outra.
- O servidor assina um JWT (RS256) com essa chave, troca por um token de acesso em `oauth2.googleapis.com` (guardado
  em memória até 1 minuto antes de vencer) e envia por HTTPS a `fcm.googleapis.com` (HTTP v1). Precisa de saída
  HTTPS para esses dois nomes.
- Variáveis para teste/rede especial: `TELACAST_FCM_ENDPOINT` (endereço completo do envio; `{project_id}` é trocado
  pelo projeto), `TELACAST_FCM_TOKEN_URI` (endereço da troca de credencial). Ajustes finos: `TELACAST_PUSH_COALESCE_MS`
  (janela de agrupamento, padrão 3000) e `TELACAST_FCM_RETRY_MS` (espera antes da nova tentativa, padrão 2000).
- O banco ganha a tabela `push_tokens` na próxima subida do servidor (migração `0008`, automática).

**Privacidade:** o aviso leva o **texto da mensagem** (até 180 caracteres, sem Markdown), o nome do autor e o nome
do canal e do servidor, e esses dados **passam pelos servidores do Google** até chegar ao aparelho. Quem não quiser
isso deve deixar o push desligado. Opção futura: enviar só um "toque" sem texto (o app acorda, abre o socket e
monta a notificação sozinho), que evitaria expor o conteúdo ao Google ao custo de depender da conexão do app.

## Atualização do app (verificador de hash)

O servidor entrega os executáveis do app; o app confere antes de trocar o próprio arquivo.

1. **Uma vez**, crie a chave de assinatura (guarde fora do repositório e faça backup):
   `telacast-server update-keygen --out ~/.config/telacast/update.key`. Ele imprime a chave pública.
2. Compile o app com a chave pública embutida: `TELACAST_UPDATE_PUBKEY=<hex> cargo build --release -p telacast`.
3. A cada versão (suba `version` no `app/Cargo.toml`), publique:
   `TELACAST_UPDATE_KEY_FILE=~/.config/telacast/update.key telacast-server publish-update --file target/release/telacast --version 0.2.0 --notes "o que mudou"`
   (plataforma padrão: a desta máquina, por exemplo `linux-x86_64`; `--platform` troca). Os arquivos ficam em
   `TELACAST_UPDATES_DIR` (padrão `./updates`, ignorado pelo git): o executável com o SHA-256 como nome e `manifest-<plataforma>.json`.
4. O app consulta `GET /update/manifest?platform=...` ao entrar. Havendo versão mais nova, mostra a faixa
   "Nova versão X disponível"; "Atualizar agora" baixa `GET /update/file/<sha256>` calculando o SHA-256 no caminho e
   **só instala se** o tamanho e o hash batem com o manifesto **e** a assinatura Ed25519 do manifesto é válida
   (versão, plataforma, hash e tamanho entram na assinatura). Falhou: apaga o arquivo parcial, não troca nada e mostra o motivo.
5. Sem chave pública embutida o app só confere o hash (protege contra corrupção, não contra um servidor adulterado):
   use a assinatura em produção.

## Teste público por túnel da Cloudflare (sem abrir porta no roteador)

1. `cloudflared` rodando com um túnel nomeado; em "Routes" adicione um hostname público (por exemplo `telecast.seudominio.com`) com serviço `http://localhost:8080` (HTTP puro: a Cloudflare faz o TLS).
2. Servidor em release: `TELACAST_FILES_DIR=~/telacast/files DATABASE_URL=... ./target/release/telacast-server serve`. Atrás do túnel o IP do cliente vem de `CF-Connecting-IP` (o servidor já usa; `X-Forwarded-For` é forjável).
3. Verifique: `curl https://telecast.seudominio.com/health` responde `ok`; o app usa `wss://telecast.seudominio.com`.
4. Convite para quem vai testar: `telacast-server invite --uses 1 --days 2 --label amigo`.
5. Windows: `cargo zigbuild --release --target x86_64-pc-windows-gnu -p telacast` gera `Telacast.exe` (só assiste transmissões; captura de tela é só Linux por enquanto). Empacotado em `dist/` (ignorado pelo git).
6. Cuidado: a Cloudflare pode limitar vídeo contínuo no plano gratuito; teste com 720p30 e 3 a 4 Mbps primeiro.

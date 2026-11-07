# R3 — Parecer de segurança do Telacast

Escopo: leitura de `server/src/main.rs` (522 linhas), `db.rs` (342), `proto/src/lib.rs`, `app/src/{gateway,net,viewer,main}.rs`, `app/Cargo.toml`, `ROADMAP.md` e do relatório 07. Nada foi compilado nem executado. Linhas = estado atual.

## 1. Veredito geral

- O núcleo está sólido para o tamanho do projeto: SQL 100% parametrizado, nenhum lock `std::sync::Mutex` atravessa um `.await`, a ordem `db -> conns -> rooms` é respeitada em todo o código (não achei deadlock), `Send`/`History`/streams checam `can_access` e a checagem de `Send` é atômica com o INSERT (mesmo guard do `db`). Falha de SQL em checagem de acesso é "fail closed" (`unwrap_or(false)`).
- O relatório 07 está **majoritariamente correto**: 17 dos 19 itens confirmados, 2 ajustados, nenhum refutado. As linhas citadas ainda batem, com pequenos desvios.
- **Não expor fora do localhost** antes de: TLS, rate limit com semáforo no Argon2, tokens com hash/expiração/logout e teto de memória no vídeo. Com cadastro por convite e grupo pequeno de confiança, o risco residual cai bastante, mas o roadmap tem 4 pontos que criariam problemas novos (seção 4).
- Achados novos mais graves: bypass da regra de DM via pedido de amizade pendente (N1), DoS de memória pelo vídeo (N2) e superfície do decoder openh264 / alocação sem limite no cliente (N3).

## 2. Os 19 riscos do relatório 07

| # | Veredito | Linha atual | Gravidade | Observação |
|---|---|---|---|---|
| 1 TLS | Confirmado | main.rs:171,186 | Crítica | Agravante: o cliente nem consegue `wss://`. `tungstenite = "0.30.0"` sem feature de TLS (app/Cargo.toml:11). |
| 2 Token na URL | Confirmado | main.rs:421-433, 476-483; net.rs:3 | Alta | O servidor não registra a URL (sem TraceLayer); o vazamento viria do proxy (Caddy loga a URI com query) e da rede sem TLS. |
| 3 Token eterno | Confirmado | db.rs:23-24, 76-87; main.rs:226,285 | Alta | Cliente "logout" só apaga o token local (main.rs:337), o servidor não revoga. Token tem 244 bits (2 UUID v4), não 256: irrelevante. |
| 4 Sem rate limit | Confirmado | main.rs:243-278 | Alta | Mínimo de senha é 6 *bytes* (`password.len()`), sem máximo. Tentativas ilimitadas também por várias conexões em paralelo. |
| 5 Argon2 síncrono | Confirmado, ajustado para Média-Alta | main.rs:256, 272 | Média-Alta | `handle` é função síncrona chamada dentro da task: trava a thread do runtime (19 MiB e dezenas de ms por hash). Com N logins simultâneos (N = núcleos) o relay de vídeo para. Alta enquanto `Login` for aberto. |
| 6 Enumeração | Confirmado | main.rs:269-277, 380-382, 262 | Baixa | `Register` também revela "nome em uso" (inerente). `FriendRequest` exige login. |
| 7 Convite fraco | Confirmado | db.rs:91; main.rs:314-327 | Média | 32 bits reais (o v4 não fixa bits nos 8 primeiros hex). Agravante novo: o convite vai no Snapshot a **todos** os membros (proto:22, main.rs:76); revogar/limitar só pelo dono exige esconder o campo. |
| 8 Sem moderação | Confirmado | db.rs:28-31; main.rs (ausente) | Alta | Nem sair do servidor nem fechar DM existem (ver N1). |
| 9 Sem bloqueio / DM por co-membro | Confirmado e **pior** | main.rs:400-413, 378-391; db.rs:331-341 | Média | Ver N1: `related_users` inclui pedidos de amizade pendentes. |
| 10 Sem revalidação em streams | Confirmado | main.rs:432, 482 | Média (futura) | Além disso, qualquer membro pode ocupar o slot de transmissão de um canal (main.rs:442) e ninguém consegue derrubá-lo. |
| 11 Sem limite/idle | Confirmado | main.rs:196-221 | Média | Detalhe: `socket.send().await` bloqueado num cliente que não lê impede o `select!` de ler, e o `unbounded_channel` cresce sem teto; a conexão nunca é derrubada (presença "online" fica presa). |
| 12 Amplificação de snapshot | Confirmado | main.rs:61-124, 126-156, 324 | Média (Alta com >100 membros) | Pior que descrito: `push_snapshot` monta o snapshot **antes** de saber se o destino está online (main.rs:110 vs 53-58) e entrar/sair da conexão (sem debounce) dispara snapshot para todos os relacionados. Um membro comum reconectando em loop gera O(M²) sob o lock global. |
| 13 Sem limites de criação | Confirmado | main.rs:303-367 | Média | Servidores, canais (só dono), mensagens/s, pedidos de amizade, streams simultâneos por usuário. |
| 14 Vídeo sem teto | Confirmado e **agravado** | main.rs:435, 445, 458-469, 485 | Média-Alta | Ver N2 (memória). O viewer aceita até 64 MiB do espectador (padrão axum), descartados após leitura. |
| 15 Unicode | Confirmado | main.rs:248-249; db.rs:21 | Baixa | Também sem filtro de controles/bidi em nome de canal, servidor e mensagem (main.rs:305, 329-335). |
| 16 Mutex/unwrap | Confirmado, ajustado para Baixa-Média | main.rs:50,54,69,122 etc. | Baixa-Média | Não achei caminho realista de pânico dentro de um lock (só rusqlite devolvendo `Result`, `uuid`, `String`), então o envenenamento é hipotético; porém **um** pânico futuro derruba todos os handlers. Trocar por `parking_lot` ou tratar `PoisonError` custa pouco. |
| 17 Privacidade | Confirmado | db.rs:40-44; main.rs:49-51 | Média | Arquivo `.db` criado com a umask padrão (provavelmente 0644/0664) contendo tokens em claro: aplicar 0600. |
| 18 Sem auditoria | Confirmado | main.rs:456,473 | Média | Também não há log de falhas de login (nada para fail2ban). |
| 19 Sem Origin | Confirmado | main.rs:192-194 | Baixa | Sem cookie, sem CSRF. Só vale se um dia houver cliente web. Mantém Baixa. |

## 3. Novos achados (por gravidade)

### Alta

**N1. Regra "só amigos ou co-membros" burlada por pedido de amizade pendente; DM não tem saída.**
`related_users` (db.rs:333) une `friends` **sem filtrar `accepted`**. Basta `FriendRequest` para o alvo entrar em `related_users(me)`, e `OpenDm` (main.rs:400-413) cria o canal e injeta a DM nos dois snapshots, sem aceite. Consequências: qualquer usuário que souber (ou adivinhar) um nome abre DM com qualquer outro; o alvo não tem como bloquear, recusar ou fechar a DM (`FriendRemove` apaga a linha, mas `dm_members` mantém a relação; `related_users` também usa `dm_members`). Também vaza presença do alvo para o solicitante e vice-versa (snapshot de `friends_of` traz `online` mesmo pendente). Correção: filtrar `accepted=1` em `related_users` para fins de DM e presença (separar "quem pode me ver" de "quem devo avisar"), exigir aceite ou co-membro, e criar `CloseDm`/bloqueio. Prioridade junto com moderação.

**N2. DoS de memória no relay de vídeo.**
`broadcast::channel(32)` (main.rs:445) guarda até 32 quadros de até 16 MiB (main.rs:435) = 512 MiB por sala, mais a leitura em memória de cada mensagem. Não há limite de salas por usuário nem global, e qualquer membro de qualquer servidor que ele mesmo crie (item 13) pode abrir várias salas. Correção: teto de quadro realista (ex.: 1-2 MiB, o IDR de 1080p cabe), buffer limitado em bytes (não em contagem), limite de taxa/bitrate por publisher (descartar/encerrar acima de X Mbps), um stream por usuário e teto global de salas, e `max_message_size` no `/watch` (ex.: 1 KiB, nada legítimo chega do espectador).

### Média

**N3. Cliente: superfície do decoder e alocação sem limite.** `viewer.rs:57-63` passa bytes do publisher (qualquer membro do canal) direto ao openh264 (código C) e faz `vec![0u8; w*h*4]` com dimensões vindas do fluxo: um SPS forjado com resolução enorme pede GB de RAM (PC de 15 GB) e a entrada hostil explora uma biblioteca nativa. Correção: validar a resolução (ex.: máx. 4096x2304) antes de alocar (descartar o quadro/encerrar), manter openh264 atualizado, decodificar numa thread/processo que possa falhar sem derrubar a UI e só assistir transmissões de gente que está em um canal que o usuário escolheu.

**N4. TLS no cliente não funciona do jeito atual.** Mesmo ligando a feature de TLS, `gateway.rs:44-46` e `viewer.rs:30-32` só definem `set_read_timeout` para `MaybeTlsStream::Plain`. Com TLS o `ws.read()` bloqueia indefinidamente: o laço do gateway deixa de enviar mensagens enfileiradas (main.rs de `gateway.rs:50-58` só roda entre leituras) e o viewer ignora `stop`. Precisa tratar a variante TLS (ex.: `get_ref().set_read_timeout` no stream interno) ou separar threads de leitura e escrita.

**N5. Convite visível a todos os membros** (proto:22): qualquer membro redistribui o convite; ver item 7.

**N6. Erros de SQL engolidos produzem estados inconsistentes** (sem transação):
- `add_member` ignorado (main.rs:321) mas o cliente recebe `ServerJoined` (mentira);
- `create_server` (db.rs:90-101) tem 3 INSERTs sem transação: falha no meio deixa servidor sem dono-membro;
- `open_dm` (db.rs:308-316) cria canal e depois dois membros sem transação (canal órfão);
- `participants()`/`channel_participants` com `unwrap_or_default()` (main.rs:361,165): mensagem gravada mas **não entregue** em silêncio;
- `Register` mapeia **qualquer** erro para "nome em uso" (main.rs:262), inclusive disco cheio.
Correção: `conn.transaction()` nesses três pontos, devolver erro real ao cliente e logar.

**N7. Squatting de transmissão.** Um membro abre `/publish` e fica parado: a sala existe para sempre (sem idle timeout nem kick) e ninguém mais transmite naquele canal; o snapshot anuncia "ao vivo" para todos (main.rs:454-455). Mitigar com timeout de "sem quadros" (ex.: 10 s) e, depois da Fase 3, derrubada pelo dono.

**N8. `JoinServer` e `FriendRequest` como spam amplificado:** cada ação empurra snapshot completo para o alvo/membros mesmo sem mudança relevante. Sem limite, um membro faz "pedir/remover amizade" em loop e força snapshots no alvo.

### Baixa

- **N9.** Token do cliente em texto no arquivo do eframe (`storage.set_string("token")`, main.rs:362): `app.ron` em `~/.local/share/...` com permissões padrão; o `ROADMAP` já prevê keyring, ok. Senha **não** é persistida (confirmado).
- **N10.** `Resume` falho limpa o token (main.rs:316) para qualquer erro, inclusive transitório: não é segurança, mas combinado com reconexão automática gera loop de Resume (precisa backoff).
- **N11.** `FriendAccept`/`FriendRemove` empurram snapshot para `user_id` arbitrário, mesmo sem relação (main.rs:394, 398): não vaza dados (o snapshot é do destinatário), mas permite notificação de spam a qualquer id. Só empurrar se o UPDATE/DELETE afetou linhas.
- **N12.** `CreateChannel` ignora o resultado (`let _`, main.rs:344) e aceita controles/bidi no nome; sem unicidade nem limite por servidor.
- **N13.** Mensagens do servidor em texto no `/publish` e `/watch` (ex.: "já há uma transmissão") revelam existência de transmissão apenas a quem já passou na autorização: sem vazamento.
- **N14.** `run_publisher` remove a sala por `channel` (main.rs:471): hoje correto, pois só um publisher por canal; ao introduzir reconexão/takeover, comparar o dono antes de remover (senão um publisher antigo derruba a sala nova).

### Refutado / verificado sem problema

- Deadlock: nenhum. `rooms` nunca é mantido junto com outro lock; guards de `db` caem no fim da instrução antes de `push_many`; `if app.db.lock()…` na condição de `if` não segura o guard no corpo (edição 2021).
- TOCTOU em `Send`/`History`: ok (mesmo guard). TOCTOU em streams (auth no upgrade, uso depois) é o item 10. `friend_request` é atômico numa chamada só.
- IDOR em `History`, `Send`, `CreateChannel` (dono), streams e Snapshot (`live` filtrado): ok. `FriendAccept` só aceita pedido recebido (db.rs:253-258).
- Recuperação de espectador lento: GOP/IDR de 2 s (gpu.rs:47-58; publisher.rs:146), então `Lagged → need_key` se recupera em ~2 s. O receio de "preso para sempre" não procede.
- Argon2 com salt aleatório automático: confirmado em password-hash 0.6.1 (`hash_password` gera salt).

## 4. Avaliação das correções do ROADMAP (Fase 0.2/0.3)

| Proposta | Suficiente? | Riscos/novos problemas |
|---|---|---|
| TLS via Caddy | Parcial | (a) Servidor ainda faz bind em `0.0.0.0:8080` por padrão: exigir `127.0.0.1` atrás do proxy e firewall. (b) **Rate limit por IP quebra atrás do proxy**: todos chegam como 127.0.0.1; é preciso ler `X-Forwarded-For` **só** de proxy confiável, senão o limite vira um balde único (DoS de todos) ou é burlável por cabeçalho forjado. (c) O Caddy registra URI com `?token=` no access log: o ticket de vídeo precisa existir antes. (d) N4 (timeouts com TLS) e a feature de TLS no tungstenite não estão no roadmap. (e) "Recusar `ws://` fora de localhost" deve parsear o host (rejeitar `127.0.0.1.exemplo.com`). |
| Rate limit em memória + Argon2 em `spawn_blocking` com semáforo | Boa ideia, com ressalvas | (a) `spawn_blocking` sem teto empilha até 512 threads x 19 MiB (~9,7 GiB): usar semáforo de 2-4 permissões com `try_acquire`/espera curta e responder "tente depois". (b) Lockout rígido por conta permite travar a conta da vítima: preferir atraso progressivo por (conta, IP) mais limite global. (c) Mapas em memória precisam de expiração/limite de tamanho (senão viram DoS) e IPv6 deve agrupar por /64. (d) Reinício zera contadores: aceitável. (e) `handle` é síncrono; a mudança exige tornar o login assíncrono sem bloquear a conexão (spawn + canal de resposta). (f) O hash falso deve ter os mesmos parâmetros e ser pré-calculado na subida. (g) Falta rate limit de **conexões** WebSocket e de mensagens por segundo na conexão geral. |
| Tokens: hash, expiração, `Logout`, revogar | Boa | (a) SHA-256 simples basta (244 bits de entropia); salt desnecessário. (b) `login()` reenvia o token no `Auth` em Resume (main.rs:233, 285): com hash no banco não dá para reenviar; só devolver token em login novo. (c) Revogar token **não derruba** WebSockets abertos (a conexão guarda só `uid`, main.rs:42): associar `session_id` à conexão e fechar ao revogar/expirar, ou revalidar periodicamente. (d) Expiração deslizante: gravar `last_used` só no Resume, não em cada ação. (e) Limite de sessões por usuário e faxina de expirados. (f) Sem dados reais, migrar descartando a tabela. |
| Ticket de vídeo (uso único) | Boa, com cuidado | (a) Emitir pelo gateway autenticado (`C2S::StreamTicket{channel,kind}`), TTL ~30 s, preso a (usuário, canal, publish/watch). (b) Consumo atômico (remover do mapa sob lock) e **reconferir `can_access` no consumo**. (c) Mapa com expiração/limite por usuário. (d) Alternativa melhor: autenticar no **primeiro frame** do WS (timeout 5 s), mantendo a URL limpa. (e) Para o item 10 é preciso um registro de espectadores/publisher por sala com sinal de cancelamento para o kick, o ticket sozinho não resolve. |
| Cadastro só por convite da instância | Boa, resolve o bootstrap | (a) Consumo do código atômico com o INSERT do usuário numa transação (`UPDATE … SET usos=usos+1 WHERE usos<max AND expira>agora AND NOT revogado`, exigir 1 linha). (b) Guardar só o hash do código; mostrar uma vez. (c) Checar o código **antes** do nome para não vazar "nome em uso" a quem não tem convite. (d) Não elimina brute force de `Login` nem DoS do Argon2: o rate limit continua obrigatório. (e) CLI abre o mesmo SQLite com o servidor rodando: usar WAL (já está) e `busy_timeout`. (f) Código viaja em claro até haver TLS. |
| Convites de servidor de 128 bits | Insuficiente sozinho | Continuam expostos a todos os membros no Snapshot (proto:22); sem esconder o campo, revogar/expirar não impede um membro de repassar. Esconder do não-dono (ou expor só por `GetInvite`). |
| Limites (conexões/IP, ping/idle, canal limitado, quadro de vídeo, servidores/canais) | Bom, falta incluir | Acrescentar: limite de salas, buffer em bytes, `max_message_size` do `/watch`, teto de snapshots/seg por usuário, debounce de presença (grace ~5-10 s), snapshot só para destinos online, e fila de saída limitada (derrubar o cliente lento em vez de crescer). |
| Nomes Unicode | Ok | Aplicar NFKC + proibir confusáveis ou restringir a ASCII; filtrar controles/bidi também em canais e servidores. |
| 0.3 Heartbeat/ACK, reconexão, `nonce` | Ok | Reconexão com backoff e limite de tentativas; `nonce` com LRU limitado por conexão/canal; `seq`+Resume com buffer limitado que **filtra por acesso atual** (não reenviar eventos de canais que o usuário perdeu). |
| 0.4 Token no keyring | Ok | Sem Secret Service (sem DBus) o fallback deve ser arquivo 0600 com aviso, não texto aberto silencioso. |
| Marco A ("pronto para a internet") | Faltam itens | Incluir antes do marco: N1 (DM/bloqueio/fechar DM), N2/N3 (limites de vídeo e decoder), sair do servidor/kick mínimo, permissões 0600 do `.db`, logs de segurança sem tokens, `cargo audit` na rotina. |

## 5. Ordem recomendada de correção

1. **Barato e sem dependência (S):** corrigir N1 (`accepted=1` em `related_users` para DM/presença; `CloseDm`), limites de vídeo (N2: quadro, buffer, salas, `/watch`), validação de resolução no cliente (N3), `chmod 600` do `.db`, transações em `create_server`/`open_dm` e erro real em `Register` (N6), `PoisonError` tratado.
2. **Autenticação (M):** Argon2 em `spawn_blocking` + semáforo + hash falso; rate limit por (conta, IP) e por conexão; tokens com hash, expiração, `Logout` que fecha sockets; cadastro por convite atômico.
3. **Transporte (M):** feature de TLS no cliente + correção dos timeouts (N4) + Caddy com bind em localhost, `X-Forwarded-For` confiável e **ticket/primeiro frame** no lugar do `?token=` (antes de ligar o proxy).
4. **Robustez:** fila de saída limitada e idle timeout/heartbeat, debounce de presença, snapshot só para online, esconder o convite de não-donos, convites de 128 bits com expiração.
5. **Moderação mínima (M):** sair do servidor, kick/ban, apagar mensagem, revalidação/derrubada em streams (registro de espectadores com cancelamento), bloqueio de usuário.
6. **Depois:** auditoria/logs, normalização de nomes, keyring, exportação/exclusão de dados, criptografia em repouso.

Confiança: **alta** nos achados de código (todos lidos linha a linha; N1, N2, N6 e o modelo de locks conferidos duas vezes); **média** nas estimativas de gravidade e de custo de recursos (19 MiB/hash e 512 MiB/sala vêm da configuração padrão e do código, não de medição); **baixa-média** em comportamentos do Caddy e do keyring, citados de conhecimento geral e não verificados aqui.

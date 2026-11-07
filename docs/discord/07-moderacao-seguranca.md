# 07 — Moderação, segurança e privacidade

Legenda: **[F]** fato verificado em fonte oficial ou no código lido nesta sessão; **[C]** conhecimento geral sobre o Discord, não reconfirmado (as páginas de suporte retornaram 403); **[I]** inferência/recomendação minha.

## 1. Visão geral

- O Discord separa moderação em camadas: permissões por cargo/canal (quem pode agir), ações manuais (timeout, expulsar, banir), automação (AutoMod, nível de verificação, filtro de mídia), rastreabilidade (log de auditoria) e denúncia à plataforma (Trust & Safety). [C]
- O Telacast hoje só tem a camada mais básica: autenticação e checagem de acesso ao canal. Não há papéis, ações de moderação, limites de taxa nem TLS.
- Para um servidor próprio (poucos usuários, comunidade conhecida), o maior retorno está em: TLS, rate limit, expiração/revogação de tokens e convites, e kick/ban pelo dono. AutoMod, denúncias e E2EE são fase posterior.

## 2. Funcionalidades do Discord

### 2.1 Ações de moderação
| Recurso | Regras | Status |
|---|---|---|
| Expulsar (kick) | `DELETE /guilds/{id}/members/{uid}`, permissão `KICK_MEMBERS`. A pessoa pode voltar com novo convite. | [F] |
| Banir | `PUT /guilds/{id}/bans/{uid}`, permissão `BAN_MEMBERS`; `delete_message_seconds` 0–604800 (até 7 dias) apaga mensagens recentes; ban em massa até 200 usuários (exige também `MANAGE_GUILD`); lista de bans até 1000 por página. | [F] |
| Timeout (silenciar) | `PATCH member` com `communication_disabled_until`, máx. 28 dias no futuro, `null` remove; permissão `MODERATE_MEMBERS`; retorna 403 se o alvo for admin ou dono. Na UI o silenciado não escreve, reage nem fala. | [F] a API; [C] o comportamento na UI |
| Prune | Remove inativos: `days` 1–30 (padrão 7); por padrão poupa quem tem cargo. | [F] |
| Hierarquia | Só se modera quem tem cargo mais baixo que o seu; o dono é intocável. | [C] |
| MFA para moderar | `mfa_level` ELEVATED exige 2FA do moderador para ações de moderação. | [F] |

### 2.2 Prevenção na entrada
- **Níveis de verificação** [F]: 0 nenhum; 1 e-mail verificado; 2 conta com mais de 5 min; 3 membro do servidor há mais de 10 min; 4 telefone verificado.
- **Filtro de conteúdo explícito** [F]: 0 desligado; 1 escaneia mídia de membros sem cargo; 2 escaneia todos.
- **Notificações padrão** [F]: todas as mensagens ou só menções (reduz estrago de spam com @everyone).
- **Raid/spam** [C]: alertas de raid (entrada em massa), filtro de DM de spam, pausa de convites, bloqueio de links suspeitos. Não consegui reconfirmar nas páginas oficiais (404/403).
- **Convites** [C]: têm expiração (30 min a nunca) e limite de usos, e podem ser revogados.

### 2.3 AutoMod [F] (docs.discord.com, resources/auto-moderation)
- Gatilhos (máx. de regras por servidor): palavras-chave (6), spam (1), preset de palavras (1), spam de menções (1), perfil de membro (1).
- Eventos: envio/edição de mensagem; edição de perfil.
- Ações: bloquear mensagem (com explicação de até 150 caracteres), enviar alerta a um canal de log, timeout (até 4 semanas; só para keyword e mention spam), bloquear interação do membro.
- Limites: até 1000 palavras (60 caracteres cada), até 10 regex (260 caracteres, sintaxe Rust), lista de exceções de 100 entradas; isenções por até 20 cargos e 50 canais.

### 2.4 Log de auditoria [F]
- Registra ações administrativas: cada entrada tem `action_type`, `user_id` (autor), `target_id`, `changes`, `options`, `reason`.
- Exemplos: `MEMBER_KICK` (20), `MEMBER_BAN_ADD` (22), `MEMBER_BAN_REMOVE` (23), `MEMBER_ROLE_UPDATE` (25), `MESSAGE_DELETE` (72), `MESSAGE_BULK_DELETE` (73).
- Retenção de **45 dias**; ver exige `VIEW_AUDIT_LOG`; o motivo (1–512 caracteres) vem no cabeçalho `X-Audit-Log-Reason`.

### 2.5 Denúncias, bloqueio, privacidade
- **Denúncia (report)** [C]: o usuário denuncia mensagem/usuário/servidor à equipe do Discord, com categoria e a mensagem como evidência; moderadores do servidor não veem a denúncia. Num clone próprio, o equivalente é a denúncia ao dono/admin.
- **Bloquear usuário** [C]: esconde mensagens do bloqueado, impede DM e pedido de amizade. Ação local entre dois usuários.
- **Privacidade de DM/servidor** [C]: opção "permitir DMs de membros do servidor", quem pode mandar pedido de amizade (todos, amigos de amigos, membros de servidores).
- **Dados/LGPD** [F, política de privacidade]: pedido de cópia dos dados em até 30 dias, em JSON e arquivos originais; exclusão da conta remove dados identificáveis e anonimiza o restante, podendo reter parte por obrigação legal; a política não dá prazos concretos de retenção e cita LGPD só de forma genérica. Direitos GDPR: acesso, retificação, apagamento, restrição, portabilidade, oposição.

### 2.6 Rate limits [F] (docs.discord.com, topics/rate-limits)
- Limite global de 50 req/s por bot; limites por rota agrupados por `X-RateLimit-Bucket`.
- Cabeçalhos `X-RateLimit-Limit/Remaining/Reset/Reset-After/Bucket`; resposta 429 com `retry_after` e `X-RateLimit-Scope` (user, global, shared).
- Requisições 401/403/429 contam como inválidas; acima de **10.000 em 10 min por IP** vem banimento temporário (Cloudflare).

### 2.7 Criptografia em chamadas [F] (docs.discord.com, topics/voice-connections)
- Sinalização por WebSocket seguro (`wss://`).
- Áudio no transporte: libsodium, modo obrigatório `aead_xchacha20_poly1305_rtpsize`, preferido `aead_aes256_gcm_rtpsize`; cabeçalho RTP não é cifrado.
- **E2EE (DAVE)**: grupo MLS (RFC 9420), gateway como "external sender"; chaves por remetente derivadas de segredo exportado de 16 bytes; cada frame Opus cifrado com AES-128-GCM (tag truncada para 8 bytes); chaves de assinatura verificáveis fora de banda. O Discord exige E2EE em DMs, chamadas de grupo, canais de voz e Go Live a partir de 1º/mar/2026. A cifra de transporte continua por cima.
- Na prática, o servidor (SFU) ainda vê metadados (quem está na chamada, tamanho/tempo dos pacotes). [I]

## 3. Auditoria do servidor Telacast (código lido: `server/src/main.rs`, `db.rs`)

Todos os itens abaixo são **[F]** quanto ao que o código faz; a gravidade é **[I]**.

| # | Risco | Onde | Gravidade |
|---|---|---|---|
| 1 | **Sem TLS**: bind em `0.0.0.0:8080`, senhas (`Register`/`Login`) e tokens trafegam em claro por `ws://`. Quem estiver na rede captura sessão. | main.rs:171, 186 | Crítica |
| 2 | **Token de transmissão na URL** (`?token=`): vaza em logs de proxy, histórico e referer. Em `ws://` é visível a qualquer um na rota. | main.rs:421-433, 476-483 | Alta |
| 3 | **Tokens nunca expiram nem são revogáveis**: sem coluna de data, sem logout, sem "sair de todas as sessões"; cada `Login` cria mais um token e a tabela cresce sem limite. Token guardado em texto puro (vazamento do `.db` = todas as sessões). | db.rs:23-24, 76-87; main.rs:226 | Alta |
| 4 | **Sem rate limit/lockout no login**: força bruta de senha ilimitada; `Register` aberto permite criar contas em massa. Senha mínima de 6 caracteres, sem lista de senhas fracas. | main.rs:243-278 | Alta |
| 5 | **Argon2 síncrono dentro do handler async** (bloqueia a thread do runtime, ~dezenas de ms e muita memória por hash). Vários logins/registros simultâneos travam o servidor inteiro (DoS barato). Sem limite máximo de tamanho de senha (limitado só por 64 KiB da mensagem). | main.rs:256, 272 | Alta |
| 6 | **Enumeração de usuários por tempo**: se o usuário não existe, nenhum hash é calculado (resposta rápida). Também `FriendRequest` responde "Usuário não encontrado". | main.rs:269-277, 380-382 | Baixa |
| 7 | **Convite fraco e eterno**: 8 hex do UUID (32 bits), sem expiração, sem limite de uso, sem revogação; `JoinServer` sem rate limit permite adivinhar. Sem ban, então não há como impedir a volta. | db.rs:91; main.rs:314-327 | Média |
| 8 | **Sem kick/ban/sair do servidor/timeout/apagar mensagem/editar mensagem**: o dono não tem como remover abusador; `members` não tem papel. Spam e assédio são irremediáveis. | db.rs:28-31; main.rs (ausente) | Alta |
| 9 | **Sem bloqueio de usuário**: qualquer co-membro de servidor abre DM (`OpenDm` aceita `related_users`) e envia pedidos de amizade ilimitados. | main.rs:400-413, 378-391 | Média |
| 10 | **Revalidação de acesso inexistente em transmissões**: `can_access` só é checado no upgrade; quem perder acesso continua publicando/assistindo até desconectar. Hoje é teórico (não há kick), vira bug ao implementar moderação. Também qualquer membro pode publicar, sem permissão específica. | main.rs:432, 482; run_publisher/run_viewer | Média (futura) |
| 11 | **Sem limite de conexões, sem ping/timeout de ociosidade**: sockets sem autenticação ficam abertos para sempre (slowloris); `unbounded_channel` por conexão cresce sem teto com cliente lento (memória). | main.rs:196-221 | Média |
| 12 | **Amplificação por snapshot**: cada mensagem de amizade, entrada ou presença chama `push_many` e remonta o snapshot completo de cada relacionado, sob o lock global do banco. Um atacante com contas falsas em um servidor grande gera carga O(membros²). | main.rs:61-124, 324 | Média |
| 13 | **Sem limite de criação**: servidores por usuário, canais por servidor, membros, mensagens por segundo (`Send` só limita 4000 caracteres). Vazamento de disco por spam. | main.rs:303-367 | Média |
| 14 | **Vídeo sem teto de banda/tempo**: quadros de até 16 MiB, sem taxa máxima, sem validação do conteúdo binário (só `data[0]`, `len>=2`); o viewer `watch` não define `max_message_size` (padrão do axum, 64 MiB de entrada do espectador ignorada, mas lida). | main.rs:435, 458-469, 485 | Média |
| 15 | **Nomes Unicode**: `is_alphanumeric` aceita letras de outros alfabetos; permite imitar nomes (homóglifos, ex. "admin" com letra cirílica). `COLLATE NOCASE` do SQLite só normaliza ASCII. | main.rs:248-249; db.rs:21 | Baixa |
| 16 | **Mutex global do SQLite** com chamadas síncronas em contexto async, e `unwrap()` em locks: um pânico envenena o lock e derruba o servidor para todos. | main.rs:41-45 e vários `.lock().unwrap()` | Média |
| 17 | **Privacidade/LGPD**: nada de exportar dados, excluir conta ou apagar mensagens; mensagens guardadas para sempre em texto puro no `.db` sem criptografia em repouso; sem política de retenção; presença (online) vazada para qualquer co-membro de servidor, sem opção de invisível. | db.rs:40-44; main.rs:49-51, 121-124 | Média |
| 18 | **Sem log de auditoria nem log de segurança** (só `println!` de início/fim de transmissão). Impossível investigar abuso. | main.rs:456, 473 | Média |
| 19 | **Sem checagem de `Origin`** no upgrade do WebSocket. Baixo risco porque a autenticação é por token em mensagem, não cookie, mas clientes web hostis podem sondar. | main.rs:192-194 | Baixa |

Pontos positivos [F]: SQL sempre parametrizado (sem injeção SQL no que li); argon2 com salt (via `phc`); token de 256 bits aleatórios (dois UUID v4); `Send`/`History`/streams verificam acesso ao canal; limite de 64 KiB por mensagem de chat e 4000 caracteres por texto; erro de login não distingue usuário inexistente da senha errada na mensagem (só no tempo, ver item 6).

## 4. Prioridades

| Pri. | Item | Esforço | Motivo |
|---|---|---|---|
| P0 | TLS (`wss://`, via proxy reverso Caddy/nginx ou `axum-server` com rustls) e fechar bind quando atrás de proxy | S/M | Item 1; senhas e tokens hoje em claro. Cliente deve recusar `ws://` fora de localhost [I] |
| P0 | Rate limit por IP e por conta: login/registro (ex. 5 tentativas/min), `Send`, `JoinServer`, pedidos de amizade; Argon2 em `spawn_blocking` com semáforo | M | Itens 4, 5, 7, 11, 13; barato com `tower_governor` ou contador em memória |
| P0 | Tokens: guardar só hash (SHA-256), coluna `expires`/`last_used`, `Logout`, revogar tudo ao trocar senha; mover token de URL para primeiro frame ou ticket de uso único de 30 s | M | Itens 2 e 3 |
| P1 | Kick, ban (tabela `bans`, checada em `JoinServer`), sair do servidor, apagar mensagem própria e pelo dono; revalidar acesso em transmissões ao expulsar | M | Itens 8 e 10; é o mínimo de moderação |
| P1 | Convites: 128 bits (ou 10+ caracteres base62), expiração e limite de usos, revogar/regenerar | S | Item 7 |
| P1 | Cadastro por convite/código ou aprovação do dono (modo "fechado"), com chave opcional no servidor | S | Item 4; evita contas em massa e é nosso "nível de verificação" [I] |
| P1 | Limites por usuário e global: servidores, canais, membros, tamanho do `.db`; teto de conexões por IP; ping/idle timeout; trocar `unbounded_channel` por canal limitado | M | Itens 11, 13 |
| P2 | Bloquear usuário (esconde mensagens, impede DM e amizade) e opção "DM só de amigos" | M | Item 9 |
| P2 | Timeout (silenciar) com `muted_until` por servidor e papéis simples (dono, moderador, membro) | M | Equivalente ao `MODERATE_MEMBERS` [F]; depende de papéis |
| P2 | Log de auditoria (tabela `audit(server, actor, action, target, reason, ts)`, retenção ~45 dias como o Discord) | S/M | Item 18 |
| P2 | Denúncia de mensagem ao dono/moderador (guarda cópia da mensagem como evidência) | S/M | Sem equipe central, o dono é o árbitro [I] |
| P2 | Direitos de dados: exportar JSON, excluir conta (anonimizar autor das mensagens), política de retenção documentada, presença invisível | M | Item 17; LGPD pede acesso, correção, eliminação e informação clara [C] |
| P2 | Hash de senha em `spawn_blocking`, hash fantasma no login de usuário inexistente, normalizar/restringir nomes (ASCII ou NFKC + confusáveis) | S | Itens 5, 6, 15 |
| P3 | AutoMod simples: lista de palavras por servidor, limite de menções, anti-flood (N msgs/10 s), ação bloquear/timeout | M/L | Só vale com comunidade grande |
| P3 | Filtro de mídia/validação de upload (se vierem anexos: tipo por magic bytes, tamanho máx., armazenar fora do webroot, nome aleatório, `Content-Disposition: attachment`) | M | Hoje não há upload; o risco entra com anexos [I] |
| P3 | Criptografia em repouso (SQLCipher ou disco cifrado), backup com cifra | M | Item 17 |
| P3 | E2EE de chamadas/transmissão (MLS ou chave compartilhada simples) | L | O Discord usa DAVE/MLS [F]; para nós, TLS + SFU confiável basta por enquanto. Alternativa barata: chave por sala trocada via mensagem cifrada e AES-GCM por quadro [I] |
| P3 | Verificação de e-mail/2FA (TOTP) | L | Exige e-mail ou app; só se abrir ao público |

### Modelo de ameaças resumido [I]
- Atacantes prováveis: espião de rede/Wi-Fi (resolvido por TLS), colega/membro abusivo (kick/ban/bloqueio), bot de registro e força bruta (rate limit + cadastro fechado), vazamento do arquivo `.db` (hash de token, criptografia em repouso), cliente malicioso enviando quadros enormes ou pacotes malformados (limites e validação).
- Fora de escopo agora: ataques em nível de nação, malware no cliente, analisar conteúdo de mídia.

## 5. Fontes

- https://docs.discord.com/developers/resources/guild (verificação, filtro, ban, kick, prune, timeout) [F]
- https://docs.discord.com/developers/resources/auto-moderation [F]
- https://docs.discord.com/developers/resources/audit-log [F]
- https://docs.discord.com/developers/topics/rate-limits [F]
- https://docs.discord.com/developers/topics/voice-connections (DAVE, MLS, criptografia) [F]
- https://discord.com/privacy [F]
- https://discord.com/blog (título "Every Voice and Video Call on Discord Is Now End-to-End Encrypted"; corpo não carregou) [parcial]
- Não acessíveis nesta sessão: support.discord.com (403) e discord.com/safety (404). Itens marcados [C] vêm de conhecimento geral e devem ser reconferidos antes de virar requisito.
- Código do Telacast: `/home/guilherme/telacast/server/src/main.rs`, `/home/guilherme/telacast/server/src/db.rs` (somente leitura).

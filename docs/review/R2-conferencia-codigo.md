# R2 - Conferência do ROADMAP contra o código

Método: leitura de server/src/{main,db}.rs, proto/src/lib.rs, app/src/*.rs, Cargo.toml/Cargo.lock e dos relatórios 01-10. Nada foi compilado nem executado. Arquivos: S = server/src/main.rs, D = server/src/db.rs, P = proto/src/lib.rs, A = app/src/main.rs, G = gateway.rs, PUB = publisher.rs, V = viewer.rs.

## Veredito geral

A tabela "Onde estamos" está **substancialmente correta** (nenhuma afirmação de "existe" é falsa), com três ressalvas: o número "231 fps" não se sustenta no código, "não lido em negrito" é só memória do cliente, e "fallback openh264" quase nunca é exercitado. Os **maiores erros do roadmap são de omissão**: (1) o cliente **não consegue** falar `wss://` hoje e o jeito de ligar TLS quebra os timeouts de leitura; (2) uma queda do servidor **derruba a sessão e apaga o token salvo**, o que é pior que "sem reconexão"; (3) o relay de vídeo entrega P-frames sem referência a quem entra no meio do GOP. Há também duplicação leve (DM já é canal com tabela própria; `before` do histórico já existe; abas de amigos e a limpeza de "recusar/cancelar" já existem). Confiança: média-alta (leitura completa, sem execução).

## Tabela "Onde estamos" e lacunas declaradas

| Afirmação | Veredito | Evidência |
|---|---|---|
| Contas: cadastro/login argon2 | Verdadeiro | S:243-278 (`Argon2::default`), D:54-68 |
| Token de sessão | Verdadeiro, mas sem expiração/logout, e cada Login cria token novo | D:76-81, S:226 (`token.or_else(create_token)`) |
| Sessão salva no app | Verdadeiro (em texto puro, arquivo 664) | A:359-365, A:263-268; ~/.local/share/telacast/app.ron contém `token` |
| Servidores: criar, convite por código, entrar | Verdadeiro; convite = 8 hex (32 bits), eterno | D:90-101, S:303-327 |
| Canais de texto, só o dono cria | Verdadeiro | S:340-343, A:611 |
| Membros online/offline | Verdadeiro (conta conexões, não heartbeat) | S:49-51, S:126-156, A:699-719 |
| Chat em tempo real | Verdadeiro | S:350-367, A:198-207 |
| Histórico das últimas 50 | Verdadeiro (`LIMIT 50`) | D:210-229 |
| Agrupamento por autor | Verdadeiro (mesmo autor, <5 min) | A:844-846 |
| Não lido em negrito | **Parcial**: só `HashSet` em memória, perde-se ao reconectar/reiniciar; mensagens recebidas offline não marcam nada | A:88, A:204-206, A:621-625 |
| Amigos pedir/aceitar/remover | Verdadeiro (recusar/cancelar = `FriendRemove`; pedido recíproco vira amizade) | S:378-399, D:233-267, A:972-994 |
| DMs 1:1 | Verdadeiro; permitido a qualquer co-membro de servidor, não só amigos | S:400-413 |
| Presença online/offline | Verdadeiro, mas sem heartbeat: "online" falso até o TCP morrer | S:196-221 (sem ping/timeout) |
| Vídeo: captura X11 | Verdadeiro, **só X11 root** (`get_image`); sob Wayland captura só XWayland/preto | PUB:100-102 |
| NVENC | Verdadeiro (P1, CBR, ULTRA_LOW_LATENCY, sem B) | gpu.rs:39-67 |
| "231 fps medidos em 1080p" | **Não verificável / enganoso**: R04 diz "até 144 fps"; o `--bench` repete **um único quadro estático** (`frame.clone()`) e mede só o encode, sem captura/rede | PUB:176-211, A:20-25; R04 linha 98 |
| Relay por WebSocket | Verdadeiro | S:426-522 |
| Fallback openh264 | **Parcial**: só entra se `InitError` (CUDA/NVENC ausente); 1080p@60+ em CPU não é realista; init do openh264 vaza thread (ver bug B7) | PUB:139-165 |
| Interface: tema escuro, barra, canais, membros, login, modais | Verdadeiro | theme.rs:1-23, A:561-722, A:1067-1187 |
| Lacuna: sem áudio | Verdadeiro | Cargo.toml (sem cpal/opus) |
| Lacuna: sem TLS | Verdadeiro **e pior**: nem o cliente suporta (ver B1) | app/Cargo.toml:10; Cargo.lock sem rustls/native-tls |
| Lacuna: sem cargos | Verdadeiro | D:20-48 (sem tabelas) |
| Lacuna: sem editar/apagar mensagem | Verdadeiro | P:74-89 |
| Lacuna: sem reconexão | Verdadeiro **e subestimado** (ver B2) | A:372-374, A:335-342 |

## Itens do roadmap que dizem "falta"

| Item | Veredito | Evidência / nota |
|---|---|---|
| 0.1 snowflake "no lugar do AUTOINCREMENT" | Verdadeiro, mas impreciso: não há AUTOINCREMENT, são `INTEGER PRIMARY KEY` e o código usa `last_insert_rowid()` em 5 pontos (create_user, create_server, create_channel, insert_message, open_dm) | D:55-59, 96, 140, 200, 310 |
| 0.1 display_name | Falta, e é mais que S: `Message.author` é nome **desnormalizado** no protocolo e `name_of` varre o snapshot | P:64-72, D:203, A:171-184 |
| 0.1 "DM vira canal com `channel_members`" | **Duplicado em parte**: DM já é canal (`server_id NULL`) com tabela `dm_members`; falta só generalizar/renomear e `kind`/`parent_id` | D:32-39, D:295-318 |
| 0.1 tabelas previstas (`invites`...) | Falta; `servers.invite` existe como coluna | D:25-27 |
| 0.2 rate limit / Argon2 em spawn_blocking | Falta (hash síncrono no handler async, sob lock) | S:256, S:272 |
| 0.2 limites | **Parcial**: já há WS 64 KiB (S:193), mensagem 4000 chars (S:352), nome de servidor 50 / canal 32 / usuário 3-32 (S:305,335,248), quadro de vídeo 16 MiB (S:435). Faltam: espectador sem teto (S:485), conexões, `unbounded_channel` (S:197), servidores/canais por usuário | |
| 0.2 homóglifos | Falta; `is_alphanumeric` aceita qualquer alfabeto e `COLLATE NOCASE` só dobra ASCII | S:249, D:21 |
| 0.3 heartbeat, nonce, ErrorCode, PROTO_VERSION, seq/Resume | Todos faltam (`Error{msg:String}`, sem versão) | P:91-101 |
| 0.3 testes automáticos | Falta: zero `#[test]`/`cfg(test)` no workspace; o "script manual" não está versionado | grep em */src |
| 0.4 token no keyring | Falta (texto puro) | A:362 |
| 0.4 "diretórios XDG" | **Parcial/duplicado**: eframe já grava em ~/.local/share/telacast/app.ron | |
| 0.4 log em arquivo, panic hook, instância única | Faltam (só `println!`/`eprintln!`) | S:186,456,473; A:22 |
| Fase 1 paginação (`before`) | **Parcial/duplicado**: `before` já existe no proto, no handler e no SQL; o cliente só manda `None`. Faltam UI, `after`, `around` | P:83, S:368-377, D:210-216, A:136 |
| Fase 1 base de UI "tokens de tema" | **Parcial**: constantes de cor e helpers já existem | theme.rs:3-23, 103-138 |
| Fase 1 Esc/clique fora nos modais | Falta (modais são `egui::Window`; Esc só sai da tela cheia) | A:500, A:1073 |
| Fase 4 "zoom" (P1) | Provável duplicata: egui/eframe já tem Ctrl +/- por padrão (a confirmar no 0.37) | R10 linha 105 |
| Fase 4 abas Bloqueados/busca em amigos | Correto: Online/Todos/Pendentes/Adicionar já existem | A:894-897 |
| Fase 4 tela de Configurações | Falta; fps/kbps só no modal "Ao vivo" | A:1141-1182 |
| Fase 2 "presets 720p30..." tratados como S | **Subestimado**: não há escalonamento; o encoder recebe a resolução nativa do monitor (`w*h`) | PUB:87, gpu.rs:61 |

## Relatórios (seções "já tem vs falta")

- **R01**: correto. Único reparo: "Token de sessão" sem expiração é fato, mas "Presença online/offline" omite que é por conexão, sem heartbeat.
- **R02**: correto ("informado pelo briefing"); confere com S:303-349. Não menciona que `JoinServer` ignora erro de `add_member` (S:321).
- **R03**: correto, incluindo o aviso de que `before` já existe (P:83). Ressalva: dizer que "agrupamento" e "histórico" existem é verdade, mas o cliente nunca pagina.
- **R04**: "até 144 fps em 1080p (teste)" é o limite do seletor (A:1159), não uma medição de ponta a ponta. Não registra a limitação X11/Wayland.
- **R05**: **desatualizado**: marca "recusar/cancelar (confirmar)" e "abas Online/Todos/Pendentes" como falta, mas existem (A:894-897, 982-993). Só Bloqueados/busca faltam. DM não lida em negrito existe (A:652-656).
- **R06**: correto ("sem heartbeat/reconexão/rate limit, ids sequenciais"). Sugere `seq` envelope S; subestima o impacto no Snapshot.
- **R07**: as linhas citadas ainda batem (main.rs:171,186; 243-278; 256,272; 400-413; 196-221). Item #14: o `watch` sem `max_message_size` é só entrada e irrelevante; o ponto real é o limite de 16 MiB do publicador.
- **R10**: "Ja existe" confere (cartão, Sair, abas, tela cheia). "Menus de contexto: provável falta" = confirmado falta (sem `context_menu`).

## Bugs e lacunas que o roadmap não cobre

| # | Gravidade | Descrição | Evidência |
|---|---|---|---|
| B1 | **Alta** | Cliente não fala `wss://`: tungstenite sem feature TLS (nenhum rustls/native-tls no lock). Pior: ao ligar TLS, o `set_read_timeout` só é aplicado em `MaybeTlsStream::Plain`; com TLS o `ws.read()` bloqueia sem timeout, então o gateway **não envia mensagens pendentes** enquanto não chegar algo, e o viewer **não vê `stop`**. A tarefa "TLS (rustls)" precisa incluir esses 3 pontos | app/Cargo.toml:10, G:44, V:43, PUB:88 |
| B2 | **Alta** | Servidor caiu = `Event::Closed` -> `logout()` que faz `token_saved.clear()` e `stop_publish/stop_watch`. O usuário perde a transmissão, a sessão e o token salvo (tem de digitar a senha de novo), embora o token ainda seja válido no banco. "Reconexão automática" deve **manter** o token e a view | A:187-190, A:335-342, A:372-374 |
| B3 | **Alta** | Espectador que entra no meio do GOP recebe o último IDR em cache e depois os P-frames **atuais**, sem os intermediários: imagem corrompida até o próximo IDR (até `2*fps` quadros, ~2 s). O comentário "novos espectadores começam por ele" descreve o defeito como feature. Correção: não enviar o cache (esperar o próximo IDR) ou cachear o GOP inteiro, ou pedir IDR ao publicador | S:491-502 |
| B4 | Média | `Send` falha em silêncio: >4000 chars, canal sem acesso ou erro de SQL retornam sem `Error`; o cliente já esvaziou o campo (`mem::take`) e a mensagem é perdida sem aviso. Também não há limite no cliente. Reforça o item `nonce`/ack | S:350-362, A:882 |
| B5 | Média | Campo de mensagem é `singleline`: não há quebra de linha, então markdown de bloco/citação (Fase 1) é inviável sem trocar para multilinha (Shift+Enter) | A:877 |
| B6 | Média | Evolução de esquema/protocolo frágil: o banco usa só `CREATE TABLE IF NOT EXISTS` (sem `user_version`/migrações), então `display_name`, `kind`, `parent_id` exigem `ALTER`; e o `telacast.db` do dev já existe. No protocolo, JSON que não parseia é **descartado em silêncio** nos dois lados (S:209, G:62): um campo novo sem `#[serde(default)]` num `Snapshot` faz o cliente antigo ignorar o snapshot inteiro e congelar. "variante Unknown" não basta: exigir `#[serde(default)]` nos structs | D:18-49, S:209, G:62 |
| B7 | Média | Vazamento de thread na captura: o `?` em `Encoder::with_api_config(...)?` (fallback) sai de `run()` antes de `cap_stop.store(true)`; a thread de captura segue em loop infinito chamando `get_image` (o `try_send` falha ignorado) | PUB:148 vs PUB:168 |
| B8 | Média | `connect()` de tungstenite é bloqueante e sem timeout em gateway, publisher e viewer: IP inalcançável deixa "Conectando…" por minutos, botão desabilitado (`busy`), sem cancelar; `stop` não é checado antes do `connect` | G:43, PUB:88, V:41, A:394 |
| B9 | Média | Sem heartbeat nem idle timeout: conexão meia-aberta mantém "online" e a entrada em `conns`; `unbounded_channel` cresce com cliente lento | S:196-221 |
| B10 | Média | Mutex global do SQLite (std) usado dentro de handlers async, incluindo hash Argon2 sob o fluxo; qualquer `unwrap()` envenenado derruba tudo. Snapshot completo por mudança de presença: O(relacionados x tamanho do snapshot) com `can_access` por sala ao vivo | S:41-45, S:61-124 |
| B11 | Média | Qualquer membro com acesso ao canal publica; o servidor não valida o conteúdo do quadro (só `data[0]`, `len>=2`) e o flag `key` vem do cliente: um publicador malicioso trava todos os espectadores em `need_key` | S:458-469 |
| B12 | Baixa | Quando o publicador sai, o espectador mantém `watching` e o último quadro congelado; o texto de erro só aparece se não houver textura, e o selo continua "AO VIVO" | A:1020-1054, V:48-75 |
| B13 | Baixa | `Logout` não revoga o token no servidor (não há `C2S::Logout`) e cada `Login` cria novo token, sem limpeza | P:75-89, S:226 |
| B14 | Baixa | Toast "Pedido de amizade enviado" é mostrado antes da resposta; se der erro ("Usuário não encontrado") aparecem os dois | A:916-919 |
| B15 | Baixa | Lista de mensagens não é virtualizada e `msgs` cresce sem teto; vira problema ao implementar paginação e anexos | A:843-869 |
| B16 | Baixa | `password.len() < 6` conta bytes, não caracteres; monitor só é listado na abertura do app (sem hot-plug); `monitor`/janela não são persistidos | S:253, A:275, A:359-365 |
| B17 | Baixa | Ficheiro `app.ron` com token tem permissão 664 (legível pelo grupo) | ls -la |
| B18 | Baixa | Captura 4K/ultrawide: `get_image` copia o root inteiro por quadro (33 MB em 4K); não há teste do limite. O teto de 16 MiB por quadro de vídeo protege pouco | PUB:100, S:435 |

## Correções sugeridas ao ROADMAP

1. Tabela "Onde estamos": trocar "231 fps medidos" por "NVENC 1080p medido só no encode com quadro estático (`--bench`); fps ponta a ponta não medido". Em "Chat", escrever "não lido em negrito (apenas em memória)". Em "Vídeo", acrescentar "somente X11".
2. 0.2 TLS: incluir "habilitar feature TLS do tungstenite nos 3 clientes (gateway, publisher, viewer) e aplicar timeout de leitura também no ramo TLS (B1)"; manter "recusar ws:// fora de localhost".
3. 0.3 Reconexão: especificar "mantém token, view e mensagens; não faz logout; para publicador/espectador, reabre ou avisa" (B2). Adicionar timeout de `connect` e cancelamento (B8).
4. 0.3 Protocolo: acrescentar "`#[serde(default)]` em todo campo novo e **erro visível** em vez de descartar JSON inválido" (B6); acrescentar "ack de `Send` com `ErrorCode`" (B4).
5. 0.1: acrescentar "migrações com `PRAGMA user_version`" antes de display_name/kind/parent_id; reescrever o item da DM como "generalizar `dm_members` + `channels.kind`" (não é criação do zero); reconhecer que display_name impacta `Message.author`.
6. Fase 1: "Paginação" passa a "UI de `before` (servidor e proto prontos) + `after`/`around`"; incluir "campo multilinha (Shift+Enter)" como pré-requisito do Markdown; incluir virtualização da lista.
7. Fase 2: reclassificar "presets de qualidade" de S para M (precisa escalonar antes do NVENC); acrescentar o bug B3 e B7 como pré-requisito de qualquer trabalho em vídeo; mencionar que Wayland não é suportado.
8. Fase 4: remover "abas Bloqueados/busca" como se fossem todas as abas (só Bloqueados falta); remover/validar "zoom".
9. Adicionar à Fase 0 uma linha "Robustez do vídeo": validar quadro e `key` no servidor, limitar espectadores, entrar apenas em IDR (B3, B11).
10. Atualizar R05 (abas e recusar/cancelar já existem) e R04 (fps).

## Testes que eu faria (sem rodar agora)

- Subir servidor e cliente; `kill -STOP` no servidor por 60 s e depois `kill -9`: confirmar B2 (token apagado em app.ron após o autosave de 30 s) e B9.
- Entrar como espectador 1 s após um IDR com movimento intenso: confirmar B3 por artefatos.
- Enviar 5000 caracteres: confirmar B4.
- Apontar o servidor para 10.255.255.1: confirmar B8.
- Teste de unidade futuro de `compute_permissions` e do gerador snowflake, como o roadmap já pede.

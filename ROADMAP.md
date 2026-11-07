# Telacast — Roadmap

Clone próprio do Discord em Rust: cliente egui, servidor axum + PostgreSQL 18, vídeo H.264 por NVENC.
Não usa a API nem os servidores do Discord. Este roadmap sai de 10 relatórios de pesquisa em
[`docs/discord/`](docs/discord/) (01 a 10), lidos e consolidados em 2026-11-07.

## Como ler

- **Esforço relativo** (não é prazo): **S** pequeno (cabe numa sessão), **M** médio (servidor + cliente), **L** grande (várias sessões ou muda o modelo de dados).
- **[R07]** = relatório 07, e assim por diante. Itens com **[minha decisão]** são ordenação ou sugestão minha, não do relatório.
- **Atenção às fontes:** vários relatórios foram escritos de memória, porque as páginas de suporte do Discord devolveram 403/404 e o cliente instalado só foi listado. O que é *número do Discord* (limites, níveis, atalhos, hexadecimais) está na seção "A confirmar" e **não deve virar requisito sem checar**. As decisões de produto abaixo não dependem desses números.

## Onde estamos

Atualizado em 2026-11-07 (marcas: `[x]` pronto, `[~]` parcial, `[ ]` pendente).

| Área | Já existe |
|---|---|
| Contas | cadastro **só com convite**, login (argon2 fora da thread principal, hash falso para usuário inexistente), token só com hash e validade deslizante de 30 dias, `Logout`, **trocar senha** e **lista/encerramento de sessões**, limite de tentativas, sessão salva no app |
| Servidores | criar, convite por código (128 bits), entrar; **categorias recolhíveis** e canais de texto (só o dono cria), membros online/offline |
| Chat | tempo real; **responder** (clicar na resposta salta até a original), **editar** ("(editado)"), **apagar** (autor ou dono), **não lido guardado no servidor** (`Ack`), **"digitando…"**, agrupamento, divisor de dia, hora ao passar o mouse, texto selecionável, menu do botão direito. Histórico: 50 por página, carrega mais ao rolar para cima |
| Social | amigos (pedir/aceitar/remover), DMs 1:1, presença com **status manual** (online, ausente, não perturbe, invisível) e **ausente automático** |
| Vídeo | transmissão de tela por canal: captura X11, NVENC, relay por WebSocket, fallback openh264 |
| Interface | tema **Grafite** (camadas de cinza, âmbar, serifa), barra de servidores com pílula 8/20/40 e sigla, cabeçalho do servidor com menu, canais com categorias, painel de membros, login, modais, barra de ações no hover |
| Infra | Postgres 18, ids snowflake `i64`, migrações embutidas, **heartbeat e reconexão automática**, testes unitários e **14 e2e**, TLS (wss, guia em `docs/DEPLOY.md`, ainda não testado numa VPS de verdade) |

Lacunas grandes: **sem áudio, sem cargos/permissões, sem moderação, sem tópicos/threads, sem notificações.** (Anexos, busca, reações, fixadas e bloqueio já existem.)

## Decisões já tomadas

| Tema | Decisão | Observação |
|---|---|---|
| Interface desktop | **egui** (descartados Tauri e iced) | Teste comparativo em `~/telacast-spike`: desempenho empatado (144 fps, ~38% de CPU, ~230 MB). O iced não tem texto selecionável nem acessibilidade; o egui tem os dois |
| Direção visual | **Grafite**: camadas de cinza, **um acento âmbar**, linhas finas, títulos em serifa, só tema escuro | A direção "Glass/Aurora" foi testada e **rejeitada** (parece interface gerada por IA). Maquete: `docs/ui-preview/grafite.html` |
| Fontes | **a escolher** (seletor na maquete; candidatas em `app/assets/fonts/`) | Ainda sem resposta |
| Banco | **PostgreSQL 18**, acesso por `sqlx`, migrações embutidas | Em desenvolvimento roda no Docker (`docker-compose.dev.yml`). O SQLite foi abandonado |
| Cache | **em memória** (`moka`), atrás de uma interface própria; Redis só se houver mais de um servidor | A confirmar |
| IDs | snowflake em `i64` | Tabelas já usam `BIGINT` |
| Cadastro | **fechado, só com convite** da instância (`telacast-server invite`) | O código aparece uma vez; no banco fica o hash |
| Tokens | opacos, só o hash no banco, com validade (30 dias deslizantes), `Logout`, troca de senha (encerra as outras sessões) e lista/encerramento de sessões (menu do usuário → "Minha conta") | ticket de uso único no vídeo |
| Hospedagem | **VPS Oracle, Ampere A1 (ARM), 2 núcleos e 12 GB**; compilar na própria VPS | Local por enquanto (Marco A ainda não chegou) |
| Plataformas | Desktop agora; **Android só depois de terminar o desktop** (Fases 0 a 4; ver "Plano Android") | O protocolo JSON/WebSocket precisa ficar estável e documentado |
| Git | repositório criado; commits sem linha de coautoria | |
| Medições | PC do desenvolvimento: Ryzen 7 5700X (16 threads), RTX 5060 Ti, 15 GB de RAM (apertada) | O gargalo do PC é memória, não CPU |

## Mapa de dependências

```
Fase 0 (fundação) ──► Fase 1 (chat) ──► Fase 2 (voz)
        │                  │
        │                  └──► Fase 3 (cargos e moderação) ──► Fase 4 (social/UX/sistema)
        └──► [Marco A: pode expor na internet]                    └──► Fase 5 (extensibilidade)
```

Cargos (Fase 3) bloqueiam moderação, canais privados e papéis de voz/palco. Voz (Fase 2) **não** depende de cargos,
então dá para trocar as fases 2 e 3 se você quiser falar antes de moderar (ver "Decisões em aberto").

---

## Fase 0 — Fundação: segurança, robustez e modelo de dados

Pré-requisito para tudo. Os itens de segurança são obrigatórios **antes de expor o servidor fora do localhost** ([R07]).

### 0.1 Decidir o modelo de dados agora (ainda não há dados reais, migrar depois custa caro)
- [x] **IDs estilo snowflake** (ms | nó | seq, gerado no servidor) em todas as tabelas, **mantendo `i64`** (cabe em 63 bits; string no JSON só se um dia houver cliente web) — [R06] P2, **[minha decisão]: fazer já**, porque sem dados reais não há migração. Fica pequeno: um gerador no servidor no lugar do `AUTOINCREMENT`. **S**
- [x] **`display_name` separado do `username`** — [R01] P0, base de identidade. **S**
- [x] `channels` ganha `kind` (texto/voz/anúncio/palco) e `parent_id` (categorias); DM vira canal com tabela `channel_members` (prepara DM em grupo) — [R02][R05]. **M**
- [~] Tabelas já previstas (feitas: `read_state`, `blocks`, convites de instância e de servidor; **faltam** `roles`, `member_roles`, `channel_overwrites`, `bans`, `invites`, `audit` — criar o esquema junto, usar nas fases seguintes. **S**

### 0.2 Segurança (auditoria do servidor, 19 riscos em [R07])
- [x] **TLS (`wss://`)**: cliente com rustls (raízes públicas + `TELACAST_CA_FILE` para CA própria), recusa `ws://` fora da máquina e da rede local (login, retomada de sessão e vídeo), erro de certificado aparece na tela; guia com Caddy, systemd e backup em `docs/DEPLOY.md`. Testado contra proxy TLS local; **falta testar na VPS da Oracle**. P0 **M**
- [x] **Rate limit** por IP e por conta (login/cadastro, `Send`, `JoinServer`, pedidos de amizade) e **Argon2 em `spawn_blocking`** com semáforo + hash falso para usuário inexistente. P0 **M**
- [~] **Tokens** (feito: só o hash, expiração, `Logout`, trocar senha revogando as outras sessões, lista de sessões com origem (IP) e encerrar uma ou todas, aviso `SessionExpired` às conexões abertas da sessão encerrada; **falta** o ticket de uso único no vídeo e derrubar de fato a conexão revogada): guardar só o hash, expiração, `Logout`, revogar ao trocar senha, lista de sessões; **tirar o token da URL do vídeo** (ticket de uso único). P0 **M** ([R07]+[R01])
- [x] **Cadastro fechado — decidido: só entra quem tem convite** (por enquanto). O `Register` passa a exigir um **código de convite da instância** (128 bits, com expiração e limite de usos, revogável), gerado pelo operador do servidor por linha de comando (`telacast-server invite`), o que também resolve a criação da primeira conta. Os convites de *servidor* (guild) também passam a 128 bits. P1 **S**
- [x] **Limites**: fila limitada, tamanho máximo de mensagem, **30 conexões por IP** (`TELACAST_MAX_CONNS_PER_IP`; a excedente recebe 429), heartbeat/idle de 60 s, **20 servidores por usuário e 200 canais por servidor**. conexões por IP, ping/idle timeout, canal limitado no lugar de `unbounded_channel`, tamanho máximo de quadro de vídeo, limites de servidores/canais por usuário. P1 **M**
- [ ] Nomes de usuário: restringir Unicode (homóglifos). P2 **S**

### 0.3 Protocolo ([R06])
- [x] **Heartbeat** (ping do servidor a cada 20 s, derruba conexão muda há 60 s; o cliente também detecta conexão morta, inclusive após suspender o PC). Falta o ACK por mensagem. P0 **S**
- [x] **Reconexão automática** com espera 1, 2, 4, 8 e 15 s, faixa "Sem conexão… Nova tentativa em N s" com botão "Tentar agora", retoma a sessão e recarrega o canal aberto; o campo de mensagem não envia enquanto está fora. P0 **S**
- [x] **`nonce`** no envio (o servidor devolve o `nonce` e descarta repetição), **mensagem otimista** (aparece apagada com "Enviando…", fica na fila se a conexão cair e é reenviada ao reconectar; "Não foi enviada" com "Tentar de novo"/"Descartar" após 15 s). P0 **S**
- [x] **`ErrorCode` estáveis** (11 códigos, `Unknown` para código novo; o app só descarta a sessão salva com `SessionExpired`/`NotAuthenticated`/`VersionMismatch`). Mensagens desconhecidas (enum externo) o cliente já ignora. P0 **S**
- [x] **`PROTO_VERSION`** (=1) em `Register`/`Login`/`Resume`; versão diferente, ou ausente (cliente antigo), recebe `VersionMismatch`. P1 **S**
- [ ] **`seq` nos eventos + Resume** com buffer; re-sync por `after`. P1 **M**
- [x] Testes automáticos do servidor (hoje só o script manual que usei) — **[minha decisão]** **S/M**

### 0.4 Cliente ([R09])
- [ ] **Token no keyring** do sistema (hoje está em texto no arquivo do app), diretórios XDG, log em arquivo, panic hook. P0 **S**
- [x] **Instância única** (`app/src/single.rs`: trava de arquivo `instance.lock` na pasta de dados + socket abstrato; a segunda abertura chama a primeira para a frente e sai). P0 **S**

**Marco A — pode expor na internet** quando 0.2 e 0.3 (P0/P1) estiverem prontos. Aí vale o plano da VPS da Oracle (Caddy + systemd + DuckDNS + backup do `.db`).

---

## Fase 1 — Chat completo

- [~] **Eventos incrementais** (feitos: `MessageEdited`, `MessageDeleted`, `Typing`; o resto ainda vem por `Snapshot`) no lugar do Snapshot completo (`MessageUpdate/Delete`, `ChannelCreate…`, `MemberAdd…`, `PresenceUpdate`), começando por presença e mensagens; manter `RequestSnapshot` como válvula. P2 do [R06], mas **[minha decisão]: já aqui**, porque editar/apagar precisam de eventos e o Snapshot atual amplifica carga ([R07] #12). **L**
- [x] **Editar e excluir** as próprias mensagens, com "(editado)". P0 **S** ([R03])
- [~] **Paginação ao rolar** (`before`): feita no app (chega perto do topo e busca 50 mensagens mais antigas, mantendo a rolagem no lugar) e o salto até uma resposta antiga busca páginas (até 500 mensagens) até achar. **Falta** `after`/`around` e rolar para baixo depois de um salto. P0 **S**
- [x] **Não lidos de verdade**: `read_state` + `Ack`, barra e pílula, **contagem de menções** (`<@id>`, `@everyone`, `@here`; selo âmbar no canal, no servidor e nas DMs; zera ao ler) e **divisor "NOVAS MENSAGENS"** ao abrir o canal (rola até ele; Esc dispensa): `last_read` no servidor, contagem de menções, divisor "novas mensagens". P0 **M** ([R03][R05][R10])
- [x] **Markdown** (crate `telacast-md`, 14 testes; renderizado em `app/src/md_view.rs`): negrito, itálico, sublinhado, tachado, código em linha e em bloco, citação (`>` e `>>>`), listas (aninhadas e numeradas), títulos 1–3, subtexto, spoiler (clique revela), link (`[texto](url)` e solto), menções `<@id>`/`<#id>`/`@everyone` e carimbo `<t:unix:estilo>` em português. **Faltam** realce de sintaxe nos blocos, emoji colorido e o composer trocar `@nome` por `<@id>`. P0 **M–L**
- [x] **Base de UI**: tokens, menu de contexto, barra de ações no hover, confirmação ao apagar e **modal padrão** (`theme::modal` sobre `egui::Modal`: bloqueia a camada de baixo, Esc, clique fora e X fecham; Ctrl+K usa o mesmo): tokens de tema, modal padronizado (Esc/clique fora/confirmação destrutiva), **menus de contexto**, **barra de ações ao passar o mouse**. P0 **M** ([R10])
- [~] **Menções**: destaque e contagem prontos; o campo de mensagem troca `@nome` (nome exato de quem está no canal) por `<@id>` ao enviar. **Falta** o autocomplete (popup ao digitar `@`) e `#canal`. P1 **M**
- [x] **Respostas** com prévia e pular para a mensagem. P1 **M**
- [x] **"Digitando…"**, **fixadas** (dono do servidor, ou qualquer pessoa numa DM; máx. 50 por canal; botão "Fixadas" abre o painel; marca "Fixada" na mensagem) e **mensagens de sistema** ("fulano entrou…" com 6 variações e "fulano fixou uma mensagem…"). P1 **S** cada
- [x] **Reações** com emoji Unicode: chips sob a mensagem (contagem, destaque se você reagiu, clique alterna), botão "Reagir" na barra de ações e no menu do botão direito, seletor com 48 emojis, evento em tempo real, contagem agregada no histórico e na busca, limite de 20 emojis por mensagem. **Falta** a dica com os nomes de quem reagiu. P1 **M**
- [~] **Busca de mensagens** (Ctrl+F): texto completo do Postgres em português (índice GIN, radicais: "jogar" acha "jogo"), escopo servidor ou canal, filtros `de:nome`/`de:eu` e `em:#canal`, 25 por página com "Mostrar mais", clicar no resultado abre o canal e salta até a mensagem (busca páginas antigas). **Faltam** `tem:`/datas/`fixadas:`, destaque dos termos e ordenar. P1 **M**
- [~] **Anexos**: upload por HTTP (`POST /upload`, até 10 MiB, `TELACAST_MAX_UPLOAD_MB`), arquivo em disco endereçado pelo hash (`TELACAST_FILES_DIR`), tipo detectado pelo conteúdo (só PNG/JPEG/GIF/WebP aparecem inline), dimensões lidas, download protegido (`GET /files/{id}/{nome}`, só quem lê o canal), mensagem só com anexos, até 10 por mensagem, botão "+" (kdialog/zenity) e arrastar-e-soltar, imagem inline até 400×300 (clique abre a prévia ampliada dentro do app, com "Baixar" e "Fechar"; só baixa se você pedir), cartão com "Baixar" para outros tipos. **Faltam** colar imagem da área de transferência, mosaico de várias imagens, spoiler de imagem, limpeza de envios órfãos, miniatura no servidor e GIF animado. P1 **L** ([R03][R06][R09])
- [ ] Prévia de link (cuidado com SSRF), emoji Unicode no picker. P2 **M**

## Fase 2 — Voz (maior lacuna do produto)

Proposta do [R04]: começar simples, trocar o transporte depois.

- [ ] **Canal de voz**: modelo, entrar/sair, lista de participantes, `voice_state` no servidor. P0 **M**
- [ ] **Áudio Opus pelo WebSocket atual** (`cpal` + Opus + jitter buffer + mixagem), com o mesmo formato de quadro que um UDP futuro usaria. P0 **M**
- [ ] **Mudo, ensurdecer, indicador de fala** (VAD por energia). P0 **S**
- [ ] **Volume por usuário**, seleção de dispositivos, push-to-talk dentro da janela. P1 **S**
- [ ] **Áudio do sistema na transmissão de tela** (monitor do PipeWire/Pulse → Opus como 2º fluxo). P1 **M**
- [ ] **Presets de qualidade** (720p30, 1080p60, fonte) mapeados nos parâmetros do NVENC. P1 **S**
- [ ] P2: **UDP com fallback para WS** (**L**), supressão de ruído `nnnoiseless` e eco (**M**), câmera V4L2 (**M**), canal de palco (**M**)
- [ ] P3: `webrtc-rs`, E2EE (MLS ou chave por sala), limites por plano, atalho global no Wayland

## Fase 3 — Cargos, permissões e moderação

Prioridade máxima do [R02]; moderação do [R07].

- [ ] **Cargos + `@everyone` + bitfield u64 + `compute_permissions(member, canal)`** único e **com testes**. P0 **L**
- [ ] **Hierarquia** e regra "só concede o que tem" (sem isso é falha de segurança). P0 **M**
- [~] **Categorias** (prontas) e **canais privados** (overrides por canal/categoria). P0 **M**
- [ ] **Sair, editar e excluir servidor**, transferir propriedade. P0 **S**
- [ ] **Kick e ban** (tabela `bans` checada no `JoinServer`), lista de banidos, **revalidar acesso nas transmissões ao expulsar**. P1 **M** ([R07] #8, #10)
- [ ] **Convites avançados**: expiração, usos máximos, revogar, listar. P1 **M**
- [ ] **UI**: editor de cargos (permissões tri-estado), atribuir cargos, lista de membros por cargo e cor, overrides por canal, filtro de membros por `VIEW_CHANNEL`. P1 **L**
- [ ] **Timeout**, **log de auditoria** (retenção ~45 dias), **denúncia ao dono**, **slow mode**, apelidos. P2 **M**
- [ ] P3: AutoMod simples (palavras, anti-flood), verificação de e-mail, filtro de mídia (só quando houver anexos de terceiros)

## Fase 4 — Social, perfil, interface e sistema

**Social ([R05])**
- [~] P0: **status manual** (online/ausente/não perturbe/invisível: feito, salvo na conta, menu no cartão do usuário, bolinha com forma própria) e **ausente automático** (10 min sem entrada, conta suspensão do PC; feito); **bloquear usuário feito** (some a amizade; ninguém abre DM nem manda mensagem privada ao outro; o bloqueado não descobre; mensagens dele em servidores ficam escondidas com "Mostrar mensagem"; aba "Bloqueados" no Início; menu do botão direito em membros e amigos). **Falta** o não perturbe silenciar notificações (ainda não há notificações). **S / M**
- [ ] P1: silenciar servidor/canal/DM, **notificação desktop + som** respeitando DND, abas Bloqueados/busca em amigos. **S–M**
- [ ] P2: **DM em grupo**, status personalizado, níveis de notificação, privacidade (quem me adiciona/manda DM), ignorar. **S–L**
- [ ] P3: chamadas em DM (depende da Fase 2), atividade/rich presence, sugestões de amigos

**Perfil ([R01])**
- [ ] P0: **upload de avatar** (usa o armazenamento da Fase 1), lista de sessões (feita na 0.2, "Minha conta"). **M**
- [ ] P1: bio, pronomes, banner, cor de destaque e **cartão de perfil**; troca de senha; recuperação de senha por e-mail (exige SMTP). **S–M**
- [ ] P2: **2FA TOTP**, apelido por servidor, **exportar dados e excluir conta** (LGPD), nota privada. **S–M**

**Interface ([R10])**
- [ ] P0: **tela de Configurações** (Minha conta, Aparência, Notificações, Voz, Sair). **M**
- [~] P1: **Ctrl+K (Quick Switcher)** feito (crate `telacast-qs`, 6 testes: prefixos `@ # *`, nota 10/7/5/3/1 × reforço de uso, setas/Enter/Esc, menções e não lidos, canais anteriores); faltam popout de perfil, **temas claro e AMOLED**, zoom e tamanho de fonte, atalhos básicos. **S–M**
- [ ] P2: busca com filtros na UI, modo compacto, pastas de servidores e arrastar para reordenar, onboarding e estados vazios, acessibilidade (AccessKit, alto contraste). **S–L**
- [ ] P3: i18n, atalhos remapeáveis, "modo desenvolvedor".

**Sistema ([R09])**
- [ ] P1: **bandeja + fechar para a bandeja**, **notificações nativas**, **atalhos globais no X11** (mutar, PTT), arrastar-e-soltar. **S–M**
- [ ] P2: detecção de jogo por `/proc` ("Jogando X"), autostart, **deep link `telacast://`** para convites, aviso de nova versão, ausente automático por suspensão. **S–M**
- [ ] P3: RPC local `telacast-ipc`, janela de overlay flutuante, auto-update assinado, atalhos no Wayland. **M–L**
- Arquitetura: manter captura + NVENC em thread/processo auxiliar, para uma falha do driver não derrubar a UI ([R09]).

## Fase 5 — Extensibilidade ([R08])

- [ ] **Webhooks de entrada** (+ embeds simples) — P1 **S**, retorno imediato (CI, scripts, RSS)
- [ ] **Contas de bot + token** (mostrado uma vez, só o hash no banco), API documentada e crate cliente `telacast-bot` — P1 **M**
- [ ] **Emoji customizado por servidor**, Rich Presence simples — P2 **M / S**
- [ ] **Slash commands** (picker `/`, interações com prazo de 3 s) — P2 **L**
- [ ] Botões, menus e modais — P3 **L**

---

## Achados dos pareceres (`docs/review/`) que entram no plano

**Reorganização da Fase 0 (R1).** Ela está inchada para um primeiro marco: dividir em **0a** (segurança + protocolo + migrações,
leva ao Marco A) e **0b** (modelo de dados e polimento). O **runner de migrações** e os **testes automáticos** vêm primeiro
(etapa 1, em andamento). `seq`/Resume depende dos eventos incrementais (Fase 1): fazer os dois juntos para não refazer.
Kick/ban do dono **não dependem de cargos** e podem vir antes da Fase 3. A tela de Configurações precisa existir antes da voz.
Faltam no plano: backup do banco, observabilidade (`tracing`) e deploy.

**Bugs no código atual que o roadmap não cobria (R2).** O app apaga o token e faz logout quando o servidor cai (deveria
reconectar); o espectador que entra no meio pode receber quadros sem referência; `Send` falha em silêncio; a thread de captura
vaza ao parar; "não lido" existe só em memória. O cliente ainda não fala `wss://` (o `tungstenite` está sem TLS e o timeout de
leitura só vale para conexão sem TLS). O número de 231 fps do NVENC veio de um teste com quadro parado e **não vale**.

**Segurança nova (R3).** Uma amizade pendente já libera DM, e a DM não tem como ser fechada; o decoder do cliente não limita a
resolução; o relay de vídeo podia usar até 512 MiB por sala (**já reduzido para 64 MiB** na etapa 1b). Pontos a cuidar nas
correções: limite de taxa por IP atrás do Caddy (**tratado**: lê `X-Forwarded-For` só de conexão local), teto de `spawn_blocking`
(**tratado**: semáforo), revogar token sem derrubar sockets abertos, o Caddy logar `?token=` antes do ticket de vídeo existir,
e o convite de servidor ainda visível a todos os membros.

**Viabilidade técnica (R5).** O crate `opus` compila a libopus sozinho (cmake e gcc já instalados): decisão do Opus resolvida.
Descartar `webrtc-audio-processing` (pede meson e ninja), `single-instance` (parado desde 2021) e `audiopus_sys`. Bandeja com
`ksni` (Rust puro). Mídia de voz numa conexão WebSocket própria, separada do vídeo; trocar para UDP depois é realista se o formato
do quadro for fixado já na versão WebSocket. Maior risco do egui: lista de altura variável e texto rico com elementos inline.

**Fatos do Discord (R4).** Cerca de 60 itens conferidos; cinco números estavam errados (voz base 96 kbps, não 64; exclusão de
conta 15 dias, não 14; upload de 20 MB sem Nitro; Comunidade não exige 2FA de moderadores; Stage com vídeo 50/150/300).
Nenhum número errado entrou neste roadmap.

**Decodificação na placa (NVDEC) rebaixada:** com um Ryzen 7 5700X, decodificar 1080p a 60 fps por software cabe num núcleo.
Fica relevante só para 4K ou 144 fps.

## Plano Android (só depois do desktop)

**Ordem decidida (2026-11-07):** o Android **só começa quando o desktop estiver terminado**, isto é, Fases 0 a 4 concluídas (a Fase 5, extensibilidade, é opcional e não bloqueia). Até lá, nada de código Android.

**Plano detalhado:** [`docs/MOBILE-PLAN.md`](docs/MOBILE-PLAN.md) (stack, passos A1–D3, protocolo de execução sem supervisão, riscos). Resumo:
- App nativo **Kotlin + Jetpack Compose** com o tema Grafite; mesmo protocolo JSON/WebSocket do desktop, garantido por arquivos de exemplo (`proto/fixtures/`) lidos pelo Rust e pelo Kotlin; telas conferidas por captura (Paparazzi), sem aparelho.
- Vídeo por `MediaCodec` (assistir) e `MediaProjection` (transmitir); notificações locais primeiro, push (FCM) só depois do servidor público.
- Egui no Android descartado (sem campo de texto nativo nem acessibilidade; tudo de vídeo e notificação viraria JNI).

**Exigências para o servidor** (entram nas Fases 0 a 4 do desktop, para o mobile não pedir retrabalho): TLS (`wss://`, o Android bloqueia `ws://` fora de debug), protocolo **versionado** e estável, token por dispositivo, histórico paginado por id, e push (Firebase ou serviço próprio) quando houver servidor público.

**Ambiente (já existe):** JDK 21, Android SDK 34, Gradle 8.9 em `~/Android/gradle-8.9` e a receita de build com pouca memória do projeto do widget. **NDK não é necessário** (Kotlin puro). Celular de teste: Redmi Note 9S (`f9b56d3c`); não tocar na tela sem pedir.

**Já feito de preparação (nada disso é código do app):**
- Plano e stack escritos (`docs/MOBILE-PLAN.md`).
- APK do Discord Android 352.17 puxado do celular para `~/discord-mobile/apk/` (fora do repositório; **não commitar**). Achado: o app é **React Native com JavaScript em bytecode Hermes versão 98** (`assets/index.android.bundle`, 70 MB), então a interface não sai legível por decompilação comum. O que dá para extrair: textos (JSON de internacionalização em `res/raw`), recursos e cores (`resources.arsc`), layouts nativos (172), classes Kotlin (3 `.dex`) e bibliotecas nativas. Análise com o REA fica para essa fase; o REA pede um JAR do JADX que ainda não está instalado.
- Para o desktop, os 10 documentos de `docs/ui-spec/` (extraídos do cliente web) servem de referência de comportamento; no mobile, o que muda é o layout (gaveta, folhas inferiores, toque longo), não as regras.

## Fora do escopo (decisão: não fazer por enquanto)

- **Usar a API, a conta ou os servidores do Discord**, incluindo logar com conta de usuário de terceiros (viola os Termos e arrisca o banimento de quem usar). Decidimos fazer o nosso.
- Nitro/boost/monetização, descoberta pública, parceiro/verificado, insights.
- Activities, Social SDK, App Directory, integrações Twitch/YouTube/Spotify, OAuth2 de terceiros.
- Passkeys/WebAuthn, MFA por SMS, conexões externas, badges do Discord.
- Sharding, formato ETF, opcodes numéricos (mantemos enums Rust tipados).
- Fundo virtual, NDI, clips, overlay injetado em jogos, E2EE por MLS (TLS + servidor confiável bastam por ora).

## Decisões em aberto (precisam de você)

1. **Ordem:** voz antes de cargos (como está) ou cargos antes? Voz não depende de cargos.
2. **Opus no Linux:** o crate `opus` exige a `libopus` do sistema (`sudo apt install`). Você prefere Rust "puro" sem pacotes do sistema; alternativa é `audiopus_sys` compilando a partir da fonte (precisa de cmake). Qual caminho?
3. **Anexos:** onde guardar (pasta local do servidor, hash de conteúdo) e qual limite de tamanho.
4. ~~**Cadastro:**~~ **decidido:** fechado, só com convite da instância (por enquanto).
5. ~~**Snowflake agora?**~~ **decidido:** sim, mantendo `i64`.

## A confirmar antes de virar requisito (vieram de memória, não de documentação lida)

Limites e números do Discord: tamanho de bio e de status, quantidade de códigos de backup, prazo de exclusão de conta
([R01]); níveis de boost, limites de emoji e requisitos de Comunidade ([R02]); tamanho máximo de mensagem e de upload ([R03]);
limites de Stage, tabela Nitro/boost, detalhes de Krisp e VAD ([R04]); regras de bloqueio, privacidade e notificações ([R05]);
limites de taxa e tamanhos do gateway ([R06]); denúncias, raids e níveis de verificação ([R07]); todo o conteúdo de [R08]
(a documentação não foi baixada); hexadecimais, larguras e atalhos de [R10]. Não afetam as decisões de produto acima, só
os números de implementação.

## Observações sobre os relatórios

- [R07] lista 19 riscos do nosso servidor, com arquivo e linha. As linhas podem ter mudado desde a leitura; conferir ao corrigir.
- [R05] e [R03] checaram o que já temos por `grep` em `proto/src/lib.rs`, sem revisar o app inteiro; a tabela "Onde estamos" acima foi conferida contra o que construímos.
- [R09] e [R04] usaram só nomes de símbolos e strings do cliente do Discord; nada foi decompilado.

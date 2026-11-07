# R4 - Verificação de fatos do Discord (relatórios R01-R10 e ROADMAP)

Data da checagem: 2026-11-07. Método: só páginas públicas. Documentação oficial (docs.discord.com/developers) via WebFetch; suporte via navegador headless (`fetch_page`), que carregou várias páginas que deram 403 antes. Sem login, token ou API do Discord.
Legenda: **OK** = confirmado em página oficial lida agora; **CORRIGIDO** = o relatório diz outra coisa; **N/V** = não consegui fonte oficial (só fóruns/terceiros ou nada).
Abreviações de URL: DEV = https://docs.discord.com/developers ; SUP = https://support.discord.com/hc/en-us/articles .

## 1. Correções (o relatório está errado ou incompleto)

| Afirmação | Relatório | Veredito | Valor correto | URL |
|---|---|---|---|---|
| Bitrate de canal de voz sem boost = 64 kbps | R04 l.41 | **CORRIGIDO** | API: servidor normal até 96 kbps; boost 1 = 128, 2 = 256, 3 (ou VIP_REGIONS) = 384. 64 kbps é o teto de canal de Stage | DEV/resources/guild |
| Prazo de exclusão de conta 14 dias | R01 l.128 | **CORRIGIDO** | Suporte oficial: "exclusão leva 15 dias"; conta fica "pendente" 15 dias. (A página de retenção fala em 15-30 dias de espera e até 45 dias para limpar backups) | SUP/212500837 e SUP/5431812448791 |
| Requisitos de Comunidade incluem 2FA para moderadores | R02 l.85 | **CORRIGIDO** | Oficial: verificação >= e-mail verificado, filtro de mídia explícita "todos os membros", canal de regras, canal de atualizações para moderadores, cumprir Diretrizes. 2FA de mod NÃO consta. Descoberta (Discovery): hoje "mais de 1000 membros" ou parceiro/verificado | SUP/360047132851 |
| Limite de upload "20 MiB base; doc de mensagens cita 25 MiB" | R03 l.56 | **parcialmente CORRIGIDO** | Sem Nitro: 20 MB (FAQ de anexos, e há experimento com limites maiores). Nitro Basic 50 MB, Nitro 1 GB. Servidor: boost 2 = 50 MB, boost 3 = 100 MB para todos. 25 MiB é o teto de tamanho da *requisição* na doc da API; a doc não dá limite por arquivo nem número de anexos | SUP/25444343291031 ; https://discord.com/nitro ; SUP/360028038352 ; DEV/resources/message |
| Display name "1-32 caracteres, qualquer caractere" e username "alterável 2x por semana" | R01 l.17,22 | display name **OK**; "2x por semana" **N/V** | Display name 1-32 confirmado. Frequência de troca de username não aparece na parte lida da página; não citar | SUP/12620128861463 |
| Limite de "voz/vídeo 50 pessoas" em Stage | R04 (Stage) | **CORRIGIDO/complementado** | Stage só áudio: palco sem limite, plateia até 10.000. Stage com vídeo/Go Live: plateia 50 (grátis), 150 (boost 2), 300 (boost 3), +30 por boost acima de 14 (máx. 10.000); até 5 câmeras e 1 Go Live ao mesmo tempo | SUP/1500005513722 |

## 2. Confirmados

### Gateway e REST (R06)
| Afirmação | Relatório | Veredito | Valor | URL |
|---|---|---|---|---|
| 120 comandos/60 s por conexão; excesso desconecta (close 4008) | R06 l.78 | OK (4008 consta como "rate limited") | 120/60 s. Mais: payload enviado <= 4096 bytes (senão close 4002); Identify total 1000/24 h; 1 Identify por `max_concurrency` a cada 5 s | DEV/events/gateway ; DEV/topics/opcodes-and-status-codes |
| Heartbeat: jitter, ACK op 11, conexão zumbi => fechar com código != 1000/1001 e fazer Resume | R06 l.43 | OK | 1º heartbeat em `interval * jitter(0-1)`; enviar `s` ou null; op 1 pede heartbeat imediato | DEV/events/gateway |
| Resume com `resume_gateway_url`, Invalid Session d=true => Resume, d=false => Identify | R06 l.49 | OK | idem. "Esperar 1-5 s aleatório" **N/V** na leitura de hoje (a página não traz o número) | DEV/events/gateway |
| Close codes 4004 não reconecta; 4007, 4008, 4009 reconectam; 4010-4014 não | R06 l.51 | OK | Tabela completa: 4000-4003, 4005, 4007-4009 reconectam; 4004, 4010-4014 não. Não existe 4006 | DEV/topics/opcodes-and-status-codes |
| Shard = `(guild_id >> 22) % num_shards`; obrigatório com 2500+ guilds | R06 l.60 | OK | idem; eventos sem guild_id vão ao shard 0 | DEV/events/gateway |
| `zlib-stream` (sufixo 0000FFFF) e `zstd-stream` | R06 l.63 | OK | idem | DEV/events/gateway |
| Limite global 50 req/s; 429 com `retry_after`/`global`; ban por IP | R06 l.77 | OK | 50/s por bot. Ban de IP (Cloudflare) com 10.000 requisições inválidas (401/403/429) em 10 min | DEV/topics/rate-limits |
| Histórico: `limit` 1-100 (padrão 50), `before/after/around` | R03/R06 | OK | idem | DEV/resources/message |
| `nonce` até 25 caracteres | R03/R06 | OK | 25 | DEV/resources/message |
| Snowflake: `(id >> 22) + 1420070400000` | R03 l.101 | OK (epoch 2015-01-01) | Não revi o texto de snowflake hoje; consistente com a doc (`>> 22` aparece na fórmula de shard) | DEV/reference |

### Mensagens (R03)
| Afirmação | Veredito | Valor | URL |
|---|---|---|---|
| Mensagem 2000 caracteres | OK | 2000 (API). Nitro: 4000 | DEV/resources/message ; https://discord.com/nitro |
| Embeds: 10/mensagem; título 256; descrição 4096; 25 fields; nome 256; valor 1024; footer 2048; autor 256; soma 6000 | OK | tudo igual | DEV/resources/message |
| Alt text de anexo até 1024 | OK | 1024 | idem |
| `allowed_mentions` listas até 100 | OK | 100 users / 100 roles; `parse` não combina com as listas | idem |
| Bulk delete 2-100 IDs, nada com mais de 2 semanas | OK | idem (duplicados também dão 400) | idem |
| Listar pins: 50 por página | OK | `limit` 1-50. **Total de pins por canal: a doc não diz** (R03 já marcou 50 como [INF]; continua N/V) | idem |
| Busca: `limit` 1-25, `offset` até 9975 | OK | idem | idem |
| Tipos: REPLY 19, THREAD_STARTER 21, POLL_RESULT 46 | OK | idem | idem |
| Slow mode `rate_limit_per_user` 0-21600 s | OK | idem | DEV/resources/guild |
| Tópico de canal até 1024 | OK | 1024 (canal: nome 1-100) | DEV/resources/guild |

### Permissões e moderação (R02, R07)
| Afirmação | Veredito | Valor | URL |
|---|---|---|---|
| Bits: KICK = 1<<1, BAN = 1<<2 (R02 l.68) | OK | CREATE_INSTANT_INVITE 0, KICK 1, BAN 2, ADMINISTRATOR 3, MANAGE_CHANNELS 4, MANAGE_GUILD 5, ... VIEW_CHANNEL 10, SEND_MESSAGES 11, MANAGE_MESSAGES 13, MANAGE_ROLES 28, MODERATE_MEMBERS 40, PIN_MESSAGES 51, BYPASS_SLOWMODE 52. Bit 47 não usado. Cabem em u64 | DEV/topics/permissions |
| Fórmula de cálculo | OK | 1) dono = tudo; 2) base = @everyone | OR dos cargos; ADMINISTRATOR = tudo; 3) overwrite de @everyone (tira deny, põe allow); 4) overwrites de cargos (OR de todos os deny e allow, tira deny, põe allow); 5) overwrite de membro (tira deny, põe allow). Regras implícitas: sem VIEW_CHANNEL nada vale; sem SEND_MESSAGES perde MENTION_EVERYONE, TTS, ATTACH_FILES, EMBED_LINKS; sem CONNECT perde o resto de voz | DEV/topics/permissions |
| Timeout: máx. 28 dias, `null` remove, MODERATE_MEMBERS, 403 se alvo for admin/dono | OK | idem. Em timeout só restam VIEW_CHANNEL e READ_MESSAGE_HISTORY (confirma R02 l.56) | DEV/resources/guild ; DEV/topics/permissions |
| Retenção do audit log 45 dias | OK | "All audit log entries are stored for 45 days". `limit` 1-100 (padrão 50); motivo `X-Audit-Log-Reason` 1-512 caracteres | DEV/resources/audit-log |
| Níveis de verificação | OK (valores) | 0 NONE; 1 LOW e-mail verificado; 2 MEDIUM conta com mais de 5 min; 3 HIGH membro há mais de 10 min; 4 VERY_HIGH telefone verificado. Filtro de conteúdo: 0/1/2 | DEV/resources/guild |
| Ban: apagar mensagens até 7 dias | OK | `delete_message_seconds` 0-604800; ban em lote até 200 IDs | idem |
| AFK timeout | OK | 60, 300, 900, 1800 ou 3600 s | idem |

### Contas e perfil (R01)
| Afirmação | Veredito | Valor | URL |
|---|---|---|---|
| Username 2-32, minúsculas, só a-z 0-9 `_` `.`, sem dois `.` seguidos, único | OK | idem. Conta não verificada recebe 5 dígitos aleatórios após 7 dias (confirma R01 l.13). Para reivindicar, conta precisa de e-mail ou telefone verificado | SUP/12620128861463 |
| Na API, username não contém `@ # : \`\`\`` nem "discord", nem é `everyone`/`here` | OK | idem; Unicode quase livre na API, mas o cliente exige o conjunto restrito acima | DEV/resources/user |
| Apelido de servidor 1-32 | OK | 1-32 | DEV/resources/user |
| Tag de servidor 4 caracteres | OK | até 4 (e "3 boosts" para a perk) | DEV/resources/user ; SUP/360028038352 |
| Código de backup: formato | OK em parte | Código de **8 dígitos**, uso único; página não diz quantos códigos | SUP/115001221072 |
| MFA: TOTP, chaves de segurança/passkeys, SMS (só após ter app TOTP) | OK | TOTP de 30 s. "6 dígitos" do R01 não está escrito na página | SUP/219576828 |

### Voz, DAVE, Go Live, boost (R04, R02)
| Afirmação | Veredito | Valor | URL |
|---|---|---|---|
| Modos de criptografia: `aead_aes256_gcm_rtpsize` (preferido), `aead_xchacha20_poly1305_rtpsize` (obrigatório); demais descontinuados em 18/11/2024; nonce de 32 bits no fim | OK | Lista dos obsoletos: `xsalsa20_poly1305`, `_suffix`, `_lite`, `_lite_rtpsize`, `aead_aes256_gcm` | DEV/topics/voice-connections |
| Voz v8; IP Discovery de 74 bytes (length=70); RTP 0x80/0x78; 5 quadros de silêncio `F8 FF FE`; Opus 48 kHz estéreo | OK | idem. Heartbeat v8 leva `seq_ack` (-1 se nenhum); ignorar `heartbeat_interval` do Ready | idem |
| DAVE obrigatório desde 01/03/2026 em DMs, grupos, canais de voz e Go Live; ops 21-31; AES128-GCM, tag de 8 bytes, marcador 0xFAFA; `max_dave_protocol_version` | OK | idem. Close 4017 = E2EE/DAVE exigido | idem ; DEV/topics/opcodes-and-status-codes |
| Go Live: até 50 pessoas com vídeo/tela por canal; grátis 720p/30 fps; Nitro até 4K/60 | OK | texto idêntico. Linux: sem captura de áudio de aplicativo (relevante para o Telacast) | SUP/360040816151 |
| Boost: 2 / 7 / 14 | OK | idem | SUP/360028038352 |
| Emoji: 50 / 100 / 150 / 250 | OK | 50 base, +50, +50, +100 | idem |
| Bitrate de áudio por nível 128/256/384 | OK | idem (ver correção da base, seção 1) | idem |
| Emoji 256 KB, PNG/GIF/WEBP | OK em parte | 256 KiB, imagem 128x128. Formatos não listados na página lida | DEV/resources/emoji |
| Stickers | **novo** | Boost 0/1/2/3 = 5/15/30/60 slots. API: arquivo até 512 KiB, 320x320, PNG/APNG/GIF/Lottie (Lottie só VERIFIED/PARTNERED), nome 2-30, descrição 2-100, tags até 200, animado até 5 s | DEV/resources/sticker ; SUP/360028038352 |
| Extras do boost | **novo** | Boost 1: ícone animado, fundo de convite. Boost 2: banner estático, ícone de cargo, upload 50 MB. Boost 3: link personalizado, banner animado, upload 100 MB. Nitro: 200 servidores por conta | SUP/360028038352 ; https://discord.com/nitro |

## 3. Não verificáveis (sem fonte oficial pública achada)

| Afirmação | Relatório | Situação |
|---|---|---|
| Bio ("Sobre mim") 190 caracteres | R01 l.51 | Só fóruns de comunidade repetem 190; nenhuma página oficial. API de usuário não cita bio. Tratar como "aprox." |
| Status personalizado 128 caracteres | R01 l.128 | Só fóruns/blogs. N/V |
| Pronomes ~40 caracteres | R01 l.51 | Nenhuma fonte. N/V |
| Avatar ~10 MB | R01 l.46 | Nenhuma fonte (API não dá limite). N/V |
| Quantidade de códigos de backup = 8 | R01 l.36,128 | Página só diz "código de 8 dígitos". Contagem N/V |
| Total de mensagens fixadas por canal = 50 | R03 l.48 | A doc só dá o limite de página (50). N/V; permissão é PIN_MESSAGES (bit 51), isso OK |
| Opções de expiração/uso de convite na UI (30 min..7 dias; 1..100 usos) | R02 l.26 | Não encontrei a página. N/V |
| Durações de timeout na UI (60 s, 5 min, 10 min, 1 h, 1 dia, 1 semana) | R02 l.68 | UI; só o máximo de 28 dias é oficial |
| `nonce`: "mesmo nonce em poucos minutos não duplica" | R06 l.71 | Janela de tempo não lida; N/V |
| Digitando: expira em ~10 s, ping a cada 8-10 s | R03/R06 | Não lido (página de Gateway Events não consultada). N/V |
| Krisp/VAD, filtros e atalhos de teclado, hexadecimais de tema, larguras | R04/R10 | Detalhes de cliente; sem fonte pública. N/V |
| Conteúdo inteiro de R08 (bots, slash commands, webhooks etc.) | R08 | Não revisado nesta rodada (foco pedido era outro) |
| Denúncias, raid, bloqueio, privacidade, notificações | R05/R07 | Sem números concretos; comportamento de produto, N/V |
| Bitrate/tamanho de "voice messages", tamanho de avatar de grupo etc. | - | Fora do escopo |

## 4. Itens do ROADMAP que dependem de números

- Nenhum item do ROADMAP está baseado em número errado. Itens com números: retenção do audit log "~45 dias" (l.115) **confirmada**; bitfield `u64` de permissões (l.108) **confirmado** (maior bit usado = 52); snowflake `i64` (l.45) coerente (42 bits de tempo + 22 = 64, e o bit de sinal não é usado na prática).
- Cuidado ao implementar R06 sec. 3 "Rate limit ... 120 comandos/min por conexão": o Discord usa 120 por **60 s** (= 120/min, coerente), mas o limite de 5 msgs/5 s por canal e "1 typing/8 s" são **escolhas nossas**, não números do Discord.
- Cuidado ao implementar R04 (teto de bitrate): usar 96 kbps como base de referência, não 64.
- Se o Telacast copiar "2FA obrigatório para moderadores" como requisito de Comunidade, estará copiando algo que o Discord não exige (R02 l.85). É decisão de produto, não paridade.
- Se copiar "exclusão em 14 dias", usar 15 dias para igualar o Discord; mas o prazo é livre no Telacast (LGPD).
- Atualizar a seção "A confirmar" do ROADMAP: passam a **confirmados** gateway (limites, close codes, heartbeat), permissões e fórmula, limites de mensagem/embed, níveis de verificação, timeout de 28 dias, audit 45 dias, boost 2/7/14, emoji 50/100/150/250, Stage 10.000, Go Live 50, DAVE e modos de criptografia, regras de username. Permanecem pendentes bio, status, pronomes, avatar, convites da UI e R08.

## 5. Placar

- Confirmados (itens individuais, tabelas da seção 2): 60 aproximadamente.
- Corrigidos (seção 1): 5 (bitrate base, 14 vs 15 dias, requisitos de Comunidade, upload, Stage) mais 1 parcial (frequência de troca de username).
- Não verificáveis (seção 3): 13 grupos.
- Observação: páginas lidas em 2026-11-07. O Discord muda limites de upload e boost com frequência (o FAQ de anexos fala de experimentos e o de boost de uma promoção em setembro de 2026); recheque antes de fixar qualquer número na UI.

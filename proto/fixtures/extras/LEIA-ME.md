# Extras: o que o cliente precisa tolerar

Arquivos de **entrada** escritos à mão (não gerados por Rust). O cliente deve decodificá-los sem falhar e
chegar ao resultado abaixo. Para os casos que decodificam, `<caso>.esperado.json` é a forma **normalizada**
(o que o Rust reserializa depois de ler: `default` preenchidos, campos novos descartados, desconhecidos
trocados). Esses `.esperado.json` são gerados por `UPDATE_FIXTURES=1 cargo test -p telacast-proto`; o teste
Kotlin pode decodificar a entrada e o esperado e exigir que os dois objetos sejam iguais.

| Entrada | Tipo | Resultado esperado |
|---|---|---|
| `error-codigo-desconhecido.json` | `S2C` | `Error` com `code = Unknown` (código novo do servidor); `msg` preservada |
| `error-sem-code.json` | `S2C` | `Error` com `code = Unknown` (servidor antigo, sem o campo) |
| `status-desconhecido.json` | `S2C` | `Auth` com `me.status = Offline` (`"Streaming"` não existe aqui) |
| `usuario-sem-status.json` | `S2C` | `Auth` com `me.status = Offline` (campo `status` ausente) |
| `campos-novos-ignorados.json` | `S2C` | `Message` normal; campos extras (`thread_id`, `campo_futuro`, `avatar` em `reply`, `blurhash` em anexo) são ignorados em qualquer nível |
| `mensagem-minima.json` | `S2C` | `Message` com `reply = null`, `nonce = 0`, `reactions = []`, `kind = 0`, `pinned = false`, `attachments = []` |
| `snapshot-minimo.json` | `S2C` | `Snapshot` com `blocked = []`, `voice = []`, `status = Offline` em todo usuário, `mentions = 0` em canal e DM, `parent = null` |
| `login-sem-proto.json` | `C2S` | `Login` com `proto = 0` |
| `send-minimo.json` | `C2S` | `Send` com `reply_to = null`, `nonce = 0`, `attachments = []` |
| `manifesto-minimo.json` | `UpdateManifest` | `notes = ""`, `signature = ""` |
| `s2c-variante-desconhecida.json` | `S2C` | **Falha de decodificação** (variante `ThreadCreated` não existe). O cliente descarta o quadro e continua; não derruba a conexão. Sem `.esperado.json` |
| `friend-status-desconhecido.json` | `S2C` | **Falha de decodificação do `Snapshot` inteiro**: `FriendStatus` não tem valor de reserva (`"Blocked"` é desconhecido). Fragilidade conhecida do protocolo; o Kotlin pode escolher ser mais tolerante (ignorar só aquele amigo), mas não é o contrato de hoje. Sem `.esperado.json` |

Regras gerais que estes casos fixam:

- `Option` ausente equivale a `null`; `null` equivale a ausente.
- Campo desconhecido é ignorado em objetos de qualquer profundidade (o Rust não usa `deny_unknown_fields`).
- Ids são inteiros de 64 bits (`112624001234567890`): ler como `Long`, nunca como `Double`.

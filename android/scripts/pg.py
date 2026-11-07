#!/usr/bin/env python3
"""Cliente mínimo de Postgres (só a biblioteca padrão) para criar e apagar o banco DESCARTÁVEL dos testes de integração.

Não há `psql` nem driver nesta máquina, e o Docker não está acessível ao usuário; então este arquivo fala o protocolo
(autenticação SCRAM-SHA-256 / md5 / senha) e roda `CREATE DATABASE` e `DROP DATABASE`.

Uso:  pg.py create telacast_it_123   |   pg.py drop telacast_it_123
Credenciais de DEV por padrão (as mesmas de server/tests/e2e.rs); troque com ADMIN_URL=postgres://u:s@host:porta/postgres.
Por segurança só aceita nomes `telacast_it_<números>`: nunca toca no banco `telacast` de uso.
"""
import base64, hashlib, hmac, os, re, socket, struct, sys
from urllib.parse import urlparse

ADMIN_URL = os.environ.get("ADMIN_URL", "postgres://telacast:telacast_dev@127.0.0.1:5432/postgres")


def _msg(tipo: bytes, corpo: bytes) -> bytes:
    return tipo + struct.pack("!I", len(corpo) + 4) + corpo


def _ler(sock):
    cab = b""
    while len(cab) < 5:
        parte = sock.recv(5 - len(cab))
        if not parte:
            raise ConnectionError("o Postgres fechou a conexão")
        cab += parte
    tipo, tam = cab[:1], struct.unpack("!I", cab[1:5])[0]
    corpo = b""
    while len(corpo) < tam - 4:
        parte = sock.recv(tam - 4 - len(corpo))
        if not parte:
            raise ConnectionError("o Postgres fechou a conexão")
        corpo += parte
    return tipo, corpo


def _erro(corpo: bytes) -> str:
    campos = [c for c in corpo.split(b"\0") if c]
    return " ".join(c[1:].decode("utf-8", "replace") for c in campos if c[:1] in (b"M", b"C"))


def _scram(sock, senha: str, mecanismos: bytes):
    if b"SCRAM-SHA-256" not in mecanismos:
        raise RuntimeError("servidor só oferece SCRAM-SHA-256 com canal (não suportado)")
    nonce = base64.b64encode(os.urandom(18)).decode()
    bare = f"n=,r={nonce}"
    inicial = ("n,," + bare).encode()
    sock.sendall(_msg(b"p", b"SCRAM-SHA-256\0" + struct.pack("!I", len(inicial)) + inicial))
    tipo, corpo = _ler(sock)
    if tipo != b"R" or struct.unpack("!I", corpo[:4])[0] != 11:
        raise RuntimeError("SCRAM: resposta inesperada: " + _erro(corpo))
    servidor_primeira = corpo[4:].decode()
    dados = dict(p.split("=", 1) for p in servidor_primeira.split(","))
    if not dados["r"].startswith(nonce):
        raise RuntimeError("SCRAM: nonce do servidor não confere")
    salgada = hashlib.pbkdf2_hmac("sha256", senha.encode(), base64.b64decode(dados["s"]), int(dados["i"]))
    chave_cliente = hmac.new(salgada, b"Client Key", "sha256").digest()
    guardada = hashlib.sha256(chave_cliente).digest()
    sem_prova = f"c=biws,r={dados['r']}"
    auth = f"{bare},{servidor_primeira},{sem_prova}".encode()
    assinatura = hmac.new(guardada, auth, "sha256").digest()
    prova = bytes(a ^ b for a, b in zip(chave_cliente, assinatura))
    final = f"{sem_prova},p={base64.b64encode(prova).decode()}".encode()
    sock.sendall(_msg(b"p", final))
    tipo, corpo = _ler(sock)
    if tipo != b"R" or struct.unpack("!I", corpo[:4])[0] != 12:
        raise RuntimeError("SCRAM: autenticação recusada: " + _erro(corpo))


def executar(sql: str):
    u = urlparse(ADMIN_URL)
    sock = socket.create_connection((u.hostname, u.port or 5432), timeout=10)
    try:
        params = b"user\0" + (u.username or "").encode() + b"\0database\0" + (u.path.lstrip("/") or "postgres").encode() + b"\0\0"
        corpo = struct.pack("!I", 196608) + params
        sock.sendall(struct.pack("!I", len(corpo) + 4) + corpo)
        while True:
            tipo, corpo = _ler(sock)
            if tipo == b"E":
                raise RuntimeError("Postgres: " + _erro(corpo))
            if tipo == b"R":
                codigo = struct.unpack("!I", corpo[:4])[0]
                if codigo == 0:
                    continue
                if codigo == 10:
                    _scram(sock, u.password or "", corpo[4:])
                elif codigo == 3:
                    sock.sendall(_msg(b"p", (u.password or "").encode() + b"\0"))
                elif codigo == 5:
                    interno = hashlib.md5((u.password or "").encode() + (u.username or "").encode()).hexdigest()
                    sock.sendall(_msg(b"p", b"md5" + hashlib.md5(interno.encode() + corpo[4:8]).hexdigest().encode() + b"\0"))
                else:
                    raise RuntimeError(f"método de autenticação {codigo} não suportado")
            if tipo == b"Z":
                break
        sock.sendall(_msg(b"Q", sql.encode() + b"\0"))
        erro = None
        while True:
            tipo, corpo = _ler(sock)
            if tipo == b"E":
                erro = _erro(corpo)
            if tipo == b"Z":
                break
        sock.sendall(_msg(b"X", b""))
        if erro:
            raise RuntimeError("Postgres: " + erro)
    finally:
        sock.close()


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("create", "drop"):
        sys.exit(__doc__)
    nome = sys.argv[2]
    if not re.fullmatch(r"telacast_it_[0-9]+", nome):
        sys.exit(f"nome recusado ({nome}): só telacast_it_<números>, para nunca tocar no banco de uso")
    if sys.argv[1] == "create":
        executar(f"CREATE DATABASE {nome}")
    else:
        executar(f"DROP DATABASE IF EXISTS {nome} WITH (FORCE)")


if __name__ == "__main__":
    main()

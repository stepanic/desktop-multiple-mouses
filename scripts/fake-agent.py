#!/usr/bin/env python3
"""Fake multi-mouse Mac agent: accepts the Windows app and prints decoded events.
Useful for testing the Windows side without a Mac.  Usage: fake-agent.py [token] [port]"""
import socket, struct, sys

token = (sys.argv[1] if len(sys.argv) > 1 else "test").encode()
port = int(sys.argv[2]) if len(sys.argv) > 2 else 47800
BUTTONS = ["Left", "Right", "Middle", "Back", "Forward"]

def read(c, n):
    b = b""
    while len(b) < n:
        chunk = c.recv(n - len(b))
        if not chunk:
            raise EOFError
        b += chunk
    return b

srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("0.0.0.0", port))
srv.listen(1)
print(f"fake agent on :{port}, token={token.decode()}", flush=True)
while True:
    c, addr = srv.accept()
    try:
        head = read(c, 6)
        assert head[:4] == b"MMUS", head
        got = read(c, head[5])
        ok = got == token and head[4] == 1
        c.sendall(bytes([0 if ok else 1]))
        print(f"client {addr} version={head[4]} token_ok={ok}", flush=True)
        if not ok:
            c.close(); continue
        moves = [0, 0, 0]
        while True:
            tag = read(c, 1)[0]
            if tag == 1:
                dx, dy = struct.unpack("<ii", read(c, 8))
                moves[0] += dx; moves[1] += dy; moves[2] += 1
                if moves[2] % 50 == 0:
                    print(f"  Move x{moves[2]} total dx={moves[0]} dy={moves[1]}", flush=True)
                continue
            if moves[2]:
                print(f"  Move x{moves[2]} total dx={moves[0]} dy={moves[1]}", flush=True)
                moves = [0, 0, 0]
            if tag == 2:
                b, down = read(c, 2)
                print(f"  Button {BUTTONS[b]} {'down' if down else 'up'}", flush=True)
            elif tag == 3:
                v, h = struct.unpack("<ii", read(c, 8))
                print(f"  Wheel v={v} h={h}", flush=True)
            elif tag == 4:
                sc, fl = struct.unpack("<HB", read(c, 3))
                print(f"  Key {'E0 ' if fl & 1 else ''}{sc:#04x} {'down' if fl & 2 else 'up'}", flush=True)
            elif tag == 5:
                print("  ReleaseAll", flush=True)
            elif tag == 6:
                pass
            else:
                print(f"  unknown tag {tag}", flush=True); break
    except (EOFError, ConnectionError) as e:
        print(f"client disconnected ({e!r})", flush=True)
    finally:
        c.close()

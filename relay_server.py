#!/usr/bin/env python3
"""
EyesLive Self-Hosted Relay Server (Standalone Python 3 / Zero Dependencies)
Created & Signed by Gabriel Yan (@gabrielyandev)
GitHub: https://github.com/gabrielyandev/eyeslive

100% binary-compatible with EyesLive's Rust bincode wire protocol on TCP port 9091.
Routes connections by 9-digit Desk ID (XXX-XXX-XXX) without accounts or time limits.
"""

import asyncio
import socket
import struct
import sys

DEFAULT_PORT = 9091

# Waiting hosts dictionary: normalized_desk_id -> (reader, writer)
waiting_hosts = {}


def normalize_id(raw: str) -> str:
    return "".join(ch for ch in raw if ch.isdigit())


def encode_relay_status(ok: bool, message: str) -> bytes:
    msg_bytes = message.encode("utf-8")
    # bincode 1.3 format for Packet::RelayStatus (enum variant 2u32):
    # u32 LE variant (2) + u8 bool (1 or 0) + u64 LE string length + utf-8 bytes
    payload = struct.pack("<IBQ", 2, 1 if ok else 0, len(msg_bytes)) + msg_bytes
    # Frame prefix: u32 LE payload length
    return struct.pack("<I", len(payload)) + payload


async def read_packet(reader: asyncio.StreamReader):
    hdr = await reader.readexactly(4)
    (length,) = struct.unpack("<I", hdr)
    if length == 0 or length > 1024 * 1024:
        raise ValueError(f"Invalid handshake packet length: {length}")
    payload = await reader.readexactly(length)
    if len(payload) < 12:
        raise ValueError("Payload too short")
    (variant, str_len) = struct.unpack("<IQ", payload[:12])
    text = payload[12 : 12 + str_len].decode("utf-8", errors="replace")
    return variant, text


async def pipe_stream(reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
    try:
        while True:
            data = await reader.read(65536)
            if not data:
                break
            writer.write(data)
            await writer.drain()
    except Exception:
        pass
    finally:
        try:
            writer.close()
        except Exception:
            pass


async def handle_client(reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
    sock = writer.get_extra_info("socket")
    if sock is not None:
        try:
            sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_KEEPALIVE, 1)
        except Exception:
            pass

    peer = writer.get_extra_info("peername")
    try:
        variant, raw_id = await asyncio.wait_for(read_packet(reader), timeout=15.0)
    except Exception:
        writer.close()
        return

    clean_id = normalize_id(raw_id)

    # Variant 0 = Packet::RegisterHost { desk_id }
    if variant == 0:
        old = waiting_hosts.pop(clean_id, None)
        if old is not None:
            try:
                old[1].close()
            except Exception:
                pass

        try:
            writer.write(
                encode_relay_status(True, f"Registered ID {clean_id} on EyesLive relay")
            )
            await writer.drain()
            waiting_hosts[clean_id] = (reader, writer)
            print(f"[EyesLive Relay] Host registered: ID {clean_id} from {peer}", flush=True)
        except Exception:
            writer.close()

    # Variant 1 = Packet::ConnectToHost { target_desk_id }
    elif variant == 1:
        host_pair = waiting_hosts.pop(clean_id, None)
        if host_pair is None:
            print(f"[EyesLive Relay] Viewer {peer} requested offline ID {clean_id}", flush=True)
            try:
                writer.write(
                    encode_relay_status(
                        False, f"Host ID {clean_id} nao esta online neste Relay"
                    )
                )
                await writer.drain()
            except Exception:
                pass
            writer.close()
            return

        host_reader, host_writer = host_pair
        try:
            # Notify Host that a Viewer connected so it immediately registers a new standby slot
            host_writer.write(
                encode_relay_status(True, "Viewer connected via EyesLive Relay")
            )
            await host_writer.drain()

            # Notify Viewer that tunnel is ready
            writer.write(
                encode_relay_status(True, "Connected to host via EyesLive Relay")
            )
            await writer.drain()
            print(
                f"[EyesLive Relay] Tunnel established for ID {clean_id} (Viewer {peer})",
                flush=True,
            )
        except Exception:
            try:
                writer.write(
                    encode_relay_status(
                        False, f"Host ID {clean_id} desconectou do Relay"
                    )
                )
                await writer.drain()
            except Exception:
                pass
            writer.close()
            try:
                host_writer.close()
            except Exception:
                pass
            return

        # Bidirectional raw TCP forwarding
        await asyncio.gather(
            pipe_stream(reader, host_writer),
            pipe_stream(host_reader, writer),
        )
    else:
        writer.close()


async def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_PORT
    server = await asyncio.start_server(handle_client, "0.0.0.0", port)
    print(
        f"[EyesLive Relay — by Gabriel Yan (@gabrielyandev)] Listening on 0.0.0.0:{port}",
        flush=True,
    )
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass

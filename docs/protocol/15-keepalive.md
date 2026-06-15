# Keepalive (Ping/Pong)

The keepalive mechanism prevents NAT routers from dropping idle TCP connections.

## Overview

Most consumer NAT routers drop idle TCP connections after 30-60 minutes. For a BBS where users may leave the client open while doing other tasks, this can cause unexpected disconnections.

The client sends periodic `Ping` messages, and the server responds with `Pong`. This traffic keeps the NAT mapping alive.

## Messages

### Ping (Client → Server)

```json
{ "type": "Ping" }
```

No fields. Sent by the client every 5 minutes when the connection is idle.

### Pong (Server → Client)

```json
{ "type": "Pong" }
```

No fields. Sent immediately by the server in response to `Ping`.

## Behavior

### Client

- Sends `Ping` every 5 minutes (300 seconds) of inactivity
- The timer resets whenever any message is sent to the server
- `Pong` responses are received but no action is taken (receiving it is sufficient)
- If sending `Ping` fails, the connection is considered dead

### Server

- Responds to `Ping` with `Pong` immediately
- Only accepted after the normal BBS handshake/login frame phases; pre-handshake
  and pre-login readers reject `Ping` as an unexpected message type
- No rate limiting (clients only send every 5 minutes)

## Why Client Pings

The client initiates pings rather than the server because:

1. **NAT is client-side** - The client is typically behind NAT, not the server
2. **Client controls timing** - Different network conditions may need different intervals
3. **Server simplicity** - Server just responds, no timer management needed
4. **Server already detects dead clients** - TCP write failures reveal dead connections

## Frame Example

```
NX|4|Ping|a1b2c3d4e5f6|2|{}\n
```

Response:

```
NX|4|Pong|a1b2c3d4e5f6|2|{}\n
```

Note: The response echoes the message ID from the request.

# RF NET
RF NET is a community oriented ham radio technology for local and long distance digital communcation. RF NET aims to unify a wide range of disparate technologies in a single place. No matter if you're running APRS, a custom modulation, on your desktop or smartphone. You'll always have access to the RF NET network.

## High level goals
RF NET has a few core ideas that are focused towards building a network that is robust and extensible.

* RF Modulation agnostic.
* Data framing agnostic.
* Built-in support for FEC, sequencing, retry, backoff and routing.
* Open interconnect protocol using existing web technologies.
* Extendable platform via the ability to call REST APIs from any radio.
* Packet + Callsign authentication with public/private key signatures.
* Message routing and inboxes via e-mail like addressing schema. Ex: "KI7EST@rfnethub.net"
* Support for community spaces to enable local clubs to communcate, coordiate events or build a meeting space around a common interest.

## Network overview
```
+---------------------+                                                  +    +---------------------+
|                     |                                                  |    |                     |
|  PC/Mobile Device   |                                                  |    |  PC/Mobile Device   |
|    (RF NET Node)    |                                                  |    | (RF NET Web Client) |
|                     |                                                  |    |                     |
+--------^-+----------+                                                  |    +---------+-^---------+
         | |                                                             |              | |
         | |                                       +---------------+     |        +-----v-+------+
     +---+-v----+                 +----------+     |               |     |        |              |
     |          |                 |          +----->      PC       +-----+-------->      PC      |
     | KISS TNC |                 | KISS TNC |     | (RF NET Link) |  REST API    | (RF NET Hub) |
     |          |                 |          <-----+               <-----+--------+              |
     +---+-^----+                 +---+-^----+     +---------------+     |        +--------------+
         | |                          | |                                |
         | |                          | |                                |
    +----v-+-----+               +----v-+--------+                       |
    |            |   APRS/Etc    |               |                       |
    | UHF/VHF/HF +--------------->   UHF/VHF/HF  |                       |
    |            |    FSK/PSK    |     Radio     |                       |
    |   Radio    <---------------+ (Coordinator) |           RF Spectrum | Internet
    |            |               |               |                       |
    +------------+               +---------------+                       +
```

### Link types
* Node - Client application that talks to Links via local KISS TNC or other interface protocols.
* Link - Local coordinating radio with connection to the internet. Listens for local Nodes and provides transport for commands and queries.
* Hub - Persistent storage and verification of RF packets.

### RF NET User architecture
RF NET is built around independently operated Hubs that provide a centralized location to do callsign verification and provide a space for community engagement.

Every packet sent over and RF link is signed and verified with a public/private key pair. Each RF NET Hub stores the public key for the callsigns registered on it. A RF NET Link can query for this key whenever it receives a new packet and verify that the sender is legitimate before sending the query/command to the internet.

## RF NET Framing Spec

* Assumptions are that underlying framing protocol has the following properties:
  * Clearly specified start/end framing for discrete packet lengths, ideally noise resistant.
  * Integrity of inner data is not required.
  * Reliablilty of inner data is not required.
* All packets contain preamble + payload for data packet.
* Preamble are FEC encodeded at 2x rate of header if FEC is enabled.
* Channel control is arbitrated by RF NET Link with Nodes requesting open channel.


#### Broadcast packet
```
1b:(PacketType+FEC_ENABLED+RETRY_ENABLED)|2b:(MajorVer+MinorVer)|2b:LINK_WIDTH|Nb:LinkCallsign|(Nb+1b+2b+2b)*2:FEC
```

##### PacketType
```
00: Link Broadcast
01: Data
10: Ack
11: Ctrl
```

#### Control packet
```
1b:(PacketType+ControlType)|Nb:Callsign\0TargetCallsign|(Nb+1)*2:FEC
```

##### Control type(64)
```
0: Reserved for extension
1: Link request ->
2: Link opened <-
3: Link close ->
4: Link clear <-
5: Node waiting <-
6: Notification <-
```

#### Data packet
```
2b:(PacketIdx or SequenceId if StartFlag is set+PacketType)|1b:BLOCK_SIZE+FEC_BYTES+START_FLAG+END_FLAG|(1b+2b)*2:HeaderFEC|LINK_WIDTH-(FEC_BYTES+3b+6b):payload|FEC_BYTES:FEC
```

```
Parameters:
    LINK_WIDTH: nominally 256 bytes but can be changed to adjust to native framing size
    FEC_BYTES: 2,4,6,8,16,32,64 number of bytes of FEC
    BLOCK_SIZE: 0,1,2,4,7,15,31,63 ratio of FEC bytes to data bytes. 0 means single block per packet

    START_FLAG:1
    END_FLAT:1
    BLOCK_SIZE:3(8)
    FEC_BYTES:3(8)
```

#### Ack packet
```
2b:(SequenceId+PacketType)|2b:(PacketIdx+NackFlag+NoResponseFlag)|1b:CorrectedErrors|10b:FEC
```

## RF Net Packet Spec

#### Request
```
64b:sign|2b:sequenceId|1b:type|Callsign@hub\0payload
```

##### Packet Type
```
0: Reserved for extension
1: REST call, payload takes format of: GET|PUT|PATCH|POST|DELETE|url\0headers\0body
2: Raw RF packet, payload takes format of: hub\0payload
```

#### Response
```
callsign@hub\0|2b:sequenceId|1b:type|payload
```

##### Packet Type
```
0: Reserved for extension
1: REST response, payload takes format of: 1b:code|body
2: Raw RF packet, payload takes format of: hub\0payload
```

## RF NET typical sequence
```
     ┌────┐                     ┌────┐                   ┌────────┐
     │Node│                     │Link│                   │Internet│
     └─┬──┘                     └─┬──┘                   └───┬────┘
       │────┐                     │                          │     
       │    │ Start Operation     │                          │     
       │<───┘                     │                          │     
       │                          │                          │     
       │   CONTROL: Request Link  │                          │     
       │ ─────────────────────────>                          │     
       │                          │                          │     
       │    CONTROL: Grant Link   │                          │     
       │ <─────────────────────────                          │     
       │                          │                          │     
       │          DATA 0          │                          │     
       │ ─────────────────────────>                          │     
       │                          │                          │     
       │           ACK 0          │                          │     
       │ <─────────────────────────                          │     
       │                          │                          │     
  ╔════╧══════════════════════════╧═══════╗                  │     
  ║Repeat until all data frames complete ░║                  │     
  ╚════╤══════════════════════════╤═══════╝                  │     
       │          DATA N          │                          │     
       │ ─────────────────────────>                          │     
       │                          │                          │     
       │           ACK N          │                          │     
       │ <─────────────────────────                          │     
       │                          │                          │     
       │                          │ REST API or RF Raw packet│     
       │                          │ ─────────────────────────>     
       │                          │                          │     
       │                          │         HTTP 200         │     
       │                          │ <─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─     
       │                          │                          │     
       │ ACK N (contains response)│                          │     
       │ <─────────────────────────                          │     
       │                          │                          │     
       │          Data 0          │                          │     
       │ <─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─                          │     
       │                          │                          │     
       │           Ack 0          │                          │     
       │  ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ >                          │     
       │                          │                          │     
  ╔════╧══════════════════════════╧═══════╗                  │     
  ║Repeat until all data frames complete ░║                  │     
  ╚════╤══════════════════════════╤═══════╝                  │     
       │          Data N          │                          │     
       │ <─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─                          │     
       │                          │                          │     
       │           Ack N          │                          │     
       │  ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ >                          │     
       │                          │                          │     
       │────┐                     │                          │     
       │    │ Operation complete  │                          │     
       │<───┘                     │                          │     
       │                          │                          │     
       │           Close          │                          │     
       │ ─────────────────────────>                          │     
       │                          │                          │     
       │          Closed          │                          │     
       │ <─────────────────────────                          │     
     ┌─┴──┐                     ┌─┴──┐                   ┌───┴────┐
     │Node│                     │Link│                   │Internet│
     └────┘                     └────┘                   └────────┘
```

## RF NET typical sequence (detailed)

```
     ┌────┐                                            ┌──┐                                            ┌────┐                                          ┌───┐          ┌──────────────┐
     │Node│                                            │RF│                                            │Link│                                          │Hub│          │testdomain.com│
     └─┬──┘                                            └┬─┘                                            └─┬──┘                                          └─┬─┘          └──────┬───────┘
       │                                                │          BROADCAST: 256LW,R,F,KI7EST-0         │                                               │                   │        
       │                                                │ <───────────────────────────────────────────────                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                          ╔═════════════════════╧═══════════════════════╗                        │                                               │                   │        
       │                          ║Repeated every 5 minutes while link is idle ░║                        │                                               │                   │        
       │                          ╚═════════════════════╤═══════════════════════╝                        │                                               │                   │        
       │                                             ╔══╧════╗                                           │                                               │                   │        
       │                                             ║. . . ░║                                           │                                               │                   │        
       │                                             ╚══╤════╝                                           │                                               │                   │        
       │     CONTROL: Request Link KI7EST-1,KI7EST-0    │                                                │                                               │                   │        
       │ ───────────────────────────────────────────────>                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │     CONTROL: Request Link KI7EST-1,KI7EST-0    │                                               │                   │        
       │                                                │ ───────────────────────────────────────────────>                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │      CONTROL: Grant Link KI7EST-0,KI7EST-1     │                                               │                   │        
       │                                                │ <───────────────────────────────────────────────                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │      CONTROL: Grant Link KI7EST-0,KI7EST-1     │                                                │                                               │                   │        
       │ <───────────────────────────────────────────────                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                ╔═══════════════╧════════════════╗                               │                                               │                   │        
       │                                ║First frame has Start Flag set ░║                               │                                               │                   │        
       │                                ╚═══════════════╤════════════════╝                               │                                               │                   │        
       │ DATA: 0x0001 256B,4F,S,_E <FEC> <payload> <FEC>│                                                │                                               │                   │        
       │ ───────────────────────────────────────────────>                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │ DATA: 0x0001 256B,4F,S,_E <FEC> <payload> <FEC>│                                               │                   │        
       │                                                │ ───────────────────────────────────────────────>                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │      ACK: 0x0001 0x0000, _N, _R, 0C, <FEC>     │                                               │                   │        
       │                                                │ <───────────────────────────────────────────────                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │      ACK: 0x0001 0x0000, _N, _R, 0C, <FEC>     │                                                │                                               │                   │        
       │ <───────────────────────────────────────────────                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │            ╔═══════════════════════════════════╧═════════════════════════════════════╗          │                                               │                   │        
       │            ║Repeat until all data frames are sent.                                  ░║          │                                               │                   │        
       │            ║                                                                         ║          │                                               │                   │        
       │            ║Note that last frame has end flag set                                    ║          │                                               │                   │        
       │            ║                                                                         ║          │                                               │                   │        
       │            ║If Link as response for command/packet then                              ║          │                                               │                   │        
       │            ║ Response Flag will be set and second data transfer happens in reverse.  ║          │                                               │                   │        
       │            ╚═══════════════════════════════════╤═════════════════════════════════════╝          │                                               │                   │        
       │ DATA: 0x0020 256B,4F,_S,E <FEC> <payload> <FEC>│                                                │                                               │                   │        
       │ ───────────────────────────────────────────────>                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │ DATA: 0x0020 256B,4F,_S,E <FEC> <payload> <FEC>│                                               │                   │        
       │                                                │ ───────────────────────────────────────────────>                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │────┐                                          │                   │        
       │                                                │                                                │    │ Final packet, assembled sequence         │                   │        
       │                                                │                                                │<───┘                                          │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │────┐                                          │                   │        
       │                                                │                                                │    │ <sig> 0001 REST KI7EST@rfnethub.net      │                   │        
       │                                                │                                                │<───┘ POST testdomain.com/ping                 │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │               /user/KI7EST/keys               │                   │        
       │                                                │                                                │ ──────────────────────────────────────────────>                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │ "MTIzNDU2NzgxMjM0NTY3ODEyMzQ1Njc4MTIzNDU2Nzg="│                   │        
       │                                                │                                                │ <──────────────────────────────────────────────                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │────┐                                          │                   │        
       │                                                │                                                │    │ Verify signature                         │                   │        
       │                                                │                                                │<───┘                                          │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │                               /ping           │                   │        
       │                                                │                                                │ ──────────────────────────────────────────────────────────────────>        
       │                                                │                                                │                                               │                   │        
       │                                                │                                                │                                200            │                   │        
       │                                                │                                                │ <──────────────────────────────────────────────────────────────────        
       │                                                │                                                │                                               │                   │        
       │                                                │      ACK: 0x0001 0x0000, _N, R, 0C, <FEC>      │                                               │                   │        
       │                                                │ <───────────────────────────────────────────────                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │      ACK: 0x0001 0x0000, _N, R, 0C, <FEC>      │                                                │                                               │                   │        
       │ <───────────────────────────────────────────────                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │         DATA: 0x0001 256B,4F,S,E <FEC>         │                                               │                   │        
       │                                                │         KI7EST@rfnethub.net 0001 REST          │                                               │                   │        
       │                                                │         200                                    │                                               │                   │        
       │                                                │ <───────────────────────────────────────────────                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │         DATA: 0x0001 256B,4F,S,E <FEC>         │                                                │                                               │                   │        
       │         KI7EST@rfnethub.net 0001 REST          │                                                │                                               │                   │        
       │         200                                    │                                                │                                               │                   │        
       │ <───────────────────────────────────────────────                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │        ACK: 0x0001 0x0000,_N,_R,0C <FEC>       │                                                │                                               │                   │        
       │ ───────────────────────────────────────────────>                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │        ACK: 0x0001 0x0000,_N,_R,0C <FEC>       │                                               │                   │        
       │                                                │ ───────────────────────────────────────────────>                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │      CONTROL: Close Link KI7EST-1,KI7EST-0     │                                                │                                               │                   │        
       │ ───────────────────────────────────────────────>                                                │                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │      CONTROL: Close Link KI7EST-1,KI7EST-0     │                                               │                   │        
       │                                                │ ───────────────────────────────────────────────>                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │                                                │     CONTROL: Closed Link KI7EST-0,KI7EST-1     │                                               │                   │        
       │                                                │ <───────────────────────────────────────────────                                               │                   │        
       │                                                │                                                │                                               │                   │        
       │     CONTROL: Closed Link KI7EST-0,KI7EST-1     │                                                │                                               │                   │        
       │ <───────────────────────────────────────────────                                                │                                               │                   │        
     ┌─┴──┐                                            ┌┴─┐                                            ┌─┴──┐                                          ┌─┴─┐          ┌──────┴───────┐
     │Node│                                            │RF│                                            │Link│                                          │Hub│          │testdomain.com│
     └────┘                                            └──┘                                            └────┘                                          └───┘          └──────────────┘
```


## Supported Radios

* KISS TNC 1200bsp AFSK (APRS)
* KISS TNC 9600pbs K9NG (APRS)
# RF NET
RF NET is a community oriented ham radio technology for local and long distance digital communcation. RF NET aims to unify a wide range of disparate technologies in a single place. No matter if you're running APRS, a custom modulation, on your desktop or smartphone. You'll always have access to the RF NET network.

## What can I do with RF NET?
* Messaging with no length limits and full support for UTF-8.
* Persistent Messages/Data via independent RF NET Hub servers.
* Support for data attachments including images, documents and any form of binary data.
* Coordination via Club/Interest spaces hosted on RF NET Hub.
* The ability to make REST calls(and responses) from any Radio. Want to get NOAA weather far from home? Get it directly from NOAA REST API.
* Full public/private key verification. Remotely control any REST API with the security of knowing only your callsign can access public endpoints.
* All RF NET features are accessible from any radio or directly from the internet.

## This is a rough draft and not final!
At this stage we're actively looking for feedback and input on what's people think is important in a communication protocol. If there's something that jumps out at you please feel free to open an issue or submit a PR!

## High level goals
RF NET has a few core ideas that are focused towards building a network that is robust and extensible.

* RF Modulation agnostic.
* Data framing agnostic.
* Built-in support for FEC, sequencing, retry, backoff and routing.
* Open interconnect protocol using existing web technologies.
* Extendable platform via the ability to call REST APIs from any radio.
* Packet + Callsign authentication with public/private key signatures.
* Message routing and offline delivery via e-mail like addressing schema. Ex: "KI7EST@rfnethub.net"
* Support for community spaces to enable local clubs to communcate, coordiate events or build a meeting space around a common interest.
* Automatic discovery of link parameters via periodic broadcast packets.
* Multi-mode support over a single link. Support clients of either 1200bps or 9600bps on the same frequency.

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

### Network types
* Node - Client application that talks to Links via KISS TNC or other interface protocols.
* Link - Local coordinating radio with connection to the internet. Listens for local Nodes and provides transport for commands and queries.
* Hub - Persistent storage and verification of RF packets.

### RF NET User architecture
RF NET is built around independently operated Hubs that provide a centralized location to do callsign verification and provide a space for community engagement.

Every packet sent over and RF link is signed and verified with a public/private key pair. Each RF NET Hub stores the public key for the callsigns registered on it. A RF NET Link can query for this key whenever it receives a new packet and verify that the sender is legitimate before sending the query/command to the internet.

## RF NET Framing Spec

RF NET is designed to work with a wide range of digital communcation formats. Regardless of baud, modulation or coding details RF NET is structured such that it can use a wide range of radio technologies to transport digital data.

* Assumptions are that underlying framing protocol has the following properties:
  * Clearly specified start/end framing for discrete packet lengths, ideally noise tolerant.
  * Integrity of inner data is not required.
  * Reliablilty of inner data is not required.
* All packets contain preamble + payload for data packet.
* Preamble is FEC encodeded at 2x rate of header if FEC is enabled.
* Channel control is arbitrated by RF NET Link with Nodes requesting open channel.

Broadly speaking this means that RF NET can support any of the following:
* All KISS based TNCs including nearly any radios that support an external APRS interface.
* Virtual TNCs such as Direwolf for sending digital data over analog radio inputs.
* Forward-looking radios like FaradayRF that support even higher data rates than traditionally found in modern ham transceivers.

Even with this wide range of radios using different baud, modulation and frequency RF NET's architecture gaurantees interoperatiblity between any user using RF NET.

The following sections lay out the discrete data format and sequencing that RF NET uses to transport digital data across the network.

## RF NET Channel Control
RF NET is a cooperative protocol. It has a single radio, a Link, that coordinates access to the frequency channel through a series of control packets. Channel control is exclusive in that only one Node can be communcating to a Link at a given time.

Nodes have a series of states that they use to represent the current status of the channel and their participation.

### Listening State
This is the initial state of a Node. It represents that the Node either doesn't know the current state of the channel or that it has recently heard some communcation on the channel that was not intended for this specific Node. The Node keeps an internal 10 second timer and any transmission heard that it is not involved in resets the timer back to 10 seconds.

While in the listening state a Node can still queue operations to perform however they will be suspended until the channel is open to transmit on.

If a node hears that the channel is clear via the Link Clear Control packet it is free to immediately transition to the Idle State.

### Idle State
This is the state when nothing has been heard on the channel for at least 10 seconds. Any Node in this state is assuming that the channel is clear to transmit and any operation will immediately be executed.

### Negotiating State
This is the state that a Node transitions to from Idle when it wants to establish a communcation with a Link. This is started by sending the Link Request Control Packet. Note that if a node doesn't hear an acknowledgment in the span of 10 seconds or hears another packet it will revert back to the Listening State.

Nodes are free to send multiple Link Request Control Packets during the negotiating period.

### Established State
This state marks that a Node has heard a Link Opened Control Packet from the Link and is clear to start transmitting data packets.

This state may be active for a long time depending on the amount of data being transferred between the Node and the Link. In order to prevent one Node from capitalizing on all the available channel time the Link will periodically pause responses(either Ack or Data) and send a Node Waiting Control Packet.

The Node Waiting control packet signals that the Link is opening up the channel for other available nodes. If nothing is heard in 20*(168/baud) seconds the communcation resumes as normal.

If a Link Request Control Packet is heard and acknowledged then the currently transmitting link will go into the Suspended State pausing all current communcations.

Nodes in the Listening state with pending communication are permitted to transmit a Link Request Control Packet during the 20*(168/baud) second period. Nodes should take two specific steps to prevent congestion in this critical section of the protocol.

1. Nodes should randomize the time that they send during the 20*(168/baud) period.
2. Nodes that fail to establish a link during a Node Waiting Control Packet should back off the next Node Waiting Control Packet by a random amount between 1 and (num attemps) ^ 2. After 5 failed attempts the node should fail the operation as busy.

Note that this algorith holds for any time a Node wants to try and establish a link on an open channel.

## Framing packet types

The first two bits of any frame packet denote the type of packet which is as follows:
```
00: Link Broadcast
01: Data
10: Ack
11: Ctrl
```

#### Broadcast packet
This packet is sent every 5 minutes while a Link is idle. It describes the common link parameters and supported API versions so that other Nodes can auto-discover their network parameters. A user needs to only setup the correct frequency and baud rate, from there RF NET handles the rest of the configuration of the network.
```
1b:(PacketType+FEC_ENABLED+RETRY_ENABLED)|2b:(MajorVer+MinorVer)|2b:LINK_WIDTH|Nb:LinkCallsign|(Nb+1b+2b+2b)*2:FEC
```

#### Control packet
Control packets are used to negotiate channel status, flow control and deliver out of band notifications.

```
1b:(PacketType+ControlType)|2b:SessionId|Nb:Callsign\0TargetCallsign|(Nb+1b+2b)*2:FEC
```

##### Control type(64)
```
0: Reserved for extension
1: Link request - A Node sends this packet to being establishing a link.
2: Link opened - A Link responds with this packet if the link was established.
3: Link close - A Node sends this when it is done communicating with a Link.
4: Link clear - A Link acknowledges closure with this packet.
5: Node waiting - A Link may send this packet to query for any nodes that are waiting to send data.
6: Notification - This packet is sent when a Link knows that there may be new data for a specific Node but that Node does not currently have an established link. A Node can use this as a hint to query the services it knows about for any new data.
```

#### Data packet
Data packets do the heavy lifting of the RF NET protocol. They are responsible for sequencing and the delivery of data. The size of data packets can be configured on a per-Link basis to best match the throughput of the specific radio parameters.

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
Used to acknowledge the receipt of data packets.
```
2b:(SequenceId+PacketType)|1b:(CorrectedErrors+NackFlag+NoResponseFlag)|6b:FEC
```

## RF Net Packet Spec
Once a sequence of framed data packets have been completely received they are assembled into a single RF NET packet to be parsed. Below lists the format of packets received by both the Link and Node during an exchange.

#### Request
Packet sent by Node to Link to perform some operation or query for data.
```
64b:sign|2b:sequenceId|1b:type|callsign@hub\0payload
```

##### Packet Type
```
0: Reserved for extension
1: REST call, payload takes format of: GET|PUT|PATCH|POST|DELETE|url\0headers\0body
2: Raw RF packet, payload takes format of: hub\0payload
```

#### Response
Packet send from Link to Node in response to a request.
```
callsign@hub\0|2b:sequenceId|1b:type|payload
```

##### Packet Type
```
0: Reserved for extension
1: REST response, payload takes format of: 1b:code|body
2: Raw RF packet, payload takes format of: hub\0payload
```

## RF NET Authentication
One of the unique things that RF NET brings as a protocol is the ability to definitively authenticate every radio transmission heard by a Link. This is accomplished via the use of public/private key cryptography to provide signature verification of any packet sent. Note that this is explicitly not encryption, each packet is still readable but is prefixed with a 64 byte signature.

When a Node sends a fully sequenced packet to a Link the Link will perform a couple extra steps to verify the sender is legitimate:
1. When a Node sends a packet it signs the packet with a Private Key that is secret and known only to the user of the Node client. It never leaves the radio or is uploaded to the internet at any point.
2. The Link verifies that the SequenceId matches SessionId+Number of packets received. The SessionId is chosen randomly on each new connection and mitigates the chance of a replay attack using the same packet data on a different Link.
3. The Link queries the Hub for the Public Keys of the Callsign provided by "callsign@hub" in the packet.
4. The Link performs a signature verification with the Signature, Payload and Public Key. If they all match the Link will execute the command. Otherwise the Link will notify the sender that the private key is not valid for that callsign and hub.

## RF NET Hub
The Hub of RF NET is a independently run host that provides a few key services of the protocol:
* Callsign registration.
* Callsign verification via published Public Keys.
* Message routing and storage for direct callsign to callsign messages.
* Discussion area for broadcast based messages based on club, location or specific interest.

All of these features are provided via an REST API described in this [Swagger Description](https://cdn.rawgit.com/vvanders/rfnet/292abfe3/docs/index.html).

## RF NET typical sequence
Below is a diagram of a typical sequence of packets for a single request from a Node. Multiple requests can be chained together inside of a Link request
as long a standard timeouts are respected.

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

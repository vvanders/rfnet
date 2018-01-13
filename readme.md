# RF NET
RF NET is a community oriented ham radio technology for local and long distance digital communcation. RF NET aims to unify a wide range of disparate technologies in a single location so that it doesn't matter if you're running APRS or on your desktop of smartphone. You'll always have access to the RF NET network.

## High level goals
With RF NET we set out to a series of goals that we think are important to build a network that would see wide adoption.

* RF Modulation agnostic.
* Data framing agnostic.
* Built-in support for FEC, sequencing, retry, backoff and routing.
* Open interconnect protocol using existing web specification and common technology.
* Packet + Callsign authentication with public/private key pairs.
* Message routing and inboxes via e-mail like addressing schema. Ex: "KI7EST@rfnethub.net"
* Support for community spaces to enable local clubs to communcate, coordiate events or build a meeting space around a common interest.
* Persistent broadcast messaging, telemetry and information grouped within a community space.


## Network overview

### Link types

### Node

### Hub

## RF NET Interconnect Spec

http://<hub>/dmessage
http://<hub>/bmessage
http://<hub>/thread
http://<hub>/list
http://<hub>/event
http://<hub>/telemetry

## RF NET Framing Spec

* Assumptions are that underlying framing protocol has the following properties:
  * Clearly specified start/end framing for discrete packet lengths, ideally noise resistant.
  * Integrity of inner data is not required or guaranteed.
* All packets contain preamble + payload for data packet.
* Preamble are FEC encodeded at 2x rate of header.
* Channel control is arbitrated by repeater with clients requesting open channel.

PacketType:
 00: Link Broadcast
 01: Data
 10: Ack
 11: Ctrl

Broadcast packet
1b:(PacketType+FEC_ENABLED+RETRY_ENABLED)|2b:(MajorVer+MinorVer)|2b:LINK_WIDTH|Nb:HubId|(Nb+1b+2b+2b)*2:FEC

Control packet

Control type:
 1: Link request ->
 2: Link opened <-
 3: Link close ->
 4: Link clear <-
 5: Notification <-

1b:(PacketType+ControlType)|Nb:Callsign|(Nb+1)*2:FEC

Data packet
Parameters:
    LINK_WIDTH: nominally 256 bytes but can be changed to adjust to native framing size
    FEC_BYTES: 2,4,6,8,16,32,64 number of bytes of FEC
    BLOCK_SIZE: 0,1,2,4,7,15,31,63 ratio of FEC bytes to data bytes. 0 means single block per packet

    START_FLAG:1
    END_FLAT:1
    BLOCK_SIZE:3(8)
    FEC_BYTES:3(8)

2b:(PacketIdx or SequenceId if StartFlag is set+PacketType)|1b:BLOCK_SIZE+FEC_BYTES+START_FLAG+END_FLAG|(1b+2b)*2:HeaderFEC|LINK_WIDTH-(FEC_BYTES+3b+6b):payload|FEC_BYTES:FEC

Ack packet
2b:(SequenceId+PacketType)|2b:(PacketIdx+NackFlag+NoResponseFlag)|1b:CorrectedErrors|10b:FEC

## RF Net Packet Spec

Request:
  64b:sign|2b:sequenceId|Callsign@hub\0hub\0payload

Response:
  2b:sequenceId|payload

## Supported Radios

* KISS TNC 1200bsp AFSK (APRS)
* KISS TNC 9600pbs K9NG (APRS)
* DSTAR 1200bps GMSK
* FaradyRF 500,000bps 2FSK

## REST API
/Verifiy
 auth tag
1. [x] KISS PHY 1200/9600 validation
2. [ ] PC-based RF NET Node:
    1. [ ] Localhost web interface.
    2. [ ] Detailed logging via web interface.
    3. [ ] HDLC derived KISS based framing.
    4. [ ] Packet sequencing and framing protocol.
    5. [ ] FEC mode for framing protocol.
    6. [ ] libsodium private/public key authentication.
3. [ ] In-memory RF NET Hub
    1. [ ] REST API stubs.
    2. [ ] User creation.
    3. [ ] Public key storage.
4. [ ] PC-based RF NET Link
    1. [ ] Web-based status/admin
    2. [ ] Detailed logging via web interface.
    3. [ ] API calls to RF NET Hub.
    4. [ ] Verification of authenticated packets.
5. [ ] Database-backed deployable RF NET Hub
    1. [ ] Pick a web framework(Rust:Iron+Diesel, Elixir:Phoenix, other?)
    2. [ ] Docker based deployment/env.
    3. [ ] Migrate #3 to framework.
    4. [ ] Implement REST APIs.
6. [ ] TUN based interface for [FaradayRF](https://faradayrf.com/) radios.
7. [ ] Serial port KISS support.
8. [ ] Raspberry Pi port of RF Net Node+Link.
9. [ ] Android port of RF Net Node.
10. [ ] Android port of RF Net Link.
11. [ ] Compat matrix for common radios (Kenwood, Yaesu, ICom, etc)
12. [ ] Integrated tutorial/setup for common radios.
13. [ ] Integrated modulation from Direwolf to simplify setup.

# Future features
* API Key storage + fowarding - Allow safe way to call REST APIs that use api keys without sending key over the radio. Centralized around the Hub you have your callsign registered to.
* OAUTH Key storage + forwarding - Same as above bit for OAuth(opens up things like Twitter, etc).
* Broadcast messages - Provide better experience for real-time two-way communcation.
* Duplex communcation through cross-band capable radios.
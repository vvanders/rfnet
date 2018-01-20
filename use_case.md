# What can I do with RF NET?
RF NET is designed to open a wide range of new ham radio opporunities through it's use of strong authentication, persistent storage and independently run servers. Below is just a sample of the things possible with RF NET.

## Direct Messaging
RF NET supports messaging through simple addressing that looks much like an email address. Simply send a message to "callsign@hub"(EX: "KI7EST@rfnethub.net") and the message will be routed to the respective server(hub) and stored there until the recipient can retrieve it. If the callsign was recently heard on a RF NET Link that link will attempt to notify the user that a new message is available.

Messages also support artibray data attachements. You can include images, spreadsheets or any other binary data that you want to convey.

## Robust modulation, error correction and multi-mode support
RF NET is modulation independent. This means as long as you can reach a RF NET Link that supports your radio's modulation you can access RF NET. Support for 1200, 9600 radios that use KISS AX.25 TNCs works out of the box and faster modes through radios such as FaradayRF will be possible as well.

In addition to modulation independece RF NET also supports multi-mode. This means that multiple modulation techniques can be used on the same broadcast frequency. Both 1200 and 9600 bps radios will be able to access the same RF NET Link on the same frequency. A RF NET Link is able to distinguish this through CSMA and HDLC framing letting you use the fastest speed possible for your radio.

## Get weather forecast, tweet from your radio and connect to the wider internet through RF NET
RF NET supports authenticated REST API calls as a first-class citizen in the protocol spec. Want to know the upcoming weather but outside of cellphone range? Make a call to NOAA's forecast API and find the high/low and upcoming weather conditions.

Want to post a tweet from your radio up on the mountain? RF NET's strong authentication makes it possible to send a tweet and *know* that it came from your radio and no-one else.

Need to control a repeater or other internet connected hardware with confidence? Want to send telemetry data to a public endpoint on the internet? Use RF NET's authentication API to verfiy that your REST calls are coming from a secure radio.

## Group messaging
All of the features found in direct messaging(routing, confirmation, data attachments) are also available in a group messaging format. Groups based on your local club, common interest or other areas can be registered on any RF NET Hub. You can then check them from your radio at any time.

Want to post a notice about an upcoming net? Looking for a central place to broadcast changes to your club's next meeting? Use RF NET to simplify your ham communcation.

## The internet as a first-class citizen
Even when you're away from the radio you still will always have access to RF NET. Once a user has authenticated themselves as a valid license holder(by transmitting a key exchange to a local RF NET Link) anything possible from your radio can be done via a simple web-based interface.

You can send and recieve messages to other users who may be out of internet range but still in range of a RF NET Link. Transfer files, update group statuses, anything you can do with a radio you can do from the internet as well.
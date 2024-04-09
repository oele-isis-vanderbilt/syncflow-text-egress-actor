# Livekit Text Egress Actor

First of all, having programmed in rust for a couple of months for now, I was curious to implement a library that others might find useful. I have been working with the Livekit API for a while now and I thought it would be a good idea to implement a library that would help others to interact with the Livekit API.


The livekit egress servers handle the complex multimedia recording. However, text/data channel egress is not supported. This library attempts to implement an actix actor that can be used to extract text from the data channel and send it to a specified endpoint (S3).

## Architecture

![Architecture](./docs/images/arch.png)

1. The TextEgressActor: This actor is responsible for extracting text from the data channel and sending it to the specified endpoint based on a specific topic.
2. Multiple RoomListeners: These actors are responsible for listening to the data channel of a specific room and saving the data to the temporary file system.
3. The S3Uploader: This actor is responsible for uploading the data to the specified S3 bucket.


> [!WARNING]  
> Work in progress, not ready for production use yet

## Funding info
This work is supported by the National Science Foundation under Grant No. DRL-2112635.

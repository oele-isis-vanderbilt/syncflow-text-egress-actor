# Livekit Text Egress Actor

First of all, having programmed in rust for a couple of months for now, I was curious to implement a library that others might find useful. I have been working with the Livekit API for a while now and I thought it would be a good idea to implement a library that would help others to interact with the Livekit API.


The livekit egress servers handle the complex multimedia recording. However, text/data channel egress is not supported. This library attempts to implement an actix actor that can be used to extract text from the data channel and send it to a specified endpoint (S3).

## Goals/ Roadmap

- [ ] Implement a basic actor and messaging model
- [ ] Implement a basic text extraction model as a task queue/ livekit client server api and tokio

> [!WARNING]  
> Work in progress, not ready for production use yet

## Funding info
This work is supported by the National Science Foundation under Grant No. DRL-2112635.

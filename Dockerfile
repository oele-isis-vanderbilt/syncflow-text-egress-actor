FROM rust:slim AS builder

WORKDIR /app

COPY . .

RUN apt-get update && apt-get install libpq-dev pkg-config g++ libx11-dev libxext-dev libgl1-mesa-dev -y && cargo build --bin syncflow-text-egress-actor --release

FROM ubuntu:latest

ARG APP=/app

ENV TZ=Etc/UTC \
    APP_USER=livekit

RUN groupadd $APP_USER && useradd -g $APP_USER $APP_USER

RUN apt-get update && apt-get install tzdata libpq-dev ca-certificates -y && rm -rf /var/cache/apk/* && update-ca-certificates

WORKDIR $APP

COPY --from=builder /app/target/release/syncflow-text-egress-actor ./service

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER

ENTRYPOINT ["./service"]

CMD ["./service"]
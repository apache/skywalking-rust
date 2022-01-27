FROM rust:1.57
WORKDIR /build
COPY . /build/
RUN cd e2e && cargo build --release --locked
ENTRYPOINT ["/build/e2e/target/release/e2e"]
